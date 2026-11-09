//! upstream: src/vs/editor/contrib/find/test/browser/replacePattern.test.ts @ 50e1350fc78d
//!
//! Ported with these adaptations: JavaScript's `RegExp.exec` is replaced by the Rust `regex`
//! crate (groups that did not participate are `""`), and the cases whose regex uses look-ahead
//! (`/this(?=.*bla)/` and friends, unsupported by Rust `regex`) pass the match array JavaScript
//! would have produced, since only the replace-string builder is under test. The expected value
//! of "replace has JavaScript semantics" is spelled out instead of calling `String.replace`.

use regex::Regex;
use vsr_find::{ReplacePattern, ReplacePiece, build_replace_string_with_case_preserved, parse_replace_string};

fn exec(re: &str, target: &str) -> Option<Vec<String>> {
    let caps = Regex::new(re).unwrap().captures(target)?;
    Some(caps.iter().map(|g| g.map_or_else(String::new, |m| m.as_str().to_string())).collect())
}

fn assert_replace(target: &str, search: &str, replace_string: &str, expected: &str) {
    let pattern = parse_replace_string(replace_string);
    let m = exec(search, target);
    assert_eq!(pattern.build_replace_string(m.as_deref(), false), expected, "{target}.replace(/{search}/, {replace_string})");
}

/// For regexes Rust cannot compile (look-ahead): the match array JavaScript returns.
fn assert_replace_with(m: &[&str], replace_string: &str, expected: &str) {
    let pattern = parse_replace_string(replace_string);
    let m: Vec<String> = m.iter().map(|s| s.to_string()).collect();
    assert_eq!(pattern.build_replace_string(Some(&m), false), expected, "{m:?} with {replace_string}");
}

fn test_parse(input: &str, expected: Vec<ReplacePiece>) {
    assert_eq!(parse_replace_string(input), ReplacePattern::new(expected), "Parsing {input}");
}

use ReplacePiece as P;

// upstream: Replace Pattern test > parse replace string
#[test]
fn parse_replace_string_test() {
    // no backslash => no treatment
    test_parse("hello", vec![P::static_value("hello")]);

    // \t => TAB
    test_parse("\\thello", vec![P::static_value("\thello")]);
    test_parse("h\\tello", vec![P::static_value("h\tello")]);
    test_parse("hello\\t", vec![P::static_value("hello\t")]);

    // \n => LF
    test_parse("\\nhello", vec![P::static_value("\nhello")]);

    // \\t => \t
    test_parse("\\\\thello", vec![P::static_value("\\thello")]);
    test_parse("h\\\\tello", vec![P::static_value("h\\tello")]);
    test_parse("hello\\\\t", vec![P::static_value("hello\\t")]);

    // \\\t => \TAB
    test_parse("\\\\\\thello", vec![P::static_value("\\\thello")]);

    // \\\\t => \\t
    test_parse("\\\\\\\\thello", vec![P::static_value("\\\\thello")]);

    // \ at the end => no treatment
    test_parse("hello\\", vec![P::static_value("hello\\")]);

    // \ with unknown char => no treatment
    test_parse("hello\\x", vec![P::static_value("hello\\x")]);

    // \ with back reference => no treatment
    test_parse("hello\\0", vec![P::static_value("hello\\0")]);

    test_parse("hello$&", vec![P::static_value("hello"), P::match_index(0)]);
    test_parse("hello$0", vec![P::static_value("hello"), P::match_index(0)]);
    test_parse("hello$02", vec![P::static_value("hello"), P::match_index(0), P::static_value("2")]);
    test_parse("hello$1", vec![P::static_value("hello"), P::match_index(1)]);
    test_parse("hello$2", vec![P::static_value("hello"), P::match_index(2)]);
    test_parse("hello$9", vec![P::static_value("hello"), P::match_index(9)]);
    test_parse("$9hello", vec![P::match_index(9), P::static_value("hello")]);

    test_parse("hello$12", vec![P::static_value("hello"), P::match_index(12)]);
    test_parse("hello$99", vec![P::static_value("hello"), P::match_index(99)]);
    test_parse("hello$99a", vec![P::static_value("hello"), P::match_index(99), P::static_value("a")]);
    test_parse("hello$1a", vec![P::static_value("hello"), P::match_index(1), P::static_value("a")]);
    test_parse("hello$100", vec![P::static_value("hello"), P::match_index(10), P::static_value("0")]);
    test_parse("hello$100a", vec![P::static_value("hello"), P::match_index(10), P::static_value("0a")]);
    test_parse("hello$10a0", vec![P::static_value("hello"), P::match_index(10), P::static_value("a0")]);
    test_parse("hello$$", vec![P::static_value("hello$")]);
    test_parse("hello$$0", vec![P::static_value("hello$0")]);

    test_parse("hello$`", vec![P::static_value("hello$`")]);
    test_parse("hello$'", vec![P::static_value("hello$'")]);
}

// upstream: Replace Pattern test > parse replace string with case modifiers
#[test]
fn parse_replace_string_with_case_modifiers() {
    // \U, \u => uppercase  \L, \l => lowercase  \E => cancel
    test_parse("hello\\U$1", vec![P::static_value("hello"), P::case_ops(1, &['U'])]);
    assert_replace("func privateFunc(", r"func (\w+)\(", "func \\U$1(", "func PRIVATEFUNC(");

    test_parse("hello\\u$1", vec![P::static_value("hello"), P::case_ops(1, &['u'])]);
    assert_replace("func privateFunc(", r"func (\w+)\(", "func \\u$1(", "func PrivateFunc(");

    test_parse("hello\\L$1", vec![P::static_value("hello"), P::case_ops(1, &['L'])]);
    assert_replace("func privateFunc(", r"func (\w+)\(", "func \\L$1(", "func privatefunc(");

    test_parse("hello\\l$1", vec![P::static_value("hello"), P::case_ops(1, &['l'])]);
    assert_replace("func PrivateFunc(", r"func (\w+)\(", "func \\l$1(", "func privateFunc(");

    test_parse(
        "hello$1\\u\\u\\U$4goodbye",
        vec![P::static_value("hello"), P::match_index(1), P::case_ops(4, &['u', 'u', 'U']), P::static_value("goodbye")],
    );
    assert_replace("hellogooDbye", r"hello(\w+)", "hello\\u\\u\\l\\l\\U$1", "helloGOodBYE");
}

// upstream: Replace Pattern test > replace has JavaScript semantics
#[test]
fn replace_has_javascript_semantics() {
    assert_replace("hi", "hi", "hello", "hello");
    assert_replace("hi", "hi", "\\t", "\t");
    assert_replace("hi", "hi", "\\n", "\n");
    assert_replace("hi", "hi", "\\\\t", "\\t");
    assert_replace("hi", "hi", "\\\\n", "\\n");

    // implicit capture group 0
    assert_replace("hi", "hi", "hello$&", "hellohi");
    assert_replace("hi", "hi", "hello$0", "hellohi");
    assert_replace("hi", "hi", "hello$&1", "hellohi1");
    assert_replace("hi", "hi", "hello$01", "hellohi1");

    // capture groups have funny semantics in replace strings
    // the replace string interprets $nn as a captured group only if it exists in the search regex
    assert_replace("hi", "(hi)", "hello$10", "hellohi0");
    assert_replace("hi", "(hi)()()()()()()()()()", "hello$10", "hello");
    assert_replace("hi", "(hi)", "hello$100", "hellohi00");
    assert_replace("hi", "(hi)", "hello$20", "hello$20");
}

// upstream: Replace Pattern test > get replace string if given text is a complete match
#[test]
fn get_replace_string_if_given_text_is_a_complete_match() {
    assert_replace("bla", "bla", "hello", "hello");
    assert_replace("bla", "(bla)", "hello", "hello");
    assert_replace("bla", "(bla)", "hello$0", "hellobla");

    let search = r#"let\s+(\w+)\s*=\s*require\s*\(\s*['"]([\w\.\-/]+)\s*['"]\s*\)\s*"#;
    assert_replace("let fs = require('fs')", search, "import * as $1 from '$2';", "import * as fs from 'fs';");
    assert_replace("let something = require('fs')", search, "import * as $1 from '$2';", "import * as something from 'fs';");
    assert_replace("let something = require('fs')", search, "import * as $1 from '$1';", "import * as something from 'something';");
    assert_replace("let something = require('fs')", search, "import * as $2 from '$1';", "import * as fs from 'something';");
    assert_replace(
        "let something = require('fs')",
        search,
        "import * as $0 from '$0';",
        "import * as let something = require('fs') from 'let something = require('fs')';",
    );
    assert_replace("let fs = require('fs')", search, "import * as $1 from '$2';", "import * as fs from 'fs';");
    assert_replace("for ()", "for(.*)", "cat$1", "cat ()");

    // issue #18111
    assert_replace("HRESULT OnAmbientPropertyChange(DISPID   dispid);", r"\b\s{3}\b", " ", " ");
}

// upstream: Replace Pattern test > get replace string if match is sub-string of the text
#[test]
fn get_replace_string_if_match_is_sub_string_of_the_text() {
    // (look-ahead regexes: the arrays are what JavaScript's exec returns)
    assert_replace("this is a bla text", "bla", "hello", "hello");
    assert_replace_with(&["this"], "that", "that"); // /this(?=.*bla)/
    assert_replace_with(&["this", "th"], "$1at", "that"); // /(th)is(?=.*bla)/
    assert_replace_with(&["this", "th"], "$1e", "the");
    assert_replace_with(&["this", "th"], "$1ere", "there");
    assert_replace_with(&["this", "th"], "$1", "th");
    assert_replace_with(&["this", "th"], "ma$1", "math");
    assert_replace_with(&["this", "th"], "ma$1s", "maths");
    assert_replace_with(&["this", "th"], "$0", "this");
    assert_replace_with(&["this", "th"], "$0$1", "thisth");
    assert_replace_with(&["bla"], "foo", "foo"); // /bla(?=\stext$)/
    assert_replace_with(&["bla", "la"], "f$1", "fla"); // /b(la)(?=\stext$)/
    assert_replace_with(&["bla", "la"], "f$0", "fbla");
    assert_replace_with(&["bla", "la"], "$0ah", "blaah");
}

// upstream: Replace Pattern test > issue #19740 Find and replace capture group/backreference inserts `undefined` instead of empty string
#[test]
fn issue_19740_unmatched_group_is_an_empty_string() {
    let pattern = parse_replace_string("a{$1}");
    let matches = exec("a(z)?", "abcd");
    assert_eq!(pattern.build_replace_string(matches.as_deref(), false), "a{}");
}

fn case_preserved(target: &[&str], replace_string: &str) -> String {
    let m: Vec<String> = target.iter().map(|s| s.to_string()).collect();
    build_replace_string_with_case_preserved(Some(&m), replace_string)
}

// upstream: Replace Pattern test > buildReplaceStringWithCasePreserved test
#[test]
fn build_replace_string_with_case_preserved_test() {
    let cases: &[(&[&str], &str, &str)] = &[
        (&["abc"], "Def", "def"),
        (&["Abc"], "Def", "Def"),
        (&["ABC"], "Def", "DEF"),
        (&["abc", "Abc"], "Def", "def"),
        (&["Abc", "abc"], "Def", "Def"),
        (&["ABC", "abc"], "Def", "DEF"),
        (&["aBc", "abc"], "Def", "def"),
        (&["AbC"], "Def", "Def"),
        (&["aBC"], "Def", "def"),
        (&["aBc"], "DeF", "deF"),
        (&["Foo-Bar"], "newfoo-newbar", "Newfoo-Newbar"),
        (&["Foo-Bar-Abc"], "newfoo-newbar-newabc", "Newfoo-Newbar-Newabc"),
        (&["Foo-Bar-abc"], "newfoo-newbar", "Newfoo-newbar"),
        (&["foo-Bar"], "newfoo-newbar", "newfoo-Newbar"),
        (&["foo-BAR"], "newfoo-newbar", "newfoo-NEWBAR"),
        (&["foO-BAR"], "NewFoo-NewBar", "newFoo-NEWBAR"),
        (&["Foo_Bar"], "newfoo_newbar", "Newfoo_Newbar"),
        (&["Foo_Bar_Abc"], "newfoo_newbar_newabc", "Newfoo_Newbar_Newabc"),
        (&["Foo_Bar_abc"], "newfoo_newbar", "Newfoo_newbar"),
        (&["Foo_Bar-abc"], "newfoo_newbar-abc", "Newfoo_newbar-abc"),
        (&["foo_Bar"], "newfoo_newbar", "newfoo_Newbar"),
        (&["Foo_BAR"], "newfoo_newbar", "Newfoo_NEWBAR"),
    ];
    for (target, replace, expected) in cases {
        assert_eq!(case_preserved(target, replace), *expected, "{target:?} {replace}");
    }
}

// upstream: Replace Pattern test > preserve case
#[test]
fn preserve_case() {
    let cases: &[(&[&str], &str, &str)] = &[
        (&["abc"], "Def", "def"),
        (&["Abc"], "Def", "Def"),
        (&["ABC"], "Def", "DEF"),
        (&["abc", "Abc"], "Def", "def"),
        (&["Abc", "abc"], "Def", "Def"),
        (&["ABC", "abc"], "Def", "DEF"),
        (&["aBc", "abc"], "Def", "def"),
        (&["AbC"], "Def", "Def"),
        (&["aBC"], "Def", "def"),
        (&["aBc"], "DeF", "deF"),
        (&["Foo-Bar"], "newfoo-newbar", "Newfoo-Newbar"),
        (&["Foo-Bar-Abc"], "newfoo-newbar-newabc", "Newfoo-Newbar-Newabc"),
        (&["Foo-Bar-abc"], "newfoo-newbar", "Newfoo-newbar"),
        (&["foo-Bar"], "newfoo-newbar", "newfoo-Newbar"),
        (&["foo-BAR"], "newfoo-newbar", "newfoo-NEWBAR"),
        (&["foO-BAR"], "NewFoo-NewBar", "newFoo-NEWBAR"),
        (&["Foo_Bar"], "newfoo_newbar", "Newfoo_Newbar"),
        (&["Foo_Bar_Abc"], "newfoo_newbar_newabc", "Newfoo_Newbar_Newabc"),
        (&["Foo_Bar_abc"], "newfoo_newbar", "Newfoo_newbar"),
        (&["Foo_Bar-abc"], "newfoo_newbar-abc", "Newfoo_newbar-abc"),
        (&["foo_Bar"], "newfoo_newbar", "newfoo_Newbar"),
        (&["foo_BAR"], "newfoo_newbar", "newfoo_NEWBAR"),
    ];
    for (target, replace, expected) in cases {
        let pattern = parse_replace_string(replace);
        let m: Vec<String> = target.iter().map(|s| s.to_string()).collect();
        assert_eq!(pattern.build_replace_string(Some(&m), true), *expected, "{target:?} {replace}");
    }
}
