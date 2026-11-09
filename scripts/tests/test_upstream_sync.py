"""Tests for scripts/upstream-sync. Run: python3 -m unittest discover -s scripts/tests -v

They build a small throwaway git repo as a fake upstream, so they do not depend
on the real (shallow) vscode-original clone.
"""

import contextlib
import importlib.machinery
import importlib.util
import io
import json
import os
import shutil
import subprocess
import tempfile
import textwrap
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(os.path.dirname(HERE), "upstream-sync")
_loader = importlib.machinery.SourceFileLoader("upstream_sync", SCRIPT)
_spec = importlib.util.spec_from_loader("upstream_sync", _loader)
us = importlib.util.module_from_spec(_spec)
_loader.exec_module(us)

GIT_ENV = dict(os.environ, GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@e", GIT_COMMITTER_NAME="t",
               GIT_COMMITTER_EMAIL="t@e", GIT_AUTHOR_DATE="2026-01-01T00:00:00Z",
               GIT_COMMITTER_DATE="2026-01-01T00:00:00Z", GIT_CONFIG_GLOBAL=os.devnull,
               GIT_CONFIG_NOSYSTEM="1")


class UpstreamSyncTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="upstream-sync-test-")
        self.up = os.path.join(self.tmp, "upstream")
        self.repo = os.path.join(self.tmp, "port")
        os.makedirs(self.up)
        os.makedirs(os.path.join(self.repo, "upstream"))
        self.git("init", "-q")
        self.write("src/model/buffer.ts", "export const a = 1;\n")
        self.write("src/core/pos.ts", "export class Position {}\n")
        self.write("src/core/pos.test.ts", "test\n")
        self.write("README.md", "hi\n")
        self.base = self.commit("initial")
        self.write("src/model/buffer.ts", "export const a = 2;\n")
        self.c_model = self.commit("model: bump a")
        self.write("README.md", "hello\n")
        self.commit("docs only")
        self.git("mv", "src/core/pos.ts", "src/core/position.ts")
        self.c_rename = self.commit("core: rename pos")
        self.head = self.git("rev-parse", "HEAD").strip()
        self.manifest = os.path.join(self.repo, "upstream", "ports.toml")
        with open(self.manifest, "w") as f:
            f.write(textwrap.dedent('''\
                # header comment survives
                [upstream]
                repo = "https://example.com/up"
                clone = "../upstream"
                pinned = "%(base)s"

                [[port]]
                id = "model"
                status = "partial"
                upstream = [
                  "src/model/**",  # inline comment
                ]
                rust = ["crates/model"]
                synced = "%(base)s"   # keep me

                [[port]]
                id = "core"
                status = "planned"
                upstream = ["src/core/pos.ts"]
                synced = "%(base)s"

                [[port]]
                id = "core-tests-excluded"
                upstream = ["src/core/**"]
                exclude = ["src/core/*.test.ts"]
                ''' % {"base": self.base}))

    def tearDown(self):
        shutil.rmtree(self.tmp)

    # helpers
    def git(self, *args):
        return subprocess.run(["git", "-C", self.up] + list(args), check=True, env=GIT_ENV,
                              stdout=subprocess.PIPE, universal_newlines=True).stdout

    def write(self, rel, text, root=None):
        path = os.path.join(root or self.up, rel)
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w") as f:
            f.write(text)

    @staticmethod
    def read(path):
        with open(path) as f:
            return f.read()

    def commit(self, msg):
        self.git("add", "-A")
        self.git("commit", "-q", "-m", msg)
        return self.git("rev-parse", "HEAD").strip()

    def run_cli(self, *argv):
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            rc = us.main(["--manifest", self.manifest] + list(argv))
        return rc, out.getvalue(), err.getvalue()

    # tests
    def test_parse_toml_subset(self):
        data, spans = us.parse_toml(self.read(self.manifest))
        self.assertEqual(data["upstream"]["clone"], "../upstream")
        self.assertEqual([p["id"] for p in data["port"]], ["model", "core", "core-tests-excluded"])
        self.assertEqual(data["port"][0]["upstream"], ["src/model/**"])
        self.assertEqual(len(spans), 3)
        with self.assertRaises(us.SyncError):
            us.parse_toml('a = "x"\na = "y"\n')

    def test_glob_match(self):
        self.assertTrue(us._glob_match("src/model/**", "src/model/a/b.ts"))
        self.assertTrue(us._glob_match("src/model", "src/model/a.ts"))
        self.assertTrue(us._glob_match("src/*.ts", "src/a.ts"))
        self.assertFalse(us._glob_match("src/*.ts", "src/x/a.ts"))
        self.assertFalse(us._glob_match("src/model/**", "src/models/a.ts"))

    def test_status_lists_only_relevant_commits(self):
        rc, out, _ = self.run_cli("status", "--json")
        self.assertEqual(rc, 0)
        entries = {e["id"]: e for e in json.loads(out)["entries"]}
        self.assertEqual([c["subject"] for c in entries["model"]["commits"]], ["model: bump a"])
        self.assertEqual(entries["model"]["files"], [{"status": "M", "paths": ["src/model/buffer.ts"]}])
        # rename out of the entry's paths shows up as a deletion warning + missing pattern
        self.assertEqual([c["subject"] for c in entries["core"]["commits"]], ["core: rename pos"])
        self.assertTrue(any("deleted src/core/pos.ts" in w for w in entries["core"]["warnings"]))
        self.assertTrue(any("matches no file" in w for w in entries["core"]["warnings"]))
        # exclude glob drops the test file, rename within the glob is reported as R
        files = entries["core-tests-excluded"]["files"]
        self.assertEqual(len(files), 1)
        self.assertTrue(files[0]["status"].startswith("R"))

    def test_status_text_and_ref(self):
        rc, out, _ = self.run_cli("--ref", self.c_model, "status")
        self.assertEqual(rc, 0)
        self.assertIn("model: bump a", out)
        self.assertNotIn("core: rename pos", out)
        self.assertNotIn("docs only", out)

    def test_diff(self):
        rc, out, _ = self.run_cli("diff", "model")
        self.assertEqual(rc, 0)
        self.assertIn("-export const a = 1;", out)
        self.assertIn("+export const a = 2;", out)
        self.assertNotIn("README", out)

    def test_report(self):
        rc, out, _ = self.run_cli("report", "--with-diff", "50")
        self.assertEqual(rc, 0)
        text = self.read(os.path.join(self.repo, "upstream", "SYNC_REPORT.md"))
        self.assertIn("## `model` (partial)", text)
        self.assertIn("https://example.com/up/commit/" + self.c_model, text)
        self.assertIn("scripts/upstream-sync mark model " + self.head[:12], text)
        self.assertIn("+export const a = 2;", text)
        self.assertNotIn("docs only", text)

    def test_mark_updates_synced_and_preserves_comments(self):
        rc, out, _ = self.run_cli("mark", "model", "--status", "ported")
        self.assertEqual(rc, 0, out)
        text = self.read(self.manifest)
        self.assertIn('synced = "%s"   # keep me' % self.head, text)
        self.assertIn("# header comment survives", text)
        self.assertIn('"src/model/**",  # inline comment', text)
        m = us.Manifest(self.manifest)
        self.assertEqual(m.get("model")["status"], "ported")
        self.assertEqual(m.get("core")["synced"], self.base)  # other entries untouched
        # entry without a synced line gets one inserted
        rc, _, _ = self.run_cli("mark", "core-tests-excluded", self.c_model[:10])
        self.assertEqual(us.Manifest(self.manifest).get("core-tests-excluded")["synced"], self.c_model)
        rc, out, _ = self.run_cli("status", "model")
        self.assertIn("up to date", out)

    def test_unknown_commit_is_clean_error(self):
        rc, _, err = self.run_cli("mark", "model", "deadbeefdeadbeef")
        self.assertEqual(rc, 2)
        self.assertIn("not in the upstream clone", err)
        rc, _, err = self.run_cli("diff", "nope")
        self.assertEqual(rc, 2)
        self.assertIn("no port entry", err)

    def test_check_headers(self):
        self.run_cli("mark", "--all")
        self.write("crates/model/lib.rs",
                   "// upstream: src/model/buffer.ts @ %s\nfn main() {}\n" % self.head[:12], root=self.repo)
        self.write("crates/model/old.rs",
                   "// upstream: src/model/buffer.ts @ %s\n" % self.base[:12], root=self.repo)
        self.write("crates/model/none.rs", "fn x() {}\n", root=self.repo)
        self.write("crates/model/multi.rs",
                   "//! upstream: src/model/buffer.ts, src/model/x.ts @ %s\n" % self.head[:12], root=self.repo)
        self.write("crates/model/stray.rs",
                   "//! upstream: src/other.ts @ %s\n" % self.head[:12], root=self.repo)
        rc, out, _ = self.run_cli("check")
        self.assertEqual(rc, 1)
        self.assertIn("old.rs: header src/model/buffer.ts @ %s, but entry 'model'" % self.base[:12], out)
        self.assertIn("none.rs: no", out)
        self.assertIn("multi.rs: header", out)
        self.assertIn("stray.rs: header path src/other.ts is not covered", out)
        self.assertNotIn("lib.rs", out)


if __name__ == "__main__":
    unittest.main()
