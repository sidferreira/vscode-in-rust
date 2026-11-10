"""Tests for scripts/dependency-tree. Run from the repo root:

    python3 -I -m unittest discover -s scripts/tests -p 'test_dependency_tree.py' -v

Graph and counting tests use inline fixtures. The repo tests read the real files (settings.toml,
the settings export, tests/e2e-scenarios, docs/dependencies.toml, extensions-compat.md) and fail
when a blocker, an exthost source or a scenario label has no node, or when the generated doc is
stale. Output goes to a fresh tempfile.mkdtemp() directory that is never deleted or reused.
"""

import collections
import importlib.machinery
import importlib.util
import io
import contextlib
import json
import os
import re
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
SCRIPT = os.path.join(os.path.dirname(HERE), "dependency-tree")
_loader = importlib.machinery.SourceFileLoader("dependency_tree", SCRIPT)
_spec = importlib.util.spec_from_loader("dependency_tree", _loader)
dt = importlib.util.module_from_spec(_spec)
_loader.exec_module(dt)

HISTORY = os.path.join(REPO, "docs", "progress-history.json")
DOC = os.path.join(REPO, "docs", "dependency-tree.md")


def node(nid, status="todo", needs=(), **extra):
    base = {"id": nid, "title": nid, "status": status, "kind": "feature", "effort": "S",
            "effort_source": "guess", "needs": list(needs), "reasons": [], "sources": [],
            "todo_globs": [], "scenarios": [], "external": "", "deferred": False, "note": ""}
    if status == "blocked-external" and not extra.get("external"):
        extra["external"] = "fixture"
    base.update(extra)
    return base


def fixture_model(nodes, settings=(), scenarios=(), phase_efforts=None):
    return {"nodes": collections.OrderedDict((n["id"], n) for n in nodes), "settings": list(settings),
            "scenarios": list(scenarios), "phase_efforts": phase_efforts or {}, "sp": None}


class ScenarioParsingTest(unittest.TestCase):
    def test_blocker_labels_split_on_semicolons(self):
        self.assertEqual(dt.blocker_labels("blocked-on-feature chat/agents; terminal sandbox"),
                         ["chat/agents", "terminal sandbox"])

    def test_bare_blocked_names_no_feature(self):
        self.assertEqual(dt.blocker_labels("blocked"), [])

    def test_free_text_blocked_is_one_label(self):
        self.assertEqual(dt.blocker_labels("blocked (needs Explorer copy, paste and duplicate)"),
                         ["blocked (needs Explorer copy, paste and duplicate)"])

    def test_buckets(self):
        self.assertEqual(dt.scenario_bucket("ready"), "ready")
        self.assertEqual(dt.scenario_bucket("pending"), "pending")
        self.assertEqual(dt.scenario_bucket("blocked-on-feature terminal"), "blocked")
        self.assertEqual(dt.scenario_bucket("unit"), "unit")


class CycleTest(unittest.TestCase):
    def test_finds_a_cycle(self):
        nodes = collections.OrderedDict((n["id"], n) for n in [
            node("a", needs=["c"]), node("b", needs=["a"]), node("c", needs=["b"])])
        cycles = dt.find_cycles(nodes)
        self.assertEqual(len(cycles), 1)
        self.assertEqual(cycles[0][0], cycles[0][-1])
        self.assertEqual(set(cycles[0]), {"a", "b", "c"})

    def test_acyclic_graph_has_no_cycle(self):
        nodes = collections.OrderedDict((n["id"], n) for n in [
            node("a"), node("b", needs=["a"]), node("c", needs=["a", "b"])])
        self.assertEqual(dt.find_cycles(nodes), [])

    def test_validate_reports_cycle(self):
        model = fixture_model([node("a", needs=["b"]), node("b", needs=["a"])])
        self.assertTrue(any("cycle" in e for e in dt.validate(model)))


class DownstreamTest(unittest.TestCase):
    def setUp(self):
        # a -> b -> c, a -> d, and extension e needs c
        self.nodes = [
            node("a"), node("b", needs=["a"]), node("c", needs=["b"]), node("d", needs=["a"]),
            node("e", needs=["c"], kind="extension", effort="M", effort_source="guess"),
        ]
        self.model = fixture_model(self.nodes)

    def test_downstream_is_transitive(self):
        children = dt.children_of(self.model["nodes"])
        self.assertEqual(dt.downstream_of("a", children), {"b", "c", "d", "e"})
        self.assertEqual(dt.downstream_of("c", children), {"e"})
        self.assertEqual(dt.downstream_of("e", children), set())

    def test_counts_dedupe_shared_settings(self):
        att = {nid: {"settings": set(), "scenarios": set()} for nid in self.model["nodes"]}
        att["a"]["settings"] = {"s1"}
        att["c"]["settings"] = {"s1", "s2"}          # shared with a's subtree
        att["b"]["scenarios"] = {("f.md", 1)}
        att["e"]["scenarios"] = {("f.md", 1), ("f.md", 2)}  # overlap counted once
        counts = dt.unlock_counts(self.model, {"attrs": att})
        self.assertEqual(counts["a"]["direct_dep_settings"], 1)
        self.assertEqual(counts["a"]["free_dep_settings"], 2)
        self.assertEqual(counts["a"]["free_scenarios"], 2)
        self.assertEqual(counts["a"]["free_extensions"], 1)
        self.assertEqual(counts["a"]["free_extension_ids"], ["e"])
        self.assertEqual(counts["a"]["direct_dependents"], ["b", "d"])
        self.assertEqual(counts["c"]["free_dep_settings"], 2)   # c's own settings plus e's (none)
        self.assertEqual(counts["e"]["free_dep_settings"], 0)

    def test_counting_stops_at_a_gate(self):
        # a -> g (blocked-external) -> c, and a -> d. Behind the gate: c and anything under it.
        nodes = [node("a"), node("g", status="blocked-external", needs=["a"]),
                 node("c", needs=["g"]), node("d", needs=["a"])]
        model = fixture_model(nodes)
        att = {nid: {"settings": set(), "scenarios": set()} for nid in model["nodes"]}
        att["a"]["settings"] = {"ka"}
        att["d"]["settings"] = {"kd"}
        att["g"]["settings"] = {"kg"}
        att["c"]["settings"] = {"kc"}
        counts = dt.unlock_counts(model, {"attrs": att})
        self.assertEqual(dt.free_downstream("a", dt.children_of(model["nodes"]), model["nodes"]), {"d"})
        self.assertEqual(counts["a"]["free_dep_settings"], 2)     # ka + kd
        self.assertEqual(counts["a"]["gated_dep_settings"], 2)    # kg + kc
        self.assertEqual(counts["a"]["gated_extensions"], 0)
        # If the gate itself were built, its dependent c would be free: kg + kc.
        self.assertEqual(counts["g"]["free_dep_settings"], 2)

    def test_a_dependent_with_another_open_need_is_not_free(self):
        nodes = [node("a"), node("x"), node("b", needs=["a", "x"])]
        model = fixture_model(nodes)
        self.assertEqual(dt.free_downstream("a", dt.children_of(model["nodes"]), model["nodes"]), set())
        nodes[1]["status"] = "done"
        model = fixture_model(nodes)
        self.assertEqual(dt.free_downstream("a", dt.children_of(model["nodes"]), model["nodes"]), {"b"})

    def test_deferred_is_a_gate_too(self):
        nodes = [node("a"), node("v", deferred=True, needs=["a"]), node("w", needs=["v"])]
        model = fixture_model(nodes)
        children = dt.children_of(model["nodes"])
        self.assertEqual(dt.free_downstream("a", children, model["nodes"]), set())
        self.assertEqual(dt.downstream_of("a", children), {"v", "w"})

    def test_score_weights_extensions(self):
        att = {nid: {"settings": set(), "scenarios": set()} for nid in self.model["nodes"]}
        counts = dt.unlock_counts(self.model, {"attrs": att})
        self.assertEqual(counts["a"]["score"], dt.SCORE_EXTENSION_WEIGHT)  # one free extension, no settings


class RankingTest(unittest.TestCase):
    def test_only_todo_with_done_needs_is_rankable(self):
        nodes = [
            node("done1", status="done"),
            node("ready", needs=["done1"]),          # rankable
            node("waits", needs=["ready"]),           # needs a todo
            node("deferred", deferred=True),          # excluded by decision
            node("external", status="blocked-external"),
            node("inprog", status="in-progress"),
            node("after-inprog", needs=["inprog"]),   # needs in-progress, not done
            node("free"),                             # no needs at all
        ]
        model = fixture_model(nodes)
        counts = dt.unlock_counts(model, {"attrs": {n["id"]: {"settings": set(), "scenarios": set()}
                                                    for n in nodes}})
        self.assertEqual(dt.ranked(model, counts), ["free", "ready"])
        self.assertEqual(dt.unlocked_by_in_progress(model), ["after-inprog"])

    def test_sorted_by_score_then_id(self):
        nodes = [node("x"), node("y"), node("z", needs=["x"])]
        model = fixture_model(nodes)
        att = {"x": {"settings": {"k1", "k2"}, "scenarios": set()},
               "y": {"settings": {"k3"}, "scenarios": set()},
               "z": {"settings": set(), "scenarios": set()}}
        counts = dt.unlock_counts(model, {"attrs": att})
        self.assertEqual(dt.ranked(model, counts), ["x", "y"])


class ValidationTest(unittest.TestCase):
    def test_clean_fixture_validates(self):
        model = fixture_model([node("a", status="done"), node("b", needs=["a"])])
        self.assertEqual(dt.validate(model), [])

    def test_unknown_need_and_done_with_todo_need(self):
        model = fixture_model([node("a"), node("b", status="done", needs=["a", "ghost"])])
        errors = dt.validate(model)
        self.assertTrue(any("unknown node 'ghost'" in e for e in errors))
        self.assertTrue(any("is done but needs a" in e for e in errors))

    def test_blocked_external_needs_text(self):
        blocked = node("c", status="blocked-external")
        blocked["external"] = ""
        model = fixture_model([blocked])
        self.assertTrue(any("blocked-external needs" in e for e in dt.validate(model)))

    def test_effort_must_match_section_4(self):
        model = fixture_model(
            [node("p", effort="L", effort_source="extensions-compat §4 (P1)")],
            phase_efforts={"P1": "M"})
        self.assertTrue(any("does not match P1" in e for e in dt.validate(model)))

    def test_reason_owned_once(self):
        settings = [{"key": "k", "status": "blocked", "reason": "r1", "source": "core"}]
        model = fixture_model([node("a", reasons=["r1"]), node("b", reasons=["r1"])], settings=settings)
        self.assertTrue(any("claimed by" in e for e in dt.validate(model)))

    def test_unmapped_blocked_reason_is_an_error(self):
        settings = [{"key": "k", "status": "blocked", "reason": "r1", "source": "core"}]
        model = fixture_model([node("a")], settings=settings)
        self.assertTrue(any("blocked reason has no node" in e for e in dt.validate(model)))

    def test_stale_scenario_label_is_an_error(self):
        rows = [{"file": "f.md", "number": 1, "cell": "blocked-on-feature x", "bucket": "blocked",
                 "labels": ["x"]}]
        model = fixture_model([node("a", scenarios=["y"])], scenarios=rows)
        errors = dt.validate(model)
        self.assertTrue(any("scenario label matches no row" in e for e in errors))
        self.assertTrue(any("scenario blocker has no node" in e for e in errors))


class AttributionTest(unittest.TestCase):
    def test_todo_globs_and_exthost_sources(self):
        nodes = [node("t", todo_globs=["terminal*"]), node("g", sources=["vscode.git"])]
        settings = [
            {"key": "terminal.integrated.shell", "status": "todo", "reason": "", "source": "core"},
            {"key": "git.enabled", "status": "exthost", "reason": "", "source": "vscode.git"},
            {"key": "editor.x", "status": "todo", "reason": "", "source": "core"},
        ]
        model = fixture_model(nodes, settings=settings)
        model["sp"] = _sp()
        att = dt.attribute(model)
        self.assertEqual(att["attrs"]["t"]["settings"], {"terminal.integrated.shell"})
        self.assertEqual(att["attrs"]["g"]["settings"], {"git.enabled"})
        self.assertEqual(att["unattributed"]["todo"], ["editor.x"])

    def test_scenario_with_two_blockers_counts_under_both(self):
        nodes = [node("a", scenarios=["x"]), node("b", scenarios=["y"])]
        rows = [{"file": "f.md", "number": 3, "cell": "blocked-on-feature x; y", "bucket": "blocked",
                 "labels": ["x", "y"]}]
        model = fixture_model(nodes, scenarios=rows)
        model["sp"] = _sp()
        att = dt.attribute(model)
        self.assertEqual(att["attrs"]["a"]["scenarios"], {("f.md", 3)})
        self.assertEqual(att["attrs"]["b"]["scenarios"], {("f.md", 3)})
        self.assertEqual(att["attributed_rows"], 1)


def _sp():
    return dt.load_settings_port()


class RealRepoTest(unittest.TestCase):
    """The checks that matter in the repo: every source item has a node, and the numbers agree."""

    @classmethod
    def setUpClass(cls):
        cls.model = dt.load_model()
        cls.errors = dt.validate(cls.model)
        cls.att = dt.attribute(cls.model)
        cls.counts = dt.unlock_counts(cls.model, cls.att)

    def test_model_is_valid(self):
        self.assertEqual(self.errors, [])

    def test_every_blocked_reason_is_mapped_to_one_node(self):
        blocked = {s["reason"] for s in self.model["settings"] if s["status"] == "blocked"}
        mapped = [r for n in self.model["nodes"].values() for r in n["reasons"]]
        self.assertEqual(set(mapped), blocked)
        self.assertEqual(len(mapped), len(set(mapped)))

    def test_every_exthost_source_is_mapped(self):
        exthost = {s["source"] for s in self.model["settings"] if s["status"] == "exthost"}
        mapped = {s for n in self.model["nodes"].values() for s in n["sources"]}
        self.assertEqual(mapped, exthost)

    def test_every_blocked_setting_is_attributed(self):
        self.assertEqual(self.att["unattributed"]["blocked"], [])
        self.assertEqual(self.att["unattributed"]["exthost"], [])

    def test_every_scenario_label_is_mapped(self):
        labels = {lab for row in self.model["scenarios"] if row["bucket"] == "blocked"
                  for lab in row["labels"]}
        mapped = {lab for n in self.model["nodes"].values() for lab in n["scenarios"]}
        self.assertTrue(labels <= mapped, sorted(labels - mapped))
        self.assertEqual(self.att["unattributed_rows"], [])

    def test_status_counts_match_latest_progress_history(self):
        with open(HISTORY, encoding="utf-8") as fh:
            latest = json.load(fh)[-1]
        counts = collections.Counter(s["status"] for s in self.model["settings"])
        self.assertEqual({k: counts[k] for k in latest["settings_counts"]}, latest["settings_counts"])

    def test_ready_scenarios_match_latest_progress_history(self):
        with open(HISTORY, encoding="utf-8") as fh:
            latest = json.load(fh)[-1]
        ready = sum(1 for r in self.model["scenarios"] if r["bucket"] == "ready")
        self.assertEqual(ready, latest["scenarios_ready"])

    def test_scenario_total_reconciles_with_history(self):
        # scripts/progress-report reads only the first status table of each file. search.md has a
        # second table (rows 8-11, "Settings migration: search group") that it never reads, so its
        # total is every row minus those four.
        with open(HISTORY, encoding="utf-8") as fh:
            latest = json.load(fh)[-1]
        missed = [r for r in self.model["scenarios"] if r["file"] == "search.md" and r["number"] >= 8]
        self.assertEqual(latest["scenarios_total"], len(self.model["scenarios"]) - len(missed))

    def test_every_extension_phase_has_a_node(self):
        phases = set(self.model["phase_efforts"]) - {"P0"}
        referenced = set()
        for n in self.model["nodes"].values():
            match = re.search(r"\((P\d+b?)\)", n["effort_source"])
            if match:
                referenced.add(match.group(1))
        self.assertEqual(phases, referenced)

    def test_adr_citations_exist_in_decisions(self):
        self.assertTrue(self.model["adrs"])
        cited = {num for n in self.model["nodes"].values()
                 for num in re.findall(r"ADR-(\d{4})", n["title"] + " " + n["note"] + " " + n["external"])}
        self.assertTrue(cited <= set(self.model["adrs"]) | dt.RESERVED_ADRS, sorted(cited))

    def test_ports_manifest_is_read(self):
        ports = self.model["ports"]
        self.assertGreater(ports["total"], 0)
        self.assertEqual(ports["total"], sum(ports["by_status"].values()))

    def test_no_cycles(self):
        self.assertEqual(dt.find_cycles(self.model["nodes"]), [])

    def test_sash_data_bug_is_mapped_to_document_symbols(self):
        reason = "needs sash grippers (the drawn handle in the middle of a sash)"
        owner = [nid for nid, n in self.model["nodes"].items() if reason in n["reasons"]]
        self.assertEqual(owner, ["document-symbols"])

    def test_generated_doc_is_current(self):
        att, counts, order = self.att, self.counts, dt.ranked(self.model, self.counts)
        text = dt.render(self.model, att, counts, order)
        with open(DOC, encoding="utf-8") as fh:
            self.assertEqual(fh.read(), text, "run python3 -I scripts/dependency-tree")


class CliTest(unittest.TestCase):
    def test_write_then_check(self):
        out_dir = tempfile.mkdtemp(prefix="depcheck-")
        out = os.path.join(out_dir, "tree.md")
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(dt.main(["--out", out]), 0)
            self.assertEqual(dt.main(["--check", "--out", out]), 0)
        with open(out, "a", encoding="utf-8") as fh:
            fh.write("stale\n")
        with contextlib.redirect_stderr(io.StringIO()):
            self.assertEqual(dt.main(["--check", "--out", out]), 1)


if __name__ == "__main__":
    unittest.main()
