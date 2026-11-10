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
