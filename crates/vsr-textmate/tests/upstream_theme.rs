//! upstream: vscode-textmate src/tests/themes.test.ts (microsoft/vscode-textmate, main, 2026-11-09)
//!
//! Ports of the pure theme tests: matching, specificity, parsing and resolving. The tests that
//! compare the trie structure (`assertThemeEqual`) are ported through `match_scopes`, since our trie
//! is private. The grammar/theme fixture tests (`test-cases/themes`) are not ported: they need the
//! upstream fixture directory; `crates/vsr-syntax/tests` tokenizes real grammars instead.

use std::cmp::Ordering;
use std::sync::Arc;
use vsr_textmate::theme::{ParsedThemeRule, StyleAttributes, font_style, parse_theme, str_arr_cmp};
use vsr_textmate::{RawThemeRule, Theme};

fn rule(scope: Option<&str>, fg: Option<&str>, bg: Option<&str>, fs: Option<&str>) -> RawThemeRule {
    RawThemeRule {
        scope: scope.map(|s| vec![s.to_string()]).unwrap_or_default(),
        scope_is_string: true,
        font_style: fs.map(str::to_string),
        foreground: fg.map(str::to_string),
        background: bg.map(str::to_string),
    }
}

fn rule_list(scopes: &[&str], fg: Option<&str>, bg: Option<&str>) -> RawThemeRule {
    RawThemeRule {
        scope: scopes.iter().map(|s| s.to_string()).collect(),
        scope_is_string: false,
        font_style: None,
        foreground: fg.map(str::to_string),
        background: bg.map(str::to_string),
    }
}

fn path(p: &[&str]) -> Vec<Arc<str>> {
    p.iter().map(|s| Arc::from(*s)).collect()
}

fn fg(theme: &Theme, p: &[&str]) -> Option<String> {
    let m = theme.match_scopes(&path(p))?;
    theme.color(m.foreground).map(str::to_string)
}

fn font_style_to_string(fs: i32) -> String {
    if fs == font_style::NOT_SET {
        return "not set".into();
    }
    let mut s = String::new();
    if fs & font_style::ITALIC != 0 {
        s.push_str("italic ");
    }
    if fs & font_style::BOLD != 0 {
        s.push_str("bold ");
    }
    if fs & font_style::UNDERLINE != 0 {
        s.push_str("underline ");
    }
    if fs & font_style::STRIKETHROUGH != 0 {
        s.push_str("strikethrough ");
    }
    if s.is_empty() {
        s = "none".into();
    }
    s.trim().to_string()
}

#[test]
fn higher_priority_to_deeper_matches() {
    let theme = Theme::from_rules(&[
        rule(None, Some("#100000"), Some("#200000"), None),
        rule(Some("punctuation.definition.string.begin.html"), Some("#300000"), None, None),
        rule(Some("meta.tag punctuation.definition.string"), Some("#400000"), None, None),
    ]);
    assert_eq!(fg(&theme, &["punctuation.definition.string.begin.html"]).as_deref(), Some("#300000"));
}

#[test]
fn higher_priority_to_parent_matches_1() {
    let theme = Theme::from_rules(&[
        rule(None, Some("#100000"), Some("#200000"), None),
        rule(Some("c a"), Some("#300000"), None, None),
        rule(Some("d a.b"), Some("#400000"), None, None),
        rule(Some("a"), Some("#500000"), None, None),
    ]);
    assert_eq!(fg(&theme, &["d", "a.b"]).as_deref(), Some("#400000"));
}

#[test]
fn higher_priority_to_parent_matches_2() {
    let theme = Theme::from_rules(&[
        rule(None, Some("#100000"), Some("#200000"), None),
        rule(Some("meta.tag entity"), Some("#300000"), None, None),
        rule(Some("meta.selector.css entity.name.tag"), Some("#400000"), None, None),
        rule(Some("entity"), Some("#500000"), None, None),
    ]);
    let r = fg(&theme, &["text.html.cshtml", "meta.tag.structure.any.html", "entity.name.tag.structure.any.html"]);
    assert_eq!(r.as_deref(), Some("#300000"));
}

fn can_match_theme() -> Theme {
    Theme::from_rules(&[
        rule(None, Some("#F8F8F2"), Some("#272822"), None),
        rule(Some("source, something"), None, Some("#100000"), None),
        rule_list(&["bar", "baz"], None, Some("#200000")),
        rule(Some("source.css selector bar"), None, None, Some("bold")),
        rule(Some("constant"), Some("#300000"), None, Some("italic")),
        rule(Some("constant.numeric"), Some("#400000"), None, None),
        rule(Some("constant.numeric.hex"), None, None, Some("bold")),
        rule(Some("constant.numeric.oct"), None, None, Some("bold italic underline")),
        rule(Some("constant.numeric.dec"), Some("#500000"), None, Some("")),
        rule(Some("storage.object.bar"), Some("#600000"), None, Some("")),
    ])
}

/// (foreground, background, font style) like the upstream `match` helper builds its object.
fn m(theme: &Theme, p: &[&str]) -> Option<(Option<String>, Option<String>, String)> {
    let r: StyleAttributes = theme.match_scopes(&path(p))?;
    Some((
        theme.color(r.foreground).map(str::to_string),
        theme.color(r.background).map(str::to_string),
        font_style_to_string(r.font_style),
    ))
}

fn expect(fg: Option<&str>, bg: Option<&str>, fs: &str) -> Option<(Option<String>, Option<String>, String)> {
    Some((fg.map(str::to_string), bg.map(str::to_string), fs.to_string()))
}

#[test]
fn theme_matching_can_match() {
    let t = can_match_theme();
    // simple matches
    for p in ["source", "source.ts", "source.tss", "something", "something.ts", "something.tss"] {
        assert_eq!(m(&t, &[p]), expect(None, Some("#100000"), "not set"), "{p}");
    }
    for p in ["baz", "baz.ts", "baz.tss"] {
        assert_eq!(m(&t, &[p]), expect(None, Some("#200000"), "not set"), "{p}");
    }
    for p in ["constant", "constant.string", "constant.hex"] {
        assert_eq!(m(&t, &[p]), expect(Some("#300000"), None, "italic"), "{p}");
    }
    for p in ["constant.numeric", "constant.numeric.baz"] {
        assert_eq!(m(&t, &[p]), expect(Some("#400000"), None, "italic"), "{p}");
    }
    for p in ["constant.numeric.hex", "constant.numeric.hex.baz"] {
        assert_eq!(m(&t, &[p]), expect(Some("#400000"), None, "bold"), "{p}");
    }
    for p in ["constant.numeric.oct", "constant.numeric.oct.baz"] {
        assert_eq!(m(&t, &[p]), expect(Some("#400000"), None, "italic bold underline"), "{p}");
    }
    for p in ["constant.numeric.dec", "constant.numeric.dec.baz"] {
        assert_eq!(m(&t, &[p]), expect(Some("#500000"), None, "none"), "{p}");
    }
    for p in ["storage.object.bar", "storage.object.bar.baz"] {
        assert_eq!(m(&t, &[p]), expect(Some("#600000"), None, "none"), "{p}");
    }
    for p in ["storage.object.bart", "storage.object", "storage"] {
        assert_eq!(m(&t, &[p]), expect(None, None, "not set"), "{p}");
    }
    // default matches
    for p in ["", "bazz", "asdfg"] {
        assert_eq!(m(&t, &[p]), expect(None, None, "not set"), "{p}");
    }
    // multi matches
    assert_eq!(m(&t, &["bar"]), expect(None, Some("#200000"), "not set"));
    assert_eq!(m(&t, &["source.css", "selector", "bar"]), expect(None, Some("#200000"), "bold"));
}

#[test]
fn theme_matching_vscode_23460() {
    let theme = Theme::from_rules(&[
        rule(None, Some("#aec2e0"), Some("#14191f"), None),
        rule(Some("meta.structure.dictionary.json string.quoted.double.json"), Some("#FF410D"), None, None),
        rule(Some("meta.structure.dictionary.json string.quoted.double.json"), Some("#ffffff"), None, None),
        rule(Some("meta.structure.dictionary.value.json string.quoted.double.json"), Some("#FF410D"), None, None),
    ]);
    let r = fg(&theme, &["source.json", "meta.structure.dictionary.json", "meta.structure.dictionary.value.json", "string.quoted.double.json"]);
    assert_eq!(r.as_deref(), Some("#FF410D"));
}

fn pr(scope: &str, parents: Option<&[&str]>, index: usize, fs: i32, fg: Option<&str>, bg: Option<&str>) -> ParsedThemeRule {
    ParsedThemeRule {
        scope: scope.into(),
        parent_scopes: parents.map(|p| p.iter().map(|s| s.to_string()).collect()),
        index,
        font_style: fs,
        foreground: fg.map(str::to_string),
        background: bg.map(str::to_string),
    }
}

#[test]
fn theme_parsing_can_parse() {
    use font_style::*;
    let actual = parse_theme(&[
        rule(None, Some("#F8F8F2"), Some("#272822"), None),
        rule(Some("source, something"), None, Some("#100000"), None),
        rule_list(&["bar", "baz"], None, Some("#010000")),
        rule(Some("source.css selector bar"), None, None, Some("bold")),
        rule(Some("constant"), Some("#ff0000"), None, Some("italic")),
        rule(Some("constant.numeric"), Some("#00ff00"), None, None),
        rule(Some("constant.numeric.hex"), None, None, Some("bold")),
        rule(Some("constant.numeric.oct"), None, None, Some("bold italic underline")),
        rule(Some("constant.numeric.bin"), None, None, Some("bold strikethrough")),
        rule(Some("constant.numeric.dec"), Some("#0000ff"), None, Some("")),
        rule(Some("foo"), Some("#CFA"), None, Some("")),
    ]);
    let expected = vec![
        pr("", None, 0, NOT_SET, Some("#F8F8F2"), Some("#272822")),
        pr("source", None, 1, NOT_SET, None, Some("#100000")),
        pr("something", None, 1, NOT_SET, None, Some("#100000")),
        pr("bar", None, 2, NOT_SET, None, Some("#010000")),
        pr("baz", None, 2, NOT_SET, None, Some("#010000")),
        pr("bar", Some(&["selector", "source.css"]), 3, BOLD, None, None),
        pr("constant", None, 4, ITALIC, Some("#ff0000"), None),
        pr("constant.numeric", None, 5, NOT_SET, Some("#00ff00"), None),
        pr("constant.numeric.hex", None, 6, BOLD, None, None),
        pr("constant.numeric.oct", None, 7, BOLD | ITALIC | UNDERLINE, None, None),
        pr("constant.numeric.bin", None, 8, BOLD | STRIKETHROUGH, None, None),
        pr("constant.numeric.dec", None, 9, NONE, Some("#0000ff"), None),
        pr("foo", None, 10, NONE, Some("#CFA"), None),
    ];
    assert_eq!(actual, expected);
}

#[test]
fn str_arr_cmp_works() {
    let sign = |o: Ordering| match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    };
    let v = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let cases: Vec<(Option<Vec<String>>, Option<Vec<String>>, i32)> = vec![
        (None, None, 0),
        (None, Some(v(&[])), -1),
        (None, Some(v(&["a"])), -1),
        (Some(v(&[])), None, 1),
        (Some(v(&["a"])), None, 1),
        (Some(v(&[])), Some(v(&[])), 0),
        (Some(v(&[])), Some(v(&["a"])), -1),
        (Some(v(&["a"])), Some(v(&[])), 1),
        (Some(v(&["a"])), Some(v(&["a"])), 0),
        (Some(v(&["a", "b"])), Some(v(&["a"])), 1),
        (Some(v(&["a"])), Some(v(&["a", "b"])), -1),
        (Some(v(&["a", "b"])), Some(v(&["a", "b"])), 0),
        (Some(v(&["a", "b"])), Some(v(&["a", "c"])), -1),
        (Some(v(&["a", "c"])), Some(v(&["a", "b"])), 1),
    ];
    for (i, (a, b, e)) in cases.iter().enumerate() {
        assert_eq!(sign(str_arr_cmp(a.as_deref(), b.as_deref())), *e, "case {i}");
    }
}

#[test]
fn resolving_always_has_defaults() {
    let t = Theme::from_rules(&[]);
    let d = t.defaults();
    assert_eq!(d.font_style, font_style::NONE);
    assert_eq!(t.color(d.foreground), Some("#000000"));
    assert_eq!(t.color(d.background), Some("#FFFFFF"));
}

#[test]
fn resolving_respects_incoming_defaults() {
    let d = |r: RawThemeRule| Theme::from_rules(&[r]).defaults();
    // 1. nothing set, 2. fontStyle none, 3. bold, 4. foreground, 5. background
    assert_eq!(d(rule(None, None, None, None)).font_style, font_style::NONE);
    assert_eq!(d(rule(None, None, None, Some(""))).font_style, font_style::NONE);
    assert_eq!(d(rule(None, None, None, Some("bold"))).font_style, font_style::BOLD);
    let t = Theme::from_rules(&[rule(None, Some("#ff0000"), None, None)]);
    assert_eq!(t.color(t.defaults().foreground), Some("#FF0000"));
    assert_eq!(t.color(t.defaults().background), Some("#FFFFFF"));
    let t = Theme::from_rules(&[rule(None, None, Some("#ff0000"), None)]);
    assert_eq!(t.color(t.defaults().foreground), Some("#000000"));
    assert_eq!(t.color(t.defaults().background), Some("#FF0000"));
}

#[test]
fn resolving_can_merge_incoming_defaults() {
    let t = Theme::from_rules(&[
        rule(None, None, Some("#ff0000"), None),
        rule(None, Some("#00ff00"), None, None),
        rule(None, None, None, Some("bold")),
    ]);
    let d = t.defaults();
    assert_eq!(d.font_style, font_style::BOLD);
    assert_eq!(t.color(d.foreground), Some("#00FF00"));
    assert_eq!(t.color(d.background), Some("#FF0000"));
}

#[test]
fn resolving_same_rules_get_merged_and_inherited() {
    // var: bold from index 1 and foreground from index 0 merge into one rule.
    let t = Theme::from_rules(&[
        rule(None, Some("#F8F8F2"), Some("#272822"), None),
        rule(Some("var"), None, None, Some("bold")),
        rule(Some("var"), Some("#ff0000"), None, None),
    ]);
    assert_eq!(m(&t, &["var"]), expect(Some("#FF0000"), None, "bold"));

    // Children inherit from their parent scope rule.
    let t = Theme::from_rules(&[
        rule(None, Some("#F8F8F2"), Some("#272822"), None),
        rule(Some("var"), Some("#ff0000"), None, Some("bold")),
        rule(Some("var.identifier"), Some("#00ff00"), None, None),
        rule(Some("constant"), Some("#100000"), None, Some("italic")),
        rule(Some("constant.numeric"), Some("#200000"), None, None),
        rule(Some("constant.numeric.hex"), None, None, Some("bold")),
        rule(Some("constant.numeric.oct"), None, None, Some("bold italic underline")),
        rule(Some("constant.numeric.dec"), Some("#300000"), None, Some("")),
    ]);
    assert_eq!(m(&t, &["var.identifier"]), expect(Some("#00FF00"), None, "bold"));
    assert_eq!(m(&t, &["constant.numeric.hex"]), expect(Some("#200000"), None, "bold"));
    assert_eq!(m(&t, &["constant.numeric.oct"]), expect(Some("#200000"), None, "italic bold underline"));
    assert_eq!(m(&t, &["constant.numeric.dec"]), expect(Some("#300000"), None, "none"));
    assert_eq!(m(&t, &["constant.numeric"]), expect(Some("#200000"), None, "italic"));
}

#[test]
fn resolving_rules_with_parent_scopes() {
    let t = Theme::from_rules(&[
        rule(None, Some("#F8F8F2"), Some("#272822"), None),
        rule(Some("var"), Some("#100000"), None, Some("bold")),
        rule(Some("var.identifier"), Some("#200000"), None, None),
        rule(Some("source.css var"), Some("#300000"), None, Some("italic")),
        rule(Some("source.css var"), None, None, Some("underline")),
    ]);
    // Without the parent: the plain rules. With `source.css` as a parent: merged, underline wins.
    assert_eq!(m(&t, &["var"]), expect(Some("#100000"), None, "bold"));
    assert_eq!(m(&t, &["source.css", "var"]), expect(Some("#300000"), None, "underline"));
    assert_eq!(m(&t, &["var.identifier"]), expect(Some("#200000"), None, "bold"));
    // The deeper `var.identifier` rule beats the inherited parent-scoped one (scope depth first).
    assert_eq!(m(&t, &["source.css", "var.identifier"]), expect(Some("#200000"), None, "bold"));
}

#[test]
fn resolving_a_rule_with_child_combinator() {
    let theme = Theme::from_rules(&[
        rule(None, Some("#100000"), None, None),
        rule(Some("b a"), Some("#200000"), None, None),
        rule(Some("b > a"), Some("#300000"), None, None),
        rule(Some("c > b > a"), Some("#400000"), None, None),
        rule(Some("a"), Some("#500000"), None, None),
    ]);
    assert_eq!(fg(&theme, &["b", "a"]).as_deref(), Some("#300000"), "b a");
    assert_eq!(fg(&theme, &["b", "c", "a"]).as_deref(), Some("#200000"), "b c a");
    assert_eq!(fg(&theme, &["c", "b", "a"]).as_deref(), Some("#400000"), "c b a");
    assert_eq!(fg(&theme, &["c", "b", "d", "a"]).as_deref(), Some("#200000"), "c b d a");
}

#[test]
fn deeper_scopes_get_higher_specificity_233() {
    let theme = Theme::from_rules(&[
        rule(None, Some("#100000"), None, None),
        rule(Some("y.z a.b"), Some("#200000"), None, None),
        rule(Some("x y a.b"), Some("#300000"), None, None),
    ]);
    assert_eq!(fg(&theme, &["x", "a.b"]), None, "x a.b");
    assert_eq!(fg(&theme, &["y", "a.b"]), None, "y a.b");
    assert_eq!(fg(&theme, &["y.z", "a"]), None, "y.z a");
    assert_eq!(fg(&theme, &["x", "y", "a.b"]).as_deref(), Some("#300000"), "x y a.b");
    assert_eq!(fg(&theme, &["x", "y.z", "a.b"]).as_deref(), Some("#200000"), "y.z a.b");
}

#[test]
fn issue_38_ignores_rules_with_invalid_colors() {
    use font_style::*;
    let actual = parse_theme(&[
        rule(None, Some("#cccccc"), Some("#222222"), None),
        rule(Some("variable"), None, None, Some("")),
        rule(Some("variable.parameter"), Some(""), None, Some("italic")),
        rule(Some("support.other.variable"), None, None, Some("")),
        rule(Some("variable.other"), Some(""), None, Some("normal")),
        rule(Some("variable.parameter.function.coffee"), Some("#F9D423"), None, Some("italic")),
    ]);
    let expected = vec![
        pr("", None, 0, NOT_SET, Some("#cccccc"), Some("#222222")),
        pr("variable", None, 1, NONE, None, None),
        pr("variable.parameter", None, 2, ITALIC, None, None),
        pr("support.other.variable", None, 3, NONE, None, None),
        pr("variable.other", None, 4, NONE, None, None),
        pr("variable.parameter.function.coffee", None, 5, ITALIC, Some("#F9D423"), None),
    ];
    assert_eq!(actual, expected);
}

#[test]
fn issue_35_trailing_comma_in_a_scope_selector() {
    use font_style::*;
    let joined = [
        "meta.at-rule.return.scss,",
        "meta.at-rule.return.scss punctuation.definition,",
        "meta.at-rule.else.scss,",
        "meta.at-rule.else.scss punctuation.definition,",
        "meta.at-rule.if.scss,",
        "meta.at-rule.if.scss punctuation.definition,",
    ]
    .join("\n");
    let actual = parse_theme(&[rule(None, Some("#EFEFEF"), Some("#25292C"), None), rule(Some(&joined), Some("#CC7832"), None, None)]);
    let expected = vec![
        pr("", None, 0, NOT_SET, Some("#EFEFEF"), Some("#25292C")),
        pr("meta.at-rule.return.scss", None, 1, NOT_SET, Some("#CC7832"), None),
        pr("punctuation.definition", Some(&["meta.at-rule.return.scss"]), 1, NOT_SET, Some("#CC7832"), None),
        pr("meta.at-rule.else.scss", None, 1, NOT_SET, Some("#CC7832"), None),
        pr("punctuation.definition", Some(&["meta.at-rule.else.scss"]), 1, NOT_SET, Some("#CC7832"), None),
        pr("meta.at-rule.if.scss", None, 1, NOT_SET, Some("#CC7832"), None),
        pr("punctuation.definition", Some(&["meta.at-rule.if.scss"]), 1, NOT_SET, Some("#CC7832"), None),
    ];
    assert_eq!(actual, expected);
}
