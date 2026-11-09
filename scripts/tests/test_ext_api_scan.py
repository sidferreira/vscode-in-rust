"""Tests for scripts/ext-api-scan. Run: python3 -I -m unittest discover -s scripts/tests -v

Fixtures are tiny: a fake vscode.d.ts + proposed d.ts, and throwaway extension folders
built in tempfile directories. Nothing here touches ~/.vscode or vscode-original.
"""

import importlib.machinery
import importlib.util
import json
import os
import shutil
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(os.path.dirname(HERE), "ext-api-scan")
_loader = importlib.machinery.SourceFileLoader("ext_api_scan", SCRIPT)
_spec = importlib.util.spec_from_loader("ext_api_scan", _loader)
eas = importlib.util.module_from_spec(_spec)
_loader.exec_module(eas)

FAKE_DTS = """\
declare module 'vscode' {
	/** A range. */
	export class Range {
		constructor(a: number, b: number);
		static fromArray(x: number[]): Range;
	}
	export enum StatusBarAlignment { Left = 1, Right = 2 }
	export namespace window {
		export const activeTextEditor: number;
		export function createWebviewPanel(viewType: string): void;
		export function registerHoverProvider(): void;
	}
	export namespace languages {
		export function registerHoverProvider(): void;
	}
	export namespace env {
		export const appName: string;
	}
	export namespace commands {
		export function executeCommand(command: string): void;
	}
	export function createCommentController(id: string, label: string): void;
}
"""

FAKE_PROPOSED = """\
declare module 'vscode' {
	export namespace window {
		export function proposedThing(): void;
	}
}
"""


def make_dts_dir():
    d = tempfile.mkdtemp()
    with open(os.path.join(d, "vscode.d.ts"), "w") as f:
        f.write(FAKE_DTS)
    with open(os.path.join(d, "vscode.proposed.fooBar.d.ts"), "w") as f:
        f.write(FAKE_PROPOSED)
    return d


def write(path, text, mode="w"):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if isinstance(text, bytes):
        with open(path, "wb") as f:
            f.write(text)
    else:
        with open(path, mode) as f:
            f.write(text)


class SurfaceTests(unittest.TestCase):
    def setUp(self):
        self.dir = make_dts_dir()
        self.surface = eas.Surface.load(self.dir)

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def test_namespaces_and_top_level_types_parsed(self):
        self.assertTrue(self.surface.loaded)
        self.assertEqual(self.surface.member_kind("window", "createWebviewPanel"), "function")
        self.assertEqual(self.surface.member_kind("window", "activeTextEditor"), "const")
        self.assertIsNone(self.surface.member_kind("window", "addEventListener"))
        self.assertTrue(self.surface.is_type("Range"))
        self.assertTrue(self.surface.is_type("StatusBarAlignment"))
        self.assertFalse(self.surface.is_type("fromArray"))  # static member, not top-level

    def test_proposed_members_tagged_and_stable_wins(self):
        self.assertEqual(self.surface.proposals[("window", "proposedThing")], {"fooBar"})
        self.assertNotIn(("window", "createWebviewPanel"), self.surface.proposals)

    def test_missing_dts_is_unloaded(self):
        empty = tempfile.mkdtemp()
        try:
            s = eas.Surface.load(empty)
            self.assertFalse(s.loaded)
            self.assertEqual(s.member_kind("window", "anything"), "unverified")
            self.assertIsNone(s.member_kind("notANamespace", "x"))
        finally:
            shutil.rmtree(empty, ignore_errors=True)


class CountingTests(unittest.TestCase):
    def setUp(self):
        self.dir = make_dts_dir()
        self.surface = eas.Surface.load(self.dir)

    def tearDown(self):
        shutil.rmtree(self.dir, ignore_errors=True)

    def count(self, js):
        return eas.count_text(js, self.surface)

    def test_const_require_alias_counted_exact(self):
        r = self.count('const v = require("vscode");\nv.window.createWebviewPanel("x");\n')
        self.assertEqual(r["exact"]["window.createWebviewPanel"], 1)
        self.assertEqual(r["heuristic"], {})
        self.assertEqual(r["aliases"], ["v"])

    def test_minified_var_single_quotes_counted(self):
        r = self.count("var e=require('vscode');e.languages.registerHoverProvider(()=>1)")
        self.assertEqual(r["exact"]["languages.registerHoverProvider"], 1)

    def test_esbuild_toesm_wrapper_counted(self):
        r = self.count('var x=__toESM(require("vscode"));x.env.appName;')
        self.assertEqual(r["exact"]["env.appName"], 1)
        self.assertEqual(r["aliases"], ["x"])

    def test_esm_import_star_counted(self):
        r = self.count('import * as vsc from "vscode";\nvsc.window.createWebviewPanel("a");')
        self.assertEqual(r["exact"]["window.createWebviewPanel"], 1)

    def test_destructured_namespaces_counted(self):
        r = self.count('const {window: w, languages} = require("vscode");\n'
                       'w.createWebviewPanel(); languages.registerHoverProvider();')
        self.assertEqual(r["exact"]["window.createWebviewPanel"], 1)
        self.assertEqual(r["exact"]["languages.registerHoverProvider"], 1)

    def test_webpack_external_only_heuristic(self):
        # webpack module factory: `t.exports=require("vscode")`, consumers use `o.window.x(`
        js = 't.exports=require("vscode");\nconst o=n(5);\no.window.createWebviewPanel("q");\n'
        r = self.count(js)
        self.assertEqual(r["exact"], {})
        self.assertEqual(r["aliases"], [])
        self.assertEqual(r["heuristic"]["window.createWebviewPanel"], 1)

    def test_heuristic_does_not_double_count_alias(self):
        r = self.count('const v=require("vscode");v.window.createWebviewPanel("x");')
        self.assertEqual(r["exact"]["window.createWebviewPanel"], 1)
        self.assertEqual(r["heuristic"], {})

    def test_dom_style_calls_not_counted(self):
        # window.addEventListener is not in the vscode surface: must not be exact or heuristic
        r = self.count('const v=require("vscode");window.addEventListener("message", f);'
                       'document.window.addEventListener("x", g);')
        self.assertNotIn("window.addEventListener", r["exact"])
        self.assertNotIn("window.addEventListener", r["heuristic"])

    def test_known_member_on_alias_unknown_member_goes_to_unknown(self):
        r = self.count('const v=require("vscode");v.window.notARealThing(1);')
        self.assertEqual(r["exact"], {})
        self.assertEqual(r["unknown"]["window.notARealThing"], 1)

    def test_new_class_counted_as_type(self):
        r = self.count('const v=require("vscode");const r=new v.Range(1,2);v.StatusBarAlignment.Left;')
        self.assertEqual(r["types"]["Range"], 1)
        self.assertEqual(r["types"]["StatusBarAlignment"], 1)

    def test_top_level_module_function_counted(self):
        r = self.count('const v=require("vscode");const c=v.createCommentController("a","b");')
        self.assertEqual(r["exact"]["module.createCommentController"], 1)
        self.assertNotIn("window.createCommentController", r["exact"])

    def test_proposed_member_tagged(self):
        r = self.count('const v=require("vscode");v.window.proposedThing();')
        self.assertEqual(r["exact"]["window.proposedThing"], 1)
        self.assertEqual(r["proposed"]["fooBar: window.proposedThing"], 1)

    def test_unloaded_surface_counts_fallback_namespaces_unverified(self):
        empty = tempfile.mkdtemp()
        try:
            s = eas.Surface.load(empty)
            r = eas.count_text('const v=require("vscode");v.window.anything(1);', s)
            self.assertEqual(r["exact"]["window.anything"], 1)
            self.assertEqual(r["heuristic"], {})
        finally:
            shutil.rmtree(empty, ignore_errors=True)


class ClassifyAndMagicTests(unittest.TestCase):
    def test_bundle_classification(self):
        self.assertEqual(eas.classify_bundle("webview/index.js", "x"), "webview")
        self.assertEqual(eas.classify_bundle("dist/webview-pr.js", "x"), "webview")
        self.assertEqual(eas.classify_bundle("dist/browser/ext.js", "x"), "browser")
        self.assertEqual(eas.classify_bundle("extension.js", "acquireVsCodeApi();"), "webview")
        # host code may embed an acquireVsCodeApi() string but still requires vscode
        self.assertEqual(eas.classify_bundle("extension.js",
                         'require("vscode");const h="acquireVsCodeApi()"'), "node")

    def test_magic_kinds(self):
        d = tempfile.mkdtemp()
        try:
            cases = {
                "elf": b"\x7fELF\x02\x01",
                "macho": b"\xcf\xfa\xed\xfe\x07\x00",
                "pe": b"MZ\x90\x00",
                "text": b"hello",
            }
            for name, data in cases.items():
                p = os.path.join(d, name)
                write(p, data)
            jar = os.path.join(d, "x.jar")
            write(jar, b"PK\x03\x04rest")
            self.assertEqual(eas.magic_kind(os.path.join(d, "elf")), "ELF")
            self.assertEqual(eas.magic_kind(os.path.join(d, "macho")), "Mach-O")
            self.assertEqual(eas.magic_kind(os.path.join(d, "pe")), "PE")
            self.assertIsNone(eas.magic_kind(os.path.join(d, "text")))
            self.assertEqual(eas.magic_kind(jar), "jar")
        finally:
            shutil.rmtree(d, ignore_errors=True)


class EndToEndTests(unittest.TestCase):
    def setUp(self):
        self.dts = make_dts_dir()
        self.up = tempfile.mkdtemp()
        os.makedirs(os.path.join(self.up, "extensions", "git"))
        with open(os.path.join(self.up, "extensions", "git", "package.json"), "w") as f:
            json.dump({"name": "git", "publisher": "vscode"}, f)
        self.surface = eas.Surface.load(self.dts)
        self.ext = tempfile.mkdtemp()
        folder = os.path.join(self.ext, "acme.demo-1.2.3-darwin-arm64")
        self.folder = folder
        manifest = {
            "name": "demo", "publisher": "acme", "version": "1.2.3",
            "main": "./out/main.js", "engines": {"vscode": "^1.90.0"},
            "activationEvents": ["onCommand:demo.run", "onStartupFinished"],
            "extensionDependencies": ["vscode.git"],
            "enabledApiProposals": ["fooBar"],
            "dependencies": {"vscode-languageclient": "9.0.0"},
            "contributes": {
                "commands": [{"command": "demo.run"}, {"command": "demo.stop"}],
                "configuration": {"title": "Demo", "properties": {"demo.a": {}, "demo.b": {}, "demo.c": {}}},
                "menus": {"commandPalette": [{}, {}], "view/title": [{}]},
                "views": {"explorer": [{"id": "demo.tree"}]},
            },
        }
        write(os.path.join(folder, "package.json"), json.dumps(manifest))
        write(os.path.join(folder, "out", "main.js"),
              'const v=require("vscode");const cp=require("child_process");\n'
              'v.window.createWebviewPanel("x");\n'
              'cp.spawn("node",["x"]);\n'
              'const lc=require("vscode-languageclient/node");const t=lc.TransportKind.stdio;\n'
              'v.window.proposedThing();\n'
              '// built-in ref\nv.commands.executeCommand("vscode.git");\n'
              'const g="vscode.git";\n')
        write(os.path.join(folder, "node_modules", "left-pad", "index.js"),
              'require("vscode").window.createWebviewPanel();')  # must be ignored (pruned)
        write(os.path.join(folder, "webview", "index.js"), "acquireVsCodeApi();window.foo();")
        write(os.path.join(folder, "bin", "helper"), b"\xcf\xfa\xed\xfe\x07\x00\x00\x01")
        os.chmod(os.path.join(folder, "bin", "helper"), 0o755)

    def tearDown(self):
        for d in (self.dts, self.up, self.ext):
            shutil.rmtree(d, ignore_errors=True)

    def test_profile_end_to_end(self):
        p = eas.build_profile(self.folder, self.surface, eas.load_builtin_ids(self.up))
        self.assertEqual(p["extension"]["id"], "acme.demo")
        self.assertEqual(p["extension"]["platform"], "darwin-arm64")
        self.assertEqual(p["manifest"]["engines_vscode"], "^1.90.0")
        self.assertEqual(p["manifest"]["contributes"]["commands"], 2)
        self.assertEqual(p["manifest"]["contributes"]["configuration"], 3)
        self.assertEqual(p["manifest"]["contributes"]["menus"], 3)
        self.assertEqual(p["manifest"]["contributes"]["views"], 1)
        self.assertEqual(p["api"]["exact"]["window.createWebviewPanel"], 1)
        self.assertEqual(p["api"]["exact"]["window.proposedThing"], 1)
        self.assertEqual(p["api"]["proposed"]["fooBar: window.proposedThing"], 1)
        self.assertEqual(p["api"]["exact"]["commands.executeCommand"], 1)
        self.assertEqual(p["api"]["webview_api_calls"]["window.createWebviewPanel"], 1)
        self.assertEqual(len(p["bundles"]["webview"]), 1)
        self.assertEqual(p["bundles"]["acquire_vscode_api_calls"], 1)
        self.assertEqual(len(p["bundles"]["host"]), 1)  # node_modules pruned, webview excluded
        self.assertTrue(p["languageclient"]["uses_languageclient"])
        self.assertEqual(p["processes"]["child_process_require_files"], 1)
        self.assertEqual(p["processes"]["spawn_call_sites_heuristic"], 1)
        kinds = {b["path"]: b["kind"] for b in p["processes"]["bundled_binaries"]}
        self.assertEqual(kinds.get(os.path.join("bin", "helper")), "Mach-O")
        self.assertEqual(p["builtin_extension_refs"], {"vscode.git": 2})
        self.assertEqual(p["manifest"]["extensionDependencies"], ["vscode.git"])
        json.dumps(p)  # profile must be JSON-serialisable

    def test_cli_writes_json_and_markdown(self):
        out = tempfile.mkdtemp()
        try:
            j = os.path.join(out, "p.json")
            m = os.path.join(out, "p.md")
            rc = eas.main(["--dts-dir", self.dts, "--upstream", self.up,
                           "--json", j, "--md", m, self.folder])
            self.assertEqual(rc, 0)
            with open(j) as f:
                data = json.load(f)
            self.assertEqual(data[0]["extension"]["id"], "acme.demo")
            with open(m) as f:
                self.assertIn("## acme.demo 1.2.3", f.read())
        finally:
            shutil.rmtree(out, ignore_errors=True)

    def test_missing_manifest_is_error(self):
        empty = tempfile.mkdtemp()
        try:
            rc = eas.main(["--dts-dir", self.dts, "--upstream", self.up, empty])
            self.assertEqual(rc, 2)
        finally:
            shutil.rmtree(empty, ignore_errors=True)


if __name__ == "__main__":
    unittest.main()
