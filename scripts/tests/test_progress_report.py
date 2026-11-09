"""Tests for scripts/progress-report and the --json status of scripts/test-port.

Run: python3 -I -m unittest discover -s scripts/tests -p 'test_progress_report.py' -v

Parsers and rendering use inline fixtures (no git, no subprocess). The test-port JSON test
builds a synthetic inventory and a throwaway Rust tree in fresh tempfile.mkdtemp() directories
(never deleted, never reused).
"""

import importlib.machinery
import importlib.util
import io
import json
import os
import contextlib
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPTS = os.path.dirname(HERE)


def _load(name, filename):
    path = os.path.join(SCRIPTS, filename)
    loader = importlib.machinery.SourceFileLoader(name, path)
    spec = importlib.util.spec_from_loader(name, loader)
    mod = importlib.util.module_from_spec(spec)
    loader.exec_module(mod)
    return mod


pr = _load("progress_report", "progress-report")
tp = _load("test_port_mod", "test-port")
us = _load("upstream_sync_mod", "upstream-sync")

ROADMAP_TEXT = """# Roadmap

Priority order set by the user.

1. **Search** (in progress)
   - [x] Search view / find in files (ADR-0023)
   - [x] In-editor Find widget: Cmd+F, replace (ADR-0027)
   - Next: Replace in the Search view, Command Palette
2. **Settings migration**
   - [x] Configuration service + live reload
   - Map all ~2062 settings, then migrate feature group by group,
     each with a headless test proving the effect
3. **Extensions** (ADR-0020)
   - P0 Node extension-host handshake

Scope: macOS desktop only (ADR-0019).
"""

SCENARIO_TEXT = """# Search e2e scenarios

Upstream dir: `test/smoke/src/areas/search`

| # | scenario (it title) | suite | upstream file:line | steps (summary) | status | notes |
|---|---|---|---|---|---|---|
| 1 | finds text in files | Search | `search.test.ts:10` | types a query | ready | |
| 2 | replaces in files | Search | `search.test.ts:20` | replaces | pending | needs replace |
| 3 | opens a terminal | Terminal | `terminal.test.ts:5` | runs a shell | blocked-on-feature terminal; extension host | |
| 4 | a pipe \\| inside a cell | Search | `search.test.ts:30` | x | blocked-on-feature chat | |
"""

DECISIONS_TEXT = """# Decisions

## ADR-0001 — Native Rust UI (2026-11-09)

body

## ADR-0019 — Platform scope: macOS desktop only; quit on last window (2026-11-09)

## ADR-0027 — Find widget (2026-11-10)
"""

PORTS_TEXT = """
[upstream]
repo = "https://github.com/microsoft/vscode"
clone = "../vscode-original"
ref = "HEAD"

[[port]]
id = "editor-text-model"
status = "partial"
upstream = [
  "src/vs/editor/common/model/**",
  "src/vs/editor/test/common/model/**",
]
rust = ["crates/vsr-text/src/lib.rs"]
synced = "abc"

[[port]]
id = "planned-thing"
status = "planned"
upstream = ["src/x/**"]
rust = []
"""


class RoadmapTest(unittest.TestCase):
    def test_sections_items_and_focus(self):
        sections = pr.parse_roadmap(ROADMAP_TEXT)
        self.assertEqual([s["num"] for s in sections], [1, 2, 3])
        search = sections[0]
        self.assertTrue(search["current"])
        self.assertEqual(search["title"], "Search")
        self.assertEqual([i["done"] for i in search["items"]], [True, True, False])
        self.assertFalse(sections[1]["current"])
        self.assertEqual(sections[2]["items"][0]["text"], "P0 Node extension-host handshake")

    def test_wrapped_continuation_is_joined_and_prose_ends_section(self):
        settings = pr.parse_roadmap(ROADMAP_TEXT)[1]
        self.assertEqual(len(settings["items"]), 2)
        self.assertIn("each with a headless test proving the effect", settings["items"][1]["text"])
        self.assertFalse(settings["items"][1]["done"])
        self.assertNotIn("Scope", " ".join(i["text"] for s in pr.parse_roadmap(ROADMAP_TEXT)
                                           for i in s["items"]))


class ScenarioTest(unittest.TestCase):
    def test_counts_per_status_bucket(self):
        row = pr.parse_scenario_text("search", SCENARIO_TEXT)
        self.assertEqual(row["title"], "Search")
        self.assertEqual((row["ready"], row["pending"], row["blocked"], row["other"]),
                         (1, 1, 2, 0))
        self.assertEqual(row["total"], 4)

    def test_separator_and_non_table_lines_are_skipped(self):
        row = pr.parse_scenario_text("x", "# X e2e scenarios\n\n|---|---|\n| 1 | a | ready |\n")
        self.assertEqual(row["total"], 0)  # no status column header, nothing counted

    def test_status_column_found_by_header_name(self):
        text = ("| # | status |\n|---|---|\n| 1 | ready |\n| 2 | pending later |\n"
                "| 3 | Blocked-on-feature x |\n")
        row = pr.parse_scenario_text("y", text)
        self.assertEqual((row["ready"], row["pending"], row["blocked"]), (1, 1, 1))


class DecisionsAndPortsTest(unittest.TestCase):
    def test_adr_count_and_latest_first(self):
        adrs = pr.parse_decisions(DECISIONS_TEXT)
        self.assertEqual([a["num"] for a in adrs], [27, 19, 1])
        self.assertEqual(adrs[1]["title"], "Platform scope: macOS desktop only; quit on last window")
        self.assertEqual(adrs[0]["date"], "2026-11-10")

    def test_ports_counts_and_path_counts(self):
        data, _ = us.parse_toml(PORTS_TEXT)
        ports = pr.parse_ports(data)
        self.assertEqual(ports["counts"], {"planned": 1, "partial": 1, "ported": 0})
        first = ports["rows"][0]
        self.assertEqual((first["id"], first["upstream"], first["rust"]), ("editor-text-model", 2, 1))


class HistoryTest(unittest.TestCase):
    def snap(self, sha, wired):
        return {"date": "2026-11-09", "sha": sha, "settings_wired": wired}

    def test_same_sha_replaces_in_place(self):
        h = [self.snap("a", 1), self.snap("b", 2)]
        out = pr.append_history(h, self.snap("a", 9))
        self.assertEqual([e["sha"] for e in out], ["a", "b"])
        self.assertEqual(out[0]["settings_wired"], 9)
        self.assertEqual(h[0]["settings_wired"], 1)  # input not mutated

    def test_new_sha_appends(self):
        out = pr.append_history([self.snap("a", 1)], self.snap("c", 3))
        self.assertEqual([e["sha"] for e in out], ["a", "c"])

    def test_last_known_skips_nulls(self):
        h = [{"rust_tests_passing": 5}, {"rust_tests_passing": None}]
        self.assertEqual(pr.last_known(h, "rust_tests_passing"), 5)
        self.assertIsNone(pr.last_known([], "rust_tests_passing"))

    def test_rust_result_parsing_sums_binaries(self):
        text = ("test result: ok. 140 passed; 0 failed; 0 ignored\n"
                "test result: ok. 0 passed; 0 failed; 1 ignored\n"
                "test result: FAILED. 2 passed; 1 failed; 0 ignored\n")
        self.assertEqual(pr.parse_rust_results(text), {"passed": 142, "failed": 1, "suites": 3})


def fixture_data(**over):
    snap = {"date": "2026-11-09", "sha": "abc1234", "settings_wired": 40,
            "settings_counts": {"wired": 40, "exthost": 426, "blocked": 30, "n/a": 510, "todo": 1056},
            "tests_ported": 248, "tests_total": 57836, "scenarios_ready": 8, "scenarios_total": 127,
            "ports_by_status": {"planned": 1, "partial": 17, "ported": 0},
            "rust_tests_passing": 512, "rust_tests_failed": 0}
    history = [dict(snap, date="2026-11-01", sha="old0000", settings_wired=30),
               snap]
    d = {
        "snapshot": snap, "history": history, "notices": ["settings figures unavailable <x>"],
        "rust_source": "run",
        "git": {"sha": "abc1234", "full": "abc", "branch": "main", "date": "2026-11-09",
                "recent": [{"sha": "abc1234", "date": "2026-11-09", "subject": "Fix <b>& stuff"}],
                "per_day": [("2026-11-08", 2), ("2026-11-09", 5)]},
        "settings": {"totals": {"wired": 40, "exthost": 426, "blocked": 30, "n/a": 510, "todo": 1056},
                     "naReasons": {"deprecated upstream": 140, "web-only": 7},
                     "blockedReasons": {"needs editor splits and editor groups": 20,
                                        "needs navigation history (Go Back / Go Forward)": 10},
                     "todoPriorities": {"P1": 298, "P2": 594, "P3": 164}},
        "tests": {"upstream": {"commit": "c", "ref": "HEAD", "files": 3, "tests": 57836},
                  "ported": 248, "total": 57836, "touched": 12,
                  "areas": [{"area": "editor/common", "layers": ["common"], "files": 71,
                             "tests": 1136, "ported": 75}],
                  "layers": [{"layer": "common", "tests": 1136, "ported": 75}],
                  "orphans": 13, "mismatched": 0, "staleHeaders": 0, "changedHeaders": 0},
        "roadmap": pr.parse_roadmap(ROADMAP_TEXT),
        "scenarios": [pr.parse_scenario_text("search", SCENARIO_TEXT)],
        "ports": pr.parse_ports(us.parse_toml(PORTS_TEXT)[0]),
        "adrs": pr.parse_decisions(DECISIONS_TEXT),
        "generated": "2026-11-09 12:00 UTC",
    }
    d.update(over)
    return d


class RenderTest(unittest.TestCase):
    def setUp(self):
        self.html = pr.render(fixture_data())

    def test_page_shell_and_no_external_resources(self):
        self.assertIn("<title>rscode progress</title>", self.html)
        self.assertIn("rscode — progress", self.html)
        self.assertIn("prefers-color-scheme: dark", self.html)
        self.assertNotIn("<script", self.html)
        self.assertNotIn("<link", self.html)
        self.assertNotIn("src=", self.html)
        self.assertNotIn("@import", self.html)
        self.assertNotIn("http://", self.html)
        self.assertNotIn("https://", self.html)

    def test_key_sections_present(self):
        for anchor in ('id="kpis"', 'id="settings"', 'id="roadmap"', 'id="tests"',
                       'id="scenarios"', 'id="modules"', 'id="trends"', 'id="recent"'):
            self.assertIn(anchor, self.html)
        self.assertIn("Settings wired", self.html)
        self.assertIn("Upstream tests ported", self.html)
        self.assertIn("Smoke scenarios ready", self.html)
        self.assertIn("Rust tests passing", self.html)
        self.assertIn("ADR-0027", self.html)
        self.assertIn("current focus", self.html)

    def test_numbers_are_in_text(self):
        self.assertIn("57,836", self.html)
        self.assertIn("40", self.html)
        self.assertIn("deprecated upstream", self.html)
        self.assertIn("P1", self.html)

    def test_commit_subjects_are_escaped(self):
        self.assertIn("Fix &lt;b&gt;&amp; stuff", self.html)
        self.assertNotIn("<b>&", self.html)

    def test_trend_text_and_svg(self):
        self.assertIn("<svg", self.html)
        self.assertIn("<polyline", self.html)
        one = pr.render(fixture_data(history=[fixture_data()["snapshot"]]))
        self.assertIn("trend appears after the next run", one)

    def test_missing_settings_renders_placeholder(self):
        html_out = pr.render(fixture_data(settings=None, tests=None))
        self.assertIn("Settings data unavailable in this run.", html_out)
        self.assertIn("Test data unavailable in this run.", html_out)

    def test_blocked_segment_legend_and_reasons(self):
        self.assertIn('class="stack"', self.html)
        self.assertIn("seg-blocked", self.html)
        self.assertIn("Blocked (needs a feature first): <strong>30</strong>", self.html)
        self.assertIn("<h3>Blocked, by reason</h3>", self.html)
        self.assertIn("needs editor splits and editor groups", self.html)
        self.assertIn("needs navigation history (Go Back / Go Forward)", self.html)
        self.assertIn("aria-label=\"Settings: 40 of 2,062 wired, 426 via extension host, 30 blocked, 510 not applicable, 1,056 to do\"",
                      self.html)

    def test_blocked_colour_defined_for_light_and_dark(self):
        self.assertIn("--blocked: #7a4f9a;", self.html)
        self.assertIn("--blocked: #b899d9;", self.html)
        self.assertLess(self.html.index("--blocked: #7a4f9a;"), self.html.index("prefers-color-scheme: dark"))

    def test_older_snapshot_without_blocked_counts_as_zero(self):
        old = dict(fixture_data()["snapshot"])
        old["settings_counts"] = {"wired": 30, "exthost": 426, "n/a": 540, "todo": 1066}
        html_out = pr.render(fixture_data(snapshot=old, settings=None))
        self.assertIn("Settings data unavailable in this run.", html_out)
        self.assertIn("of 2,062", html_out)
        self.assertEqual(pr.settings_counts_from(old["settings_counts"])["blocked"], 0)

    def test_settings_without_blocked_reasons_renders_none(self):
        data = fixture_data()
        del data["settings"]["blockedReasons"]
        data["settings"]["totals"].pop("blocked")
        html_out = pr.render(data)
        self.assertIn("<h3>Blocked, by reason</h3>", html_out)
        self.assertIn("None.", html_out)

    def test_settings_counts_from_totals(self):
        self.assertIsNone(pr.settings_counts_from(None))
        self.assertEqual(pr.settings_counts_from({"wired": 1, "blocked": 2}),
                         {"wired": 1, "exthost": 0, "blocked": 2, "n/a": 0, "todo": 0})

    def test_rust_not_run_text(self):
        html_out = pr.render(fixture_data(rust_source=None))
        self.assertIn("not run yet", html_out)


class CollectHelpersTest(unittest.TestCase):
    def test_scenario_bucket(self):
        self.assertEqual(pr.scenario_bucket("ready"), "ready")
        self.assertEqual(pr.scenario_bucket("blocked-on-feature x"), "blocked")
        self.assertEqual(pr.scenario_bucket(""), "other")


class TestPortJsonTest(unittest.TestCase):
    def _inv(self, clone):
        return {"commit": "c" * 40, "ref": "HEAD", "tree": {}, "files": [{
            "path": "src/vs/editor/test/common/model/a.test.ts", "layer": "common",
            "area": "editor/common", "module": "src/vs/editor/common/model/a.test.ts",
            "suites": [],
            "tests": [
                {"suite": ["S"], "title": "one", "dynamic": False, "skip": False,
                 "looped": False, "line": 1, "key": "S > one"},
                {"suite": ["S"], "title": "two", "dynamic": False, "skip": False,
                 "looped": False, "line": 2, "key": "S > two"},
                {"suite": ["S"], "title": "three", "dynamic": False, "skip": True,
                 "looped": False, "line": 3, "key": "S > three"},
            ]}]}

    def test_status_json_matches_status_data(self):
        root = tempfile.mkdtemp(prefix="progress-report-root-")
        rust_dir = os.path.join(root, "crates", "x", "tests")
        os.makedirs(rust_dir)
        with open(os.path.join(rust_dir, "a.rs"), "w", encoding="utf-8") as fh:
            fh.write("//! upstream: src/vs/editor/test/common/model/a.test.ts @ abc1234\n"
                     "#[test] fn one() {}\n"
                     "// upstream: S > one\n"
                     "// upstream: S > missing\n")
        inv = self._inv(root)
        d = tp.status_data(inv, root)
        out = tp.status_json(inv, d)
        text = json.dumps(out, indent=2)
        parsed = json.loads(text)
        self.assertEqual(parsed["total"], 3)
        self.assertEqual(parsed["ported"], d["ported"])
        self.assertEqual(parsed["ported"], 1)
        self.assertEqual(parsed["orphans"], len(d["orphans"]))
        self.assertEqual(parsed["orphans"], 1)
        self.assertEqual(parsed["areas"][0]["area"], "editor/common")
        self.assertEqual(parsed["areas"][0]["tests"], 3)
        self.assertEqual(parsed["layers"], [{"layer": "common", "tests": 3, "ported": 1}])
        self.assertEqual(parsed["upstream"]["commit"], "c" * 40)

    def test_status_json_flag_prints_json_only(self):
        root = tempfile.mkdtemp(prefix="progress-report-root-")
        inv = self._inv(root)
        out = io.StringIO()
        args = type("A", (), {"all_files": False, "json": True})()
        with contextlib.redirect_stdout(out):
            tp.cmd_status(args, inv, root)
        parsed = json.loads(out.getvalue())
        self.assertEqual(parsed["total"], 3)


if __name__ == "__main__":
    unittest.main()
