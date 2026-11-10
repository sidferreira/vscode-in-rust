//! upstream: src/vs/editor/test/common/modes/supports/onEnter.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/modes/supports/onEnterRules.ts @ 50e1350fc78d
//!
//! Ports of the `OnEnter` suite. `javascript_on_enter_rules` is upstream's `javascriptOnEnterRules`
//! test fixture (the JavaScript language configuration's rules).

use vsr_languages::config::{AutoIndentStrategy, EnterAction, IndentAction, JsRegex, OnEnterRule};
use vsr_languages::on_enter::OnEnterSupport;

fn re(p: &str) -> JsRegex {
    JsRegex::new(p, "").unwrap_or_else(|| panic!("pattern does not compile: {p}"))
}

fn rule(before: &str, after: Option<&str>, previous: Option<&str>, action: IndentAction, append: Option<&str>, remove: Option<usize>) -> OnEnterRule {
    OnEnterRule {
        before_text: re(before),
        after_text: after.map(re),
        previous_line_text: previous.map(re),
        action: EnterAction { indent_action: action, append_text: append.map(String::from), remove_text: remove },
    }
}

fn javascript_on_enter_rules() -> Vec<OnEnterRule> {
    use IndentAction::{Indent, IndentOutdent, Outdent};
    vec![
        rule(r"^\s*\/\*\*(?!\/)([^\*]|\*(?!\/))*$", Some(r"^\s*\*\/$"), None, IndentOutdent, Some(" * "), None),
        rule(r"^\s*\/\*\*(?!\/)([^\*]|\*(?!\/))*$", None, None, None_, Some(" * "), None),
        rule(r"^(\t|[ ])*[ ]\*([ ]([^\*]|\*(?!\/))*)?$", None, Some(r"(?=^(\s*(\/\*\*|\*)).*)(?=(?!(\s*\*\/)))"), None_, Some("* "), None),
        rule(r"^(\t|[ ])*[ ]\*\/\s*$", None, None, None_, None, Some(1)),
        rule(r"^(\t|[ ])*[ ]\*[^/]*\*\/\s*$", None, None, None_, None, Some(1)),
        rule(r"^\s*(\bcase\s.+:|\bdefault:)$", Some(r"^(?!\s*(\bcase\b|\bdefault\b))"), None, Indent, None, None),
        rule(r"^\s+([^{i\s]|i(?!f\b))", None, Some(r"^\s*(((else ?)?if|for|while)\s*\(.*\)\s*|else\s*)$"), Outdent, None, None),
        rule(r"^.*\([^\)]*$", Some(r"^\s*\).*$"), None, IndentOutdent, Some("\t"), None),
        rule(r"^.*\{[^\}]*$", Some(r"^\s*\}.*$"), None, IndentOutdent, Some("\t"), None),
        rule(r"^.*\[[^\]]*$", Some(r"^\s*\].*$"), None, IndentOutdent, Some("\t"), None),
    ]
}

use IndentAction::None as None_;

// upstream: OnEnter > uses brackets
#[test]
fn uses_brackets() {
    let brackets = vec![("(".to_string(), ")".to_string()), ("begin".to_string(), "end".to_string())];
    let support = OnEnterSupport::new(Some(brackets), None);
    let test = |before: &str, after: &str, expected: IndentAction| {
        let actual = support.on_enter(AutoIndentStrategy::Advanced, "", before, after);
        if expected == IndentAction::None {
            assert_eq!(actual, Option::None, "{before:?} {after:?}");
        } else {
            assert_eq!(actual.map(|a| a.indent_action), Some(expected), "{before:?} {after:?}");
        }
    };
    test("a", "", IndentAction::None);
    test("", "b", IndentAction::None);
    test("(", "b", IndentAction::Indent);
    test("a", ")", IndentAction::None);
    test("begin", "ending", IndentAction::Indent);
    test("abegin", "end", IndentAction::None);
    test("begin", ")", IndentAction::Indent);
    test("begin", "end", IndentAction::IndentOutdent);
    test("begin ", " end", IndentAction::IndentOutdent);
    test(" begin", "end//as", IndentAction::IndentOutdent);
    test("(", ")", IndentAction::IndentOutdent);
    test("( ", ")", IndentAction::IndentOutdent);
    test("a(", ")b", IndentAction::IndentOutdent);
    test("(", "", IndentAction::Indent);
    test("(", "foo", IndentAction::Indent);
    test("begin", "foo", IndentAction::Indent);
    test("begin", "", IndentAction::Indent);
}

// upstream: OnEnter > Issue #121125: onEnterRules with global modifier
// (JavaScript's `lastIndex` does not exist here: the same rule matches twice in a row)
#[test]
fn issue_121125_on_enter_rules_with_global_modifier() {
    let support = OnEnterSupport::new(
        Option::None,
        Some(vec![rule(r"^\s*\/{3}.*$", Option::None, Option::None, IndentAction::Outdent, Some("/// "), Option::None)]),
    );
    for _ in 0..2 {
        let actual = support.on_enter(AutoIndentStrategy::Advanced, "/// line", "/// line", "").expect("matches");
        assert_eq!(actual.indent_action, IndentAction::Outdent);
        assert_eq!(actual.append_text.as_deref(), Some("/// "));
    }
}

// upstream: OnEnter > uses regExpRules
#[test]
fn uses_reg_exp_rules() {
    let support = OnEnterSupport::new(Option::None, Some(javascript_on_enter_rules()));
    let test = |previous: &str, before: &str, after: &str, expected: Option<IndentAction>, append: Option<&str>, remove: usize| {
        let actual = support.on_enter(AutoIndentStrategy::Advanced, previous, before, after);
        match expected {
            Option::None => assert_eq!(actual, Option::None, "isNull: {before:?}"),
            Some(action) => {
                let actual = actual.unwrap_or_else(|| panic!("isNotNull: {before:?}"));
                assert_eq!(actual.indent_action, action, "indentAction: {before:?}");
                if let Some(append) = append {
                    assert_eq!(actual.append_text.as_deref(), Some(append), "appendText: {before:?}");
                }
                if remove != 0 {
                    assert_eq!(actual.remove_text, Some(remove), "removeText: {before:?}");
                }
            }
        }
    };
    use IndentAction::{Indent, IndentOutdent};
    let none = Some(IndentAction::None);

    test("", "\t/**", " */", Some(IndentOutdent), Some(" * "), 0);
    test("", "\t/**", "", none, Some(" * "), 0);
    test("", "\t/** * / * / * /", "", none, Some(" * "), 0);
    test("", "\t/** /*", "", none, Some(" * "), 0);
    test("", "/**", "", none, Some(" * "), 0);
    test("", "\t/**/", "", Option::None, Option::None, 0);
    test("", "\t/***/", "", Option::None, Option::None, 0);
    test("", "\t/*******/", "", Option::None, Option::None, 0);
    test("", "\t/** * * * * */", "", Option::None, Option::None, 0);
    test("", "\t/** */", "", Option::None, Option::None, 0);
    test("", "\t/** asdfg */", "", Option::None, Option::None, 0);
    test("", "\t/* asdfg */", "", Option::None, Option::None, 0);
    test("", "\t/* asdfg */", "", Option::None, Option::None, 0);
    test("", "\t/** asdfg */", "", Option::None, Option::None, 0);
    test("", "*/", "", Option::None, Option::None, 0);
    test("", "\t/*", "", Option::None, Option::None, 0);
    test("", "\t*", "", Option::None, Option::None, 0);

    test("\t/**", "\t *", "", none, Some("* "), 0);
    test("\t * something", "\t *", "", none, Some("* "), 0);
    test("\t *", "\t *", "", none, Some("* "), 0);

    test("", "\t */", "", none, Option::None, 1);
    test("", "\t * */", "", none, Option::None, 1);
    test("", "\t * * / * / * / */", "", Option::None, Option::None, 0);

    test("\t/**", "\t * ", "", none, Some("* "), 0);
    test("\t * something", "\t * ", "", none, Some("* "), 0);
    test("\t *", "\t * ", "", none, Some("* "), 0);

    test("/**", " * ", "", none, Some("* "), 0);
    test(" * something", " * ", "", none, Some("* "), 0);
    test(" *", " * asdfsfagadfg", "", none, Some("* "), 0);

    test("/**", " * asdfsfagadfg * * * ", "", none, Some("* "), 0);
    test(" * something", " * asdfsfagadfg * * * ", "", none, Some("* "), 0);
    test(" *", " * asdfsfagadfg * * * ", "", none, Some("* "), 0);

    test("/**", " * /*", "", none, Some("* "), 0);
    test(" * something", " * /*", "", none, Some("* "), 0);
    test(" *", " * /*", "", none, Some("* "), 0);

    test("/**", " * asdfsfagadfg * / * / * /", "", none, Some("* "), 0);
    test(" * something", " * asdfsfagadfg * / * / * /", "", none, Some("* "), 0);
    test(" *", " * asdfsfagadfg * / * / * /", "", none, Some("* "), 0);

    test("/**", " * asdfsfagadfg * / * / * /*", "", none, Some("* "), 0);
    test(" * something", " * asdfsfagadfg * / * / * /*", "", none, Some("* "), 0);
    test(" *", " * asdfsfagadfg * / * / * /*", "", none, Some("* "), 0);

    test("", " */", "", none, Option::None, 1);
    test(" */", " * test() {", "", Some(Indent), Option::None, 0);
    test("", "\t */", "", none, Option::None, 1);
    test("", "\t\t */", "", none, Option::None, 1);
    test("", "   */", "", none, Option::None, 1);
    test("", "     */", "", none, Option::None, 1);
    test("", "\t     */", "", none, Option::None, 1);
    test("", " *--------------------------------------------------------------------------------------------*/", "", none, Option::None, 1);

    // issue #43469
    test("class A {", "    * test() {", "", Some(Indent), Option::None, 0);
    test("", "    * test() {", "", Some(Indent), Option::None, 0);
    test("    ", "    * test() {", "", Some(Indent), Option::None, 0);
    test("class A {", "  * test() {", "", Some(Indent), Option::None, 0);
    test("", "  * test() {", "", Some(Indent), Option::None, 0);
    test("  ", "  * test() {", "", Some(Indent), Option::None, 0);
}

// upstream: OnEnter > issue #141816
#[test]
fn issue_141816() {
    let support = OnEnterSupport::new(Option::None, Some(javascript_on_enter_rules()));
    let test = |before: &str, after: &str| {
        assert_eq!(support.on_enter(AutoIndentStrategy::Advanced, "", before, after), Option::None, "{before:?}");
    };
    test("const r = /{/;", "");
    test("const r = /{[0-9]/;", "");
    test("const r = /[a-zA-Z]{/;", "");
}
