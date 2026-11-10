//! Folding in the editing core: collapsed regions hide lines in the view-line projection, the caret
//! stays out of them, and edits keep or reveal them (ADR-0042).
//!
//! upstream: src/vs/editor/contrib/folding/browser/folding.ts @ 50e1350fc78d

use vsr_editor::{Editor, EditorOptions, FoldAction, FoldingParams, Position};
use vsr_text::{DefaultEol, TextBuffer};

const CODE: &str = "fn a() {\n    let x = 1;\n    let y = 2;\n}\nfn b() {\n    if x {\n        z();\n    }\n}\nend";

fn editor(text: &str) -> Editor {
    let e = Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    e.set_folding(Some(FoldingParams { off_side: false, markers: None, max_regions: 5000 }));
    e
}

fn caret(e: &mut Editor, line: usize, col: usize) {
    e.set_cursor(Position::new(line, col), false);
}

#[test]
fn fold_hides_lines_and_unfold_restores() {
    let mut e = editor(CODE);
    assert_eq!(e.view_line_count(), 10);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    assert_eq!(e.view_line_count(), 8, "lines 2-3 are hidden");
    assert_eq!(e.hidden_ranges(), [(2, 3)]);
    let v = e.view_lines().unwrap();
    assert_eq!(v.view_line(1).model_line, 4, "the row after the header shows line 4");
    drop(v);
    e.fold_action(FoldAction::Unfold);
    assert_eq!(e.view_line_count(), 10);
    assert!(e.view_lines().is_none(), "no projection needed without folds or wrapping");
}

#[test]
fn fold_all_and_unfold_all() {
    let mut e = editor(CODE);
    e.fold_action(FoldAction::FoldAll);
    // Regions: (1,3), (5,8), (6,7): hidden 2-3 and 6-8.
    assert_eq!(e.hidden_ranges(), [(2, 3), (6, 8)]);
    assert_eq!(e.view_line_count(), 5);
    e.fold_action(FoldAction::UnfoldAll);
    assert_eq!(e.view_line_count(), 10);
}

#[test]
fn fold_level_one_folds_the_top_level() {
    let mut e = editor(CODE);
    caret(&mut e, 10, 1); // regions that contain the caret line are left alone
    e.fold_action(FoldAction::FoldLevel(1));
    assert_eq!(e.hidden_ranges(), [(2, 3), (6, 8)]);
}

#[test]
fn folding_off_shows_everything() {
    let mut e = editor(CODE);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    assert_eq!(e.view_line_count(), 8);
    e.set_folding(None);
    assert_eq!(e.view_line_count(), 10);
    assert!(!e.fold_action(FoldAction::Fold), "folding is off");
}

#[test]
fn caret_inside_a_folded_region_moves_to_the_header_end() {
    let mut e = editor(CODE);
    caret(&mut e, 2, 5);
    e.fold_action(FoldAction::Fold);
    assert_eq!(e.selection().active, Position::new(1, 9), "end of `fn a() {{`");
}

#[test]
fn arrow_keys_skip_the_fold() {
    let mut e = editor(CODE);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    caret(&mut e, 1, 9);
    e.move_right(false);
    assert_eq!(e.selection().active, Position::new(4, 1), "right at the end of the header goes past the fold");
    e.move_left(false);
    assert_eq!(e.selection().active, Position::new(1, 9));
    e.move_down(false);
    assert_eq!(e.selection().active.line, 4);
    e.move_up(false);
    assert_eq!(e.selection().active.line, 1);
}

#[test]
fn editing_inside_a_fold_unfolds_it() {
    let mut e = editor(CODE);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    // Enter at the end of the folded header inserts a line inside the region.
    caret(&mut e, 1, 9);
    e.enter();
    assert_eq!(e.hidden_ranges(), [], "the caret landed in the region, which unfolds");
    assert_eq!(e.view_line_count(), 11);
}

#[test]
fn edits_above_a_fold_move_it() {
    let mut e = editor(CODE);
    caret(&mut e, 5, 1);
    e.fold_action(FoldAction::Fold);
    assert_eq!(e.hidden_ranges(), [(6, 8)]);
    caret(&mut e, 1, 1);
    e.enter();
    assert_eq!(e.hidden_ranges(), [(7, 9)], "still folded, one line lower");
}

#[test]
fn a_selection_that_starts_in_a_fold_reveals_it() {
    let mut e = editor(CODE);
    caret(&mut e, 5, 1);
    e.fold_action(FoldAction::Fold);
    e.set_selection(vsr_editor::Selection::caret(Position::new(7, 3)));
    assert_eq!(e.hidden_ranges(), []);
}

#[test]
fn region_markers_fold_in_a_rust_file() {
    let text = "// #region first\nlet a = 1;\nlet b = 2;\n// #endregion\nlet c = 3;\n";
    let e = Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    e.set_folding(Some(FoldingParams {
        off_side: false,
        markers: Some((r"^\s*//\s*#?region\b".into(), r"^\s*//\s*#?endregion\b".into())),
        max_regions: 5000,
    }));
    assert_eq!(e.fold_regions(), [(1, 4, false)]);
    let mut e = e;
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    assert_eq!(e.view_line_count(), 3);
    e.fold_action(FoldAction::Unfold);
    e.fold_action(FoldAction::FoldAllMarkerRegions);
    assert_eq!(e.hidden_ranges(), [(2, 4)]);
}

#[test]
fn folds_compose_with_word_wrap() {
    use vsr_editor::wrap::{WrapParams, WrappingIndent};
    let long = "x".repeat(25);
    let text = format!("fn a() {{\n    {long}\n    {long}\n}}\nend");
    let mut e = editor(&text);
    e.set_wrapping(Some(WrapParams::monospace(4, 10, WrappingIndent::None)));
    let before = e.view_line_count();
    assert!(before > 5);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    assert_eq!(e.view_line_count(), 3, "the header, the closing brace and `end`");
}

#[test]
fn toggle_by_click_and_recursive_click() {
    let mut e = editor(CODE);
    assert!(e.toggle_fold_at(5, false, false));
    assert_eq!(e.hidden_ranges(), [(6, 8)]);
    e.toggle_fold_at(5, false, false);
    assert_eq!(e.hidden_ranges(), []);
    // A shift click folds the children first.
    e.toggle_fold_at(5, true, false);
    assert_eq!(e.fold_regions().iter().filter(|r| r.2).count(), 1);
    assert_eq!(e.fold_at_line(6).map(|f| f.collapsed), Some(true));
}

#[test]
fn sticky_lines_are_the_headers_above_the_viewport() {
    let e = editor(CODE);
    assert_eq!(e.sticky_lines(0, 5), Vec::<usize>::new(), "scrolled to the top");
    assert_eq!(e.sticky_lines(1, 5), [1], "line 2 is inside `fn a`");
    assert_eq!(e.sticky_lines(6, 5), [5, 6], "line 7 is inside `fn b` and `if x`");
    assert_eq!(e.sticky_lines(6, 1), [5], "at most maxLineCount, outermost first");
    assert_eq!(e.sticky_lines(4, 5), Vec::<usize>::new(), "a header itself on top needs no sticky line");
    assert_eq!(e.sticky_lines(9, 5), Vec::<usize>::new(), "past the regions");
    e.set_folding(None);
    assert_eq!(e.sticky_lines(6, 5), Vec::<usize>::new(), "no folding regions without folding");
}

#[test]
fn sticky_lines_account_for_hidden_lines() {
    let mut e = editor(CODE);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold); // hides lines 2-3: row 1 is now line 4
    assert_eq!(e.sticky_lines(1, 5), Vec::<usize>::new(), "row 1 is the closing brace of the folded region");
}

#[test]
fn undo_after_unfold_by_edit_keeps_the_text_right() {
    let mut e = editor(CODE);
    caret(&mut e, 1, 1);
    e.fold_action(FoldAction::Fold);
    caret(&mut e, 1, 9);
    e.enter();
    e.undo();
    assert_eq!(e.buffer().value(), CODE);
}
