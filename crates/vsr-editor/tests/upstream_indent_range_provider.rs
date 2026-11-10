//! Folding regions from indentation and region markers, ported from upstream's
//! `indentRangeProvider.test.ts`. All 26 tests (both suites) are ported.
//! upstream: src/vs/editor/contrib/folding/test/browser/indentRangeProvider.test.ts @ 50e1350fc78d

use vsr_editor::folding::{FoldingMarkers, MAX_FOLDING_REGIONS_FOR_INDENT_DEFAULT, compute_ranges};
use vsr_text::{DefaultEol, TextBuffer};

/// (start line, end line, parent index or -1)
type R = (u32, u32, i64);

fn r(start: u32, end: u32, parent: i64) -> R {
    (start, end, parent)
}

fn assert_ranges(lines: &[&str], expected: &[R], offside: bool, markers: Option<&FoldingMarkers>) {
    let model = TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true);
    let actual = compute_ranges(&model, 4, offside, markers, MAX_FOLDING_REGIONS_FOR_INDENT_DEFAULT);
    let got: Vec<R> = (0..actual.len())
        .map(|i| (actual.start_line_number(i), actual.end_line_number(i), actual.parent_index(i).map_or(-1, |p| p as i64)))
        .collect();
    assert_eq!(got, expected);
}

// upstream: Indentation Folding > Fold one level
#[test]
fn fold_one_level() {
    let range = ["A", "  A", "  A", "  A"];
    assert_ranges(&range, &[r(1, 4, -1)], true, None);
    assert_ranges(&range, &[r(1, 4, -1)], false, None);
}

// upstream: Indentation Folding > Fold two levels
#[test]
fn fold_two_levels() {
    let range = ["A", "  A", "  A", "    A", "    A"];
    assert_ranges(&range, &[r(1, 5, -1), r(3, 5, 0)], true, None);
    assert_ranges(&range, &[r(1, 5, -1), r(3, 5, 0)], false, None);
}

// upstream: Indentation Folding > Fold three levels
#[test]
fn fold_three_levels() {
    let range = ["A", "  A", "    A", "      A", "A"];
    assert_ranges(&range, &[r(1, 4, -1), r(2, 4, 0), r(3, 4, 1)], true, None);
    assert_ranges(&range, &[r(1, 4, -1), r(2, 4, 0), r(3, 4, 1)], false, None);
}

// upstream: Indentation Folding > Fold decreasing indent
#[test]
fn fold_decreasing_indent() {
    let range = ["    A", "  A", "A"];
    assert_ranges(&range, &[], true, None);
    assert_ranges(&range, &[], false, None);
}

// upstream: Indentation Folding > Fold Java
#[test]
fn fold_java() {
    assert_ranges(
        &[
            "class A {",
            "  void foo() {",
            "    console.log();",
            "    console.log();",
            "  }",
            "",
            "  void bar() {",
            "    console.log();",
            "  }",
            "}",
            "interface B {",
            "  void bar();",
            "}",
        ],
        &[r(1, 9, -1), r(2, 4, 0), r(7, 8, 0), r(11, 12, -1)],
        false,
        None,
    );
}

// upstream: Indentation Folding > Fold Javadoc
#[test]
fn fold_javadoc() {
    assert_ranges(
        &["/**", " * Comment", " */", "class A {", "  void foo() {", "  }", "}"],
        &[r(1, 3, -1), r(4, 6, -1)],
        false,
        None,
    );
}

// upstream: Indentation Folding > Fold Whitespace Java
#[test]
fn fold_whitespace_java() {
    assert_ranges(
        &["class A {", "", "  void foo() {", "     ", "     return 0;", "  }", "      ", "}"],
        &[r(1, 7, -1), r(3, 5, 0)],
        false,
        None,
    );
}

// upstream: Indentation Folding > Fold Whitespace Python
#[test]
fn fold_whitespace_python() {
    assert_ranges(
        &["def a:", "  pass", "   ", "  def b:", "    pass", "  ", "      ", "def c: # since there was a deintent here"],
        &[r(1, 5, -1), r(4, 5, 0)],
        true,
        None,
    );
}

// upstream: Indentation Folding > Fold Tabs
#[test]
fn fold_tabs() {
    assert_ranges(
        &["class A {", "\t\t", "\tvoid foo() {", "\t \t//hello", "\t    return 0;", "  \t}", "      ", "}"],
        &[r(1, 7, -1), r(3, 5, 0)],
        false,
        None,
    );
}

fn markers() -> FoldingMarkers {
    FoldingMarkers::new(r"^\s*#region\b", r"^\s*#endregion\b").unwrap()
}

// upstream: Folding with regions > Inside region, indented
#[test]
fn inside_region_indented() {
    assert_ranges(
        &["class A {", "  #region", "  void foo() {", "     ", "     return 0;", "  }", "  #endregion", "}"],
        &[r(1, 7, -1), r(2, 7, 0), r(3, 5, 1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Inside region, not indented
#[test]
fn inside_region_not_indented() {
    assert_ranges(
        &["var x;", "#region", "void foo() {", "     ", "     return 0;", "  }", "#endregion", ""],
        &[r(2, 7, -1), r(3, 6, 0)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Empty Regions
#[test]
fn empty_regions() {
    assert_ranges(
        &["var x;", "#region", "#endregion", "#region", "", "#endregion", "var y;"],
        &[r(2, 3, -1), r(4, 6, -1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Nested Regions
#[test]
fn nested_regions() {
    assert_ranges(
        &["var x;", "#region", "#region", "", "#endregion", "#endregion", "var y;"],
        &[r(2, 6, -1), r(3, 5, 0)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Nested Regions 2
#[test]
fn nested_regions_2() {
    assert_ranges(
        &["class A {", "  #region", "", "  #region", "", "  #endregion", "  // comment", "  #endregion", "}"],
        &[r(1, 8, -1), r(2, 8, 0), r(4, 6, 1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Incomplete Regions
#[test]
fn incomplete_regions() {
    assert_ranges(&["class A {", "#region", "  // comment", "}"], &[r(2, 3, -1)], false, Some(&markers()));
}

// upstream: Folding with regions > Incomplete Regions 2
#[test]
fn incomplete_regions_2() {
    assert_ranges(
        &["", "#region", "#region", "#region", "  // comment", "#endregion", "#endregion", " // hello"],
        &[r(3, 7, -1), r(4, 6, 0)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Indented region before
#[test]
fn indented_region_before() {
    assert_ranges(
        &["if (x)", "  return;", "", "#region", "  // comment", "#endregion"],
        &[r(1, 3, -1), r(4, 6, -1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Indented region before 2
#[test]
fn indented_region_before_2() {
    assert_ranges(
        &["if (x)", "  log();", "", "    #region", "      // comment", "    #endregion"],
        &[r(1, 6, -1), r(2, 6, 0), r(4, 6, 1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Indented region in-between
#[test]
fn indented_region_in_between() {
    assert_ranges(
        &["#region", "  // comment", "  if (x)", "    return;", "", "#endregion"],
        &[r(1, 6, -1), r(3, 5, 0)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Indented region after
#[test]
fn indented_region_after() {
    assert_ranges(
        &["#region", "  // comment", "", "#endregion", "  if (x)", "    return;"],
        &[r(1, 4, -1), r(5, 6, -1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > With off-side
#[test]
fn with_off_side() {
    assert_ranges(&["#region", "  ", "", "#endregion", ""], &[r(1, 4, -1)], true, Some(&markers()));
}

// upstream: Folding with regions > Nested with off-side
#[test]
fn nested_with_off_side() {
    assert_ranges(
        &["#region", "  ", "#region", "", "#endregion", "", "#endregion", ""],
        &[r(1, 7, -1), r(3, 5, 0)],
        true,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Issue 35981
#[test]
fn issue_35981() {
    assert_ranges(
        &[
            "function thisFoldsToEndOfPage() {",
            "  const variable = []",
            "    // #region",
            "    .reduce((a, b) => a,[]);",
            "}",
            "",
            "function thisFoldsProperly() {",
            "  const foo = \"bar\"",
            "}",
        ],
        &[r(1, 4, -1), r(2, 4, 0), r(7, 8, -1)],
        false,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Misspelled Markers
#[test]
fn misspelled_markers() {
    assert_ranges(
        &["#Region", "#endregion", "#regionsandmore", "#endregion", "#region", "#end region", "#region", "#endregionff"],
        &[],
        true,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Issue 79359
#[test]
fn issue_79359() {
    assert_ranges(
        &["#region", "", "class A", "  foo", "", "class A", "  foo", "", "#endregion"],
        &[r(1, 9, -1), r(3, 4, 0), r(6, 7, 0)],
        true,
        Some(&markers()),
    );
}

// upstream: Folding with regions > Markers with stateful flags
// (JavaScript's `g` flag has no counterpart; the `i` flag is what the cases tell apart.)
#[test]
fn markers_with_stateful_flags() {
    let m = |si, ei| FoldingMarkers::with_flags(r"^\s*#region\b", si, r"^\s*#endregion\b", ei).unwrap();
    assert_ranges(&["#region", "content", "#endregion"], &[r(1, 3, -1)], false, Some(&m(false, false)));
    assert_ranges(&["#REGION", "content", "#endregion"], &[r(1, 3, -1)], false, Some(&m(true, false)));
    assert_ranges(&["#REGION", "content", "#ENDREGION"], &[], false, Some(&m(true, false)));
    assert_ranges(&["#REGION", "content", "#ENDREGION"], &[], false, Some(&m(false, true)));
}
