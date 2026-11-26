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

use super::{SettingsView, install, model};
use crate::Workbench;
use vsr_settings::{SettingsRegistry, TocNode};

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
    cx.update(|cx| crate::config::AppConfig::install(cx, Some(path)));
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
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

/// The TOC node that lists `key` directly, skipping "Commonly Used" (which repeats settings).
#[gpui_kit::test]
fn settings_search_toc_behavior_hide_empties_the_toc_while_searching(cx: &mut TestAppContext) {
    let (wb, cx, _settings) = workbench(cx, "search-toc-hide");
    cx.update(|_, cx| crate::config::AppConfig::set_value(cx, "workbench.settings.settingsSearchTocBehavior", Some(serde_json::json!("hide"))).unwrap());
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let view = view_of(&wb, cx);
    cx.simulate_input("font size");
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| v.visible_toc().is_empty()), "hide: no TOC while searching");
    assert!(view.read_with(cx, |v, _| v.result_count()) > 0, "the list still shows the matches");
    // Back to the default (filter): the matching TOC nodes show again.
    cx.update(|_, cx| crate::config::AppConfig::set_value(cx, "workbench.settings.settingsSearchTocBehavior", None).unwrap());
    cx.run_until_parked();
    assert!(view.read_with(cx, |v, _| !v.visible_toc().is_empty()), "filter (default): the TOC narrows to matches");
}

#[gpui_kit::test]
fn settings_editor_json_opens_settings_json_instead_of_the_ui(cx: &mut TestAppContext) {
    let (wb, cx, settings) = workbench(cx, "editor-json");
    cx.update(|_, cx| crate::config::AppConfig::set_value(cx, "workbench.settings.editor", Some(serde_json::json!("json"))).unwrap());
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.settings.is_none()), "json: the Settings editor is not opened");
    assert!(wb.read_with(cx, |wb, _| wb.editors.open.iter().any(|e| e.path == settings)), "settings.json is opened as a tab");
}

fn group_of<'a>(toc: &'a [TocNode], key: &str) -> Option<&'a TocNode> {
    toc.iter().find_map(|n| {
        if n.id != "commonlyUsed" && n.settings.iter().any(|k| k == key) {
            Some(n)
        } else {
            group_of(&n.children, key)
        }
    })
}

#[gpui_kit::test]
fn search_filters_the_toc_to_matching_groups_and_esc_restores_it(cx: &mut TestAppContext) {
    let (wb, cx, _settings) = workbench(cx, "toc-search");
    cx.simulate_keystrokes("cmd-,");
    let view = view_of(&wb, cx);
    let full = view.read_with(cx, |v, _| v.visible_toc());
    assert!(full.iter().any(|(label, _)| label == "Commonly Used"));
    assert!(full.iter().all(|(_, count)| count.is_none()), "no counts outside search");

    cx.simulate_input("font size");
    cx.run_until_parked();
    let toc = view.read_with(cx, |v, _| v.visible_toc());
    let registry = SettingsRegistry::load_default();
    fn node_count(toc: &[TocNode]) -> usize {
        toc.iter().map(|n| 1 + node_count(&n.children)).sum()
    }
    assert!(toc.len() < node_count(registry.toc()), "the TOC narrows while searching");
    assert!(toc.iter().all(|(_, count)| count.is_some_and(|n| n > 0)), "only nodes with matches: {toc:?}");

    // The group that holds editor.fontSize is listed, with the matches inside it.
    let group = group_of(registry.toc(), "editor.fontSize").expect("editor.fontSize is in a group");
    let listed = toc.iter().find(|(label, _)| *label == group.label).expect("its group is listed");
    assert!(listed.1.is_some_and(|n| n >= 1));

    let result_count = view.read_with(cx, |v, _| v.result_count());
    assert_eq!(result_count, view.read_with(cx, |v, _| v.visible_keys()).len());

    // Clicking a group filters the list to its settings; the header count is unchanged.
    // debug_bounds takes a 'static selector; the leak is one short string in a test.
    let row: &'static str = Box::leak(format!("toc:{}", group.id).into_boxed_str());
    let bounds = cx.debug_bounds(row).expect("the group is listed in the TOC");
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
    let members = model::subtree_keys(registry.toc(), &group.id);
    let filtered = view.read_with(cx, |v, _| v.visible_keys());
    assert!(!filtered.is_empty() && filtered.iter().all(|k| members.contains(k)), "{filtered:?}");
    assert_eq!(view.read_with(cx, |v, _| v.result_count()), result_count);

    // Esc clears the search: the full TOC is back, collapsed and without counts.
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.visible_toc()), full);
}

// ---- search filters, filter menu, header buttons, advanced settings ---------------------------

fn open_settings(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Entity<SettingsView> {
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    view_of(wb, cx)
}

fn set_user(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| crate::config::AppConfig::set_value(cx, key, Some(value)).unwrap());
}

/// Run a search in the open view (the same path as typing in the box).
fn search(view: &Entity<SettingsView>, cx: &mut VisualTestContext, text: &str) {
    view.update_in(cx, |v, window, cx| v.set_search_text(text, window, cx));
    cx.run_until_parked();
}

fn keys(view: &Entity<SettingsView>, cx: &mut VisualTestContext) -> Vec<String> {
    view.read_with(cx, |v, _| v.visible_keys())
}

/// The first registry setting that carries the `advanced` tag, and a word of its key that is not
/// the whole key.
fn advanced_setting() -> (String, String) {
    fn collect(nodes: &[TocNode], out: &mut Vec<String>) {
        for n in nodes {
            out.extend(n.settings.iter().cloned());
            collect(&n.children, out);
        }
    }
    let reg = SettingsRegistry::load_default();
    let (mut plain, mut all) = (Vec::new(), Vec::new());
    collect(reg.toc_for(false), &mut plain);
    collect(reg.toc_for(true), &mut all);
    let key = all
        .into_iter()
        .find(|k| !plain.contains(k) && reg.get(k).unwrap().extension_id.is_none() && k.contains('.'))
        .expect("an advanced core setting in the second TOC");
    let tail = key.rsplit('.').next().unwrap().to_string();
    (key, tail)
}

#[gpui_kit::test]
fn modified_filter_lists_only_changed_settings_and_rows_show_the_indicator(cx: &mut TestAppContext) {
    let (wb, cx, _settings) = workbench(cx, "modified");
    set_user(cx, "editor.tabSize", serde_json::json!(2));
    set_user(cx, "editor.minimap.enabled", serde_json::json!(false));
    let view = open_settings(&wb, cx);

    search(&view, cx, "@modified");
    let mut found = keys(&view, cx);
    found.sort();
    assert_eq!(found, ["editor.minimap.enabled", "editor.tabSize"]);
    assert_eq!(view.read_with(cx, |v, _| v.result_count()), 2);
    assert!(cx.debug_bounds("modified:search:editor.tabSize").is_some(), "modified rows draw the left bar");

    // Combined with text: only modified settings that match the words.
    search(&view, cx, "tab size @modified");
    assert_eq!(keys(&view, cx), ["editor.tabSize"]);

    // An unmodified setting has no indicator.
    search(&view, cx, "@id:editor.lineNumbers");
    assert!(cx.debug_bounds("modified:search:editor.lineNumbers").is_none());
}

#[gpui_kit::test]
fn id_filter_takes_lists_and_wildcards(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "id-filter");
    let view = open_settings(&wb, cx);
    search(&view, cx, "@id:editor.tabSize,editor.fontSize");
    let mut found = keys(&view, cx);
    found.sort();
    assert_eq!(found, ["editor.fontSize", "editor.tabSize"]);
    search(&view, cx, "@id:editor.minimap.*");
    let found = keys(&view, cx);
    assert!(found.len() > 3 && found.iter().all(|k| k.starts_with("editor.minimap.")), "{found:?}");
    search(&view, cx, "@id:editor.tabSiz");
    assert!(keys(&view, cx).is_empty(), "ids match exactly");
}

#[gpui_kit::test]
fn ext_filter_lists_that_extensions_settings(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "ext-filter");
    let reg = SettingsRegistry::load_default();
    let ext_id = reg
        .keys()
        .filter_map(|k| reg.get(k))
        .find(|s| s.extension_id.is_some() && !s.tags.iter().any(|t| t == "advanced"))
        .and_then(|s| s.extension_id.clone())
        .unwrap();
    let view = open_settings(&wb, cx);
    search(&view, cx, &format!("@ext:{}", ext_id.to_uppercase()));
    let found = keys(&view, cx);
    assert!(!found.is_empty());
    assert!(found.iter().all(|k| reg.get(k).unwrap().extension_id.as_deref() == Some(ext_id.as_str())), "{found:?}");
    // Core settings are never listed.
    assert!(!found.iter().any(|k| k == "editor.fontSize"));
}

#[gpui_kit::test]
fn filter_menu_toggles_filters_in_the_search_box(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "filter-menu");
    set_user(cx, "editor.tabSize", serde_json::json!(2));
    let view = open_settings(&wb, cx);
    let button = cx.debug_bounds("settings-filter").expect("filter button is rendered");
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    let modified = cx.debug_bounds("filter-modified").expect("menu lists Modified");
    cx.simulate_click(modified.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), "@modified");
    assert_eq!(keys(&view, cx), ["editor.tabSize"]);

    // Preview then Experimental replace each other; Modified stays.
    for (item, expected) in [("filter-preview", "@modified @tag:preview"), ("filter-experimental", "@modified @tag:experimental")] {
        let b = cx.debug_bounds("settings-filter").unwrap();
        cx.simulate_click(b.center(), Default::default());
        cx.run_until_parked();
        let i = cx.debug_bounds(item).unwrap();
        cx.simulate_click(i.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), expected);
    }

    // Picking Modified again removes it.
    let b = cx.debug_bounds("settings-filter").unwrap();
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
    let i = cx.debug_bounds("filter-modified").unwrap();
    cx.simulate_click(i.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), "@tag:experimental");

    // "Setting ID..." leaves the caret after `@id:` for the user to type the key.
    let b = cx.debug_bounds("settings-filter").unwrap();
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
    let i = cx.debug_bounds("filter-id").unwrap();
    cx.simulate_click(i.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), "@tag:experimental @id:");
}

#[gpui_kit::test]
fn filter_button_closes_its_open_menu_when_clicked_again(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "funnel-toggle");
    let _view = open_settings(&wb, cx);
    let button = cx.debug_bounds("settings-filter").expect("filter button is rendered");
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("filter-modified").is_some(), "the funnel opens the menu");
    let button = cx.debug_bounds("settings-filter").unwrap();
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("filter-modified").is_none(), "the funnel closes an open menu");
    cx.simulate_click(button.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("filter-modified").is_some(), "and reopens it on the next click");
}

#[gpui_kit::test]
fn lang_and_feature_filters_show_no_settings_until_applied(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "lang-filter");
    let view = open_settings(&wb, cx);
    for q in ["@lang:css", "@feature:editor", "@lang:css @feature:x"] {
        search(&view, cx, q);
        assert!(keys(&view, cx).is_empty(), "{q}: no settings listed (not applied yet)");
        assert_eq!(view.read_with(cx, |v, _| v.result_count()), 0, "{q}");
    }
    // Applied filters still narrow as before.
    search(&view, cx, "@lang:css @id:editor.tabSize");
    assert_eq!(keys(&view, cx), ["editor.tabSize"]);
}

#[gpui_kit::test]
fn advanced_tag_is_case_insensitive_and_needs_a_whole_key(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "advanced-case");
    let (key, tail) = advanced_setting();
    let view = open_settings(&wb, cx);

    search(&view, cx, &format!("{tail} @tag:ADVANCED"));
    assert!(keys(&view, cx).contains(&key), "@tag:ADVANCED shows advanced settings");

    // The key inside a longer word does not name the setting, so it stays hidden.
    search(&view, cx, &format!("x{key}y"));
    assert!(!keys(&view, cx).contains(&key), "a key inside a longer word does not show it");
    // Typed as a whole word it does.
    search(&view, cx, &key);
    assert!(keys(&view, cx).contains(&key), "typed as a whole key it is shown");
}

#[gpui_kit::test]
fn more_actions_copies_setting_id_and_json(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "copy-json");
    set_user(cx, "editor.tabSize", serde_json::json!(2));
    let view = open_settings(&wb, cx);
    search(&view, cx, "@id:editor.tabSize,editor.wordWrap");
    for (key, expected) in [("editor.tabSize", "\"editor.tabSize\": 2"), ("editor.wordWrap", "\"editor.wordWrap\": \"off\"")] {
        let gear: &'static str = Box::leak(format!("more-actions:search:{key}").into_boxed_str());
        let b = cx.debug_bounds(gear).expect("gear is rendered");
        cx.simulate_click(b.center(), Default::default());
        cx.run_until_parked();
        let item: &'static str = Box::leak(format!("copy-json:search:{key}").into_boxed_str());
        let i = cx.debug_bounds(item).expect("menu lists Copy Setting as JSON");
        cx.simulate_click(i.center(), Default::default());
        cx.run_until_parked();
        assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some(expected));
    }
    // Copy Setting ID still works.
    let gear = cx.debug_bounds("more-actions:search:editor.tabSize").unwrap();
    cx.simulate_click(gear.center(), Default::default());
    cx.run_until_parked();
    let id = cx.debug_bounds("copy-id:search:editor.tabSize").unwrap();
    cx.simulate_click(id.center(), Default::default());
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("editor.tabSize"));
}

#[gpui_kit::test]
fn header_open_settings_json_button_opens_the_file(cx: &mut TestAppContext) {
    let (wb, cx, settings) = workbench(cx, "header-json");
    open_settings(&wb, cx);
    let b = cx.debug_bounds("open-settings-json").expect("the header has an Open Settings (JSON) button");
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.editors.open.iter().any(|e| e.path == settings)), "settings.json is opened as a tab");
}

#[gpui_kit::test]
fn workspace_tab_explains_that_only_user_settings_exist(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "scopes");
    open_settings(&wb, cx);
    assert!(cx.debug_bounds("scope-tab:User").is_some());
    let ws = cx.debug_bounds("scope-tab:Workspace").expect("the Workspace tab is shown");
    assert!(cx.debug_bounds("settings-workspace-note").is_none());
    cx.simulate_click(ws.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-workspace-note").is_some());
    let user = cx.debug_bounds("scope-tab:User").unwrap();
    cx.simulate_click(user.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("settings-workspace-note").is_none());
}

#[gpui_kit::test]
fn advanced_settings_are_hidden_until_asked_for(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "advanced");
    let (key, tail) = advanced_setting();
    let view = open_settings(&wb, cx);
    assert!(!keys(&view, cx).contains(&key), "the grouped list has no advanced settings by default");

    search(&view, cx, &tail);
    assert!(!keys(&view, cx).contains(&key), "search leaves advanced settings out");
    // `@tag:advanced` shows them, and so does naming the setting by id.
    search(&view, cx, &format!("{tail} @tag:advanced"));
    assert!(keys(&view, cx).contains(&key));
    search(&view, cx, &format!("@id:{key}"));
    assert_eq!(keys(&view, cx), std::slice::from_ref(&key));
    // The filter menu's Advanced entry adds the tag.
    search(&view, cx, "");
    let b = cx.debug_bounds("settings-filter").unwrap();
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
    let i = cx.debug_bounds("filter-advanced").unwrap();
    cx.simulate_click(i.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |v, _| v.query().to_string()), "@tag:advanced");
    assert!(keys(&view, cx).contains(&key));
}

#[gpui_kit::test]
fn always_show_advanced_settings_adds_them_to_the_toc_and_search(cx: &mut TestAppContext) {
    let (wb, cx, _s) = workbench(cx, "always-advanced");
    let (key, tail) = advanced_setting();
    let view = open_settings(&wb, cx);
    let before = keys(&view, cx).len();
    assert!(!keys(&view, cx).contains(&key));

    set_user(cx, "workbench.settings.alwaysShowAdvancedSettings", serde_json::json!(true));
    cx.run_until_parked();
    let after = keys(&view, cx);
    assert!(after.len() > before && after.contains(&key), "the second TOC is used");
    search(&view, cx, &tail);
    assert!(keys(&view, cx).contains(&key));
    assert!(view.read_with(cx, |v, _| !v.visible_toc().is_empty()));

    // Turning it off goes back to the default TOC.
    search(&view, cx, "");
    cx.update(|_, cx| crate::config::AppConfig::set_value(cx, "workbench.settings.alwaysShowAdvancedSettings", None).unwrap());
    cx.run_until_parked();
    assert!(!keys(&view, cx).contains(&key));
}
