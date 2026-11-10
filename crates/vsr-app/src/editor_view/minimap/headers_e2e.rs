//! Headless end-to-end tests of the minimap section headers (docs/testing.md tier 2;
//! tests/e2e-scenarios/preferences.md, "Minimap"). Our own tests: the finder's upstream tests are
//! ported in `vsr-editor`.
//!
//! upstream: src/vs/editor/common/services/findSectionHeaders.ts @ 50e1350fc78d

use std::fs;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

const RUST: &str = "//#region Alpha section\nfn a() {}\n//#endregion\n// MARK: - Beta section\nfn b() {}\n// TODO: later\n";

fn open<'a>(cx: &'a mut TestAppContext, name: &str, file: &str, text: &str, settings: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-headers-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let settings_path = dir.join("settings.json");
    fs::write(&settings_path, settings).unwrap();
    fs::write(dir.join(file), text).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        AppConfig::install(cx, Some(settings_path));
    });
    let root = dir.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    let path = dir.join(file);
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

/// The `(line, text, has separator)` of the headers the minimap found.
fn headers(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<(usize, String, bool)> {
    wb.read_with(cx, |wb, _| {
        wb.editor_view
            .minimap
            .section_headers
            .borrow()
            .as_ref()
            .map(|(_, l)| l.iter().map(|h| (h.start_line_number, h.text.clone(), h.has_separator_line)).collect())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn region_and_mark_headers_show_their_names(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "names", "a.rs", RUST, "{}");
    assert_eq!(
        headers(&wb, cx),
        [(1, "Alpha section".to_string(), false), (4, "Beta section".to_string(), true)],
        "the region name and the MARK name (`-` makes a separator); the TODO is not a header"
    );
    assert!(cx.debug_bounds("minimap-header:1").is_some(), "the region title is on the minimap");
    assert!(cx.debug_bounds("minimap-header:4").is_some(), "the mark title is on the minimap");
    assert!(cx.debug_bounds("minimap-header:6").is_none());
}

#[gpui::test]
fn the_settings_switch_each_kind_and_change_the_regex_and_size(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "settings", "a.rs", RUST, "{}");
    let h1 = cx.debug_bounds("minimap-header:1").unwrap();
    set(cx, "editor.minimap.showRegionSectionHeaders", json!(false));
    assert_eq!(headers(&wb, cx).iter().map(|h| h.0).collect::<Vec<_>>(), [4], "no region headers");
    assert!(cx.debug_bounds("minimap-header:1").is_none());
    set(cx, "editor.minimap.showRegionSectionHeaders", json!(true));
    set(cx, "editor.minimap.showMarkSectionHeaders", json!(false));
    assert_eq!(headers(&wb, cx).iter().map(|h| h.0).collect::<Vec<_>>(), [1], "no mark headers");
    set(cx, "editor.minimap.showMarkSectionHeaders", json!(true));
    set(cx, "editor.minimap.markSectionHeaderRegex", json!(r"TODO:\s*(?<label>.*)$"));
    assert_eq!(headers(&wb, cx), [(1, "Alpha section".to_string(), false), (6, "later".to_string(), false)], "a custom regex");
    set(cx, "editor.minimap.sectionHeaderFontSize", json!(12));
    let h1_big = cx.debug_bounds("minimap-header:1").unwrap();
    assert!(h1_big.size.height > h1.size.height, "a larger section header font makes a taller title");
}

#[gpui::test]
fn plain_text_has_no_mark_headers_but_regions_need_a_marker(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "plain", "a.txt", "MARK: not a comment\n#region x\n", "{}");
    assert_eq!(headers(&wb, cx), [], "a MARK outside a comment, and plain text has no region markers");
}
