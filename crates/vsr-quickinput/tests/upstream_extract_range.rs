//! upstream: src/vs/workbench/contrib/search/test/common/extractRange.test.ts @ 50e1350fc78d
//!
//! Ports of VS Code's `extractRangeFromFilter` tests onto
//! `vsr_quickinput::open::extract_range_from_filter`.
//!
//! Conventions:
//! - Upstream `range.startLineNumber` / `startColumn` / `endLineNumber` / `endColumn` are the
//!   `start_line` / `start_column` / `end_line` / `end_column` fields of [`Range`] (u32, 1-based).
//! - Upstream `extractRangeFromFilter(filter, unless?)` takes `unless` as a slice; an absent
//!   `unless` is an empty slice.
//! - `undefined` results are `None`; `{ filter, range }` results are `Some(FilterAndRange)`.
//! - Upstream's `ensureNoDisposablesAreLeakedInTestSuite` has no counterpart and is not ported.
//! - The generated tests (`test(filter, ...)` in suite `ranges`, and `test(\`${filter} - ${unless}\`)`
//!   in suite `unless`) are each one Rust test running a loop over the same specs. The comment on
//!   each names the upstream suite it covers.

use vsr_quickinput::open::{FilterAndRange, Range, extract_range_from_filter};

/// A range from the four upstream fields, in the order `startLine, startColumn, endLine, endColumn`.
fn range(start_line: u32, start_column: u32, end_line: u32, end_column: u32) -> Range {
    Range { start_line, start_column, end_line, end_column }
}

// upstream: extractRangeFromFilter > basics
#[test]
fn basics() {
    assert!(extract_range_from_filter("", &[]).is_none());
    assert!(extract_range_from_filter("/some/path", &[]).is_none());
    assert!(extract_range_from_filter("/some/path/file.txt", &[]).is_none());

    let base = "/some/path/file.txt";
    for line_sep in [":", "#", "(", ":line "] {
        for col_sep in [":", "#", ","] {
            let res = extract_range_from_filter(&format!("{base}{line_sep}20"), &[]).expect("line only");
            assert_eq!(res.filter, base);
            assert_eq!(res.range.start_line, 20);
            assert_eq!(res.range.start_column, 1);

            let res = extract_range_from_filter(&format!("{base}{line_sep}20{col_sep}"), &[]).expect("dangling column separator");
            assert_eq!(res.filter, base);
            assert_eq!(res.range.start_line, 20);
            assert_eq!(res.range.start_column, 1);

            let res = extract_range_from_filter(&format!("{base}{line_sep}20{col_sep}3"), &[]).expect("line and column");
            assert_eq!(res.filter, base);
            assert_eq!(res.range.start_line, 20);
            assert_eq!(res.range.start_column, 3);
        }
    }
}

// upstream: extractRangeFromFilter > allow space after path
#[test]
fn allow_space_after_path() {
    let res = extract_range_from_filter("/some/path/file.txt (19,20)", &[]).expect("space before paren");

    assert_eq!(res.filter, "/some/path/file.txt");
    assert_eq!(res.range.start_line, 19);
    assert_eq!(res.range.start_column, 20);
}

// Generated tests in upstream suite `extractRangeFromFilter > ranges`: one per spec below,
// title = the filter. Each must give `{ filter: base, range }`.
// (dynamic titles: no per-test upstream ref)
#[test]
fn ranges_generated_specs() {
    let base = "/some/path/file.txt";
    let specs = [
        // line range: "20-40"
        (format!("{base}:20-40"), range(20, 1, 40, 1)),
        // line and column range: "20:3-40:5"
        (format!("{base}:20:3-40:5"), range(20, 3, 40, 5)),
        // end column defaults to start of the end line: "20:3-40"
        (format!("{base}:20:3-40"), range(20, 3, 40, 1)),
        // mixed separators: "20#3-40,5"
        (format!("{base}#20#3-40,5"), range(20, 3, 40, 5)),
        // paren style: "(20,3-40,5)"
        (format!("{base}(20,3-40,5)"), range(20, 3, 40, 5)),
        // dangling separator falls back to single line: "20-"
        (format!("{base}:20-"), range(20, 1, 20, 1)),
    ];
    for (filter, range) in specs {
        assert_eq!(
            extract_range_from_filter(&filter, &[]),
            Some(FilterAndRange { filter: base.to_string(), range }),
            "filter {filter:?}"
        );
    }
}

// upstream: extractRangeFromFilter > ranges > hyphen in path is not treated as a range
#[test]
fn hyphen_in_path_is_not_treated_as_a_range() {
    assert!(extract_range_from_filter("/some/path/my-file.txt", &[]).is_none());
    assert!(extract_range_from_filter("/some/path/file-2.txt", &[]).is_none());
}

// Generated tests in upstream suite `extractRangeFromFilter > unless`: one per spec below, title =
// `${filter} - ${JSON.stringify(unless)}`. Each must give the listed result.
// (dynamic titles: no per-test upstream ref)
#[test]
fn unless_generated_specs() {
    let specs: Vec<(&str, &[&str], Option<FilterAndRange>)> = vec![
        // alpha-only symbol after unless
        ("/some/path/file.txt@alphasymbol", &["@"], None),
        // unless as first char
        ("@/some/path/file.txt (19,20)", &["@"], None),
        // unless as last char
        ("/some/path/file.txt (19,20)@", &["@"], None),
        // unless before ,
        (
            "/some/@path/file.txt (19,20)",
            &["@"],
            Some(FilterAndRange { filter: "/some/@path/file.txt".into(), range: range(19, 20, 19, 20) }),
        ),
        // unless before :
        (
            "/some/@path/file.txt:19:20",
            &["@"],
            Some(FilterAndRange { filter: "/some/@path/file.txt".into(), range: range(19, 20, 19, 20) }),
        ),
        // unless before #
        (
            "/some/@path/file.txt#19",
            &["@"],
            Some(FilterAndRange { filter: "/some/@path/file.txt".into(), range: range(19, 1, 19, 1) }),
        ),
    ];
    for (filter, unless, result) in specs {
        assert_eq!(extract_range_from_filter(filter, unless), result, "filter {filter:?} unless {unless:?}");
    }
}
