"""Tests for scripts/gen-color-defaults. Run: python3 -I -m unittest discover -s scripts/tests -p 'test_gen_color_defaults.py' -v

Pure parser tests on inline TypeScript snippets; no upstream clone needed.
"""

import importlib.machinery
import importlib.util
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
SCRIPT = os.path.join(os.path.dirname(HERE), "gen-color-defaults")
_loader = importlib.machinery.SourceFileLoader("gen_color_defaults", SCRIPT)
_spec = importlib.util.spec_from_loader("gen_color_defaults", _loader)
g = importlib.util.module_from_spec(_spec)
_loader.exec_module(g)


def table(src):
    """The defaults table of the registerColor calls in `src`."""
    toks = g.tokenize(src)
    consts = dict(g.find_constants(toks))
    parsed = []
    for name, at in g.find_registrations(toks):
        p = g.Parser(toks, at)
        key = p.expr()
        p.expect(",")
        parsed.append((name, key[1], p.expr()))
    names = {n: k for n, k, _ in parsed if n}
    g.NUM_CONSTS.clear()
    g.NUM_CONSTS.update({k: v for k, v in consts.items() if v[0] == "num"})
    conv = g.Converter(names, consts)
    return {k: conv.defaults(d) for _, k, d in parsed}


class ParserTests(unittest.TestCase):
    def test_per_type_literals_and_null(self):
        t = table("""export const foreground = registerColor('foreground',
            { dark: '#CCCCCC', light: '#616161', hcDark: Color.white, hcLight: null },
            nls.localize('foreground', "x"));""")
        self.assertEqual(t["foreground"], {"dark": "#CCCCCC", "light": "#616161", "hcDark": "#FFFFFF", "hcLight": None})

    def test_references_and_transforms(self):
        t = table("""
            const fg = registerColor('foreground', { dark: '#CCC', light: '#111', hcDark: '#FFF', hcLight: '#000' }, 'd');
            const desc = registerColor('descriptionForeground',
                { light: '#717171', dark: transparent(fg, 0.7), hcDark: transparent(fg, .7), hcLight: fg }, 'd');""")
        d = t["descriptionForeground"]
        self.assertEqual(d["dark"], {"op": "transparent", "v": {"ref": "foreground"}, "f": 0.7})
        self.assertEqual(d["hcDark"]["f"], 0.7)
        self.assertEqual(d["hcLight"], {"ref": "foreground"})

    def test_uniform_default_and_null_default(self):
        t = table("""const a = registerColor('a', '#FF0000', 'd'); const b = registerColor('b', null, 'd');""")
        self.assertEqual(set(t["a"].values()), {"#FF0000"})
        self.assertEqual(set(t["b"].values()), {None})

    def test_color_chains_and_constants(self):
        t = table("""
            const base = Color.fromHex('#40C8AE').transparent(headerTransparency);
            const headerTransparency = 0.5;
            const x = registerColor('x', { dark: base, light: new Color(new RGBA(1, 2, 3, .5)), hcDark: Color.fromHex('#0F4A85').transparent(0.1), hcLight: null }, 'd');""")
        self.assertEqual(t["x"]["dark"], {"op": "transparent", "v": "#40C8AE", "f": 0.5})
        self.assertEqual(t["x"]["light"], {"rgba": [1, 2, 3, 0.5]})
        self.assertEqual(t["x"]["hcDark"], {"op": "transparent", "v": "#0F4A85", "f": 0.1})

    def test_one_of_if_defined_and_less_prominent(self):
        t = table("""
            const a = registerColor('a', '#000', 'd'); const b = registerColor('b', '#111', 'd');
            const c = registerColor('c', { dark: oneOf(a, b), light: ifDefinedThenElse(a, b, '#222'),
                hcDark: lessProminent(a, b, 0.3, 0.6), hcLight: opaque(a, b) }, 'd');""")
        self.assertEqual(t["c"]["dark"], {"op": "oneOf", "values": [{"ref": "a"}, {"ref": "b"}]})
        self.assertEqual(t["c"]["light"], {"op": "ifDefined", "key": "a", "then": {"ref": "b"}, "else": "#222"})
        self.assertEqual(t["c"]["hcDark"], {"op": "lessProminent", "v": {"ref": "a"}, "bg": {"ref": "b"}, "f": 0.3, "t": 0.6})
        self.assertEqual(t["c"]["hcLight"], {"op": "opaque", "v": {"ref": "a"}, "bg": {"ref": "b"}})

    def test_mix_transform_object(self):
        t = table("""const a = registerColor('a', '#000', 'd');
            const m = registerColor('m', { op: ColorTransformType.Mix, color: a, with: '#FFF', ratio: 0.33 }, 'd');""")
        self.assertEqual(t["m"]["dark"], {"op": "mix", "v": {"ref": "a"}, "with": "#FFF", "ratio": 0.33})

    def test_comments_and_template_keys(self):
        t = table("""// c\n/* d */ const a = registerColor(`a.b`, '#000', /* inline */ 'd');""")
        self.assertIn("a.b", t)

    def test_unknown_constructs_fail_loudly(self):
        with self.assertRaises(g.ParseError):
            table("""const a = registerColor('a', { dark: someFunction(1), light: '#000', hcDark: null, hcLight: null }, 'd');""")
        with self.assertRaises(g.ParseError):
            table("""const a = registerColor('a', { dark: missingConstant, light: '#000', hcDark: null, hcLight: null }, 'd');""")


if __name__ == "__main__":
    unittest.main()
