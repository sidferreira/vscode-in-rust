//! upstream: src/vs/workbench/services/search/test/common/search.test.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/test/common/queryBuilder.test.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/test/browser/queryBuilder.test.ts @ 50e1350fc78d
//!
//! Not ported from search.test.ts: "exclude exceptions" (`QueryGlobTester` and the provider
//! exclude resolution have no counterpart; exceptions are handled by `FileFilter`). From the
//! browser queryBuilder test only the single-folder pure cases (smart case, `parseSearchPaths`,
//! newline normalisation); multi-root, tilde and file-query cases are out of scope.

use std::collections::BTreeSet;
use std::path::PathBuf;

use vsr_search::TextSearchQuery;
use vsr_search::glob::{escape_glob_pattern, glob_match};
use vsr_search::patterns::{SearchPathPattern, parse_search_paths, resolve_resources_for_search_includes, split_pattern_input};
use vsr_search::preview::{PreviewOptions, SearchRange, TextSearchMatch};

const OPTS1: PreviewOptions = PreviewOptions { match_lines: 1, chars_per_line: 100 };

fn assert_preview_range_text(text: &str, r: &TextSearchMatch) {
    assert_eq!(r.range_locations.len(), 1);
    let p = r.range_locations[0].preview;
    let u: Vec<u16> = r.preview_text.encode_utf16().collect();
    assert_eq!(String::from_utf16_lossy(&u[p.start_column.min(u.len())..p.end_column.min(u.len())]), text);
}

fn first_source(r: &TextSearchMatch) -> SearchRange {
    r.range_locations[0].source
}

// upstream: TextSearchResult > empty without preview options
#[test]
fn empty_without_preview_options() {
    let range = SearchRange::one_line(5, 0, 0);
    let r = TextSearchMatch::new("", &[range], None);
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("", &r);
}

// upstream: TextSearchResult > empty with preview options
#[test]
fn empty_with_preview_options() {
    let range = SearchRange::one_line(5, 0, 0);
    let r = TextSearchMatch::new("", &[range], Some(OPTS1));
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("", &r);
}

// upstream: TextSearchResult > short without preview options
#[test]
fn short_without_preview_options() {
    let range = SearchRange::one_line(5, 4, 7);
    let r = TextSearchMatch::new("foo bar", &[range], None);
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("bar", &r);
}

// upstream: TextSearchResult > short with preview options
#[test]
fn short_with_preview_options() {
    let range = SearchRange::one_line(5, 4, 7);
    let r = TextSearchMatch::new("foo bar", &[range], Some(OPTS1));
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("bar", &r);
}

// upstream: TextSearchResult > leading
#[test]
fn leading() {
    let range = SearchRange::one_line(5, 25, 28);
    let r = TextSearchMatch::new("long text very long text foo", &[range], Some(OPTS1));
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("foo", &r);
}

// upstream: TextSearchResult > trailing
#[test]
fn trailing() {
    let range = SearchRange::one_line(5, 0, 3);
    let text = "foo long text very long text long text very long text long text very long text long text very long text long text very long text";
    let r = TextSearchMatch::new(text, &[range], Some(OPTS1));
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("foo", &r);
}

// upstream: TextSearchResult > middle
#[test]
fn middle() {
    let range = SearchRange::one_line(5, 30, 33);
    let text = "long text very long text long foo text very long text long text very long text long text very long text long text very long text";
    let r = TextSearchMatch::new(text, &[range], Some(OPTS1));
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("foo", &r);
}

// upstream: TextSearchResult > truncating match
#[test]
fn truncating_match() {
    let range = SearchRange::one_line(0, 4, 7);
    let r = TextSearchMatch::new("foo bar", &[range], Some(PreviewOptions { match_lines: 1, chars_per_line: 1 }));
    assert_eq!(first_source(&r), range);
    assert_preview_range_text("b", &r);
}

// upstream: TextSearchResult > one line of multiline match
#[test]
fn one_line_of_multiline_match() {
    let range = SearchRange::new(5, 4, 6, 3);
    let r = TextSearchMatch::new("foo bar\nfoo bar", &[range], Some(PreviewOptions { match_lines: 1, chars_per_line: 10000 }));
    assert_eq!(first_source(&r), range);
    assert_eq!(r.preview_text, "foo bar\nfoo bar");
    assert_eq!(r.range_locations.len(), 1);
    assert_eq!(r.range_locations[0].preview, SearchRange::new(0, 4, 1, 3));
}

// upstream: TextSearchResult > compacts multiple ranges on long lines
#[test]
fn compacts_multiple_ranges_on_long_lines() {
    let ranges = [SearchRange::new(5, 4, 5, 7), SearchRange::new(5, 133, 5, 136), SearchRange::new(5, 141, 5, 144)];
    let text = "foo bar 123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890 foo bar baz bar";
    let r = TextSearchMatch::new(text, &ranges, Some(PreviewOptions { match_lines: 1, chars_per_line: 10 }));
    let previews: Vec<_> = r.range_locations.iter().map(|e| e.preview).collect();
    assert_eq!(previews, [SearchRange::one_line(0, 4, 7), SearchRange::one_line(0, 42, 45), SearchRange::one_line(0, 50, 53)]);
    assert_eq!(r.preview_text, "foo bar 123456⟪ 117 characters skipped ⟫o bar baz bar");
}

// upstream: TextSearchResult > trims lines endings
#[test]
fn trims_lines_endings() {
    let range = SearchRange::new(5, 3, 5, 5);
    let o = Some(PreviewOptions { match_lines: 1, chars_per_line: 10000 });
    assert_eq!(TextSearchMatch::new("foo bar\n", &[range], o).preview_text, "foo bar");
    assert_eq!(TextSearchMatch::new("foo bar\r\n", &[range], o).preview_text, "foo bar");
}

// upstream: isFilePatternMatch > glob match is case-sensitive by default
#[test]
fn glob_match_is_case_sensitive_by_default() {
    assert!(glob_match("**/*.ts", "src/Foo.ts", false));
    assert!(!glob_match("**/*.TS", "src/Foo.ts", false));
}

// upstream: isFilePatternMatch > glob match is case-insensitive when ignoreCase is true
#[test]
fn glob_match_is_case_insensitive_when_ignore_case() {
    assert!(glob_match("**/*.TS", "src/Foo.ts", true));
    assert!(glob_match("**/*.Ts", "src/Foo.ts", true));
}

// upstream: isFilePatternMatch > glob match with mixed case pattern
#[test]
fn glob_match_with_mixed_case_pattern() {
    assert!(!glob_match("**/*.tsx", "src/MyComponent.TSX", false));
    assert!(glob_match("**/*.tsx", "src/MyComponent.TSX", true));
}

// upstream: QueryBuilderCommon > escapeGlobPattern properly escapes square brackets for literal matching
#[test]
fn escape_glob_pattern_escapes_square_brackets() {
    let file = "file[test].txt";
    assert!(!glob_match(file, file, false), "unescaped is a character class");
    let escaped = escape_glob_pattern(file);
    assert!(glob_match(&escaped, file, false));
    assert_eq!(escaped, "file[[]test[]].txt");
}

// upstream: QueryBuilderCommon > resolveResourcesForSearchIncludes passes through paths without special glob characters
#[test]
fn resolve_resources_passes_through_plain_paths() {
    let root = PathBuf::from("/testWorkspace");
    let actual = resolve_resources_for_search_includes(&root, &[PathBuf::from("/testWorkspace/pages/blog")]);
    assert_eq!(actual, ["./pages/blog"]);
}

// upstream: QueryBuilderCommon > resolveResourcesForSearchIncludes escapes paths with special characters
#[test]
fn resolve_resources_escapes_special_characters() {
    let root = PathBuf::from("/testWorkspace");
    let actual = resolve_resources_for_search_includes(&root, &[PathBuf::from("/testWorkspace/pages/blog/[postId]")]);
    assert_eq!(actual, ["./pages/blog/[[]postId[]]"]);
}

// ---- QueryBuilder (browser) -----------------------------------------------------------------

fn case_sensitive(pattern: &str, is_regex: bool, flag: bool, smart: bool) -> bool {
    TextSearchQuery { pattern: pattern.into(), is_regex, is_case_sensitive: flag, ..Default::default() }
        .effective_case_sensitive(smart)
}

// upstream: QueryBuilder > smartCase > no flags -> no change
#[test]
fn smart_case_no_flags() {
    assert!(!case_sensitive("a", false, false, false));
}

// upstream: QueryBuilder > smartCase > maintains isCaseSensitive when smartCase not set
#[test]
fn smart_case_maintains_flag_when_not_set() {
    assert!(case_sensitive("a", false, true, false));
}

// upstream: QueryBuilder > smartCase > maintains isCaseSensitive when smartCase set
#[test]
fn smart_case_maintains_flag_when_set() {
    assert!(case_sensitive("a", false, true, true));
}

// upstream: QueryBuilder > smartCase > smartCase determines not case sensitive
#[test]
fn smart_case_determines_not_case_sensitive() {
    assert!(!case_sensitive("abcd", false, false, true));
}

// upstream: QueryBuilder > smartCase > smartCase determines case sensitive
#[test]
fn smart_case_determines_case_sensitive() {
    assert!(case_sensitive("abCd", false, false, true));
}

// upstream: QueryBuilder > smartCase > smartCase determines not case sensitive (regex)
#[test]
fn smart_case_determines_not_case_sensitive_regex() {
    assert!(!case_sensitive("ab\\Sd", true, false, true));
}

// upstream: QueryBuilder > smartCase > smartCase determines case sensitive (regex)
#[test]
fn smart_case_determines_case_sensitive_regex() {
    assert!(case_sensitive("ab[A-Z]d", true, false, true));
}

// upstream: QueryBuilder > normalize literal newlines
#[test]
fn normalize_literal_newlines() {
    let regex = TextSearchQuery { pattern: "foo\nbar".into(), is_regex: true, ..Default::default() };
    assert!(regex.effective_multiline());
    assert_eq!(vsr_search::query::regex_source(&regex), "foo\\r?\\nbar");
    let literal = TextSearchQuery { pattern: "foo\nbar".into(), ..Default::default() };
    assert!(literal.effective_multiline());
}

fn set(v: &[&str]) -> BTreeSet<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// upstream: QueryBuilder > parseSearchPaths 1 > simple includes
#[test]
fn parse_search_paths_simple_includes() {
    let root = PathBuf::from("/foo/root1");
    let cases: [(&str, &[&str]); 6] = [
        ("a", &["**/a/**", "**/a"]),
        ("a/b", &["**/a/b", "**/a/b/**"]),
        ("a/b,  c", &["**/a/b", "**/c", "**/a/b/**", "**/c/**"]),
        ("a,.txt", &["**/a", "**/a/**", "**/*.txt", "**/*.txt/**"]),
        ("a,,,b", &["**/a", "**/a/**", "**/b", "**/b/**"]),
        ("**/a,b/**", &["**/a", "**/a/**", "**/b/**"]),
    ];
    for (input, expected) in cases {
        let r = parse_search_paths(&root, &split_pattern_input(input));
        assert_eq!(r.patterns.into_iter().collect::<BTreeSet<_>>(), set(expected), "{input}");
        assert!(r.search_paths.is_empty());
    }
}

fn sp(dir: &str, patterns: &[&str]) -> (PathBuf, BTreeSet<String>) {
    (PathBuf::from(dir), set(patterns))
}

fn flat(v: Vec<SearchPathPattern>) -> Vec<(PathBuf, BTreeSet<String>)> {
    v.into_iter().map(|s| (s.dir, s.patterns.into_iter().collect())).collect()
}

// upstream: QueryBuilder > parseSearchPaths 1 > absolute includes
#[test]
fn parse_search_paths_absolute_includes() {
    let root = PathBuf::from("/foo/root1");
    let run = |s: &str| parse_search_paths(&root, &split_pattern_input(s));
    assert_eq!(flat(run("/foo/bar").search_paths), [sp("/foo/bar", &[])]);
    let r = run("/foo/bar,a");
    assert_eq!(flat(r.search_paths), [sp("/foo/bar", &[])]);
    assert_eq!(r.patterns.into_iter().collect::<BTreeSet<_>>(), set(&["**/a/**", "**/a"]));
    assert_eq!(flat(run("/foo/bar,/1/2").search_paths), [sp("/foo/bar", &[]), sp("/1/2", &[])]);
    assert_eq!(flat(run("/foo/bar,/foo/../foo/bar/fooar/..").search_paths), [sp("/foo/bar", &[])]);
    assert_eq!(flat(run("/foo/bar/**/*.ts").search_paths), [sp("/foo/bar", &["**/*.ts", "**/*.ts/**"])]);
    assert_eq!(flat(run("/foo/bar/*a/b/c").search_paths), [sp("/foo/bar", &["*a/b/c", "*a/b/c/**"])]);
    assert_eq!(flat(run("/*a/b/c").search_paths), [sp("/", &["*a/b/c", "*a/b/c/**"])]);
    assert_eq!(flat(run("/foo/{b,c}ar").search_paths), [sp("/foo", &["{b,c}ar", "{b,c}ar/**"])]);
}

// upstream: QueryBuilder > parseSearchPaths 1 > relative includes w/single root folder
#[test]
fn parse_search_paths_relative_single_root() {
    let root = PathBuf::from("/foo/root1");
    let run = |s: &str| flat(parse_search_paths(&root, &split_pattern_input(s)).search_paths);
    assert_eq!(run("./a"), [sp("/foo/root1", &["a", "a/**"])]);
    assert_eq!(run("./a/"), [sp("/foo/root1", &["a", "a/**"])]);
    assert_eq!(run("./a/*b/c"), [sp("/foo/root1", &["a/*b/c", "a/*b/c/**"])]);
    assert_eq!(
        run("./a/*b/c, /project/foo"),
        [sp("/foo/root1", &["a/*b/c", "a/*b/c/**"]), sp("/project/foo", &[])]
    );
    assert_eq!(run("./a/b/,./c/d"), [sp("/foo/root1", &["a/b", "a/b/**", "c/d", "c/d/**"])]);
    assert_eq!(run("../"), [sp("/foo", &[])]);
    assert_eq!(run(".."), [sp("/foo", &[])]);
    assert_eq!(run("..\\bar"), [sp("/foo/bar", &[])]);
}
