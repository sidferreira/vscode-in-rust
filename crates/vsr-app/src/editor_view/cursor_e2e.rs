//! Headless end-to-end tests of the caret settings: `editor.cursorHeight`, and
//! `editor.cursorSurroundingLines` with `editor.cursorSurroundingLinesStyle` (tests/e2e-scenarios/
//! editor-cursor.md). Same harness as `wrap_e2e.rs`: the real `Workbench` in gpui's test platform,
//! a temp `settings.json`. The checks are on the scroll state and the caret box, not on pixels.

use std::fs;
use std::path::PathBuf;

use gpui::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-cursor-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workbench over a temp folder with one file `a.txt` (opened), settings from `settings` (JSON text).
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str, text: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let dir = unique_dir(name);
    let settings_path = dir.join("settings.json");
    fs::write(&settings_path, settings).unwrap();
    fs::write(dir.join("a.txt"), text).unwrap();
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
    let path = dir.join("a.txt");
    wb.update(vcx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    vcx.run_until_parked();
    (wb, vcx)
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// `n` presses of `key`, then a frame so the list applies the deferred scroll.
fn press(cx: &mut VisualTestContext, key: &str, n: usize) {
    cx.simulate_keystrokes(&vec![key; n].join(" "));
    frame(cx);
}

/// Redraws the window (the list applies its deferred scroll while painting) and runs the tasks.
fn frame(cx: &mut VisualTestContext) {
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

/// The caret's 0-based view row and the first visible row, with the rows per page.
fn caret_row_and_top(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (usize, usize, usize) {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        let row = e.editor.selection().active.line - 1;
        // The first visible row, from the scroll offset (`logical_scroll_top` does not track the uniform list).
        let line_height = wb.editor_view.cfg.line_height;
        let top = wb.editor_view.scrolls.get(&e.path).map_or(0, |s| {
            (-f32::from(s.0.borrow().base_handle.offset().y) / line_height).floor().max(0.0) as usize
        });
        (row, top, wb.editor_view.rows_per_page())
    })
}

fn numbered_lines(n: usize) -> String {
    (1..=n).map(|i| format!("line {i}\n")).collect()
}

#[gpui::test]
fn surrounding_lines_keep_a_margin_when_the_caret_moves_by_keys(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "keys", r#"{ "editor.cursorSurroundingLines": 3 }"#, &numbered_lines(200));
    cx.run_until_parked();
    let (_, _, page) = caret_row_and_top(&wb, cx);
    assert!(page > 8, "the test window shows several rows");
    // Down past the bottom: the caret row keeps three rows below it (minimum one, the trailing rule).
    press(cx, "down", page + 5);
    let (row, top, _) = caret_row_and_top(&wb, cx);
    assert_eq!(row, page + 5);
    assert_eq!(top, row + 3 + 1 - page, "the trailing margin of 3 rows is visible");
    // Up past the top: the caret row keeps three rows above it.
    press(cx, "up", page);
    let (row, top, _) = caret_row_and_top(&wb, cx);
    assert_eq!(row, 5);
    assert_eq!(top, row - 3, "the leading margin of 3 rows is visible");
}

#[gpui::test]
fn default_style_does_not_enforce_the_margin_for_a_click(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "click", r#"{ "editor.cursorSurroundingLines": 3 }"#, &numbered_lines(200));
    cx.run_until_parked();
    let (_, _, page) = caret_row_and_top(&wb, cx);
    // The row two above the bottom edge of the page: inside the trailing margin.
    let target = page - 2;
    // `debug_bounds` takes a `'static` selector: one leaked string per test is fine.
    let selector: &'static str = Box::leak(format!("row:{target}").into_boxed_str());
    let center = cx.debug_bounds(selector).expect("the row is painted").center();
    cx.simulate_click(center, Default::default());
    frame(cx);
    let (row, top, _) = caret_row_and_top(&wb, cx);
    assert_eq!(row, target, "the click placed the caret");
    assert_eq!(top, 0, "a click does not scroll for the margin with the default style");
    set(cx, "editor.cursorSurroundingLinesStyle", json!("all"));
    cx.simulate_click(center, Default::default());
    frame(cx);
    let (row, top, _) = caret_row_and_top(&wb, cx);
    assert_eq!(row, target);
    assert_eq!(top, row + 3 + 1 - page, "style all enforces the margin for clicks too");
}

#[gpui::test]
fn cursor_height_sizes_the_line_caret(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "height", "{}", "abc\n");
    cx.run_until_parked();
    let full = cx.debug_bounds("caret:1").expect("the caret is drawn").size.height;
    set(cx, "editor.cursorHeight", json!(6));
    let short = cx.debug_bounds("caret:1").expect("the caret is drawn").size.height;
    assert!(f32::from(short) < f32::from(full), "a cursorHeight below the line height shortens the caret");
    assert!((f32::from(short) - 6.0).abs() < 0.5);
    set(cx, "editor.cursorHeight", json!(0));
    let back = cx.debug_bounds("caret:1").expect("the caret is drawn").size.height;
    assert_eq!(back, full, "0 is the full line height again");
    assert_eq!(wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().value()), "abc\n");
}

#[gpui::test]
fn tab_focus_mode_keeps_the_tab_out_of_the_text(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tabfocus", "{}", "x\n");
    cx.run_until_parked();
    let text = |cx: &mut VisualTestContext| wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().value());
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_ne!(text(cx), "x\n", "by default Tab types a tab (spaces)");
    set(cx, "editor.tabFocusMode", json!(true));
    let before = text(cx);
    cx.simulate_keystrokes("tab");
    cx.run_until_parked();
    assert_eq!(text(cx), before, "with tabFocusMode the editor does not take the tab");
}
