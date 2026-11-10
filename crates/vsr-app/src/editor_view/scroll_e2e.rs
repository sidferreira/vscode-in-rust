//! Headless end-to-end tests of the editor's horizontal scroll room (docs/testing.md tier 2;
//! tests/e2e-scenarios/editor.md, "Scroll beyond the last column"). The scroll extent is the
//! list's `max_offset`, which the paint of the widest row sets.
//!
//! upstream: src/vs/editor/common/viewLayout/viewLayout.ts @ 50e1350fc78d

use std::fs;
use std::path::PathBuf;

use gpui::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str, text: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-scroll-e2e-{name}-{}-{nanos}", std::process::id()));
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

/// The horizontal extent the list can scroll to, and the character advance it was laid out with.
fn scroll_extent(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (f32, f32) {
    wb.update(cx, |wb, _| {
        let path = wb.editors.active_editor().unwrap().path.clone();
        let s = wb.editor_view.scrolls.get(&path).expect("scroll handle").clone();
        let max_x = f32::from(s.0.borrow().base_handle.max_offset().x);
        (max_x, wb.editor_view.char_width())
    })
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// Wider than any test window, so the list has a horizontal extent to scroll.
fn long_line() -> String {
    format!("let a = \"{}\";\nlet b = 1;\n", "0123456789".repeat(40))
}

#[gpui_kit::test]
fn scroll_beyond_last_column_adds_room_after_the_widest_line_live(cx: &mut TestAppContext) {
    // Upstream source: viewLayout.ts `_updateContentWidth` (no upstream unit test covers it).
    let (wb, cx) = workbench(cx, "columns", "{}", &long_line());
    let (default_x, char_w) = scroll_extent(&wb, cx);
    assert!(default_x > 0.0, "the widest line scrolls");

    set(cx, "editor.scrollBeyondLastColumn", json!(0));
    let (none_x, _) = scroll_extent(&wb, cx);
    assert!((default_x - none_x - 4.0 * char_w).abs() < 1.0, "4 columns less without the room: {default_x} vs {none_x}");

    set(cx, "editor.scrollBeyondLastColumn", json!(10));
    let (more_x, _) = scroll_extent(&wb, cx);
    assert!((more_x - none_x - 10.0 * char_w).abs() < 1.0, "10 columns of room: {more_x} vs {none_x}");
}

#[gpui_kit::test]
fn scroll_beyond_last_column_does_not_apply_with_word_wrap(cx: &mut TestAppContext) {
    // Upstream source: viewLayout.ts `_updateContentWidth`, the viewport-wrapping branch (no upstream unit test).
    let (wb, cx) = workbench(cx, "wrap", r#"{ "editor.wordWrap": "on" }"#, &long_line());
    let (wrapped_x, _) = scroll_extent(&wb, cx);
    set(cx, "editor.scrollBeyondLastColumn", json!(12));
    let (still_x, _) = scroll_extent(&wb, cx);
    assert_eq!(wrapped_x, 0.0, "wrapped lines fit the viewport");
    assert_eq!(still_x, 0.0, "no room is added while wrapping");
}
