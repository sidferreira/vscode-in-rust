//! Headless E2E tests of the theme service (docs/testing.md tier 2): the default theme, switching
//! with `workbench.colorTheme`, `workbench.colorCustomizations`, the `Preferences: Color Theme`
//! picker (live preview, Escape reverts, Enter saves) and `window.autoDetectColorScheme` with an
//! injected system appearance. The in-memory configuration is used: never the real settings file.
//! Expected colours are read from the bundled theme JSON, not retyped.

use std::fs;

use gpui_kit::{Entity, TestAppContext, VisualTestContext, WindowAppearance};
use serde_json::{Value, json};
use vsr_theme::{ThemeCatalog, hex};
use vsr_quickinput::pick::PickRow;

use crate::Workbench;
use crate::config::AppConfig;
use crate::quick_input::QuickPick;
use crate::theme::Rgba8;
use crate::theme_service::ThemeService;

fn start<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        crate::quick_open::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None); // in memory: never the real settings file
    });
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-e2e-theme-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), "hello\n").unwrap();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(dir, cx));
    cx.run_until_parked();
    (wb, cx)
}

fn set(cx: &mut VisualTestContext, key: &str, value: Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// A colour of a bundled theme as the JSON chain gives it (no customisations).
fn json_color(settings_id: &str, key: &str) -> Rgba8 {
    let catalog = ThemeCatalog::builtin();
    let theme = catalog.load(catalog.find(settings_id, None).unwrap()).unwrap();
    theme.rgba8(key).unwrap()
}

fn theme(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> crate::theme::Theme {
    wb.read_with(cx, |wb, _| wb.theme.clone())
}

fn picker(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Entity<QuickPick>> {
    wb.read_with(cx, |wb, _| wb.quick.picker().cloned())
}

fn active_label(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    picker(wb, cx)?.read_with(cx, |p, _| p.active_item().map(|i| i.label.clone()))
}

fn active_id(cx: &mut VisualTestContext) -> String {
    cx.update(|_, cx| ThemeService::active_settings_id(cx)).unwrap()
}

#[gpui_kit::test]
fn the_default_theme_is_dark_2026_with_the_colours_of_its_json(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "default");
    let t = theme(&wb, cx);
    assert_eq!(active_id(cx), "Dark 2026");
    for (field, key) in [(t.editor_bg, "editor.background"), (t.editor_fg, "editor.foreground"), (t.side_bar_bg, "sideBar.background"), (t.status_bar_bg, "statusBar.background"), (t.focus_border, "focusBorder")] {
        assert_eq!(field, json_color("Dark 2026", key), "{key}");
    }
    assert_eq!(t.editor_bg, hex("#121314").unwrap());
}

#[gpui_kit::test]
fn a_light_system_defaults_to_light_2026_with_the_colours_of_its_json(cx: &mut TestAppContext) {
    // Upstream source: workbenchThemeService.ts ThemeSettingDefaults.COLOR_THEME_LIGHT (ADR-0049)
    let (wb, cx) = start(cx, "default-light");
    set(cx, "window.autoDetectColorScheme", json!(true));
    cx.update(|_, cx| ThemeService::set_system_appearance(cx, WindowAppearance::Light));
    cx.run_until_parked();
    assert_eq!(active_id(cx), "Light 2026", "no preferredLightColorTheme set: the light default");
    let t = theme(&wb, cx);
    for (field, key) in [(t.editor_bg, "editor.background"), (t.editor_fg, "editor.foreground"), (t.side_bar_bg, "sideBar.background"), (t.status_bar_bg, "statusBar.background"), (t.focus_border, "focusBorder")] {
        assert_eq!(field, json_color("Light 2026", key), "{key}");
    }
}

#[gpui_kit::test]
fn setting_the_color_theme_repaints_with_the_new_colours(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "switch");
    let dark = theme(&wb, cx);
    set(cx, "workbench.colorTheme", json!("Light Modern"));
    let light = theme(&wb, cx);
    assert_ne!(light.editor_bg, dark.editor_bg);
    assert_ne!(light.editor_fg, dark.editor_fg);
    assert_eq!(light.editor_bg, json_color("Light Modern", "editor.background"));
    assert_eq!(light.editor_fg, json_color("Light Modern", "editor.foreground"));
    assert_eq!(light.side_bar_bg, json_color("Light Modern", "sideBar.background"));
    // an unknown theme falls back to the default of the colour mode
    set(cx, "workbench.colorTheme", json!("Monokai"));
    assert_eq!(theme(&wb, cx).editor_bg, dark.editor_bg);
    // resetting the setting goes back to the default
    cx.update(|_, cx| AppConfig::set_value(cx, "workbench.colorTheme", None)).unwrap();
    cx.run_until_parked();
    assert_eq!(theme(&wb, cx), dark);
}

#[gpui_kit::test]
fn color_customizations_override_one_key_and_a_theme_block_wins(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "custom");
    let base = theme(&wb, cx);
    set(cx, "workbench.colorCustomizations", json!({ "editor.background": "#102030" }));
    let t = theme(&wb, cx);
    assert_eq!(t.editor_bg, hex("#102030").unwrap());
    assert_eq!(t.editor_fg, base.editor_fg, "other colours are untouched");
    set(cx, "workbench.colorCustomizations", json!({ "editor.background": "#102030", "[Dark 2026]": { "editor.background": "#405060" } }));
    assert_eq!(theme(&wb, cx).editor_bg, hex("#405060").unwrap());
    set(cx, "workbench.colorCustomizations", json!({ "editor.background": "default" }));
    // "default" means the registry default of the theme type, not the theme's own colour
    assert_eq!(theme(&wb, cx).editor_bg, hex("#1E1E1E").unwrap());
}

#[gpui_kit::test]
fn select_theme_previews_on_arrow_keys_and_escape_reverts(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "pick-escape");
    let original = theme(&wb, cx);
    cx.simulate_keystrokes("cmd-k cmd-t");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_some(), "Cmd+K Cmd+T opens the theme picker");
    assert_eq!(active_label(&wb, cx).as_deref(), Some("Dark 2026"), "the current theme is preselected");
    let rows: Vec<String> = picker(&wb, cx).unwrap().read_with(cx, |p, _| {
        p.list().rows().iter().map(|r| match r { PickRow::Separator(s) => format!("--{s}"), PickRow::Item(i) => i.label.clone() }).collect()
    });
    assert_eq!(rows.iter().filter(|r| r.starts_with("--")).count(), 3, "light, dark and high contrast groups: {rows:?}");
    assert_eq!(rows[0], "--light themes");

    cx.simulate_keystrokes("down");
    cx.run_until_parked();
    let label = active_label(&wb, cx).unwrap();
    assert_ne!(label, "Dark 2026");
    let catalog = ThemeCatalog::builtin();
    let previewed = catalog.themes().iter().find(|t| t.label == label).unwrap();
    assert_eq!(active_id(cx), previewed.settings_id, "the theme under the cursor is showing");
    assert_eq!(theme(&wb, cx).editor_bg, json_color(previewed.settings_id, "editor.background"));
    // the setting is not touched by a preview
    assert!(cx.update(|_, cx| AppConfig::get(cx).user().get("workbench.colorTheme").is_none()));

    // move into the light group and see it change
    for _ in 0..8 {
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
    }
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
    assert_eq!(theme(&wb, cx), original, "Escape goes back to the configured theme");
    assert_eq!(active_id(cx), "Dark 2026");
}

#[gpui_kit::test]
fn select_theme_enter_saves_the_theme(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "pick-enter");
    cx.simulate_keystrokes("cmd-k cmd-t");
    cx.run_until_parked();
    // go to "Light Modern": a separator then the light group is on top, so walk up from Dark 2026
    let mut guard = 0;
    while active_label(&wb, cx).as_deref() != Some("Light Modern") && guard < 20 {
        cx.simulate_keystrokes("up");
        cx.run_until_parked();
        guard += 1;
    }
    assert_eq!(active_label(&wb, cx).as_deref(), Some("Light Modern"));
    assert_eq!(theme(&wb, cx).editor_bg, json_color("Light Modern", "editor.background"), "previewed");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
    let saved = cx.update(|_, cx| AppConfig::get(cx).user().get("workbench.colorTheme").cloned());
    assert_eq!(saved, Some(json!("Light Modern")));
    assert_eq!(theme(&wb, cx).editor_bg, json_color("Light Modern", "editor.background"));
}

#[gpui_kit::test]
fn the_command_palette_lists_the_color_theme_command(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "palette");
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
    cx.simulate_input("color theme");
    cx.run_until_parked();
    assert_eq!(active_label(&wb, cx).as_deref(), Some("Preferences: Color Theme"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.quick.theme_pick), "running the command opens the theme picker");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn auto_detect_color_scheme_follows_the_injected_appearance(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "auto");
    set(cx, "window.autoDetectColorScheme", json!(true));
    set(cx, "workbench.preferredLightColorTheme", json!("Light+"));
    set(cx, "workbench.preferredDarkColorTheme", json!("Dark+"));
    set(cx, "workbench.colorTheme", json!("Visual Studio Dark"));
    cx.update(|_, cx| ThemeService::set_system_appearance(cx, WindowAppearance::Light));
    cx.run_until_parked();
    assert_eq!(active_id(cx), "Light+", "a light system picks preferredLightColorTheme");
    assert_eq!(theme(&wb, cx).editor_bg, json_color("Light+", "editor.background"));
    cx.update(|_, cx| ThemeService::set_system_appearance(cx, WindowAppearance::Dark));
    cx.run_until_parked();
    assert_eq!(active_id(cx), "Dark+");
    assert_eq!(theme(&wb, cx).editor_bg, json_color("Dark+", "editor.background"));
    // not following the system: workbench.colorTheme decides again
    set(cx, "window.autoDetectColorScheme", json!(false));
    assert_eq!(active_id(cx), "Visual Studio Dark");
    // the picker saves into the preferred setting of the mode it is detecting
    set(cx, "window.autoDetectColorScheme", json!(true));
    assert_eq!(cx.update(|_, cx| ThemeService::theme_setting_key(cx)), "workbench.preferredDarkColorTheme");
}
