//! Headless end-to-end tests of sticky scroll (docs/testing.md tier 2; tests/e2e-scenarios/preferences.md,
//! "Sticky scroll"). The scroll offset is set on the list's scroll handle, like a wheel would.
//!
//! upstream: src/vs/editor/contrib/stickyScroll/browser/stickyScrollController.ts @ 50e1350fc78d

use std::fs;
use std::path::PathBuf;

use gpui::{Entity, TestAppContext, VisualTestContext, point, px};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str, text: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-sticky-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let settings_path = dir.join("settings.json");
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
    (wb, vcx, dir)
}

fn scroll_to_row(wb: &Entity<Workbench>, cx: &mut VisualTestContext, row: usize) {
    wb.update(cx, |wb, cx| {
        let lh = wb.editor_view.cfg.line_height;
        let path = wb.editors.active_editor().unwrap().path.clone();
        let s = wb.editor_view.scrolls.get(&path).expect("scroll handle").clone();
        let x = s.0.borrow().base_handle.offset().x;
        s.0.borrow().base_handle.set_offset(point(x, px(-(row as f32) * lh)));
        cx.notify();
    });
    cx.run_until_parked();
}

fn code() -> String {
    let mut t = String::from("fn a() {\n    if x {\n");
    for i in 0..40 {
        t.push_str(&format!("        let v{i} = {i};\n"));
    }
    t.push_str("    }\n}\nend\n");
    t
}

#[gpui::test]
fn scrolled_headers_stay_pinned_at_the_top(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "pinned", "{}", &code());
    assert!(cx.debug_bounds("sticky:1").is_none(), "scrolled to the top: nothing is pinned");
    scroll_to_row(&wb, cx, 10);
    assert!(cx.debug_bounds("sticky:1").is_some() && cx.debug_bounds("sticky:2").is_some(), "`fn a` and `if x` are pinned");
    let (first, second) = (cx.debug_bounds("sticky:1").unwrap(), cx.debug_bounds("sticky:2").unwrap());
    assert!(first.origin.y < second.origin.y, "outermost first");
    scroll_to_row(&wb, cx, 0);
    assert!(cx.debug_bounds("sticky:1").is_none());
}

#[gpui::test]
fn max_line_count_enabled_and_folding_follow_the_settings(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "settings", "{}", &code());
    scroll_to_row(&wb, cx, 10);
    assert!(cx.debug_bounds("sticky:2").is_some());
    cx.update(|_, cx| AppConfig::set_value(cx, "editor.stickyScroll.maxLineCount", Some(json!(1)))).unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("sticky:1").is_some() && cx.debug_bounds("sticky:2").is_none(), "at most one line");
    cx.update(|_, cx| AppConfig::set_value(cx, "editor.stickyScroll.enabled", Some(json!(false)))).unwrap();
    cx.run_until_parked();
    assert!(cx.debug_bounds("sticky:1").is_none(), "disabled");
}

#[gpui::test]
fn clicking_a_pinned_header_moves_the_caret_there(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "click", "{}", &code());
    scroll_to_row(&wb, cx, 10);
    let at = cx.debug_bounds("sticky:2").unwrap().center();
    cx.simulate_mouse_move(at, None, gpui::Modifiers::default());
    cx.simulate_click(at, gpui::Modifiers::default());
    cx.run_until_parked();
    let caret = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!((caret.line, caret.column), (2, 1));
}
