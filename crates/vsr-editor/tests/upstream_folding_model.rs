//! The folding model, ported from upstream's `foldingModel.test.ts`: 21 of the 22 tests.
//! upstream: src/vs/editor/contrib/folding/test/browser/foldingModel.test.ts @ 50e1350fc78d
//!
//! Not ported: `folding decoration` (it asserts the model decorations that upstream attaches to
//! the regions; rscode has none, see `vsr_editor::folding::LineMap`). `update` and `delete`, which
//! rely on those decorations to follow an edit, are ported with a `LineMap` computed from the
//! edit.

use regex::Regex;
use vsr_editor::folding::*;
use vsr_text::{DefaultEol, Position, Range, TextBuffer};

type R = (u32, u32, bool);

fn r(start: u32, end: u32, collapsed: bool) -> R {
    (start, end, collapsed)
}

fn buffer(lines: &[&str]) -> TextBuffer {
    TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true)
}

fn markers_region() -> FoldingMarkers {
    FoldingMarkers::new(r"^//#region$", r"^//#endregion$").unwrap()
}

fn model_for(buf: &TextBuffer, off_side: bool, markers: Option<&FoldingMarkers>) -> FoldingModel {
    let mut m = FoldingModel::new();
    m.update(&compute_ranges(buf, 4, off_side, markers, 5000), None, buf.line_count(), None);
    m
}

fn ranges(m: &FoldingModel) -> Vec<R> {
    let a = m.regions();
    (0..a.len()).map(|i| r(a.start_line_number(i), a.end_line_number(i), a.is_collapsed(i))).collect()
}

fn folded(m: &FoldingModel) -> Vec<R> {
    ranges(m).into_iter().filter(|x| x.2).map(|(s, e, _)| r(s, e, false)).collect()
}

fn regions(rs: &[FoldingRegion]) -> Vec<R> {
    rs.iter().map(|x| r(x.start_line_number, x.end_line_number, x.is_collapsed)).collect()
}

fn toggle(m: &mut FoldingModel, lines: &[u32]) {
    let rs: Vec<_> = lines.iter().map(|&l| m.get_region_at_line(l).unwrap()).collect();
    m.toggle_collapse_state(&rs);
}

const NESTED: [&str; 13] = [
    "//#region",
    "//#endregion",
    "class A {",
    "  void foo() {",
    "    if (true) {",
    "        return;",
    "    }",
    "",
    "    if (true) {",
    "      return;",
    "    }",
    "  }",
    "}",
];

fn nested_model() -> (TextBuffer, FoldingModel) {
    let buf = buffer(&NESTED);
    let m = model_for(&buf, false, Some(&markers_region()));
    (buf, m)
}

// upstream: Folding Model > getRegionAtLine
#[test]
fn get_region_at_line() {
    let buf = buffer(&["/**", " * Comment", " */", "class A {", "  void foo() {", "    // comment {", "  }", "}"]);
    let m = model_for(&buf, false, None);
    let (r1, r2, r3) = (r(1, 3, false), r(4, 7, false), r(5, 6, false));
    assert_eq!(ranges(&m), [r1, r2, r3]);
    let at = |l: u32| m.get_region_at_line(l).map(|x| r(x.start_line_number, x.end_line_number, x.is_collapsed));
    assert_eq!(at(1), Some(r1), "1");
    assert_eq!(at(2), Some(r1), "2");
    assert_eq!(at(3), Some(r1), "3");
    assert_eq!(at(4), Some(r2), "4");
    assert_eq!(at(5), Some(r3), "5");
    assert_eq!(at(6), Some(r3), "5");
    assert_eq!(at(7), Some(r2), "6");
    assert_eq!(at(8), None, "7");
}

// upstream: Folding Model > collapse
#[test]
fn collapse() {
    let buf = buffer(&["/**", " * Comment", " */", "class A {", "  void foo() {", "    // comment {", "  }", "}"]);
    let mut m = model_for(&buf, false, None);
    let fresh = compute_ranges(&buf, 4, false, None, 5000);
    let (r1, r2, r3) = (r(1, 3, false), r(4, 7, false), r(5, 6, false));
    assert_eq!(ranges(&m), [r1, r2, r3]);

    toggle(&mut m, &[1]);
    m.update(&fresh, None, buf.line_count(), None);
    assert_eq!(ranges(&m), [r(1, 3, true), r2, r3]);

    toggle(&mut m, &[5]);
    m.update(&fresh, None, buf.line_count(), None);
    assert_eq!(ranges(&m), [r(1, 3, true), r2, r(5, 6, true)]);

    toggle(&mut m, &[7]);
    m.update(&fresh, None, buf.line_count(), None);
    assert_eq!(ranges(&m), [r(1, 3, true), r(4, 7, true), r(5, 6, true)]);
}

// upstream: Folding Model > update
#[test]
fn update() {
    let mut buf = buffer(&["/**", " * Comment", " */", "class A {", "  void foo() {", "    // comment {", "  }", "}"]);
    let mut m = model_for(&buf, false, None);
    let (r1, r2, r3) = (r(1, 3, false), r(4, 7, false), r(5, 6, false));
    assert_eq!(ranges(&m), [r1, r2, r3]);
    toggle(&mut m, &[2, 5]);

    buf.insert_at_offset(buf.offset_at(Position::new(4, 1)), "//hello\n");
    // One line was inserted at line 4: lines 1-3 and 4-8 (now 5-9) are unchanged.
    let map = LineMap { pre: 3, old_len: 0, new_len: 1, suf: 5 };
    m.update(&compute_ranges(&buf, 4, false, None, 5000), None, buf.line_count(), Some(&map));
    assert_eq!(ranges(&m), [r(1, 3, true), r(5, 8, false), r(6, 7, true)]);
}

// upstream: Folding Model > delete
#[test]
fn delete() {
    let mut buf = buffer(&[
        "function foo() {",
        "  switch (x) {",
        "    case 1:",
        "      //hello1",
        "      break;",
        "    case 2:",
        "      //hello2",
        "      break;",
        "    case 3:",
        "      //hello3",
        "      break;",
        "  }",
        "}",
    ]);
    let mut m = model_for(&buf, false, None);
    let (r1, r2, r3, r4, r5) = (r(1, 12, false), r(2, 11, false), r(3, 5, false), r(6, 8, false), r(9, 11, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    toggle(&mut m, &[6]);

    let from = buf.offset_at(Position::new(6, 11));
    let to = buf.offset_at(Position::new(9, 1));
    buf.delete_at_offset(from, to - from);
    // Lines 6-9 became line 6; lines 1-5 and 10-13 (now 7-10) are unchanged... except line 9 merged.
    let map = LineMap { pre: 5, old_len: 4, new_len: 1, suf: 4 };
    m.update(&compute_ranges(&buf, 4, true, None, 5000), Some(&SelectedLines(vec![7])), buf.line_count(), Some(&map));
    assert_eq!(ranges(&m), [r(1, 9, false), r(2, 8, false), r(3, 5, false), r(6, 8, false)]);
}

// upstream: Folding Model > getRegionsInside
#[test]
fn get_regions_inside() {
    let buf = buffer(&["/**", " * Comment", " */", "class A {", "  void foo() {", "    // comment {", "  }", "}"]);
    let m = model_for(&buf, false, None);
    let (r1, r2, r3) = (r(1, 3, false), r(4, 7, false), r(5, 6, false));
    assert_eq!(ranges(&m), [r1, r2, r3]);
    let region = |l: u32| m.get_region_at_line(l).unwrap();
    assert_eq!(regions(&m.get_regions_inside(None, None)), [r1, r2, r3], "1");
    assert_eq!(regions(&m.get_regions_inside(Some(&region(r1.0)), None)), [], "2");
    assert_eq!(regions(&m.get_regions_inside(Some(&region(r2.0)), None)), [r3], "3");
    assert_eq!(regions(&m.get_regions_inside(Some(&region(r3.0)), None)), [], "4");
}

// upstream: Folding Model > getRegionsInsideWithLevel
#[test]
fn get_regions_inside_with_level() {
    let buf = buffer(&[
        "//#region",
        "//#endregion",
        "class A {",
        "  void foo() {",
        "    if (true) {",
        "        return;",
        "    }",
        "    if (true) {",
        "      return;",
        "    }",
        "  }",
        "}",
    ]);
    let m = model_for(&buf, false, Some(&markers_region()));
    let (r1, r2, r3, r4, r5) = (r(1, 2, false), r(3, 11, false), r(4, 10, false), r(5, 6, false), r(8, 9, false));
    let region = |l: u32| m.get_region_at_line(l).unwrap();
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    let at_level = |from: Option<&FoldingRegion>, lvl: usize| regions(&m.get_regions_inside_with_level(from, &|_, level| level == lvl));
    assert_eq!(at_level(None, 1), [r1, r2], "1");
    assert_eq!(at_level(None, 2), [r3], "2");
    assert_eq!(at_level(None, 3), [r4, r5], "3");
    assert_eq!(at_level(Some(&region(r2.0)), 1), [r3], "4");
    assert_eq!(at_level(Some(&region(r2.0)), 2), [r4, r5], "5");
    assert_eq!(at_level(Some(&region(r3.0)), 1), [r4, r5], "6");
    assert_eq!(regions(&m.get_regions_inside_with_level(Some(&region(r2.0)), &|r, _| r.hides_line(9))), [r3, r5], "7");
    assert_eq!(at_level(Some(&region(r1.0)), 1), [], "8");
}

// upstream: Folding Model > getRegionAtLine2
#[test]
fn get_region_at_line_2() {
    let buf = buffer(&[
        "//#region",
        "class A {",
        "  void foo() {",
        "    if (true) {",
        "      //hello",
        "    }",
        "",
        "  }",
        "}",
        "//#endregion",
        "",
    ]);
    let m = model_for(&buf, false, Some(&markers_region()));
    let (r1, r2, r3, r4) = (r(1, 10, false), r(2, 8, false), r(3, 7, false), r(4, 5, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4]);
    let all = |l: u32| regions(&m.get_all_regions_at_line(l, None));
    assert_eq!(all(1), [r1], "1");
    assert_eq!(all(2), [r2, r1], "2");
    assert_eq!(all(3), [r3, r2, r1], "3");
    assert_eq!(all(4), [r4, r3, r2, r1], "4");
    assert_eq!(all(5), [r4, r3, r2, r1], "5");
    assert_eq!(all(6), [r3, r2, r1], "6");
    assert_eq!(all(7), [r3, r2, r1], "7");
    assert_eq!(all(8), [r2, r1], "8");
    assert_eq!(all(9), [r1], "9");
    assert_eq!(all(10), [r1], "10");
    assert_eq!(all(11), [], "10");
}

// upstream: Folding Model > setCollapseStateRecursivly
#[test]
fn set_collapse_state_recursively() {
    let (_, mut m) = nested_model();
    let (r1, r2, r3, r4, r5) = (r(1, 2, false), r(3, 12, false), r(4, 11, false), r(5, 6, false), r(9, 10, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    set_collapse_state_levels_down(&mut m, true, usize::MAX, Some(&[4]));
    assert_eq!(folded(&m), [r3, r4, r5], "1");
    set_collapse_state_levels_down(&mut m, false, usize::MAX, Some(&[8]));
    assert_eq!(folded(&m), [], "2");
    set_collapse_state_levels_down(&mut m, true, usize::MAX, Some(&[12]));
    assert_eq!(folded(&m), [r2, r3, r4, r5], "1");
    set_collapse_state_levels_down(&mut m, false, usize::MAX, Some(&[7]));
    assert_eq!(folded(&m), [r2], "1");
    set_collapse_state_levels_down(&mut m, false, usize::MAX, None);
    assert_eq!(folded(&m), [], "1");
    set_collapse_state_levels_down(&mut m, true, usize::MAX, None);
    assert_eq!(folded(&m), [r1, r2, r3, r4, r5], "1");
}

// upstream: Folding Model > setCollapseStateAtLevel
#[test]
fn set_collapse_state_at_level_test() {
    let buf = buffer(&[
        "//#region",
        "//#endregion",
        "class A {",
        "  void foo() {",
        "    if (true) {",
        "        return;",
        "    }",
        "",
        "    if (true) {",
        "      return;",
        "    }",
        "  }",
        "  //#region",
        "  const bar = 9;",
        "  //#endregion",
        "}",
    ]);
    let markers = FoldingMarkers::new(r"^\s*//#region$", r"^\s*//#endregion$").unwrap();
    let mut m = model_for(&buf, false, Some(&markers));
    let (r1, r2, r3, r4, r5, r6) =
        (r(1, 2, false), r(3, 15, false), r(4, 11, false), r(5, 6, false), r(9, 10, false), r(13, 15, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5, r6]);
    set_collapse_state_at_level(&mut m, 1, true, &[]);
    assert_eq!(folded(&m), [r1, r2], "1");
    set_collapse_state_at_level(&mut m, 1, false, &[5]);
    assert_eq!(folded(&m), [r2], "2");
    set_collapse_state_at_level(&mut m, 1, false, &[1]);
    assert_eq!(folded(&m), [], "3");
    set_collapse_state_at_level(&mut m, 2, true, &[]);
    assert_eq!(folded(&m), [r3, r6], "4");
    set_collapse_state_at_level(&mut m, 2, false, &[5, 6]);
    assert_eq!(folded(&m), [r3], "5");
    set_collapse_state_at_level(&mut m, 3, true, &[4, 9]);
    assert_eq!(folded(&m), [r3, r4], "6");
    set_collapse_state_at_level(&mut m, 3, false, &[4, 9]);
    assert_eq!(folded(&m), [r3], "7");
}

// upstream: Folding Model > setCollapseStateLevelsDown
#[test]
fn set_collapse_state_levels_down_test() {
    let (_, mut m) = nested_model();
    let (r1, r2, r3, r4, r5) = (r(1, 2, false), r(3, 12, false), r(4, 11, false), r(5, 6, false), r(9, 10, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    set_collapse_state_levels_down(&mut m, true, 1, Some(&[4]));
    assert_eq!(folded(&m), [r3], "1");
    set_collapse_state_levels_down(&mut m, true, 2, Some(&[4]));
    assert_eq!(folded(&m), [r3, r4, r5], "2");
    set_collapse_state_levels_down(&mut m, false, 2, Some(&[3]));
    assert_eq!(folded(&m), [r4, r5], "3");
    set_collapse_state_levels_down(&mut m, false, 2, Some(&[2]));
    assert_eq!(folded(&m), [r4, r5], "4");
    set_collapse_state_levels_down(&mut m, true, 4, Some(&[2]));
    assert_eq!(folded(&m), [r1, r4, r5], "5");
    set_collapse_state_levels_down(&mut m, false, 4, Some(&[2, 3]));
    assert_eq!(folded(&m), [], "6");
}

// upstream: Folding Model > setCollapseStateLevelsUp
#[test]
fn set_collapse_state_levels_up_test() {
    let (_, mut m) = nested_model();
    let (r1, r2, r3, r4, r5) = (r(1, 2, false), r(3, 12, false), r(4, 11, false), r(5, 6, false), r(9, 10, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    set_collapse_state_levels_up(&mut m, true, 1, &[4]);
    assert_eq!(folded(&m), [r3], "1");
    set_collapse_state_levels_up(&mut m, true, 2, &[4]);
    assert_eq!(folded(&m), [r2, r3], "2");
    set_collapse_state_levels_up(&mut m, false, 4, &[1, 3, 4]);
    assert_eq!(folded(&m), [], "3");
    set_collapse_state_levels_up(&mut m, true, 2, &[10]);
    assert_eq!(folded(&m), [r3, r5], "4");
}

// upstream: Folding Model > setCollapseStateUp
#[test]
fn set_collapse_state_up_test() {
    let (_, mut m) = nested_model();
    let (r1, r2, r3, r4, r5) = (r(1, 2, false), r(3, 12, false), r(4, 11, false), r(5, 6, false), r(9, 10, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    set_collapse_state_up(&mut m, true, &[5]);
    assert_eq!(folded(&m), [r4], "1");
    set_collapse_state_up(&mut m, true, &[5]);
    assert_eq!(folded(&m), [r3, r4], "2");
    set_collapse_state_up(&mut m, true, &[4]);
    assert_eq!(folded(&m), [r2, r3, r4], "2");
    let _ = r5;
}

// upstream: Folding Model > setCollapseStateForMatchingLines
#[test]
fn set_collapse_state_for_matching_lines_test() {
    let buf = buffer(&[
        "/**",
        " * the class",
        " */",
        "class A {",
        "  /**",
        "   * the foo",
        "   */",
        "  void foo() {",
        "    /*",
        "     * the comment",
        "     */",
        "  }",
        "}",
    ]);
    let mut m = model_for(&buf, false, Some(&markers_region()));
    let (r1, r2, r3, r4, r5) = (r(1, 3, false), r(4, 12, false), r(5, 7, false), r(8, 11, false), r(9, 11, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    let re = Regex::new(&format!("^\\s*{}", regex::escape("/*"))).unwrap();
    set_collapse_state_for_matching_lines(&mut m, &buf, &re, true);
    assert_eq!(folded(&m), [r1, r3, r5], "1");
}

// upstream: Folding Model > setCollapseStateForRest
#[test]
fn set_collapse_state_for_rest_test() {
    let (_, mut m) = nested_model();
    let (r1, r2, r3, r4, r5) = (r(1, 2, false), r(3, 12, false), r(4, 11, false), r(5, 6, false), r(9, 10, false));
    assert_eq!(ranges(&m), [r1, r2, r3, r4, r5]);
    set_collapse_state_for_rest(&mut m, true, &[5]);
    assert_eq!(folded(&m), [r1, r5], "1");
    set_collapse_state_for_rest(&mut m, false, &[5]);
    assert_eq!(folded(&m), [], "2");
    set_collapse_state_for_rest(&mut m, true, &[1]);
    assert_eq!(folded(&m), [r2, r3, r4, r5], "3");
    set_collapse_state_for_rest(&mut m, true, &[3]);
    assert_eq!(folded(&m), [r1, r2, r3, r4, r5], "3");
}

// upstream: Folding Model > fold jumping
#[test]
fn fold_jumping() {
    let buf = buffer(&[
        "class A {",
        "  void foo() {",
        "    if (1) {",
        "      a();",
        "    } else if (2) {",
        "      if (true) {",
        "        b();",
        "      }",
        "    } else {",
        "      c();",
        "    }",
        "  }",
        "}",
    ]);
    let m = model_for(&buf, false, None);
    let expect = [r(1, 12, false), r(2, 11, false), r(3, 4, false), r(5, 8, false), r(6, 7, false), r(9, 10, false)];
    assert_eq!(ranges(&m), expect);
    // Jump to parent.
    assert_eq!(get_parent_fold_line(7, &m), Some(6));
    assert_eq!(get_parent_fold_line(6, &m), Some(5));
    assert_eq!(get_parent_fold_line(5, &m), Some(2));
    assert_eq!(get_parent_fold_line(2, &m), Some(1));
    assert_eq!(get_parent_fold_line(1, &m), None);
    // Jump to previous.
    assert_eq!(get_previous_fold_line(10, &m), Some(9));
    assert_eq!(get_previous_fold_line(9, &m), Some(5));
    assert_eq!(get_previous_fold_line(5, &m), Some(3));
    assert_eq!(get_previous_fold_line(3, &m), None);
    // Not on a folding region start line.
    assert_eq!(get_previous_fold_line(4, &m), Some(3));
    assert_eq!(get_previous_fold_line(7, &m), Some(6));
    assert_eq!(get_previous_fold_line(8, &m), Some(6));
    // Jump to next.
    assert_eq!(get_next_fold_line(3, &m), Some(5));
    assert_eq!(get_next_fold_line(5, &m), Some(9));
    assert_eq!(get_next_fold_line(9, &m), None);
    // Not on a folding region start line.
    assert_eq!(get_next_fold_line(4, &m), Some(5));
    assert_eq!(get_next_fold_line(7, &m), Some(9));
    assert_eq!(get_next_fold_line(8, &m), Some(9));
}

// upstream: Folding Model > fold jumping issue #129503
#[test]
fn fold_jumping_issue_129503() {
    let buf = buffer(&["", "if True:", "  print(1)", "if True:", "  print(1)", ""]);
    let m = model_for(&buf, false, None);
    assert_eq!(ranges(&m), [r(2, 3, false), r(4, 6, false)]);
    // Jump to next.
    assert_eq!(get_next_fold_line(1, &m), Some(2));
    assert_eq!(get_next_fold_line(2, &m), Some(4));
    assert_eq!(get_next_fold_line(3, &m), Some(4));
    assert_eq!(get_next_fold_line(4, &m), None);
    assert_eq!(get_next_fold_line(5, &m), None);
    assert_eq!(get_next_fold_line(6, &m), None);
    // Jump to previous.
    assert_eq!(get_previous_fold_line(1, &m), None);
    assert_eq!(get_previous_fold_line(2, &m), None);
    assert_eq!(get_previous_fold_line(3, &m), Some(2));
    assert_eq!(get_previous_fold_line(4, &m), Some(2));
    assert_eq!(get_previous_fold_line(5, &m), Some(4));
    assert_eq!(get_previous_fold_line(6, &m), Some(4));
}

fn fr(start: u32, end: u32, source: FoldSource) -> FoldRange {
    FoldRange { start_line_number: start, end_line_number: end, kind: None, is_collapsed: false, source }
}

fn plain(rs: &[(u32, u32)]) -> Vec<R> {
    rs.iter().map(|&(s, e)| r(s, e, false)).collect()
}

fn model_with(ranges_: &[FoldRange], lines: usize) -> FoldingModel {
    let mut m = FoldingModel::new();
    m.update(&FoldingRegions::from_fold_ranges(ranges_), None, lines, None);
    m
}

// upstream: Folding Model > removeManualRanges - cursor on manual range removes innermost only
#[test]
fn remove_manual_ranges_cursor_on_manual_range_removes_innermost_only() {
    let mut m = model_with(&[fr(1, 6, FoldSource::Provider), fr(2, 5, FoldSource::UserDefined), fr(3, 4, FoldSource::UserDefined)], 7);
    assert_eq!(ranges(&m), plain(&[(1, 6), (2, 5), (3, 4)]));
    // Cursor on line 4 (inside innermost manual range 3-4): should remove only 3-4.
    m.remove_manual_ranges(&[Range::new(Position::new(4, 1), Position::new(4, 1))]);
    assert_eq!(ranges(&m), plain(&[(1, 6), (2, 5)]));
    // Cursor on line 3 (inside remaining manual range 2-5): should remove only 2-5.
    m.remove_manual_ranges(&[Range::new(Position::new(3, 1), Position::new(3, 1))]);
    assert_eq!(ranges(&m), plain(&[(1, 6)]));
}

// upstream: Folding Model > removeManualRanges - cursor skips provider ranges to remove nearest manual range
#[test]
fn remove_manual_ranges_cursor_skips_provider_ranges_to_remove_nearest_manual_range() {
    let mut m = model_with(&[fr(1, 5, FoldSource::UserDefined), fr(2, 4, FoldSource::Provider)], 5);
    m.remove_manual_ranges(&[Range::new(Position::new(3, 1), Position::new(3, 1))]);
    assert_eq!(ranges(&m), plain(&[(2, 4)]));
}

// upstream: Folding Model > removeManualRanges - cursor not on manual range removes all manual ranges
#[test]
fn remove_manual_ranges_cursor_not_on_manual_range_removes_all_manual_ranges() {
    let mut m = model_with(&[fr(2, 4, FoldSource::Provider), fr(3, 4, FoldSource::UserDefined)], 6);
    assert_eq!(ranges(&m), plain(&[(2, 4), (3, 4)]));
    // A single-line selection outside manual ranges should preserve them.
    m.remove_manual_ranges(&[Range::new(Position::new(6, 1), Position::new(6, 2))]);
    assert_eq!(ranges(&m), plain(&[(2, 4), (3, 4)]));
    // Cursor on line 6 (not inside any manual range): should remove all manual ranges.
    m.remove_manual_ranges(&[Range::new(Position::new(6, 1), Position::new(6, 1))]);
    assert_eq!(ranges(&m), plain(&[(2, 4)]));
}

// upstream: Folding Model > removeManualRanges - single-line selection removes all intersecting manual ranges
#[test]
fn remove_manual_ranges_single_line_selection_removes_all_intersecting_manual_ranges() {
    let mut m = model_with(&[fr(1, 6, FoldSource::Provider), fr(2, 5, FoldSource::UserDefined), fr(3, 4, FoldSource::UserDefined)], 7);
    m.remove_manual_ranges(&[Range::new(Position::new(4, 1), Position::new(4, 2))]);
    assert_eq!(ranges(&m), plain(&[(1, 6)]));
}

// upstream: Folding Model > removeManualRanges - selection range removes intersecting manual ranges
#[test]
fn remove_manual_ranges_selection_range_removes_intersecting_manual_ranges() {
    let mut m = model_with(&[fr(1, 5, FoldSource::Provider), fr(2, 3, FoldSource::UserDefined), fr(4, 5, FoldSource::UserDefined)], 6);
    assert_eq!(ranges(&m), plain(&[(1, 5), (2, 3), (4, 5)]));
    // Selection spanning lines 2-3: removes only the first manual range.
    m.remove_manual_ranges(&[Range::new(Position::new(2, 1), Position::new(3, 1))]);
    assert_eq!(ranges(&m), plain(&[(1, 5), (4, 5)]));
}
