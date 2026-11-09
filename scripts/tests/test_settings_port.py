"""Tests for scripts/settings-port. Run: python3 -I -m unittest discover -s scripts/tests -p 'test_settings_port.py' -v

Pure functions use inline fixtures. The consumer-index test builds a small throwaway git repo as a
fake upstream in a fresh tempfile.mkdtemp() directory (never deleted, never reused).
"""

import importlib.machinery
import importlib.util
import os
import subprocess
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(os.path.dirname(HERE), "settings-port")
_loader = importlib.machinery.SourceFileLoader("settings_port", SCRIPT)
_spec = importlib.util.spec_from_loader("settings_port", _loader)
sp = importlib.util.module_from_spec(_spec)
sys.modules["settings_port"] = sp   # multiprocessing pickles worker functions by module name
_loader.exec_module(sp)

GIT_ENV = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@e", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@e", GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")


def core(key, **extra):
    rec = {"key": key, "source": "core", "tags": []}
    rec.update(extra)
    return rec


def cfg(rules=(), groups=(), priority=None, policy=None, components=()):
    return {
        "upstream": {}, "policy": policy or {}, "priority": priority or {},
        "groups": list(groups), "rules": list(rules), "components": list(components),
    }


def rule(status, match=(), source=(), reason="r"):
    return {"match": list(match), "source": list(source), "status": status, "reason": reason,
            "comment": ""}


class TomlTests(unittest.TestCase):
    def test_tables_arrays_and_comments(self):
        text = '\n'.join([
            "# top comment",
            "[upstream]",
            'clone = "../x"   # trailing comment',
            "[[rule]]",
            'status = "n/a"',
            'match = ["*.windows", "a.b"]',
            "[[rule]]",
            'status = "todo"',
            "flag = true",
            "count = 3",
        ])
        out = sp.parse_toml(text)
        self.assertEqual(out["upstream"], {"clone": "../x"})
        self.assertEqual(len(out["rule"]), 2)
        self.assertEqual(out["rule"][0]["match"], ["*.windows", "a.b"])
        self.assertTrue(out["rule"][1]["flag"])
        self.assertEqual(out["rule"][1]["count"], 3)

    def test_multiline_array_with_comments(self):
        text = 'p1 = [\n  "a.b",  # first\n  "c.*",\n]\n'
        self.assertEqual(sp.parse_toml(text)["p1"], ["a.b", "c.*"])

    def test_unsupported_value_is_an_error(self):
        with self.assertRaises(sp.SettingsPortError):
            sp.parse_toml("x = {a = 1}\n")

    def test_bad_rule_status_is_rejected(self):
        path = os.path.join(tempfile.mkdtemp(prefix="settings-port-test-"), "settings.toml")
        with open(path, "w") as fh:
            fh.write('[[rule]]\nstatus = "wired"\nmatch = ["a.b"]\n')
        with self.assertRaises(sp.SettingsPortError):
            sp.load_config(path)


class GlobTests(unittest.TestCase):
    def test_wildcard_case_insensitive_whole_key(self):
        self.assertTrue(sp.glob_match("editor.cursor*", "editor.cursorBlinking"))
        self.assertTrue(sp.glob_match("EDITOR.*", "editor.fontSize"))
        self.assertFalse(sp.glob_match("editor.cursor", "editor.cursorBlinking"))
        self.assertFalse(sp.glob_match("editor.*", "workbench.editor.x"))
        self.assertTrue(sp.glob_match("*.windows", "terminal.integrated.profiles.windows"))


class TocTests(unittest.TestCase):
    LAYOUT = {
        "toc": [
            {"id": "editor", "label": "Text Editor", "settings": ["editor.*"], "children": [
                {"id": "editor/minimap", "label": "Minimap", "settings": ["editor.minimap.*"]},
            ]},
            {"id": "features", "label": "Features", "children": [
                {"id": "features/issueReporter", "label": "Issue Reporter",
                 "settings": ["issueReporter.*"], "hideOn": "desktop"},
            ]},
            {"id": "application", "label": "Application", "children": [
                {"id": "application/other", "label": "Other", "settings": ["application.*"],
                 "hideOn": "windows"},
            ]},
        ]
    }

    def test_children_claim_before_parent(self):
        settings = [core("editor.minimap.enabled"), core("editor.fontSize")]
        placement, paths = sp.resolve_toc(self.LAYOUT, settings)
        self.assertEqual(placement["editor.minimap.enabled"], "editor/minimap")
        self.assertEqual(placement["editor.fontSize"], "editor")
        self.assertEqual(paths["editor/minimap"], "Text Editor > Minimap")

    def test_desktop_hidden_node_places_nothing(self):
        placement, _ = sp.resolve_toc(self.LAYOUT, [core("issueReporter.enabled")])
        self.assertNotIn("issueReporter.enabled", placement)

    def test_windows_hidden_node_is_visible_on_mac(self):
        placement, _ = sp.resolve_toc(self.LAYOUT, [core("application.x")])
        self.assertEqual(placement["application.x"], "application/other")

    def test_advanced_settings_are_not_placed(self):
        placement, _ = sp.resolve_toc(self.LAYOUT, [core("editor.fontSize", tags=["advanced"])])
        self.assertNotIn("editor.fontSize", placement)

    def test_multi_group_setting_keeps_a_claim_for_next_node(self):
        layout = {"toc": [
            {"id": "a", "label": "A", "settings": ["x.*"]},
            {"id": "b", "label": "B", "settings": ["x.*"]},
        ]}
        placement, _ = sp.resolve_toc(layout, [core("x.one")])
        self.assertEqual(placement["x.one"], "a")


class GroupTests(unittest.TestCase):
    def test_two_segment_keys_split_by_first_word_when_group_is_big_enough(self):
        keys = ["editor.fontSize", "editor.fontFamily", "editor.fontWeight", "editor.wordWrap"]
        groups = sp.assign_groups(keys, [])
        self.assertEqual(groups["editor.fontSize"], "editor.font")
        self.assertEqual(groups["editor.fontWeight"], "editor.font")
        # A lone two-segment key falls back to its parent, as does a group under MIN_GROUP.
        self.assertEqual(groups["editor.wordWrap"], "editor")
        pair = sp.assign_groups(["editor.fontSize", "editor.fontFamily"], [])
        self.assertEqual(pair["editor.fontSize"], "editor")

    def test_small_groups_merge_into_parent(self):
        keys = ["debug.console.a", "debug.console.b", "debug.other"]
        groups = sp.assign_groups(keys, [])
        self.assertEqual(groups["debug.console.a"], "debug")   # two keys < MIN_GROUP
        keys += ["debug.console.c"]
        groups = sp.assign_groups(keys, [])
        self.assertEqual(groups["debug.console.a"], "debug.console")

    def test_override_wins_and_is_not_merged(self):
        overrides = [{"match": ["editor.font*"], "id": "editor.typography"}]
        keys = ["editor.fontSize", "editor.fontFamily", "editor.fontWeight"]
        groups = sp.assign_groups(keys, overrides)
        self.assertEqual(set(groups.values()), {"editor.typography"})

    def test_big_group_is_split_by_next_segment(self):
        keys = ["term.integrated.font.x%d" % i for i in range(sp.MAX_GROUP)]
        keys += ["term.integrated.cursor.a", "term.integrated.cursor.b", "term.integrated.cursor.c"]
        groups = sp.assign_groups(keys, [])
        self.assertEqual(groups["term.integrated.font.x0"], "term.integrated.font")
        self.assertEqual(groups["term.integrated.cursor.a"], "term.integrated.cursor")


class StatusTests(unittest.TestCase):
    def test_precedence_wired_over_rule_over_exthost(self):
        c = cfg(rules=[rule("n/a", match=["git.x"], reason="policy")])
        rec = {"key": "git.x", "source": "vscode.git", "tags": []}
        self.assertEqual(sp.decide_status(rec, c, set()), ("n/a", "policy"))
        self.assertEqual(sp.decide_status(rec, c, {"git.x"})[0], "wired")
        rec2 = {"key": "git.y", "source": "vscode.git", "tags": []}
        self.assertEqual(sp.decide_status(rec2, c, set())[0], "exthost")

    def test_first_matching_rule_wins(self):
        c = cfg(rules=[rule("todo", match=["a.*"], reason="first"), rule("n/a", match=["a.b"])])
        self.assertEqual(sp.decide_status(core("a.b"), c, set()), ("todo", "first"))

    def test_source_rule_matches_extension_and_key_rule_is_optional(self):
        c = cfg(rules=[rule("n/a", source=["vscode.github*"], reason="product")])
        rec = {"key": "github.x", "source": "vscode.github-authentication", "tags": []}
        self.assertEqual(sp.decide_status(rec, c, set())[0], "n/a")
        other = {"key": "git.x", "source": "vscode.git", "tags": []}
        self.assertEqual(sp.decide_status(other, c, set())[0], "exthost")

    def test_deprecated_policy(self):
        rec = core("old.x", deprecated=True)
        self.assertEqual(sp.decide_status(rec, cfg(policy={"deprecated": "n/a"}), set())[0], "n/a")
        self.assertEqual(sp.decide_status(rec, cfg(), set())[0], "todo")

    def test_priority_rules(self):
        commonly = {"editor.fontSize"}
        c = cfg(priority={"p1": ["files.*"], "p3": ["mergeEditor"]})
        self.assertEqual(sp.decide_priority(core("editor.fontSize"), "editor.font", c, commonly), "P1")
        self.assertEqual(sp.decide_priority(core("files.exclude"), "files.auto", c, commonly), "P1")
        self.assertEqual(sp.decide_priority(core("files.exclude"), "files", c, commonly), "P2")
        self.assertEqual(sp.decide_priority(core("x.y", tags=["experimental"]), "x", c, commonly), "P3")
        self.assertEqual(sp.decide_priority(core("mergeEditor.x"), "mergeEditor", c, commonly), "P3")
        self.assertEqual(sp.decide_priority(core("x.y"), "x", c, commonly), "P2")


class WiredTests(unittest.TestCase):
    def test_parses_quoted_keys_and_ignores_missing_file(self):
        d = tempfile.mkdtemp(prefix="settings-port-test-")
        path = os.path.join(d, "wired.rs")
        with open(path, "w") as fh:
            fh.write('pub const WIRED: &[&str] = &["editor.fontSize", "not a key", "files.exclude"];\n'
                     '// "editor.cursorStyle" is only mentioned in a comment and not in keys\n')
        self.assertEqual(sp.load_wired(path, keys={"editor.fontSize", "files.exclude"}),
                         {"editor.fontSize", "files.exclude"})
        self.assertEqual(sp.load_wired(os.path.join(d, "missing.rs")), set())


class TextTests(unittest.TestCase):
    def test_plain_and_first_sentence(self):
        text = "Controls the `#editor.tabSize#` value. See [docs](http://x). Second sentence."
        self.assertEqual(sp.plain(text), "Controls the editor.tabSize value. See docs. Second sentence.")
        self.assertEqual(sp.first_sentence(text), "Controls the editor.tabSize value.")

    def test_diff_exports(self):
        old = {"settings": [core("a.x", default=1), core("gone.x"), core("b.x", type="string")]}
        new = {"settings": [core("a.x", default=2), core("new.x"), core("b.x", type="string")]}
        added, removed, changed = sp.diff_exports(old, new)
        self.assertEqual(added, ["new.x"])
        self.assertEqual(removed, ["gone.x"])
        self.assertEqual([(k, [f for f, _, _ in fl]) for k, fl in changed], [("a.x", ["default"])])


class IndexTests(unittest.TestCase):
    def setUp(self):
        self.root = tempfile.mkdtemp(prefix="settings-port-upstream-")
        files = {
            "src/vs/editor/common/config/editorOptions.ts": (
                "export const enum EditorOption {\n"
                "\tfontSize,\n"
                "\tminimap,\n"
                "}\n"
                "new SimpleEditorOption(EditorOption.fontSize, 'fontSize', 14);\n"
                "new SimpleEditorOption(EditorOption.minimap, 'minimap', {});\n"),
            "src/vs/editor/contrib/find/consumer.ts": (
                "const a = config.getValue('editor.fontSize');\n"),
            "src/vs/editor/contrib/find/usesOption.ts": (
                "const b = options.get(EditorOption.fontSize);\n"),
            "src/vs/editor/contrib/find/test/consumer.test.ts": (
                "assert(config.getValue('editor.fontSize'));\n"),
            "src/vs/workbench/contrib/files/files.contribution.ts": (
                "Registry.add({ 'files.exclude': {\n  type: 'object'\n} });\n"
                "const k = \"files.exclude\";\n"),
            "extensions/git/package.json": '{"contributes": {"git.enabled": {"type": "boolean"}}}\n',
        }
        for rel, text in files.items():
            path = os.path.join(self.root, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w") as fh:
                fh.write(text)
        subprocess.run(["git", "-C", self.root, "init", "-q"], env=GIT_ENV, check=True)
        subprocess.run(["git", "-C", self.root, "add", "-A"], env=GIT_ENV, check=True)

    def test_consumer_index_on_fixture_tree(self):
        keys = {"editor.fontSize", "files.exclude", "git.enabled", "editor.minimap.enabled"}
        index = sp.build_index(self.root, keys)
        self.assertEqual(index["option_map"].get("fontSize"), "fontSize")
        self.assertEqual(index["lits"]["editor.fontSize"],
                         {"src/vs/editor/contrib/find/consumer.ts": 1})
        self.assertIn("src/vs/workbench/contrib/files/files.contribution.ts",
                      index["regs"]["files.exclude"])
        self.assertIn("extensions/git/package.json", index["regs"]["git.enabled"])
        # Tests are excluded from the consumer scan, and the EnumOption use is counted separately.
        self.assertNotIn("src/vs/editor/contrib/find/test/consumer.test.ts",
                         index["lits"]["editor.fontSize"])
        # The registration line itself also uses EditorOption.fontSize; consumers_for drops it.
        self.assertEqual(index["enums"]["fontSize"]["src/vs/editor/contrib/find/usesOption.ts"], 1)

    def test_consumers_for_merges_option_usage_and_drops_registration_files(self):
        keys = {"editor.fontSize"}
        index = sp.build_index(self.root, keys)
        top, hits, files = sp.consumers_for("editor.fontSize", index, "fontSize",
                                            ["src/vs/editor/common/config/editorOptions.ts"])
        self.assertEqual(hits, 2)
        self.assertEqual(files, 2)
        self.assertEqual(sorted(p for p, _ in top),
                         ["src/vs/editor/contrib/find/consumer.ts",
                          "src/vs/editor/contrib/find/usesOption.ts"])

    def test_registration_prefers_configuration_files(self):
        self.assertEqual(sp.registration_for("k", ["src/a/zeta.ts", "src/a/configurationRegistry.ts"]),
                         "src/a/configurationRegistry.ts")
        self.assertIsNone(sp.registration_for("k", []))


class CacheAndLayoutTests(unittest.TestCase):
    def test_real_layout_resolves_every_core_key_or_marks_other(self):
        import json
        with open(sp.EXPORT_PATH, encoding="utf-8") as fh:
            export = json.load(fh)
        with open(sp.LAYOUT_PATH, encoding="utf-8") as fh:
            layout = json.load(fh)
        records = [sp.raw_record(s) for s in export["settings"]]
        settings, groups = sp.analyze({"settings": records, "upstreamHead": "x"},
                                      sp.load_config(os.path.join(sp.REPO_ROOT, "upstream", "settings.toml")),
                                      layout, set())
        self.assertEqual(len(settings), len(export["settings"]))
        self.assertTrue(all(g["total"] >= 1 for g in groups))
        self.assertEqual(sum(g["total"] for g in groups), len(settings))


if __name__ == "__main__":
    unittest.main()
