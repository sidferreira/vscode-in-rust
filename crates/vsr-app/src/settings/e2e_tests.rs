//! Headless E2E tests for the Settings editor (docs/testing.md, tier 2).
//!
//! They run the real `Workbench` / `SettingsView` in gpui's test platform (no window, no GPU):
//! keystrokes, typing and clicks go through gpui's event dispatch. Settings are written to a
//! temp file, never the real user settings.json.
//!
//! Scenario source: the "Settings editor" scenarios of the preferences smoke area (see
//! tests/e2e-scenarios/preferences.md). These are our own tests; no upstream test ports here.
//!

use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::{SettingsView, install};
use crate::Workbench;

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rscode-e2e-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn init(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        install(cx);
    });
}

/// A workbench over an empty temp folder whose Settings editor writes to `<dir>/settings.json`.
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    init(cx);
    let dir = temp_dir(name);
    let settings = dir.join("settings.json");
    let root = dir.clone();
    let path = settings.clone();
    let (wb, vcx) = cx.add_window_view(move |_, _| {
        let mut wb = Workbench::new(root);
        wb.settings_path = path;
        wb
    });
    (wb, vcx, settings)
}

fn view_of(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Entity<SettingsView> {
    wb.read_with(cx, |wb, _| wb.settings.clone()).expect("settings editor is open")
}

#[gpui_kit::test]
fn cmd_comma_opens_settings_and_search_ranks_font_size_first(cx: &mut TestAppContext) {
    let (wb, cx, _settings) = workbench(cx, "search");
    assert!(wb.read_with(cx, |wb, _| wb.settings.is_none()));

    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.settings.is_some() && wb.settings_active), "Cmd+, opens Settings");

    // Only one instance: pressing it again keeps the same view.
    let first = view_of(&wb, cx);
    cx.simulate_keystrokes("cmd-,");
    assert_eq!(first, view_of(&wb, cx));

    let view = view_of(&wb, cx);
    cx.simulate_input("font size");
    cx.run_until_parked();
    let keys = view.read_with(cx, |v, _| v.visible_keys());
    assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), "font size");
    assert_eq!(keys.first().map(String::as_str), Some("editor.fontSize"), "results: {:?}", &keys[..keys.len().min(5)]);

    // Esc clears the search and restores the grouped list.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), "");
    assert!(view.read_with(cx, |v, _| v.visible_keys().len()) > 100);
}

#[gpui_kit::test]
fn toggling_a_bool_setting_persists_to_user_settings(cx: &mut TestAppContext) {
    let (wb, cx, settings) = workbench(cx, "toggle");
    cx.simulate_keystrokes("cmd-,");
    let view = view_of(&wb, cx);
    cx.simulate_input("editor.minimap.enabled");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.visible_keys()).first().map(String::as_str), Some("editor.minimap.enabled"));
    assert!(!view.read_with(cx, |v, _| v.user_settings().is_modified("editor.minimap.enabled")));

    let bounds = cx.debug_bounds("checkbox:search:editor.minimap.enabled").expect("checkbox is rendered");
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();

    // Default is true, so the click turns it off and records it.
    let value = view.read_with(cx, |v, _| v.user_settings().get("editor.minimap.enabled").cloned());
    assert_eq!(value, Some(serde_json::Value::Bool(false)));
    let on_disk = vsr_settings::UserSettings::load(&settings).unwrap();
    assert_eq!(on_disk.get("editor.minimap.enabled"), Some(&serde_json::Value::Bool(false)));

    // Toggling back to the default removes the key (like upstream).
    let bounds = cx.debug_bounds("checkbox:search:editor.minimap.enabled").unwrap();
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    assert!(!vsr_settings::UserSettings::load(&settings).unwrap().is_modified("editor.minimap.enabled"));
}

#[gpui_kit::test]
fn settings_view_loads_existing_user_settings(cx: &mut TestAppContext) {
    init(cx);
    let dir = temp_dir("load");
    let path = dir.join("settings.json");
    std::fs::write(&path, "{ // comment\n \"editor.tabSize\": 2, }\n").unwrap();
    let p = path.clone();
    let (view, cx) = cx.add_window_view(move |window, cx| SettingsView::new(p, window, cx));
    assert!(view.read_with(cx, |v, _| v.user_settings().is_modified("editor.tabSize")));
}

#[gpui_kit::test]
fn number_setting_edit_shows_modified_and_reset_restores_default(cx: &mut TestAppContext) {
    let (wb, cx, settings) = workbench(cx, "number");
    cx.simulate_keystrokes("cmd-,");
    let view = view_of(&wb, cx);
    cx.simulate_input("@id:editor.tabSize");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.visible_keys()), ["editor.tabSize"]);

    // Focus the number input, replace its text with 6.
    let bounds = cx.debug_bounds("input:search:editor.tabSize").expect("number input is rendered");
    cx.simulate_click(bounds.center(), Default::default());
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("6");
    cx.run_until_parked();
    let on_disk = vsr_settings::UserSettings::load(&settings).unwrap();
    assert_eq!(on_disk.get("editor.tabSize"), Some(&serde_json::json!(6)));

    // Invalid input is not written and shows a message under the input.
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("x");
    cx.run_until_parked();
    assert_eq!(vsr_settings::UserSettings::load(&settings).unwrap().get("editor.tabSize"), Some(&serde_json::json!(6)));
    assert!(view.read_with(cx, |v, _| v.errors.contains_key("search:editor.tabSize")));

    // More Actions > Reset Setting removes the key.
    let gear = cx.debug_bounds("more-actions:search:editor.tabSize").expect("gear is rendered");
    cx.simulate_click(gear.center(), Default::default());
    cx.run_until_parked();
    let reset = cx.debug_bounds("reset:search:editor.tabSize").expect("menu is open");
    cx.simulate_click(reset.center(), Default::default());
    cx.run_until_parked();
    assert!(!vsr_settings::UserSettings::load(&settings).unwrap().is_modified("editor.tabSize"));
    assert!(view.read_with(cx, |v, _| v.errors.is_empty()));
}

#[gpui_kit::test]
fn enum_dropdown_selects_a_value(cx: &mut TestAppContext) {
    let (wb, cx, settings) = workbench(cx, "enum");
    cx.simulate_keystrokes("cmd-,");
    let view = view_of(&wb, cx);
    cx.simulate_input("@id:editor.lineNumbers");
    cx.run_until_parked();
    let select = cx.debug_bounds("select:search:editor.lineNumbers").expect("dropdown is rendered");
    cx.simulate_click(select.center(), Default::default());
    cx.run_until_parked();
    let off = cx.debug_bounds("option:search:editor.lineNumbers:0").expect("options are listed");
    cx.simulate_click(off.center(), Default::default());
    cx.run_until_parked();
    let _ = &view;
    assert_eq!(
        vsr_settings::UserSettings::load(&settings).unwrap().get("editor.lineNumbers"),
        Some(&serde_json::json!("off"))
    );
}
