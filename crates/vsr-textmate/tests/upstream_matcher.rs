//! upstream: vscode-textmate src/tests/matcher.test.ts (microsoft/vscode-textmate, main, 2026-11-09)
//!
//! upstream: src/vs/workbench/services/textMate/common/TMScopeRegistry.ts @ 50e1350fc78d

use vsr_textmate::scope::create_matchers;

/// The test's own `nameMatcher`: identifiers match scope names exactly, in order.
fn exact(identifiers: &[String], stack: &[&str]) -> bool {
    let mut last = 0;
    identifiers.iter().all(|id| {
        for (i, s) in stack.iter().enumerate().skip(last) {
            if s == id {
                last = i + 1;
                return true;
            }
        }
        false
    })
}

const TESTS: &[(&str, &[&str], bool)] = &[
    ("foo", &["foo"], true),
    ("foo", &["bar"], false),
    ("- foo", &["foo"], false),
    ("- foo", &["bar"], true),
    ("- - foo", &["bar"], false),
    ("bar foo", &["foo"], false),
    ("bar foo", &["bar"], false),
    ("bar foo", &["bar", "foo"], true),
    ("bar - foo", &["bar"], true),
    ("bar - foo", &["foo", "bar"], false),
    ("bar - foo", &["foo"], false),
    ("bar, foo", &["foo"], true),
    ("bar, foo", &["bar"], true),
    ("bar, foo", &["bar", "foo"], true),
    ("bar, -foo", &["bar", "foo"], true),
    ("bar, -foo", &["yo"], true),
    ("bar, -foo", &["foo"], false),
    ("(foo)", &["foo"], true),
    ("(foo - bar)", &["foo"], true),
    ("(foo - bar)", &["foo", "bar"], false),
    ("foo bar - (yo man)", &["foo", "bar"], true),
    ("foo bar - (yo man)", &["foo", "bar", "yo"], true),
    ("foo bar - (yo man)", &["foo", "bar", "yo", "man"], false),
    ("foo bar - (yo | man)", &["foo", "bar", "yo", "man"], false),
    ("foo bar - (yo | man)", &["foo", "bar", "yo"], false),
    ("R:text.html - (comment.block, text.html source)", &["text.html", "bar", "source"], false),
    (
        "text.html.php - (meta.embedded | meta.tag), L:text.html.php meta.tag, L:source.js.embedded.html",
        &["text.html.php", "bar", "source.js"],
        true,
    ),
];

#[test]
fn matcher_tests() {
    for (i, (expr, input, expected)) in TESTS.iter().enumerate() {
        let matchers = create_matchers(expr);
        let result = matchers.iter().any(|m| m.selector.matches_by(input, &exact));
        assert_eq!(result, *expected, "Matcher Test #{i}: {expr}");
    }
}

#[test]
fn priorities_and_prefix_matching() {
    let m = create_matchers("L:source.js -comment, R:text.html, plain");
    assert_eq!(m.iter().map(|m| m.priority).collect::<Vec<_>>(), [-1, 1, 0]);
    // The real matcher accepts scope prefixes (`source` matches `source.js`).
    let sel = &create_matchers("source meta.tag")[0].selector;
    assert!(sel.matches(&["source.js", "meta.tag.html"]));
    assert!(!sel.matches(&["sourcejs", "meta.tag.html"]));
}
