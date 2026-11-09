"""Tests for scripts/test-port. Run: python3 -I -m unittest discover -s scripts/tests -p 'test_test_port.py' -v

Parser and matcher tests use inline fixtures. The CLI test builds a small throwaway git repo
as a fake upstream in a fresh tempfile.mkdtemp() directory (never deleted, never reused).
"""

import contextlib
import importlib.machinery
import importlib.util
import io
import os
import subprocess
import tempfile
import textwrap
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(os.path.dirname(HERE), "test-port")
_loader = importlib.machinery.SourceFileLoader("test_port", SCRIPT)
_spec = importlib.util.spec_from_loader("test_port", _loader)
tp = importlib.util.module_from_spec(_spec)
_loader.exec_module(tp)

GIT_ENV = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@e", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@e", GIT_AUTHOR_DATE="2026-01-01T00:00:00Z",
               GIT_COMMITTER_DATE="2026-01-01T00:00:00Z", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_CONFIG_NOSYSTEM="1")


def titles(tests):
    return [(tuple(t["suite"]), t["title"], t["dynamic"], t["skip"], t["looped"]) for t in tests]


class LexerTest(unittest.TestCase):
    def test_comments_and_strings_are_not_code(self):
        src = "// test('nope', () => {})\n/* test('nope2') */ const s = 'test(\"x\")';"
        suites, tests = tp.parse_ts(src)
        self.assertEqual(tests, [])
        self.assertEqual(suites, [])

    def test_regex_literal_with_quote_does_not_break_lexing(self):
        src = "const r = /['\"]/g;\nsuite('S', () => { test('after regex', () => {}); });"
        _, tests = tp.parse_ts(src)
        self.assertEqual(titles(tests), [(("S",), "after regex", False, False, False)])

    def test_division_is_not_a_regex(self):
        src = "const x = a / b / c;\ntest('divided', () => {});"
        _, tests = tp.parse_ts(src)
        self.assertEqual([t["title"] for t in tests], ["divided"])

    def test_template_with_expression_keeps_raw_text(self):
        src = "test(`hello ${name + '}'} world`, () => {});"
        _, tests = tp.parse_ts(src)
        self.assertEqual(tests[0]["title"], "hello ${name + '}'} world")
        self.assertFalse(tests[0]["dynamic"])

    def test_template_literal_without_expression(self):
        _, tests = tp.parse_ts("suite(`Suite A`, () => { test(`case \\`one\\``, () => {}); });")
        self.assertEqual(tests[0]["suite"], ["Suite A"])
        self.assertEqual(tests[0]["title"], "case \\`one\\`")

    def test_string_concatenation_is_joined(self):
        _, tests = tp.parse_ts("test('first ' + \"second\", () => {});")
        self.assertEqual(tests[0]["title"], "first second")
        self.assertFalse(tests[0]["dynamic"])

    def test_identifier_title_is_dynamic(self):
        _, tests = tp.parse_ts("test(titleVar, () => {});\ntest(make() + 'x', () => {});")
        self.assertEqual([t["dynamic"] for t in tests], [True, True])
        self.assertTrue(all(t["title"] is None for t in tests))


class ParserTest(unittest.TestCase):
    def test_nested_suites(self):
        src = textwrap.dedent("""
            suite('Outer', () => {
                suite('Inner', () => {
                    test('deep', () => {});
                });
                test('shallow', () => {});
            });
            test('top level', () => {});
        """)
        suites, tests = tp.parse_ts(src)
        self.assertEqual([s["path"] for s in suites], [["Outer"], ["Outer", "Inner"]])
        self.assertEqual(titles(tests), [
            (("Outer", "Inner"), "deep", False, False, False),
            (("Outer",), "shallow", False, False, False),
            ((), "top level", False, False, False),
        ])
        self.assertEqual(tp.tests_key(["Outer", "Inner"], "deep"), "Outer > Inner > deep")

    def test_describe_and_it_are_accepted(self):
        src = "describe('D', function () {\n  it('works', async function () {});\n});"
        _, tests = tp.parse_ts(src)
        self.assertEqual(titles(tests), [(("D",), "works", False, False, False)])

    def test_skip_and_only_modifiers(self):
        src = textwrap.dedent("""
            suite.skip('Skipped suite', () => {
                test('inside skipped', () => {});
            });
            suite('Live', () => {
                test.skip('one skip', () => {});
                test.only('only one', () => {});
                test('plain', () => {});
            });
        """)
        suites, tests = tp.parse_ts(src)
        self.assertEqual([s["skip"] for s in suites], [True, False])
        self.assertEqual([(t["title"], t["skip"]) for t in tests], [
            ("inside skipped", True), ("one skip", True), ("only one", False), ("plain", False)])

    def test_ensure_no_disposables_lines_are_ignored(self):
        src = textwrap.dedent("""
            suite('S', () => {
                ensureNoDisposablesAreLeakedInTestSuite();
                const store = ensureNoDisposablesAreLeakedInTestSuite();
                test('only real test', () => {});
            });
        """)
        suites, tests = tp.parse_ts(src)
        self.assertEqual(len(suites), 1)
        self.assertEqual([t["title"] for t in tests], ["only real test"])

    def test_loop_generated_tests_are_marked(self):
        src = textwrap.dedent("""
            suite('Loops', () => {
                for (const x of [1, 2]) {
                    test('generated ' + x, () => {});
                }
                ['a', 'b'].forEach(name => {
                    test(name, () => {});
                });
                test('plain', () => {});
            });
        """)
        _, tests = tp.parse_ts(src)
        # 'generated ' + x has an identifier operand: dynamic, not a literal title.
        self.assertEqual([(t["title"], t["looped"], t["dynamic"]) for t in tests], [
            (None, True, True), (None, True, True), ("plain", False, False)])

    def test_function_declaration_named_test_is_not_a_call(self):
        _, tests = tp.parse_ts("function test(a) { return a; }\nfunction suite(b) {}\n")
        self.assertEqual(tests, [])

    def test_member_call_named_test_is_not_counted(self):
        _, tests = tp.parse_ts("runner.test('x', () => {});\ntest('y', () => {});")
        self.assertEqual([t["title"] for t in tests], ["y"])

    def test_line_numbers(self):
        _, tests = tp.parse_ts("\n\ntest('third', () => {});")
        self.assertEqual(tests[0]["line"], 3)

    def test_rust_tests_in_mods(self):
        src = textwrap.dedent("""
            fn helper() {}

            #[test]
            fn top() {}

            #[cfg(test)]
            mod tests {
                #[test]
                fn inner() {
                    let s = "}";
                }

                #[tokio::test]
                #[ignore]
                async fn slow() {}
            }
        """)
        suites, tests = tp.parse_rs(src)
        self.assertEqual(suites[0]["path"], ["tests"])
        self.assertEqual([(t["suite"], t["title"], t["skip"]) for t in tests], [
            ([], "top", False), (["tests"], "inner", False), (["tests"], "slow", True)])


class ClassifyTest(unittest.TestCase):
    def test_layer_and_area(self):
        self.assertEqual(tp.classify("src/vs/base/test/common/strings.test.ts")[:2],
                         ("common", "base/common"))
        self.assertEqual(tp.classify("src/vs/editor/test/common/model/x.test.ts")[:2],
                         ("common", "editor/common"))
        self.assertEqual(tp.classify("src/vs/workbench/contrib/chat/test/browser/a.test.ts")[:2],
                         ("browser", "workbench/contrib/chat"))
        self.assertEqual(tp.classify("src/vs/platform/agentHost/test/node/a.test.ts")[:2],
                         ("node", "platform/agentHost"))
        self.assertEqual(tp.classify("extensions/git/src/test/x.test.ts")[:2],
                         ("node", "extensions/git"))

    def test_module_path_drops_test_dir(self):
        self.assertEqual(tp.classify("src/vs/editor/test/common/model/x.test.ts")[2],
                         "src/vs/editor/common/model/x.test.ts")

    def test_scope(self):
        self.assertTrue(tp.in_scope("src/vs/base/test/common/a.test.ts"))
        self.assertTrue(tp.in_scope("extensions/git/src/test/a.test.ts"))
        self.assertFalse(tp.in_scope("extensions/copilot/src/x/test/a.test.ts"))
        self.assertFalse(tp.in_scope("src/vs/base/common/a.ts"))
        self.assertTrue(tp.in_scope("cli/tests/agent_tunnel.rs"))

    def test_crate_tiers(self):
        self.assertEqual(tp.crate_for("src/vs/editor/common/model/x.test.ts"), ("vsr-text", "A"))
        self.assertEqual(tp.crate_for("src/vs/editor/common/cursor/c.test.ts"), ("vsr-editor", "A"))
        self.assertEqual(tp.crate_for("src/vs/base/common/s.test.ts")[1], "B")
        self.assertEqual(tp.crate_for("src/vs/platform/url/u.test.ts"), (None, None))


class MatcherTest(unittest.TestCase):
    def setUp(self):
        self.inv = {"commit": "c" * 40, "ref": "HEAD", "tree": {}, "files": [{
            "path": "src/vs/editor/test/common/model/a.test.ts", "layer": "common",
            "area": "editor/common", "module": "src/vs/editor/common/model/a.test.ts",
            "suites": [],
            "tests": [
                {"suite": ["S"], "title": "one", "dynamic": False, "skip": False,
                 "looped": False, "line": 1, "key": "S > one"},
                {"suite": ["S"], "title": "one", "dynamic": False, "skip": False,
                 "looped": False, "line": 2, "key": "S > one"},
                {"suite": ["S"], "title": "two", "dynamic": False, "skip": True,
                 "looped": False, "line": 3, "key": "S > two"},
                {"suite": [], "title": None, "dynamic": True, "skip": False,
                 "looped": True, "line": 4, "key": None},
            ]}]}
        self.hdr = "src/vs/editor/test/common/model/a.test.ts"

    def test_refs_match_by_header_file_and_key(self):
        headers = {"crates/x/tests/a.rs": [(self.hdr, "abc1234", 1)]}
        refs = [("crates/x/tests/a.rs", 5, "S > one"), ("crates/x/tests/a.rs", 6, "S > two"),
                ("crates/x/tests/a.rs", 7, "S > gone")]
        index, matched, orphans, mismatched = tp.match_refs(self.inv, headers, refs)
        self.assertEqual(matched[(self.hdr, "S > one")], 1)
        self.assertEqual(matched[(self.hdr, "S > two")], 1)
        self.assertEqual([o[2] for o in orphans], ["S > gone"])
        self.assertEqual(mismatched, [])
        pf = tp.ported_per_file(self.inv, index, matched)
        # "S > one" exists twice upstream but is referenced once: one ported, two total.
        self.assertEqual(pf[self.hdr], (2, 3))

    def test_duplicate_refs_cap_at_upstream_count(self):
        headers = {"crates/x/tests/a.rs": [(self.hdr, "abc1234", 1)]}
        refs = [("crates/x/tests/a.rs", n, "S > one") for n in (5, 6, 7)]
        index, matched, _, _ = tp.match_refs(self.inv, headers, refs)
        self.assertEqual(tp.ported_per_file(self.inv, index, matched)[self.hdr], (2, 3))

    def test_ref_in_wrong_header_is_a_mismatch_not_an_orphan(self):
        headers = {"crates/x/tests/a.rs": [("src/vs/other.test.ts", "abc1234", 1)]}
        refs = [("crates/x/tests/a.rs", 5, "S > one")]
        _, matched, orphans, mismatched = tp.match_refs(self.inv, headers, refs)
        self.assertEqual(orphans, [])
        self.assertEqual(len(mismatched), 1)
        self.assertEqual(len(matched), 0)

    def test_unknown_key_is_an_orphan(self):
        headers = {"crates/x/tests/a.rs": [(self.hdr, "abc1234", 1)]}
        _, _, orphans, _ = tp.match_refs(self.inv, headers, [("crates/x/tests/a.rs", 9, "Renamed")])
        self.assertEqual([o[2] for o in orphans], ["Renamed"])

    def test_scan_rust_finds_headers_and_refs(self):
        root = tempfile.mkdtemp(prefix="test-port-scan-")
        d = os.path.join(root, "crates", "c", "tests")
        os.makedirs(d)
        with open(os.path.join(d, "t.rs"), "w", encoding="utf-8") as fh:
            fh.write("//! upstream: src/a.test.ts @ 50e1350fc78d\n\n"
                     "// upstream: Suite > case\n#[test]\nfn x() {}\n")
        headers, refs = tp.scan_rust(root)
        self.assertEqual(headers, {"crates/c/tests/t.rs": [("src/a.test.ts", "50e1350fc78d", 1)]})
        self.assertEqual(refs, [("crates/c/tests/t.rs", 3, "Suite > case")])


class CliTest(unittest.TestCase):
    def _fake_upstream(self):
        up = tempfile.mkdtemp(prefix="test-port-up-")
        subprocess.run(["git", "init", "-q", up], check=True, env=GIT_ENV)
        files = {
            "src/vs/editor/test/common/model/m.test.ts": textwrap.dedent("""
                suite('ModelBuilder', () => {
                    test('t1', () => {});
                    test.skip('t2', () => {});
                });
            """),
            "src/vs/platform/x/test/browser/b.test.ts": "test('dom', () => {});\n",
            "extensions/e/src/test/n.test.ts": "suite('E', () => { test('x' + 'y', () => {}); });\n",
        }
        for rel, text in files.items():
            path = os.path.join(up, rel)
            os.makedirs(os.path.dirname(path), exist_ok=True)
            with open(path, "w", encoding="utf-8") as fh:
                fh.write(text)
        subprocess.run(["git", "-C", up, "add", "-A"], check=True, env=GIT_ENV)
        subprocess.run(["git", "-C", up, "commit", "-q", "-m", "init"], check=True, env=GIT_ENV)
        return up

    def test_inventory_and_status_end_to_end(self):
        up = self._fake_upstream()
        inv = tp.load_inventory(up, "HEAD")
        self.assertEqual(len(inv["files"]), 3)
        all_tests = [t for f in inv["files"] for t in f["tests"]]
        self.assertEqual(len(all_tests), 4)
        self.assertEqual(sum(t["skip"] for t in all_tests), 1)

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            rc = tp.main(["--clone", up, "inventory", "--files"])
        self.assertEqual(rc, 0)
        self.assertIn("src/vs/editor/test/common/model/m.test.ts", out.getvalue())
        self.assertIn("browser", out.getvalue())

        out = io.StringIO()
        with contextlib.redirect_stdout(out):
            rc = tp.main(["--clone", up, "status"])
        self.assertEqual(rc, 0)
        self.assertIn("ported: 0 / 4 tests", out.getvalue())

    def test_missing_clone_exits_2(self):
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            rc = tp.main(["--clone", os.path.join(tempfile.gettempdir(), "no-such-clone-xyz"),
                          "inventory"])
        self.assertEqual(rc, 2)
        self.assertIn("clone not found", err.getvalue())


if __name__ == "__main__":
    unittest.main()
