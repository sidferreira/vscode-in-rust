"""Tests for the argument handling of scripts/build-codeoss-exthost.

Run: python3 -I -m unittest discover -s scripts/tests -p 'test_build_codeoss_exthost.py' -v

Only `--plan` is exercised: it prints the steps the script would run and exits before touching
the disk or the network. The real install is exercised by hand (see ADR-0101).
"""

import os
import subprocess
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(os.path.dirname(HERE), "build-codeoss-exthost")


def plan(*args):
    return subprocess.run(["bash", SCRIPT, "--plan", *args], capture_output=True, text=True)


class LanguageFeaturesArgument(unittest.TestCase):
    def test_default_has_no_language_feature_step(self):
        r = plan()
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertNotIn("language-features", r.stdout)

    def test_json_css_html_select_the_three_extensions(self):
        r = plan("--language-features", "json,css,html")
        self.assertEqual(r.returncode, 0, r.stderr)
        for name in ("json-language-features", "css-language-features", "html-language-features"):
            self.assertIn(f"./{name}", r.stdout)
        self.assertIn("--frozen-lockfile", r.stdout)
        self.assertIn("--ignore-scripts", r.stdout)

    def test_only_requested_extensions_are_planned(self):
        r = plan("--language-features=json")
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("./json-language-features", r.stdout)
        self.assertNotIn("css-language-features", r.stdout)
        self.assertNotIn("html-language-features", r.stdout)

    def test_unknown_feature_is_rejected(self):
        r = plan("--language-features", "json,rust")
        self.assertEqual(r.returncode, 2)
        self.assertIn("rust", r.stderr)

    def test_missing_value_is_rejected(self):
        r = plan("--language-features")
        self.assertEqual(r.returncode, 2)

    def test_existing_destination_still_refused(self):
        # --plan must not hide the "DEST exists" refusal for real runs; plan prints it too.
        r = plan("--language-features", "json", "/")
        self.assertEqual(r.returncode, 1)


if __name__ == "__main__":
    unittest.main()
