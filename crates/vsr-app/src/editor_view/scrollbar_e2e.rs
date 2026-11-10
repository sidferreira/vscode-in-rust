//! Headless end-to-end tests of the editor's scrollbars (docs/testing.md tier 2;
//! tests/e2e-scenarios/preferences.md, "Editor scrollbars"). The bars are found by their debug
//! selectors, so the tests read where they are drawn, and they drive them with simulated pointers.
//!
//! upstream: src/vs/base/browser/ui/scrollbar/abstractScrollbar.ts @ 50e1350fc78d

use std::fs;
use std::path::PathBuf;

use gpui::{Bounds, Entity, MouseButton, Pixels, TestAppContext, VisualTestContext, point, px};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

const V_BAR: &str = "editor-scrollbar-vertical";
const V_SLIDER: &str = "editor-scrollbar-vertical-slider";
const H_BAR: &str = "editor-scrollbar-horizontal";

fn workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    settings: &str,
    text: &str,
) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "rscode-scrollbar-e2e-{name}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).unwrap();
    let settings_path: PathBuf = dir.join("settings.json");
    fs::write(&settings_path, settings).unwrap();
    fs::write(dir.join("a.rs"), text).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        AppConfig::install(cx, Some(settings_path));
    });
    let root = dir.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    let path = dir.join("a.rs");
    wb.update(vcx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    vcx.run_until_parked();
    (wb, vcx)
}

/// A document taller than any test window.
fn tall_text() -> String {
    (0..400).map(|i| format!("let line{i} = {i};\n")).collect()
}

/// A line wider than any test window.
fn wide_text() -> String {
    format!("let a = \"{}\";\n", "0123456789".repeat(40))
}

fn set(wb: &Entity<Workbench>, cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value)))
        .unwrap();
    // The bars are built with the editor, so repaint the workbench after a change.
    wb.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

/// Rebuilds the editor (the bars are built with it) and paints it.
fn settle(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    wb.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    wb.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn bar(cx: &mut VisualTestContext, selector: &'static str) -> Option<Bounds<Pixels>> {
    cx.debug_bounds(selector)
}

/// The list's vertical scroll offset in pixels.
fn scroll_top(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> f32 {
    wb.update(cx, |wb, _| {
        let path = wb.editors.active_editor().unwrap().path.clone();
        let s = wb
            .editor_view
            .scrolls
            .get(&path)
            .expect("scroll handle")
            .clone();
        -f32::from(s.0.borrow().base_handle.offset().y)
    })
}

fn max_scroll_top(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> f32 {
    wb.update(cx, |wb, _| {
        let path = wb.editors.active_editor().unwrap().path.clone();
        let s = wb
            .editor_view
            .scrolls
            .get(&path)
            .expect("scroll handle")
            .clone();
        f32::from(s.0.borrow().base_handle.max_offset().y)
    })
}

#[gpui_kit::test]
fn auto_shows_the_vertical_bar_when_the_document_is_taller_than_the_viewport(
    cx: &mut TestAppContext,
) {
    // Upstream source: scrollbarState.ts `isNeeded` (the visibility of `auto`).
    let (wb, cx) = workbench(cx, "auto-tall", "{}", &tall_text());
    settle(&wb, cx);
    let track = bar(cx, V_BAR).expect("a bar for an overflowing document");
    assert_eq!(
        f32::from(track.size.width),
        14.0,
        "verticalScrollbarSize (default 14)"
    );
    assert!(bar(cx, V_SLIDER).is_some(), "the slider is drawn");
}

#[gpui_kit::test]
fn auto_hides_the_vertical_bar_when_the_document_fits(cx: &mut TestAppContext) {
    // One line and no trailing newline: a second line would overflow by a row, as upstream's
    // `scrollBeyondLastLine` height does.
    let (wb, cx) = workbench(cx, "auto-short", "{}", "let a = 1;");
    settle(&wb, cx);
    assert!(bar(cx, V_BAR).is_none(), "no bar for a document that fits");
}

#[gpui_kit::test]
fn visible_always_shows_the_vertical_bar(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "visible", "{}", "let a = 1;");
    set(&wb, cx, "editor.scrollbar.vertical", json!("visible"));
    settle(&wb, cx);
    assert!(bar(cx, V_BAR).is_some(), "drawn although nothing overflows");
}

#[gpui_kit::test]
fn hidden_never_shows_the_vertical_bar(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(
        cx,
        "hidden",
        r#"{ "editor.scrollbar.vertical": "hidden" }"#,
        &tall_text(),
    );
    settle(&wb, cx);
    assert!(
        bar(cx, V_BAR).is_none(),
        "no bar even though the document overflows"
    );
}

#[gpui_kit::test]
fn vertical_scrollbar_size_sets_the_bar_width_live(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "size", "{}", &tall_text());
    set(&wb, cx, "editor.scrollbar.verticalScrollbarSize", json!(20));
    settle(&wb, cx);
    assert_eq!(f32::from(bar(cx, V_BAR).expect("bar").size.width), 20.0);
}

#[gpui_kit::test]
fn click_on_the_track_below_the_slider_centres_the_slider_on_the_click(cx: &mut TestAppContext) {
    // Upstream source: abstractScrollbar.ts `_onPointerDown`, the jump (scrollByPage false).
    let (wb, cx) = workbench(cx, "jump", "{}", &tall_text());
    settle(&wb, cx);
    let track = bar(cx, V_BAR).expect("bar");
    let at = point(track.center().x, track.origin.y + track.size.height * 0.8);
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    settle(&wb, cx);
    assert!(scroll_top(&wb, cx) > 0.0, "the list moved");
    let slider = bar(cx, V_SLIDER).expect("slider");
    let centre = f32::from(slider.center().y - track.origin.y);
    let clicked = f32::from(at.y - track.origin.y);
    // The centre is where the click was, unless the list reached its end.
    let at_end = (scroll_top(&wb, cx) - max_scroll_top(&wb, cx)).abs() < 1.0;
    assert!(
        at_end || (centre - clicked).abs() < 2.0,
        "slider centre {centre} vs click {clicked}"
    );
}

#[gpui_kit::test]
fn scroll_by_page_click_moves_one_viewport(cx: &mut TestAppContext) {
    // Upstream source: scrollbarState.ts `getDesiredScrollPositionFromOffsetPaged`.
    let (wb, cx) = workbench(
        cx,
        "page",
        r#"{ "editor.scrollbar.scrollByPage": true }"#,
        &tall_text(),
    );
    settle(&wb, cx);
    let track = bar(cx, V_BAR).expect("bar");
    let at = point(track.center().x, track.origin.y + track.size.height * 0.95);
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    settle(&wb, cx);
    let page = f32::from(track.size.height);
    let moved = scroll_top(&wb, cx);
    assert!(
        (moved - page).abs() <= 2.0,
        "one page ({page}px) down, moved {moved}px"
    );
}

#[gpui_kit::test]
fn dragging_the_slider_moves_the_list_by_the_slider_ratio(cx: &mut TestAppContext) {
    // Upstream source: scrollbarState.ts `getDesiredScrollPositionFromDelta` (the slider moves by the pointer's delta).
    let (wb, cx) = workbench(cx, "drag", "{}", &tall_text());
    settle(&wb, cx);
    let track = bar(cx, V_BAR).expect("bar");
    let slider = bar(cx, V_SLIDER).expect("slider");
    let max = max_scroll_top(&wb, cx);
    let start = point(slider.center().x, slider.center().y);
    cx.simulate_mouse_move(start, None, Default::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.run_until_parked();
    let to = point(start.x, start.y + px(40.0));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.run_until_parked();
    let slider_room = f32::from(track.size.height - slider.size.height);
    let expected = 40.0 * max / slider_room;
    let got = scroll_top(&wb, cx);
    assert!(
        (got - expected).abs() <= 2.0,
        "moved {got}, expected {expected} (max {max}, room {slider_room})"
    );
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn horizontal_auto_shows_for_a_wide_line_and_not_while_wrapping(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "h-auto", "{}", &wide_text());
    settle(&wb, cx);
    let h = bar(cx, H_BAR).expect("a horizontal bar for a wide line");
    assert_eq!(
        f32::from(h.size.height),
        12.0,
        "horizontalScrollbarSize (default 12)"
    );

    set(&wb, cx, "editor.wordWrap", json!("on"));
    settle(&wb, cx);
    assert!(
        bar(cx, H_BAR).is_none(),
        "wrapped lines fit the viewport, so no bar"
    );
}
