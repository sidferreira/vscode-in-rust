"""Tests for scripts/install-rode and the installed `rode` command (ADR-0110).

Run: python3 -I -m unittest discover -s scripts/tests -p 'test_rode.py' -v

The install uses `--binary` with a stand-in executable and temp app/bin dirs under a temp HOME,
so nothing is built and the real ~/Applications, ~/.local/bin and ~/.Trash are never touched.
`rode` is only run with --dry-run, --version, --help or bad options: no window is ever opened.
"""

import os
import subprocess
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPTS = os.path.dirname(HERE)
INSTALL = os.path.join(SCRIPTS, "install-rode")


def fake_env(home, **extra):
    env = {k: v for k, v in os.environ.items() if not k.startswith("RSCODE_")}
    env["HOME"] = home
    env.update(extra)
    return env


class Installed(unittest.TestCase):
    def setUp(self):
        self.home = os.path.realpath(tempfile.mkdtemp(prefix="rode-test-"))
        self.apps = os.path.join(self.home, "Applications")
        self.bin = os.path.join(self.home, "bin")
        self.fake = os.path.join(self.home, "rscode")
        with open(self.fake, "w") as f:
            f.write("#!/bin/sh\nexit 0\n")
        os.chmod(self.fake, 0o755)
        self.code_oss = os.path.join(self.home, "codeoss")
        self.install_env = fake_env(self.home, RSCODE_CODE_OSS=self.code_oss, RSCODE_NODE="/opt/node24/bin/node")
        r = subprocess.run(
            ["bash", INSTALL, "--binary", self.fake, "--app-dir", self.apps, "--bin-dir", self.bin],
            capture_output=True, text=True, env=self.install_env,
        )
        self.assertEqual(r.returncode, 0, r.stdout + r.stderr)
        self.app = os.path.join(self.apps, "rscode.app")
        self.rode = os.path.join(self.bin, "rode")
        self.cwd = os.path.join(self.home, "project")
        os.makedirs(self.cwd)

    def run_rode(self, *args, **env):
        return subprocess.run([self.rode, *args], capture_output=True, text=True, cwd=self.cwd,
                              env=fake_env(self.home, **env))

    def dry(self, *args, **env):
        r = self.run_rode("--dry-run", *args, **env)
        self.assertEqual(r.returncode, 0, r.stderr)
        return r.stdout.splitlines()

    def test_goto_is_passed_through_with_its_file_argument(self):
        out = self.dry("-g", "src/main.rs:12:3")
        self.assertEqual([l for l in out if l.startswith("arg: ")], ["arg: -g", "arg: src/main.rs:12:3"])
        self.assertEqual([l for l in self.dry("--goto", "a.txt:4") if l.startswith("arg: ")],
                         ["arg: --goto", "arg: a.txt:4"])

    def test_installs_the_bundle_and_links_rode_into_it(self):
        self.assertTrue(os.path.islink(self.rode))
        self.assertEqual(os.path.realpath(self.rode),
                         os.path.realpath(os.path.join(self.app, "Contents/Resources/bin/rode")))
        self.assertTrue(os.access(os.path.join(self.app, "Contents/MacOS/rscode"), os.X_OK))
        self.assertTrue(os.path.isfile(os.path.join(self.app, "Contents/Info.plist")))

    def test_dry_run_through_the_link_finds_the_installed_binary_and_passes_paths(self):
        out = self.dry(".", "src/main.rs")
        self.assertIn(f"bin: {os.path.realpath(self.app)}/Contents/MacOS/rscode", out)
        self.assertIn(f"cwd: {self.cwd}", out)
        self.assertEqual([l for l in out if l.startswith("arg: ")], ["arg: .", "arg: src/main.rs"])
        self.assertIn("wait: 0", out)
        self.assertIn("wait: 1", self.dry("-w"))

    def test_install_time_defaults_apply_and_user_extensions_are_off(self):
        out = self.dry()
        self.assertIn(f"env: RSCODE_CODE_OSS={self.code_oss}", out)
        self.assertIn("env: RSCODE_NODE=/opt/node24/bin/node", out)
        self.assertIn("env: RSCODE_EXTENSIONS_DIR=", out)

    def test_the_shell_environment_wins_over_the_defaults(self):
        out = self.dry(RSCODE_EXTENSIONS_DIR="/x/exts", RSCODE_NODE="/y/node")
        self.assertIn("env: RSCODE_EXTENSIONS_DIR=/x/exts", out)
        self.assertIn("env: RSCODE_NODE=/y/node", out)

    def test_version_help_and_unknown_options(self):
        r = self.run_rode("--version")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertRegex(r.stdout, r"^rode \d+\.\d+\.\d+ \(")
        r = self.run_rode("--help")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("rode [options] [folder] [file ...]", r.stdout)
        self.assertNotIn("set -euo", r.stdout)
        r = self.run_rode("--bogus")
        self.assertEqual(r.returncode, 2)
        self.assertIn("unknown option --bogus", r.stderr)

    def test_reinstall_replaces_the_link_and_refuses_a_real_file(self):
        r = subprocess.run(["bash", INSTALL, "--binary", self.fake, "--app-dir", self.apps, "--bin-dir", self.bin],
                           capture_output=True, text=True, env=self.install_env)
        self.assertEqual(r.returncode, 0, r.stderr)
        other_bin = os.path.join(self.home, "otherbin")
        os.makedirs(other_bin)
        with open(os.path.join(other_bin, "rode"), "w") as f:
            f.write("mine\n")
        r = subprocess.run(["bash", INSTALL, "--binary", self.fake, "--app-dir", self.apps, "--bin-dir", other_bin],
                           capture_output=True, text=True, env=self.install_env)
        self.assertEqual(r.returncode, 1)
        with open(os.path.join(other_bin, "rode")) as f:
            self.assertEqual(f.read(), "mine\n")

    def test_uninstall_removes_the_link_and_moves_the_app_to_the_trash(self):
        r = subprocess.run(["bash", INSTALL, "--uninstall", "--app-dir", self.apps, "--bin-dir", self.bin],
                           capture_output=True, text=True, env=self.install_env)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertFalse(os.path.lexists(self.rode))
        self.assertFalse(os.path.exists(self.app))
        trashed = os.listdir(os.path.join(self.home, ".Trash"))
        self.assertEqual(len(trashed), 1)
        self.assertTrue(trashed[0].startswith("rscode-") and trashed[0].endswith(".app"))


class NotInstalled(unittest.TestCase):
    def test_rode_outside_a_bundle_says_how_to_install(self):
        home = os.path.realpath(tempfile.mkdtemp(prefix="rode-test-"))
        r = subprocess.run([os.path.join(SCRIPTS, "rode"), "."], capture_output=True, text=True,
                           env=fake_env(home, RODE_APP=os.path.join(home, "nope.app")))
        self.assertEqual(r.returncode, 1)
        self.assertIn("scripts/install-rode", r.stderr)


if __name__ == "__main__":
    unittest.main()
