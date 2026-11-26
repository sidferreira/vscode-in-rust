//! Headless end-to-end tests of the overview ruler (docs/testing.md tier 2;
//! tests/e2e-scenarios/preferences.md, "Overview ruler"). The marks and the border are found by their
//! debug selectors.
//!
//! upstream: src/vs/editor/browser/viewParts/overviewRuler/decorationsOverviewRuler.ts @ 50e1350fc78d

use gpui::TestAppContext;
use serde_json::json;

use super::scrollbar_e2e::{bar, set, settle, workbench};

const BORDER: &str = "editor-overview-border";
const CURSOR: &str = "overview-cursor";

#[gpui_kit::test]
fn the_border_is_drawn_by_default_and_removed_by_overview_ruler_border(cx: &mut TestAppContext) {
    // Upstream source: decorationsOverviewRuler.ts (`renderBorder` draws the border along the lane).
    let (wb, cx) = workbench(cx, "border", "{}", "let a = 1;");
    settle(&wb, cx);
    assert!(
        bar(cx, BORDER).is_some(),
        "editor.overviewRulerBorder defaults to true"
    );
    set(&wb, cx, "editor.overviewRulerBorder", json!(false));
    settle(&wb, cx);
    assert!(bar(cx, BORDER).is_none(), "no border when turned off");
}

#[gpui_kit::test]
fn the_cursor_is_marked_at_the_top_of_the_lane_for_a_caret_on_the_first_line(
    cx: &mut TestAppContext,
) {
    let (wb, cx) = workbench(cx, "cursor", "{}", "let a = 1;\nlet b = 2;\n");
    settle(&wb, cx);
    let lane = bar(cx, "editor-scrollbar-vertical").expect("lane");
    let mark = bar(cx, CURSOR).expect("the cursor mark");
    let top = f32::from(mark.origin.y - lane.origin.y);
    assert!(
        top.abs() < 1.0,
        "the caret is on line 1, so the mark is at the top: {top}"
    );
    assert!(
        f32::from(mark.size.height) >= 1.0,
        "a mark is at least a pixel tall"
    );
}

#[gpui_kit::test]
fn hide_cursor_in_overview_ruler_removes_the_cursor_mark(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "hide-cursor", "{}", "let a = 1;\n");
    settle(&wb, cx);
    assert!(bar(cx, CURSOR).is_some(), "the cursor is marked by default");
    set(&wb, cx, "editor.hideCursorInOverviewRuler", json!(true));
    settle(&wb, cx);
    assert!(bar(cx, CURSOR).is_none(), "hidden when the setting is on");
}

const FOLD_CODE: &str = "fn a() {\n    let x = 1;\n    let y = 2;\n}\nfn b() {\n    if x {\n        z();\n    }\n}\nend\n";

#[gpui_kit::test]
fn marks_sit_at_their_view_line_when_a_region_is_folded(cx: &mut TestAppContext) {
    // The zones follow the view layout (upstream `viewLayout.getVerticalOffsetForLineNumber`), so with
    // lines hidden by a fold a mark sits at its visible row, not at its document line (ADR-0042).
    // `scrollBeyondLastLine` is off, so the lane is exactly the viewport and one row is `line_height`.
    let (wb, cx) = workbench(cx, "fold-marks", r#"{"editor.scrollBeyondLastLine": false}"#, FOLD_CODE);
    settle(&wb, cx);
    // Folds lines 2-3, under `fn a() {`.
    cx.simulate_keystrokes("alt-cmd-[");
    cx.run_until_parked();
    let lh = wb.read_with(cx, |wb, _| wb.editor_view.cfg.line_height);
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let line = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active.line);
    assert_eq!(line, 4, "down skips the folded lines");
    settle(&wb, cx);
    let lane = bar(cx, "editor-scrollbar-vertical").expect("lane");
    let top = f32::from(bar(cx, CURSOR).expect("the cursor mark").origin.y - lane.origin.y);
    assert!(
        (top - lh).abs() < 1.0,
        "line 4 is the second visible row, so its mark is one row down: {top} vs {lh}"
    );
    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    settle(&wb, cx);
    let top = f32::from(bar(cx, CURSOR).expect("the cursor mark").origin.y - lane.origin.y);
    assert!(
        (top - 2.0 * lh).abs() < 1.0,
        "line 5 is the third visible row: {top} vs {}",
        2.0 * lh
    );
}
