//! Engine tests over temp workspaces (no upstream counterpart; upstream's engine tests spawn rg).

use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tempfile::TempDir;
use vsr_search::{FileMatch, SearchOptions, TextSearchQuery, search, search_collect, split_pattern_input};

fn ws(files: &[(&str, &str)]) -> TempDir {
    let d = tempfile::tempdir().unwrap();
    for (p, c) in files {
        let path = d.path().join(p);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, c).unwrap();
    }
    d
}

fn run(d: &Path, q: TextSearchQuery, tweak: impl FnOnce(&mut SearchOptions)) -> Vec<FileMatch> {
    let mut o = SearchOptions::new(d);
    tweak(&mut o);
    search_collect(&q, &o).unwrap().0
}

fn names(d: &Path, v: &[FileMatch]) -> Vec<String> {
    v.iter().map(|f| f.path.strip_prefix(d).unwrap().to_string_lossy().into_owned()).collect()
}

fn total(v: &[FileMatch]) -> usize {
    v.iter().map(FileMatch::result_count).sum()
}

#[test]
fn literal_pattern_is_escaped() {
    let d = ws(&[("a.txt", "a.c\nabc\n")]);
    let r = run(d.path(), TextSearchQuery::new("a.c"), |_| {});
    assert_eq!(total(&r), 1);
    assert_eq!(r[0].matches[0].line, 1);
    let mut q = TextSearchQuery::new("a.c");
    q.is_regex = true;
    assert_eq!(total(&run(d.path(), q, |_| {})), 2);
}

#[test]
fn case_sensitivity_and_smart_case() {
    let d = ws(&[("a.txt", "Foo foo FOO\n")]);
    assert_eq!(total(&run(d.path(), TextSearchQuery::new("foo"), |_| {})), 3);
    let mut q = TextSearchQuery::new("foo");
    q.is_case_sensitive = true;
    assert_eq!(total(&run(d.path(), q, |_| {})), 1);
    // smart case: all lowercase -> insensitive; with an uppercase letter -> sensitive
    assert_eq!(total(&run(d.path(), TextSearchQuery::new("foo"), |o| o.smart_case = true)), 3);
    assert_eq!(total(&run(d.path(), TextSearchQuery::new("Foo"), |o| o.smart_case = true)), 1);
}

#[test]
fn whole_word() {
    let d = ws(&[("a.txt", "foo foobar barfoo foo_ foo.\n")]);
    let mut q = TextSearchQuery::new("foo");
    q.is_word_match = true;
    assert_eq!(total(&run(d.path(), q, |_| {})), 2); // "foo" and "foo."
    let mut q = TextSearchQuery::new("foo(");
    q.is_word_match = true;
    let d2 = ws(&[("a.txt", "call foo(1) and xfoo(2)\n")]);
    assert_eq!(total(&run(d2.path(), q, |_| {})), 1); // \b only next to word characters: xfoo( is not a match
}

#[test]
fn include_and_exclude_globs() {
    let d = ws(&[("a.rs", "x\n"), ("b.txt", "x\n"), ("src/c.rs", "x\n"), ("docs/d.md", "x\n")]);
    let q = || TextSearchQuery::new("x");
    let r = run(d.path(), q(), |o| o.includes = split_pattern_input("*.rs"));
    assert_eq!(names(d.path(), &{ let mut r = r; r.sort_by(|a, b| a.path.cmp(&b.path)); r }), ["a.rs", "src/c.rs"]);
    let r = run(d.path(), q(), |o| o.includes = split_pattern_input("./src"));
    assert_eq!(names(d.path(), &r), ["src/c.rs"]);
    let r = run(d.path(), q(), |o| o.excludes = split_pattern_input("docs, *.txt"));
    assert_eq!(r.len(), 2);
    let r = run(d.path(), q(), |o| o.includes = split_pattern_input("{a,b}.*"));
    assert_eq!(r.len(), 2);
}

#[test]
fn gitignore_is_respected_and_can_be_disabled() {
    let d = ws(&[(".gitignore", "ignored/\n*.log\n"), ("ignored/a.txt", "x\n"), ("b.log", "x\n"), ("keep.txt", "x\n")]);
    let r = run(d.path(), TextSearchQuery::new("x"), |_| {});
    assert_eq!(names(d.path(), &r), ["keep.txt"]);
    let r = run(d.path(), TextSearchQuery::new("x"), |o| {
        o.use_ignore_files = false;
    });
    assert_eq!(r.len(), 3);
}

#[test]
fn settings_excludes_apply() {
    let d = ws(&[("node_modules/p/a.js", "x\n"), (".git/config", "x\n"), ("a.js", "x\n")]);
    let r = run(d.path(), TextSearchQuery::new("x"), |_| {});
    assert_eq!(names(d.path(), &r), ["a.js"]);
    let r = run(d.path(), TextSearchQuery::new("x"), |o| o.exclude_globs_from_settings.clear());
    assert_eq!(r.len(), 3);
    // exception re-includes a folder inside node_modules
    let r = run(d.path(), TextSearchQuery::new("x"), |o| o.exclude_globs_from_settings.push("!node_modules/p".into()));
    assert_eq!(r.len(), 2);
}

#[test]
fn binary_files_are_skipped() {
    let d = ws(&[("t.txt", "needle\n")]);
    fs::write(d.path().join("b.bin"), b"\0\0needle\n").unwrap();
    let r = run(d.path(), TextSearchQuery::new("needle"), |_| {});
    assert_eq!(names(d.path(), &r), ["t.txt"]);
}

#[test]
fn cancellation_stops_early() {
    let files: Vec<(String, String)> = (0..200).map(|i| (format!("f{i}.txt"), "x\n".into())).collect();
    let refs: Vec<(&str, &str)> = files.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
    let d = ws(&refs);
    let cancel = AtomicBool::new(false);
    let count = Mutex::new(0usize);
    let done = search(&TextSearchQuery::new("x"), &SearchOptions::new(d.path()), &cancel, |_| {
        let mut c = count.lock().unwrap();
        *c += 1;
        if *c == 3 {
            cancel.store(true, Ordering::Relaxed);
        }
    })
    .unwrap();
    assert!(done.cancelled);
    assert!(*count.lock().unwrap() < 200);
}

#[test]
fn max_results_sets_limit_hit() {
    let d = ws(&[("a.txt", "x x x x x\nx x x\n")]);
    let (files, done) = search_collect(&TextSearchQuery::new("x"), &{
        let mut o = SearchOptions::new(d.path());
        o.max_results = Some(4);
        o
    })
    .unwrap();
    assert!(done.limit_hit);
    assert_eq!(total(&files), 4);
    let (files, done) = search_collect(&TextSearchQuery::new("x"), &SearchOptions::new(d.path())).unwrap();
    assert!(!done.limit_hit);
    assert_eq!(total(&files), 8);
}

#[test]
fn utf16_columns_with_emoji() {
    let d = ws(&[("a.txt", "😀😀 foo\n")]);
    let r = run(d.path(), TextSearchQuery::new("foo"), |_| {});
    let m = &r[0].matches[0];
    // two emoji are 4 UTF-16 units, plus a space: foo starts at 0-based col 5 -> 1-based 6
    assert_eq!((m.ranges[0].start_column, m.ranges[0].end_column), (6, 9));
    assert_eq!(m.preview, "😀😀 foo");
    assert_eq!((m.preview_ranges[0].start_column, m.preview_ranges[0].end_column), (5, 8));
}

#[test]
fn multiple_matches_on_a_line_and_line_numbers() {
    let d = ws(&[("a.txt", "no\nfoo bar foo\n")]);
    let r = run(d.path(), TextSearchQuery::new("foo"), |_| {});
    let m = &r[0].matches[0];
    assert_eq!(m.line, 2);
    assert_eq!(m.ranges.len(), 2);
    assert_eq!((m.ranges[1].start_column, m.ranges[1].end_column), (9, 12));
}

#[test]
fn multiline_regex_and_crlf() {
    let d = ws(&[("a.txt", "foo\r\nbar\r\nbaz\n")]);
    let mut q = TextSearchQuery::new("foo\nbar");
    q.is_regex = true;
    let r = run(d.path(), q, |_| {});
    assert_eq!(total(&r), 1);
    let m = &r[0].matches[0];
    assert_eq!((m.ranges[0].start_line, m.ranges[0].end_line), (1, 2));
    let mut q = TextSearchQuery::new("foo\nbar"); // literal multiline
    q.is_multiline = true;
    assert_eq!(total(&run(d.path(), q, |_| {})), 1);
    let mut q = TextSearchQuery::new("bar$");
    q.is_regex = true;
    assert_eq!(total(&run(d.path(), q, |_| {})), 1); // $ before \r\n
}

#[test]
fn invalid_regex_is_an_error() {
    let d = ws(&[("a.txt", "x\n")]);
    let mut q = TextSearchQuery::new("(");
    q.is_regex = true;
    assert!(search_collect(&q, &SearchOptions::new(d.path())).is_err());
    assert!(search_collect(&TextSearchQuery::new(""), &SearchOptions::new(d.path())).is_err());
}

#[test]
fn hidden_files_are_searched() {
    let d = ws(&[(".hidden/a.txt", "x\n"), (".env", "x\n")]);
    assert_eq!(run(d.path(), TextSearchQuery::new("x"), |_| {}).len(), 2);
}

#[test]
fn long_line_preview_is_elided() {
    let line = format!("needle {} needle\n", "z".repeat(3000));
    let d = ws(&[("a.txt", &line)]);
    let r = run(d.path(), TextSearchQuery::new("needle"), |_| {});
    assert!(r[0].matches[0].preview.contains("characters skipped"));
}
