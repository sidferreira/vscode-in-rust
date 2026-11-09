//! Headless E2E tests for the Search view (docs/testing.md, tier 2; scenarios in
//! tests/e2e-scenarios/search.md). They drive the real `Workbench` + `SearchView` in gpui's test
//! platform over a temp workspace: Shift+Cmd+F, typing, the debounce, toggles, include globs,
//! clicking a match. No pixel metrics are asserted (the test text system is a no-op).

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use super::model::{Row, result_count_message};
use super::{SearchView, Status};
use crate::config::AppConfig;
use crate::{ActivityView, Workbench};

impl SearchView {
    fn focus_include(&self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        self.include.update(cx, |s, cx| s.focus(window, cx));
    }
}

fn workspace(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("rscode-e2e-search-{name}-{}", std::process::id()));
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::create_dir_all(dir.join("docs")).unwrap();
    fs::write(dir.join("a.txt"), "alpha beta\nAlpha\n").unwrap();
    fs::write(dir.join("src/b.rs"), "alpha alpha\n").unwrap();
    fs::write(dir.join("docs/c.md"), "nothing here\n").unwrap();
    dir
}

fn start<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
    });
    let dir = workspace(name);
    let root = dir.clone();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    cx.run_until_parked();
    (wb, cx, dir)
}

fn view(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Entity<SearchView> {
    wb.read_with(cx, |wb, _| wb.search.clone()).expect("search view exists")
}

/// Let the debounce elapse and the search finish.
fn settle(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(Duration::from_millis(400));
    cx.run_until_parked();
}

fn summary(v: &Entity<SearchView>, cx: &mut VisualTestContext) -> Option<String> {
    v.read_with(cx, |v, _| v.summary())
}

/// (file name, match count) per result file.
fn files(v: &Entity<SearchView>, cx: &mut VisualTestContext) -> Vec<(String, usize)> {
    v.read_with(cx, |v, _| v.results().files.iter().map(|f| (f.name.clone(), f.matches.len())).collect())
}

#[gpui_kit::test]
fn shift_cmd_f_then_typing_searches_after_the_debounce(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "type");
    cx.simulate_keystrokes("cmd-shift-f");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.view == ActivityView::Search && wb.sidebar_visible));
    let v = view(&wb, cx);

    cx.simulate_input("alpha");
    cx.run_until_parked();
    assert_eq!(v.read_with(cx, |v, cx| v.query_text(cx)), "alpha");
    // Search on type waits for searchOnTypeDebouncePeriod (300ms).
    assert_eq!(summary(&v, cx), None);
    settle(cx);

    assert_eq!(summary(&v, cx), Some(result_count_message(4, 2)));
    assert_eq!(summary(&v, cx).as_deref(), Some("4 results in 2 files"));
    assert_eq!(files(&v, cx), [("a.txt".into(), 2), ("b.rs".into(), 2)]);
    assert!(matches!(v.read_with(cx, |v, _| v.status().clone()), Status::Done { limit_hit: false, .. }));
}

#[gpui_kit::test]
fn match_case_toggle_changes_counts(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "case");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert_eq!(summary(&v, cx).as_deref(), Some("4 results in 2 files"));

    cx.simulate_keystrokes("alt-cmd-c"); // Match Case
    cx.run_until_parked();
    assert_eq!(summary(&v, cx).as_deref(), Some("3 results in 2 files"));
    assert_eq!(files(&v, cx), [("a.txt".into(), 1), ("b.rs".into(), 2)]);

    cx.simulate_keystrokes("alt-cmd-c");
    cx.run_until_parked();
    assert_eq!(summary(&v, cx).as_deref(), Some("4 results in 2 files"));
}

#[gpui_kit::test]
fn whole_word_and_regex_toggles(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "word");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("alph");
    settle(cx);
    assert_eq!(summary(&v, cx).as_deref(), Some("4 results in 2 files"));
    cx.simulate_keystrokes("alt-cmd-w");
    cx.run_until_parked();
    assert_eq!(summary(&v, cx), None, "whole word `alph` matches nothing");
    cx.simulate_keystrokes("alt-cmd-w alt-cmd-r");
    cx.run_until_parked();
    cx.simulate_input("a"); // regex `alpha`, typed as extra char: pattern is now "alpha"
    settle(cx);
    assert_eq!(summary(&v, cx).as_deref(), Some("4 results in 2 files"));
}

#[gpui_kit::test]
fn include_glob_narrows_the_results(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "include");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert_eq!(summary(&v, cx).as_deref(), Some("4 results in 2 files"));

    cx.simulate_keystrokes("ctrl-shift-j"); // Toggle Search Details
    cx.run_until_parked();
    assert!(v.read_with(cx, |v, _| v.details_open));
    cx.update(|window, app| v.update(app, |v, cx| v.focus_include(window, cx)));
    cx.simulate_input("*.rs");
    settle(cx);
    assert_eq!(summary(&v, cx).as_deref(), Some("2 results in 1 file"));
    assert_eq!(files(&v, cx), [("b.rs".into(), 2)]);
}

#[gpui_kit::test]
fn no_results_status_and_message(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "none");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("zzzzzz");
    settle(cx);
    assert_eq!(summary(&v, cx), None);
    assert!(matches!(v.read_with(cx, |v, _| v.status().clone()), Status::Done { .. }));
    assert_eq!(v.read_with(cx, |v, _| v.results().file_count()), 0);
}

#[gpui_kit::test]
fn clicking_a_match_opens_the_file_and_selects_it(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "open");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    // rows: [a.txt, m, m, b.rs, m, m]; click the second match in b.rs ("alpha alpha": cols 7..12).
    let rows = v.read_with(cx, |v, _| v.rows.clone());
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[5], Row::Match(1, 1));
    let b = cx.debug_bounds("search-row:5").expect("row is laid out");
    cx.simulate_mouse_move(b.center(), None, Default::default());
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();

    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().expect("file opened");
        assert_eq!(e.path, dir.join("src/b.rs"));
        let s = e.editor.selection();
        assert_eq!((s.anchor.line, s.anchor.column), (1, 7));
        assert_eq!((s.active.line, s.active.column), (1, 12));
        assert_eq!(e.editor.selected_text(), "alpha");
    });
}

#[gpui_kit::test]
fn file_rows_fold_and_collapse_all(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "fold");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert_eq!(v.read_with(cx, |v, _| v.rows.len()), 6);
    cx.update(|_, app| v.update(app, |v, cx| v.activate(Row::File(0), cx)));
    assert_eq!(v.read_with(cx, |v, _| v.rows.len()), 4);
    cx.update(|_, app| v.update(app, |v, cx| v.toggle_collapse_all(cx))); // something expanded: collapse all
    assert_eq!(v.read_with(cx, |v, _| v.rows.len()), 2);
    cx.update(|_, app| v.update(app, |v, cx| v.toggle_collapse_all(cx))); // now Expand All
    assert_eq!(v.read_with(cx, |v, _| v.rows.len()), 6);
}

#[gpui_kit::test]
fn clearing_the_query_clears_results(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "clear");
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert!(summary(&v, cx).is_some());
    cx.simulate_keystrokes("backspace backspace backspace backspace backspace");
    settle(cx);
    assert_eq!(summary(&v, cx), None);
    assert_eq!(v.read_with(cx, |v, _| v.status().clone()), Status::Idle);
}

// ---- search settings (`search.*`, `files.exclude`) from the configuration -------------------

/// Like `start`, but with a temp settings.json (never the user's) installed as the configuration,
/// and `docs/c.md` holding the word the tests search for.
fn start_configured<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let dir = workspace(name);
    fs::write(dir.join("docs/c.md"), "alpha in docs\n").unwrap();
    let settings_dir = std::env::temp_dir().join(format!("rscode-e2e-search-settings-{name}-{}", std::process::id()));
    fs::create_dir_all(&settings_dir).unwrap();
    let settings = settings_dir.join("settings.json");
    fs::write(&settings, "{}").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        AppConfig::install(cx, Some(settings));
    });
    let root = dir.clone();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    cx.run_until_parked();
    (wb, cx, dir)
}

fn set_setting(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
}

#[gpui_kit::test]
fn search_exclude_setting_hides_a_folder_and_reruns_the_query(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start_configured(cx, "exclude");
    cx.simulate_keystrokes("cmd-shift-f");
    cx.run_until_parked();
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert!(files(&v, cx).iter().any(|(n, _)| n == "c.md"), "docs/c.md matches before the setting");
    set_setting(cx, "search.exclude", json!({ "**/docs": true }));
    settle(cx);
    let after = files(&v, cx);
    assert!(!after.iter().any(|(n, _)| n == "c.md"), "docs/ is excluded and the query ran again: {after:?}");
    assert!(after.iter().any(|(n, _)| n == "a.txt"));
}

#[gpui_kit::test]
fn smart_case_setting_makes_an_uppercase_query_case_sensitive(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start_configured(cx, "smartcase");
    cx.simulate_keystrokes("cmd-shift-f");
    cx.run_until_parked();
    let v = view(&wb, cx);
    cx.simulate_input("Alpha");
    settle(cx);
    let insensitive = files(&v, cx);
    assert!(insensitive.iter().any(|(n, _)| n == "b.rs"), "smart case is off: {insensitive:?}");
    set_setting(cx, "search.smartCase", json!(true));
    settle(cx);
    // Only the line that really reads "Alpha" in a.txt.
    assert_eq!(files(&v, cx), vec![("a.txt".to_string(), 1)]);
}

#[gpui_kit::test]
fn search_on_type_off_waits_for_enter(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start_configured(cx, "ontype");
    set_setting(cx, "search.searchOnType", json!(false));
    cx.simulate_keystrokes("cmd-shift-f");
    cx.run_until_parked();
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert_eq!(summary(&v, cx), None, "typing does not search");
    cx.simulate_keystrokes("enter");
    settle(cx);
    assert!(summary(&v, cx).is_some(), "Enter searches");
}

#[gpui_kit::test]
fn max_results_setting_limits_the_results_and_reruns_the_query(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start_configured(cx, "maxresults");
    cx.simulate_keystrokes("cmd-shift-f");
    cx.run_until_parked();
    let v = view(&wb, cx);
    cx.simulate_input("alpha");
    settle(cx);
    assert!(!matches!(v.read_with(cx, |v, _| v.status().clone()), Status::Done { limit_hit: true, .. }));
    set_setting(cx, "search.maxResults", json!(2));
    settle(cx);
    assert!(matches!(v.read_with(cx, |v, _| v.status().clone()), Status::Done { limit_hit: true, .. }));
}
