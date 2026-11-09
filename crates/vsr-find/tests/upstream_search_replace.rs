//! upstream: src/vs/workbench/services/search/test/common/replace.test.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/test/browser/searchModel.test.ts @ 50e1350fc78d
//!
//! Ported with these adaptations: JavaScript regexes become Rust `regex` ones, so the patterns
//! with look-ahead (`this(?=.*bla)`, `b(la)(?=\stext$)`) are written without it (`this`, `b(la)`);
//! on the texts used they match the same text and groups, which is all the replace string
//! depends on. The `isRegExp` flag and the pattern text come through a `TextSearchQuery`.

use regex::Regex;
use vsr_find::SearchReplacePattern;
use vsr_search::TextSearchQuery;

fn rp(replace: &str, pattern: &str, is_regex: bool) -> SearchReplacePattern {
    let query = TextSearchQuery {
        pattern: pattern.to_string(),
        is_regex,
        is_case_sensitive: true,
        is_word_match: false,
        is_multiline: false,
    };
    SearchReplacePattern::new(replace, &query, false).unwrap()
}

const REQUIRE: &str = r#"let\s+(\w+)\s*=\s*require\s*\(\s*['"]([\w.\-/]+)['"]\s*\)\s*"#;

// upstream: Replace Pattern test > parse replace string
#[test]
fn parse_replace_string() {
    let test_parse = |input: &str, expected: &str, expected_has_parameters: bool| {
        let actual = rp(input, "somepattern", true);
        assert_eq!(actual.pattern(), expected, "{input}");
        assert_eq!(actual.has_parameters(), expected_has_parameters, "{input}");

        let actual = rp(&format!("hello{input}hi"), "sonepattern", true);
        assert_eq!(actual.pattern(), format!("hello{expected}hi"));
        assert_eq!(actual.has_parameters(), expected_has_parameters);
    };

    // no backslash => no treatment
    test_parse("hello", "hello", false);
    // \t => TAB
    test_parse("\\thello", "\thello", false);
    // \n => LF
    test_parse("\\nhello", "\nhello", false);
    // \\t => \t
    test_parse("\\\\thello", "\\thello", false);
    // \\\t => \TAB
    test_parse("\\\\\\thello", "\\\thello", false);
    // \\\\t => \\t
    test_parse("\\\\\\\\thello", "\\\\thello", false);
    // \ at the end => no treatment
    test_parse("hello\\", "hello\\", false);
    // \ with unknown char => no treatment
    test_parse("hello\\x", "hello\\x", false);
    // \ with back reference => no treatment
    test_parse("hello\\0", "hello\\0", false);

    // $1 .. $99 are parameters, $100 is not
    test_parse("hello$1", "hello$1", true);
    test_parse("hello$2", "hello$2", true);
    test_parse("hello$12", "hello$12", true);
    test_parse("hello$99", "hello$99", true);
    test_parse("hello$99a", "hello$99a", true);
    test_parse("hello$100", "hello$100", false);
    test_parse("hello$100a", "hello$100a", false);
    test_parse("hello$10a0", "hello$10a0", true);
    test_parse("hello$$", "hello$$", false);
    test_parse("hello$$0", "hello$$0", false);

    // $0 => $&
    test_parse("hello$0", "hello$&", true);
    test_parse("hello$02", "hello$&2", true);

    test_parse("hello$`", "hello$`", true);
    test_parse("hello$'", "hello$'", true);
}

// upstream: Replace Pattern test > create pattern by passing regExp
#[test]
fn create_pattern_by_passing_regexp() {
    let re = Regex::new("abc").unwrap();
    assert_eq!(SearchReplacePattern::with_regex("hello", false, re.clone()).get_replace_string("abc", false).as_deref(), Some("hello"));
    // Upstream also checks that a global regex is turned into a non-global one: not a concept here.

    let test_object = SearchReplacePattern::with_regex("hello$0", false, re.clone());
    assert!(!test_object.has_parameters());

    let test_object = SearchReplacePattern::with_regex("hello$0", true, re);
    assert!(test_object.has_parameters());
}

// upstream: Replace Pattern test > get replace string if given text is a complete match
#[test]
fn get_replace_string_if_given_text_is_a_complete_match() {
    let get = |replace: &str, pattern: &str, is_regex: bool, text: &str| rp(replace, pattern, is_regex).get_replace_string(text, false);

    assert_eq!(get("hello", "bla", true, "bla").as_deref(), Some("hello"));
    assert_eq!(get("hello", "bla", false, "bla").as_deref(), Some("hello"));
    assert_eq!(get("hello", "(bla)", true, "bla").as_deref(), Some("hello"));
    assert_eq!(get("hello$0", "(bla)", true, "bla").as_deref(), Some("hellobla"));

    assert_eq!(get("import * as $1 from '$2';", REQUIRE, true, "let fs = require('fs')").as_deref(), Some("import * as fs from 'fs';"));
    assert_eq!(
        get("import * as $1 from '$2';", REQUIRE, true, "let something = require('fs')").as_deref(),
        Some("import * as something from 'fs';")
    );
    assert_eq!(get("import * as $1 from '$2';", REQUIRE, true, "let require('fs')"), None);
    assert_eq!(
        get("import * as $1 from '$1';", REQUIRE, true, "let something = require('fs')").as_deref(),
        Some("import * as something from 'something';")
    );
    assert_eq!(
        get("import * as $2 from '$1';", REQUIRE, true, "let something = require('fs')").as_deref(),
        Some("import * as fs from 'something';")
    );
    assert_eq!(
        get("import * as $0 from '$0';", REQUIRE, true, "let something = require('fs');").as_deref(),
        Some("import * as let something = require('fs') from 'let something = require('fs')';")
    );
    // not a regex search: the pattern is searched as text
    assert_eq!(get("import * as $1 from '$2';", REQUIRE, false, "let fs = require('fs');"), None);
    assert_eq!(get("cat$1", "for(.*)", true, "for ()").as_deref(), Some("cat ()"));
}

// upstream: Replace Pattern test > case operations
#[test]
fn case_operations() {
    let t = rp("a\\u$1l\\u\\l\\U$2M$3n", "a(l)l(good)m(e)n", true);
    assert_eq!(t.get_replace_string("allgoodmen", false).as_deref(), Some("aLlGoODMen"));
}

// upstream: Replace Pattern test > case operations - no false positive
#[test]
fn case_operations_no_false_positive() {
    let t = rp("\\left $1", "(pattern)", true);
    assert_eq!(t.get_replace_string("pattern", false).as_deref(), Some("\\left pattern"));

    let t = rp("\\hi \\left $1", "(pattern)", true);
    assert_eq!(t.get_replace_string("pattern", false).as_deref(), Some("\\hi \\left pattern"));

    let t = rp("\\left \\L$1", "PATT(ERN)", true);
    assert_eq!(t.get_replace_string("PATTERN", false).as_deref(), Some("\\left ern"));
}

// upstream: Replace Pattern test > case operations and newline
#[test]
fn case_operations_and_newline() {
    // #140734
    let t = rp("$1\n\\U$2", "(multi)(line)", true);
    assert_eq!(t.get_replace_string("multiline", false).as_deref(), Some("multi\nLINE"));
}

// upstream: Replace Pattern test > get replace string for no matches
#[test]
fn get_replace_string_for_no_matches() {
    assert_eq!(rp("hello", "bla", true).get_replace_string("foo", false), None);
    assert_eq!(rp("hello", "bla", false).get_replace_string("foo", false), None);
}

// upstream: Replace Pattern test > get replace string if match is sub-string of the text
#[test]
fn get_replace_string_if_match_is_sub_string_of_the_text() {
    let text = "this is a bla text";
    let get = |replace: &str, pattern: &str, is_regex: bool| rp(replace, pattern, is_regex).get_replace_string(text, false);

    assert_eq!(get("hello", "bla", true).as_deref(), Some("hello"));
    assert_eq!(get("hello", "bla", false).as_deref(), Some("hello"));
    // `this(?=.*bla)` upstream
    assert_eq!(get("that", "this", true).as_deref(), Some("that"));
    // `(th)is(?=.*bla)` upstream
    for (replace, expected) in [
        ("$1at", "that"),
        ("$1e", "the"),
        ("$1ere", "there"),
        ("$1", "th"),
        ("ma$1", "math"),
        ("ma$1s", "maths"),
        ("$0", "this"),
        ("$0$1", "thisth"),
    ] {
        assert_eq!(get(replace, "(th)is", true).as_deref(), Some(expected), "{replace}");
    }
    // `bla(?=\stext$)` and `b(la)(?=\stext$)` upstream
    assert_eq!(get("foo", "bla", true).as_deref(), Some("foo"));
    assert_eq!(get("f$1", "b(la)", true).as_deref(), Some("fla"));
    assert_eq!(get("f$0", "b(la)", true).as_deref(), Some("fbla"));
    assert_eq!(get("$0ah", "b(la)", true).as_deref(), Some("blaah"));

    let with_regex = |replace: &str, re: &str, text: &str| {
        SearchReplacePattern::with_regex(replace, true, Regex::new(re).unwrap()).get_replace_string(text, true)
    };
    assert_eq!(with_regex("newrege$1", "Testrege(\\w*)", "Testregex").as_deref(), Some("Newregex"));
    assert_eq!(with_regex("newrege$1", "TESTREGE(\\w*)", "TESTREGEX").as_deref(), Some("NEWREGEX"));
    assert_eq!(with_regex("new_rege$1", "Test_Rege(\\w*)", "Test_Regex").as_deref(), Some("New_Regex"));
    assert_eq!(with_regex("new-rege$1", "Test-Rege(\\w*)", "Test-Regex").as_deref(), Some("New-Regex"));
}

// upstream: SearchModel > getReplaceString returns proper replace string for regExpressions
// (searchModel.test.ts, the replace-string part: the model plumbing is not ported)
#[test]
fn get_replace_string_for_regular_expressions() {
    // The replace pattern sees the text of one match; "review" stands for it where upstream's
    // fixture has the model produce it from "preview 1".
    assert_eq!(rp("hello", "re", false).get_replace_string("re", false).as_deref(), Some("hello"));
    assert_eq!(rp("hello", "re", true).get_replace_string("re", false).as_deref(), Some("hello"));
    assert_eq!(rp("hello", "re(?:vi)", true).get_replace_string("review", false).as_deref(), Some("hello"));
    assert_eq!(rp("hello", "r(e)(?:vi)", true).get_replace_string("review", false).as_deref(), Some("hello"));
    assert_eq!(rp("hello$1", "r(e)(?:vi)", true).get_replace_string("review", false).as_deref(), Some("helloe"));
}
