//! upstream: src/vs/base/test/common/filters.test.ts @ 50e1350fc78d
//!
//! Ports of the VS Code `filters.test.ts` tests for the filters the Command Palette uses:
//! `or`, `matchesPrefix`, `matchesContiguousSubString`, `matchesBaseContiguousSubString` and
//! `matchesWords`. Not ported: the camel case, strict prefix, `matchesSubString`, `matchesFuzzy`
//! and `fuzzyScore*` suites (not used by the palette; `fuzzyScore` is covered by the scorer tests).
//!
//! Positions are char indexes (identical to UTF-16 offsets for these inputs).

use std::cell::RefCell;
use std::rc::Rc;

use vsr_quickinput::filters::{
    Filter, matches_base_contiguous_sub_string, matches_contiguous_sub_string, matches_words, or,
};
use vsr_quickinput::fuzzy::{Match, matches_prefix};

fn m(start: usize, end: usize) -> Match {
    Match::new(start, end)
}

fn filter_ok(f: impl Fn(&str, &str) -> Option<Vec<Match>>, word: &str, target: &str, highlights: Option<&[Match]>) {
    let r = f(word, target).unwrap_or_else(|| panic!("{word} didn't match {target}"));
    if let Some(h) = highlights {
        assert_eq!(r, h, "{word} vs {target}");
    }
}

fn filter_not_ok(f: impl Fn(&str, &str) -> Option<Vec<Match>>, word: &str, target: &str) {
    assert!(f(word, target).is_none(), "{word} matched {target}");
}

fn words(w: &str, t: &str) -> Option<Vec<Match>> {
    matches_words(w, t, false)
}

// upstream: Filters > or
#[test]
fn or_returns_the_first_match() {
    let counters = Rc::new(RefCell::new([0, 0]));
    let new_filter = |i: usize, r: bool| -> Filter<'static> {
        let counters = Rc::clone(&counters);
        Box::new(move |_, _| {
            counters.borrow_mut()[i] += 1;
            r.then(Vec::new)
        })
    };

    *counters.borrow_mut() = [0, 0];
    let f = or(vec![new_filter(0, false), new_filter(1, false)]);
    filter_not_ok(&f, "anything", "anything");
    assert_eq!(*counters.borrow(), [1, 1]);

    *counters.borrow_mut() = [0, 0];
    let f = or(vec![new_filter(0, true), new_filter(1, false)]);
    filter_ok(&f, "anything", "anything", None);
    assert_eq!(*counters.borrow(), [1, 0]);

    *counters.borrow_mut() = [0, 0];
    let f = or(vec![new_filter(0, true), new_filter(1, true)]);
    filter_ok(&f, "anything", "anything", None);
    assert_eq!(*counters.borrow(), [1, 0]);

    *counters.borrow_mut() = [0, 0];
    let f = or(vec![new_filter(0, false), new_filter(1, true)]);
    filter_ok(&f, "anything", "anything", None);
    assert_eq!(*counters.borrow(), [1, 1]);
}

// upstream: Filters > PrefixFilter - ignore case
#[test]
fn prefix_filter_ignore_case() {
    filter_ok(matches_prefix, "alpha", "alpha", Some(&[m(0, 5)]));
    filter_ok(matches_prefix, "alpha", "alphasomething", Some(&[m(0, 5)]));
    filter_not_ok(matches_prefix, "alpha", "alp");
    filter_ok(matches_prefix, "a", "alpha", Some(&[m(0, 1)]));
    filter_ok(matches_prefix, "ä", "Älpha", Some(&[m(0, 1)]));
    filter_not_ok(matches_prefix, "x", "alpha");
    filter_ok(matches_prefix, "A", "alpha", Some(&[m(0, 1)]));
    filter_ok(matches_prefix, "AlPh", "alPHA", Some(&[m(0, 4)]));
    filter_not_ok(matches_prefix, "T", "4"); // https://github.com/microsoft/vscode/issues/22401
}

// upstream: Filters > matchesContiguousSubString
#[test]
fn contiguous_sub_string() {
    filter_ok(matches_contiguous_sub_string, "cela", "cancelAnimationFrame()", Some(&[m(3, 7)]));
}

// upstream: Filters > matchesBaseContiguousSubString
#[test]
fn base_contiguous_sub_string() {
    let f = matches_base_contiguous_sub_string;
    filter_ok(f, "cela", "cancelAnimationFrame()", Some(&[m(3, 7)]));
    filter_ok(f, "cafe", "café", Some(&[m(0, 4)]));
    filter_ok(f, "cafe", "caféBar", Some(&[m(0, 4)]));
    filter_ok(f, "resume", "résumé", Some(&[m(0, 6)]));
    filter_ok(f, "naïve", "naïve", Some(&[m(0, 5)]));
    filter_ok(f, "naive", "naïve", Some(&[m(0, 5)]));
    filter_ok(f, "aeou", "àéöü", Some(&[m(0, 4)]));
}

// upstream: Filters > WordFilter
#[test]
fn word_filter() {
    let f = words;
    filter_ok(f, "alpha", "alpha", Some(&[m(0, 5)]));
    filter_ok(f, "alpha", "alphasomething", Some(&[m(0, 5)]));
    filter_not_ok(f, "alpha", "alp");
    filter_ok(f, "a", "alpha", Some(&[m(0, 1)]));
    filter_not_ok(f, "x", "alpha");
    filter_ok(f, "A", "alpha", Some(&[m(0, 1)]));
    filter_ok(f, "AlPh", "alPHA", Some(&[m(0, 4)]));
    assert!(f("Debug Console", "Open: Debug Console").is_some());

    filter_ok(f, "gp", "Git: Pull", Some(&[m(0, 1), m(5, 6)]));
    filter_ok(f, "g p", "Git: Pull", Some(&[m(0, 1), m(5, 6)]));
    filter_ok(f, "gipu", "Git: Pull", Some(&[m(0, 2), m(5, 7)]));

    filter_ok(f, "gp", "Category: Git: Pull", Some(&[m(10, 11), m(15, 16)]));
    filter_ok(f, "g p", "Category: Git: Pull", Some(&[m(10, 11), m(15, 16)]));
    filter_ok(f, "gipu", "Category: Git: Pull", Some(&[m(10, 12), m(15, 17)]));

    filter_not_ok(f, "it", "Git: Pull");
    filter_not_ok(f, "ll", "Git: Pull");

    filter_ok(f, "git: プル", "git: プル", Some(&[m(0, 7)]));
    filter_ok(f, "git プル", "git: プル", Some(&[m(0, 3), m(5, 7)]));

    filter_ok(f, "öäk", "Öhm: Älles Klar", Some(&[m(0, 1), m(5, 6), m(11, 12)]));

    // Handles issue #123915
    filter_ok(f, "C++", "C/C++: command", Some(&[m(2, 5)]));

    // Handles issue #154533
    filter_ok(f, ".", ":", Some(&[]));
    filter_ok(f, ".", ".", Some(&[m(0, 1)]));

    filter_ok(f, "bar", "foo-bar", None);
    filter_ok(f, "bar test", "foo-bar test", None);
    filter_ok(f, "fbt", "foo-bar test", None);
    filter_ok(f, "bar test", "foo-bar (test)", None);
    filter_ok(f, "foo bar", "foo (bar)", None);

    filter_not_ok(f, "bar est", "foo-bar test");
    filter_not_ok(f, "fo ar", "foo-bar test");
    filter_not_ok(f, "for", "foo-bar test");

    filter_ok(f, "foo bar", "foo-bar", None);
    filter_ok(f, "foo bar", "123 foo-bar 456", None);
    filter_ok(f, "foo-bar", "foo bar", None);
    filter_ok(f, "foo:bar", "foo:bar", None);
}

// upstream: Filters > matchesWords performance (#309582)
#[test]
fn words_performance_with_separators() {
    let targets = [
        "workbench.action.terminal.focusNextLine",
        "editor.action.clipboardCopyAction",
        "workbench.action.editor.changeLanguageMode",
        "editor.action.smartSelect.expand",
        "workbench.action.files.saveAll",
    ];
    for _ in 0..1000 {
        for t in targets {
            words("editor.action", t);
        }
    }
}
