//! Headless end-to-end tests for settings taking effect (docs/testing.md, tier 2).
//!
//! The real `Workbench` runs in gpui's test platform with an injected temp settings.json, so the
//! real user settings are never touched. Layout is asserted with `debug_selector` /
//! `debug_bounds`; the test platform's text system is a no-op, so no font metrics are asserted.
//!
//! Scenario source: tests/e2e-scenarios/preferences.md ("Settings take effect"). Our own tests.

use std::fs;
use std::path::{Path, PathBuf};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::{AppConfig, POLL_INTERVAL};
use crate::Workbench;

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-config-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Fixture {
    dir: PathBuf,
    settings: PathBuf,
}

/// A workbench over a temp folder with `files`, whose configuration lives in `<dir>/settings.json`
/// (written first from `initial` when given).
fn workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    initial: Option<&str>,
    files: &[(&str, &str)],
) -> (Entity<Workbench>, &'a mut VisualTestContext, Fixture) {
    let dir = unique_dir(name);
    let settings = dir.join("settings.json");
    if let Some(text) = initial {
        fs::write(&settings, text).unwrap();
    }
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    let path = settings.clone();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        AppConfig::install(cx, Some(path));
    });
    let root = dir.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    (wb, vcx, Fixture { dir, settings })
}

/// Lets the settings-file watcher run `n` times (a change is applied on the second sighting).
fn ticks(cx: &mut VisualTestContext, n: usize) {
    for _ in 0..n {
        cx.executor().advance_clock(POLL_INTERVAL);
        cx.run_until_parked();
    }
}

fn open_file(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> gpui_kit::Bounds<gpui_kit::Pixels> {
    cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} is not rendered"))
}

fn editor_value(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().value())
}

/// Searches the Settings editor for one setting and returns its row id prefix.
fn search_setting(wb: &Entity<Workbench>, cx: &mut VisualTestContext, key: &str) {
    cx.simulate_keystrokes("cmd-,");
    cx.run_until_parked();
    let _ = wb;
    cx.simulate_input(&format!("@id:{key}"));
    cx.run_until_parked();
}

// ---- (a) side bar location through the Settings UI --------------------------------------------

#[gpui_kit::test]
fn side_bar_location_right_moves_the_side_bar_and_activity_bar(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "sidebar", None, &[("a.txt", "x")]);

    // Default: activity bar, side bar, then the editor group, left to right.
    let (activity, side, editor) = (bounds(cx, "activity-bar"), bounds(cx, "side-bar"), bounds(cx, "editor-group"));
    assert!(activity.right() <= side.left() && side.right() <= editor.left(), "{activity:?} {side:?} {editor:?}");

    // Set it from the Settings editor: open the dropdown and pick "right".
    search_setting(&wb, cx, "workbench.sideBar.location");
    let select = bounds(cx, "select:search:workbench.sideBar.location");
    cx.simulate_click(select.center(), Default::default());
    cx.run_until_parked();
    let right = bounds(cx, "option:search:workbench.sideBar.location:1");
    cx.simulate_click(right.center(), Default::default());
    cx.run_until_parked();

    let on_disk = vsr_settings::UserSettings::load(&f.settings).unwrap();
    assert_eq!(on_disk.get("workbench.sideBar.location"), Some(&json!("right")));

    // Applied immediately: editor group, then side bar, then the activity bar on the outer edge.
    let (activity, side, editor) = (bounds(cx, "activity-bar"), bounds(cx, "side-bar"), bounds(cx, "editor-group"));
    assert!(editor.right() <= side.left(), "side bar right of the editor: {side:?} vs {editor:?}");
    assert!(side.right() <= activity.left(), "activity bar right of the side bar: {activity:?} vs {side:?}");
    let sash = bounds(cx, "sidebar-resize");
    assert!(sash.right() <= side.left() + gpui_kit::px(1.) && sash.left() >= editor.right() - gpui_kit::px(1.), "resize handle sits between the editor and the side bar");
}

#[gpui_kit::test]
fn activity_bar_hidden_and_top_strip(cx: &mut TestAppContext) {
    let (_wb, cx, _f) = workbench(cx, "activity", None, &[]);
    set(cx, "workbench.activityBar.location", json!("hidden"));
    assert!(cx.debug_bounds("activity-bar").is_none());
    assert!(cx.debug_bounds("side-bar").is_some());

    set(cx, "workbench.activityBar.location", json!("top"));
    let (strip, side) = (bounds(cx, "activity-bar"), bounds(cx, "side-bar"));
    assert!(strip.top() >= side.top() && strip.bottom() <= side.bottom(), "strip lives inside the side bar column");
    assert!(strip.size.width > strip.size.height, "horizontal");
}

// ---- (b) status bar visibility --------------------------------------------------------------

#[gpui_kit::test]
fn status_bar_visible_false_hides_the_status_bar(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "statusbar", None, &[]);
    assert!(cx.debug_bounds("status-bar").is_some());

    search_setting(&wb, cx, "workbench.statusBar.visible");
    let checkbox = bounds(cx, "checkbox:search:workbench.statusBar.visible");
    cx.simulate_click(checkbox.center(), Default::default());
    cx.run_until_parked();

    assert!(cx.debug_bounds("status-bar").is_none(), "status bar is gone");
    let on_disk = vsr_settings::UserSettings::load(&f.settings).unwrap();
    assert_eq!(on_disk.get("workbench.statusBar.visible"), Some(&json!(false)));

    // Toggling back restores the default (and removes the key).
    let checkbox = bounds(cx, "checkbox:search:workbench.statusBar.visible");
    cx.simulate_click(checkbox.center(), Default::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("status-bar").is_some());
    assert!(!vsr_settings::UserSettings::load(&f.settings).unwrap().is_modified("workbench.statusBar.visible"));
}

#[gpui_kit::test]
fn show_tabs_none_hides_the_tab_strip(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "tabs", None, &[("a.txt", "a"), ("b.txt", "b")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));
    open_file(&wb, cx, &f.dir.join("b.txt"));
    assert!(cx.debug_bounds("tab-strip").is_some());
    assert!(cx.debug_bounds("tab:0").is_some() && cx.debug_bounds("tab:1").is_some());

    set(cx, "workbench.editor.showTabs", json!("single"));
    assert!(cx.debug_bounds("tab:1").is_some(), "the active tab stays");
    assert!(cx.debug_bounds("tab:0").is_none(), "other tabs are not shown");

    set(cx, "workbench.editor.showTabs", json!("none"));
    assert!(cx.debug_bounds("tab-strip").is_none());
}

// ---- (c) tab size / insert spaces ----------------------------------------------------------

#[gpui_kit::test]
fn tab_size_two_makes_tab_insert_two_spaces_and_applies_live(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "tabsize", Some(r#"{ "editor.tabSize": 2 }"#), &[("a.txt", "")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));

    cx.simulate_keystrokes("tab");
    assert_eq!(editor_value(&wb, cx), "  ");
    let status = wb.read_with(cx, |wb, _| crate::editor_view::indent_status(wb.editors.active_editor().unwrap()));
    assert_eq!(status, "Spaces: 2");

    // Changing the setting applies to the editor that is already open.
    set(cx, "editor.tabSize", json!(4));
    cx.simulate_keystrokes("tab");
    assert_eq!(editor_value(&wb, cx), "    ");

    // insertSpaces=false inserts a tab character, and the status bar follows.
    set(cx, "editor.insertSpaces", json!(false));
    cx.simulate_keystrokes("tab");
    assert_eq!(editor_value(&wb, cx), "    \t");
    let status = wb.read_with(cx, |wb, _| crate::editor_view::indent_status(wb.editors.active_editor().unwrap()));
    assert_eq!(status, "Tab Size: 4");

    // A file opened afterwards gets the current options.
    fs::write(f.dir.join("b.txt"), "").unwrap();
    open_file(&wb, cx, &f.dir.join("b.txt"));
    cx.simulate_keystrokes("tab");
    assert_eq!(editor_value(&wb, cx), "\t");
}

// ---- (d) external edits are picked up live ---------------------------------------------------

#[gpui_kit::test]
fn external_edit_of_settings_json_applies_line_numbers_off(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "external", None, &[("a.txt", "one\ntwo\n")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));
    let with_numbers = bounds(cx, "gutter:1").size.width;

    // Another program writes the file.
    fs::write(&f.settings, "{ // edited elsewhere\n  \"editor.lineNumbers\": \"off\",\n}\n").unwrap();
    ticks(cx, 1);
    assert_eq!(bounds(cx, "gutter:1").size.width, with_numbers, "a change is applied only after it is seen twice (debounce)");
    ticks(cx, 3);

    let without_numbers = bounds(cx, "gutter:1").size.width;
    assert!(without_numbers < with_numbers, "gutter shrinks: {without_numbers:?} < {with_numbers:?}");
    assert_eq!(wb.read_with(cx, |wb, _| wb.editor_settings.line_numbers), crate::config::settings::LineNumbers::Off);

    // Removing the key restores the default.
    fs::write(&f.settings, "{}\n").unwrap();
    ticks(cx, 4);
    assert_eq!(bounds(cx, "gutter:1").size.width, with_numbers);
}

#[gpui_kit::test]
fn own_writes_are_not_reapplied_and_invalid_files_are_kept_out(cx: &mut TestAppContext) {
    let (_wb, cx, f) = workbench(cx, "echo", None, &[]);
    set(cx, "editor.tabSize", json!(3));
    ticks(cx, 4);
    let still_ours = cx.update(|_, cx| AppConfig::get(cx).last_change().affects("editor.tabSize"));
    assert!(still_ours, "the watcher did not re-apply our own write (that would clear last_change)");

    // A syntax error keeps the previous configuration and is reported; writes are refused.
    fs::write(&f.settings, "{ \"editor.tabSize\": ").unwrap();
    ticks(cx, 4);
    let (tab_size, error) = cx.update(|_, cx| {
        let c = AppConfig::get(cx);
        (c.configuration().get_f64("editor.tabSize"), c.error().map(str::to_string))
    });
    assert_eq!(tab_size, Some(3.0));
    assert!(error.is_some());
    let refused = cx.update(|_, cx| AppConfig::set_value(cx, "editor.fontSize", Some(json!(20))));
    assert!(refused.is_err());
    assert_eq!(fs::read_to_string(&f.settings).unwrap(), "{ \"editor.tabSize\": ", "the broken file is left as it is");

    // Fixing the file clears the error and applies it.
    fs::write(&f.settings, r#"{ "editor.tabSize": 6 }"#).unwrap();
    ticks(cx, 4);
    let (tab_size, error) = cx.update(|_, cx| {
        let c = AppConfig::get(cx);
        (c.configuration().get_f64("editor.tabSize"), c.error().map(str::to_string))
    });
    assert_eq!((tab_size, error), (Some(6.0), None));
}

// ---- (e) save transforms ---------------------------------------------------------------------

#[gpui_kit::test]
fn insert_final_newline_and_trim_whitespace_apply_on_save(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "save",
        Some(r#"{ "files.insertFinalNewline": true, "files.trimTrailingWhitespace": true }"#),
        &[("a.txt", "a  \nb"), ("keep.txt", "c  \nd")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(fs::read_to_string(f.dir.join("a.txt")).unwrap(), "a\nb\n");
    assert!(!wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().is_dirty()));

    // The transforms are ordinary edits: undo brings the original text back.
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(editor_value(&wb, cx), "a\nb");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(editor_value(&wb, cx), "a  \nb");

    // Turning both off (live) saves the text as is.
    set(cx, "files.insertFinalNewline", json!(false));
    set(cx, "files.trimTrailingWhitespace", json!(false));
    open_file(&wb, cx, &f.dir.join("keep.txt"));
    cx.simulate_keystrokes("x cmd-s");
    assert_eq!(fs::read_to_string(f.dir.join("keep.txt")).unwrap(), "xc  \nd");
}

// ---- explorer, coverage marker -----------------------------------------------------------------

#[gpui_kit::test]
fn files_exclude_applies_to_the_explorer(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "explorer", None, &[("a.txt", ""), ("b.log", "")]);
    fs::create_dir_all(f.dir.join(".git")).unwrap();
    let names = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| -> Vec<String> {
        wb.read_with(cx, |wb, _| {
            let root = wb.root.clone();
            wb.children[&root].iter().map(|e| e.name.clone()).collect()
        })
    };
    // The default value hides .git and OS files; settings.json is not excluded.
    assert!(!names(&wb, cx).contains(&".git".to_string()));
    set(cx, "files.exclude", json!({ "**/*.log": true }));
    let shown = names(&wb, cx);
    assert!(!shown.contains(&".git".to_string()), "the user's map merges with the default: {shown:?}");
    assert!(!shown.contains(&"b.log".to_string()) && shown.contains(&"a.txt".to_string()));
    // Showing .git again takes an explicit false.
    set(cx, "files.exclude", json!({ "**/*.log": true, "**/.git": false }));
    assert!(names(&wb, cx).contains(&".git".to_string()));
}

#[gpui_kit::test]
fn settings_editor_marks_unwired_settings_only(cx: &mut TestAppContext) {
    let (wb, cx, _f) = workbench(cx, "coverage", None, &[]);
    search_setting(&wb, cx, "editor.minimap.enabled");
    assert!(cx.debug_bounds("unsupported:search:editor.minimap.enabled").is_some(), "unwired setting is marked");
    cx.simulate_keystrokes("cmd-a");
    cx.simulate_input("@id:editor.fontSize");
    cx.run_until_parked();
    assert!(cx.debug_bounds("unsupported:search:editor.fontSize").is_none(), "wired setting is not");
}

// ---- (e) editor groups: preview, order, limit, closing deleted files ----------------------------

/// The file names of the open tabs, in tab order.
fn tab_titles(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, _| wb.editors.open.iter().map(|e| e.title.clone()).collect())
}

/// Opens `name` the way a single click in the Explorer does (a preview unless previews are off).
fn click_explorer(wb: &Entity<Workbench>, cx: &mut VisualTestContext, dir: &Path, name: &str) {
    let path = dir.join(name);
    wb.update(cx, |wb, cx| {
        wb.open_from(&path, crate::config::settings::OpenSource::Explorer { double_click: false });
        cx.notify();
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn single_click_previews_and_the_next_click_replaces_the_preview(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "preview", None, &[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]);
    click_explorer(&wb, cx, &f.dir, "a.txt");
    assert!(wb.read_with(cx, |wb, _| wb.editors.is_preview(0)), "a single click previews");
    click_explorer(&wb, cx, &f.dir, "b.txt");
    assert_eq!(tab_titles(&wb, cx), ["b.txt"], "the preview is replaced");
    assert!(wb.read_with(cx, |wb, _| wb.editors.is_preview(0)));

    // Editing the preview pins it: the next preview opens beside it.
    cx.simulate_keystrokes("x");
    assert_eq!(editor_value(&wb, cx), "xb");
    cx.run_until_parked();
    assert!(!wb.read_with(cx, |wb, _| wb.editors.is_preview(0)), "an edit pins the preview");
    click_explorer(&wb, cx, &f.dir, "c.txt");
    assert_eq!(tab_titles(&wb, cx), ["b.txt", "c.txt"]);
}

#[gpui_kit::test]
fn turning_previews_off_opens_every_file_pinned(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "preview-off", Some(r#"{ "workbench.editor.enablePreview": false }"#), &[("a.txt", "a"), ("b.txt", "b")]);
    click_explorer(&wb, cx, &f.dir, "a.txt");
    click_explorer(&wb, cx, &f.dir, "b.txt");
    assert_eq!(tab_titles(&wb, cx), ["a.txt", "b.txt"]);
    assert!(wb.read_with(cx, |wb, _| !wb.editors.is_preview(0) && !wb.editors.is_preview(1)));
}

#[gpui_kit::test]
fn open_positioning_first_puts_new_tabs_at_the_start(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "positioning", None, &[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));
    open_file(&wb, cx, &f.dir.join("b.txt"));
    assert_eq!(tab_titles(&wb, cx), ["a.txt", "b.txt"], "right of the active tab by default");

    set(cx, "workbench.editor.openPositioning", json!("first"));
    open_file(&wb, cx, &f.dir.join("c.txt"));
    assert_eq!(tab_titles(&wb, cx), ["c.txt", "a.txt", "b.txt"]);
}

#[gpui_kit::test]
fn limit_closes_the_least_recently_used_editor_when_exceeded(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "limit", None, &[("a.txt", "a"), ("b.txt", "b"), ("c.txt", "c")]);
    set(cx, "workbench.editor.limit.enabled", json!(true));
    set(cx, "workbench.editor.limit.value", json!(2));
    open_file(&wb, cx, &f.dir.join("a.txt"));
    open_file(&wb, cx, &f.dir.join("b.txt"));
    open_file(&wb, cx, &f.dir.join("c.txt"));
    assert_eq!(tab_titles(&wb, cx), ["b.txt", "c.txt"], "a.txt was used least recently");
}

#[gpui_kit::test]
fn close_on_file_delete_closes_clean_tabs_and_keeps_dirty_ones(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "delete", None, &[("a.txt", "a"), ("b.txt", "b")]);
    set(cx, "workbench.editor.closeOnFileDelete", json!(true));
    open_file(&wb, cx, &f.dir.join("a.txt"));
    open_file(&wb, cx, &f.dir.join("b.txt"));
    cx.simulate_keystrokes("x");
    cx.run_until_parked();
    fs::remove_file(f.dir.join("a.txt")).unwrap();
    fs::remove_file(f.dir.join("b.txt")).unwrap();
    cx.executor().advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(tab_titles(&wb, cx), ["b.txt"], "the clean tab closes, the edited one stays");
}

#[gpui_kit::test]
fn close_on_file_delete_off_keeps_the_tab(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "delete-off", None, &[("a.txt", "a")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));
    fs::remove_file(f.dir.join("a.txt")).unwrap();
    cx.executor().advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(tab_titles(&wb, cx), ["a.txt"]);
}

