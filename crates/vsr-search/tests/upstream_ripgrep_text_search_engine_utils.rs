//! upstream: src/vs/workbench/services/search/test/node/ripgrepTextSearchEngineUtils.test.ts @ 50e1350fc78d
//!
//! Ports of the pure-logic tests. Not ported: `RipgrepParser` (we call the ripgrep libraries
//! in-process, there is no JSON stream to parse), `getRgArgs` (no command line; its glob
//! handling is replaced by `patterns::FileFilter`, tested in `search.rs`) and
//! "exclude exception globs" (same reason).

use regex::Regex;
use vsr_search::glob::perform_brace_expansion;
use vsr_search::query::{fix_newline, fix_regex_newline, unicode_escapes_to_pcre2};

// upstream: RipgrepTextSearchEngine > unicodeEscapesToPCRE2
#[test]
fn unicode_escapes_to_pcre2_cases() {
    let t = unicode_escapes_to_pcre2;
    assert_eq!(t("\\u1234"), "\\x{1234}");
    assert_eq!(t("\\u1234\\u0001"), "\\x{1234}\\x{0001}");
    assert_eq!(t("foo\\u1234bar"), "foo\\x{1234}bar");
    assert_eq!(t("\\\\\\u1234"), "\\\\\\x{1234}");
    assert_eq!(t("foo\\\\\\u1234"), "foo\\\\\\x{1234}");

    assert_eq!(t("\\u{1234}"), "\\x{1234}");
    assert_eq!(t("\\u{1234}\\u{0001}"), "\\x{1234}\\x{0001}");
    assert_eq!(t("foo\\u{1234}bar"), "foo\\x{1234}bar");
    assert_eq!(t("[\\u00A0-\\u00FF]"), "[\\x{00A0}-\\x{00FF}]");

    assert_eq!(t("foo\\u{123456}7bar"), "foo\\u{123456}7bar");
    assert_eq!(t("\\u123"), "\\u123");
    assert_eq!(t("foo"), "foo");
    assert_eq!(t(""), "");
}

// upstream: RipgrepTextSearchEngine > fixRegexNewline - src
#[test]
fn fix_regex_newline_src() {
    let table = [
        ["foo", "foo"],
        ["invalid(", "invalid("],
        ["fo\\no", "fo\\r?\\no"],
        ["f\\no\\no", "f\\r?\\no\\r?\\no"],
        ["f[a-z\\n1]", "f(?:[a-z1]|\\r?\\n)"],
        ["f[\\n-a]", "f[\\n-a]"],
        ["(?<=\\n)\\w", "(?<=\\n)\\w"],
        ["fo\\n+o", "fo(?:\\r?\\n)+o"],
        ["fo[^\\n]o", "fo(?!\\r?\\n)o"],
        ["fo[^\\na-z]o", "fo(?!\\r?\\n|[a-z])o"],
        ["foo[^\\n]+o", "foo.+o"],
        ["foo[^\\nzq]+o", "foo[^zq]+o"],
        ["foo[^\\nzq]+o", "foo[^zq]+o"],
        // preserves quantifies, #137899
        ["fo[^\\S\\n]*o", "fo[^\\S]*o"],
        ["fo[^\\S\\n]{3,}o", "fo[^\\S]{3,}o"],
    ];
    for [input, expected] in table {
        assert_eq!(fix_regex_newline(input), expected, "{input} -> {expected}");
    }
}

// upstream: RipgrepTextSearchEngine > fixRegexNewline - re
#[test]
fn fix_regex_newline_re() {
    let cases: [(&str, &str, bool); 12] = [
        ("foo", "foo", true),
        ("foo\\n", "foo\r\n", true),
        ("foo\\n\\n", "foo\n\n", true),
        ("foo\\n\\n", "foo\r\n\r\n", true),
        ("foo\\n", "foo\n", true),
        ("foo\\nabc", "foo\r\nabc", true),
        ("foo\\nabc", "foo\nabc", true),
        ("foo\\r\\n", "foo\r\n", true),
        ("foo\\n+abc", "foo\r\nabc", true),
        ("foo\\n+abc", "foo\n\n\nabc", true),
        ("foo\\n+abc", "foo\r\n\r\n\r\nabc", true),
        ("foo[\\n-9]+abc", "foo1abc", true),
    ];
    for (input, text, should) in cases {
        let fixed = fix_regex_newline(input);
        let re = Regex::new(&fixed).unwrap();
        assert_eq!(re.is_match(text), should, "{input} => {fixed}, {text:?}");
    }
}

// upstream: RipgrepTextSearchEngine > fixNewline - matching
#[test]
fn fix_newline_matching() {
    let cases: [(&str, &str, bool); 8] = [
        ("foo", "foo", true),
        ("foo\n", "foo\r\n", true),
        ("foo\n", "foo\n", true),
        ("foo\nabc", "foo\r\nabc", true),
        ("foo\nabc", "foo\nabc", true),
        ("foo\r\n", "foo\r\n", true),
        ("foo\nbarc", "foobar", false),
        ("foobar", "foo\nbar", false),
    ];
    for (input, text, should) in cases {
        let fixed = fix_newline(input);
        let re = Regex::new(&fixed).unwrap();
        assert_eq!(re.is_match(text), should, "{input} => {fixed}, {text:?}");
    }
}

// upstream: RipgrepTextSearchEngine > brace expansion for ripgrep
#[test]
fn brace_expansion_for_ripgrep() {
    let cases: [(&str, &[&str]); 18] = [
        ("eep/{a,b}/test", &["eep/a/test", "eep/b/test"]),
        ("eep/{a,b}/{c,d,e}", &["eep/a/c", "eep/a/d", "eep/a/e", "eep/b/c", "eep/b/d", "eep/b/e"]),
        ("eep/{a,b}/\\{c,d,e}", &["eep/a/{c,d,e}", "eep/b/{c,d,e}"]),
        ("eep/{a,b\\}/test", &["eep/{a,b}/test"]),
        ("eep/{a,b\\\\}/test", &["eep/a/test", "eep/b\\\\/test"]),
        ("eep/{a,b\\\\\\}/test", &["eep/{a,b\\\\}/test"]),
        ("e\\{ep/{a,b}/test", &["e{ep/a/test", "e{ep/b/test"]),
        ("eep/{a,\\b}/test", &["eep/a/test", "eep/\\b/test"]),
        ("{a/*.*,b/*.*}", &["a/*.*", "b/*.*"]),
        ("{{}", &["{{}"]),
        ("aa{{}", &["aa{{}"]),
        ("{b{}", &["{b{}"]),
        ("{{}c", &["{{}c"]),
        ("{{}}", &["{{}}"]),
        ("\\{{}}", &["{}"]),
        ("{}foo", &["foo"]),
        ("bar{ }foo", &["bar foo"]),
        ("{}", &[""]),
    ];
    for (input, expected) in cases {
        assert_eq!(perform_brace_expansion(input), expected, "{input}");
    }
}
