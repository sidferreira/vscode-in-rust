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
    fn focus_replace(&self, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        self.replace.update(cx, |s, cx| s.focus(window, cx));
    }

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

// ---- replace (ADR-0029) ----------------------------------------------------------------------

/// Search for `query`, open the replace row and type `replacement` into it.
fn search_and_type_replacement(
    wb: &Entity<Workbench>,
    cx: &mut VisualTestContext,
    query: &str,
    replacement: &str,
) -> Entity<SearchView> {
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(wb, cx);
    cx.simulate_input(query);
    settle(cx);
    open_replace(&v, cx, replacement);
    v
}

/// Open the replace row, focus its input and type `replacement`.
fn open_replace(v: &Entity<SearchView>, cx: &mut VisualTestContext, replacement: &str) {
    v.update(cx, |v, cx| v.toggle_replace(cx));
    cx.run_until_parked();
    cx.update(|window, app| v.update(app, |v, cx| v.focus_replace(window, cx)));
    cx.simulate_input(replacement);
    cx.run_until_parked();
}

fn click_selector(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} is laid out"));
    cx.simulate_mouse_move(b.center(), None, Default::default());
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
}

fn read(path: &std::path::Path) -> String {
    fs::read_to_string(path).unwrap()
}

#[gpui_kit::test]
fn replace_all_rewrites_files_on_disk_and_open_editors_undoably(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "replace-all");
    let a = dir.join("a.txt");
    wb.update(cx, |wb, cx| {
        wb.open(&a);
        cx.notify();
    });
    cx.run_until_parked();
    let v = search_and_type_replacement(&wb, cx, "alpha", "omega");
    assert!(v.read_with(cx, |v, _| v.replace_active()));
    assert_eq!(v.read_with(cx, |v, cx| v.replace_text(cx)), "omega");

    // Alt+Cmd+Enter asks first; nothing is touched yet.
    cx.simulate_keystrokes("alt-cmd-enter");
    cx.run_until_parked();
    assert_eq!(
        v.read_with(cx, |v, _| v.confirm.clone()).as_deref(),
        Some("Replace 4 occurrences across 2 files with 'omega'?")
    );
    assert_eq!(read(&a), "alpha beta\nAlpha\n");
    click_selector(cx, "replace-confirm-ok");

    // Disk: the file that is not open, written directly; the open one, saved from its editor.
    assert_eq!(read(&dir.join("src/b.rs")), "omega omega\n");
    assert_eq!(read(&a), "omega beta\nomega\n");
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        assert_eq!(e.buffer().value(), "omega beta\nomega\n");
        assert!(!e.is_dirty(), "upstream saves the files it replaced in (ReplaceService.replace)");
    });
    // The view: replaced files leave the results and the message says what happened.
    assert_eq!(v.read_with(cx, |v, _| v.results().file_count()), 0);
    assert_eq!(
        v.read_with(cx, |v, _| v.after_replace.clone()).as_deref(),
        Some("Replaced 4 occurrences across 2 files with 'omega'.")
    );

    // Cmd+Z in the editor undoes the replacement (one undo step) and leaves it dirty.
    wb.update(cx, |wb, cx| {
        let e = wb.editors.active_editor().unwrap();
        wb.editor_view.reveal(e);
        cx.notify();
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-z");
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        assert_eq!(e.buffer().value(), "alpha beta\nAlpha\n");
        assert!(e.is_dirty());
    });
    assert_eq!(read(&a), "omega beta\nomega\n", "undo does not write the file");
}

#[gpui_kit::test]
fn escape_cancels_the_question_and_replace_is_off_while_results_are_stale(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "replace-cancel");
    let v = search_and_type_replacement(&wb, cx, "alpha", "omega");
    cx.simulate_keystrokes("alt-cmd-enter");
    assert!(v.read_with(cx, |v, _| v.confirm.is_some()));
    cx.simulate_keystrokes("escape");
    assert!(v.read_with(cx, |v, _| v.confirm.is_none()));
    assert_eq!(read(&dir.join("src/b.rs")), "alpha alpha\n");

    // Typing in the query makes the shown results stale: Replace All is off until a search ends.
    cx.update(|window, app| v.update(app, |v, cx| v.focus_query(window, cx)));
    cx.simulate_input("x");
    assert!(!v.read_with(cx, |v, _| v.replace_active()));
    cx.simulate_keystrokes("alt-cmd-enter");
    assert!(v.read_with(cx, |v, _| v.confirm.is_none()), "disabled");
    settle(cx);
    assert!(!v.read_with(cx, |v, _| v.replace_active()), "no results for `alphax`");
}

#[gpui_kit::test]
fn per_match_replace_removes_the_match_and_moves_the_others(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "replace-one");
    let v = search_and_type_replacement(&wb, cx, "alpha", "OMEGA-LONG");
    // rows: [a.txt, m, m, b.rs, m, m]. Replace the first match of b.rs by clicking its action.
    assert_eq!(v.read_with(cx, |v, _| v.rows.len()), 6);
    click_selector(cx, "sr-replace:4");
    assert_eq!(read(&dir.join("src/b.rs")), "OMEGA-LONG alpha\n");
    assert_eq!(files(&v, cx), [("a.txt".into(), 2), ("b.rs".into(), 1)]);
    // The remaining match of b.rs moved with the text: replacing it hits the right characters.
    let range = v.read_with(cx, |v, _| v.results().files[1].matches[0].range);
    assert_eq!((range.start_column, range.end_column), (12, 17));
    v.update(cx, |v, cx| v.replace_row(Row::Match(1, 0), cx));
    assert_eq!(read(&dir.join("src/b.rs")), "OMEGA-LONG OMEGA-LONG\n");
    assert_eq!(files(&v, cx), [("a.txt".into(), 2)]);
    assert_eq!(read(&dir.join("a.txt")), "alpha beta\nAlpha\n", "other files are untouched");

    // A file's Replace All (no question): rows are now [a.txt, m, m].
    v.update(cx, |v, cx| v.replace_row(Row::File(0), cx));
    assert_eq!(read(&dir.join("a.txt")), "OMEGA-LONG beta\nOMEGA-LONG\n");
    assert_eq!(v.read_with(cx, |v, _| v.results().file_count()), 0);
}

#[gpui_kit::test]
fn regex_replace_uses_capture_groups(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "replace-regex");
    fs::write(dir.join("mail.txt"), "bob@x.org amy@y.org\n").unwrap();
    cx.simulate_keystrokes("cmd-shift-f");
    let v = view(&wb, cx);
    cx.simulate_keystrokes("alt-cmd-r");
    cx.simulate_input(r"(\w+)@");
    settle(cx);
    open_replace(&v, cx, "$1#");
    // the inline preview
    let shown: Vec<_> = v.read_with(cx, |v, cx| {
        v.results().files[0].matches.iter().map(|m| v.replacement_for(m, cx)).collect()
    });
    assert_eq!(shown, [Some("bob#".to_string()), Some("amy#".to_string())]);
    cx.simulate_keystrokes("alt-cmd-enter");
    click_selector(cx, "replace-confirm-ok");
    assert_eq!(read(&dir.join("mail.txt")), "bob#x.org amy#y.org\n");
    assert_eq!(
        v.read_with(cx, |v, _| v.after_replace.clone()).as_deref(),
        Some("Replaced 2 occurrences across 1 file with '$1#'.")
    );
}

#[gpui_kit::test]
fn preserve_case_follows_each_matched_text(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "replace-case");
    fs::write(dir.join("case.txt"), "alpha Alpha ALPHA\n").unwrap();
    let v = search_and_type_replacement(&wb, cx, "alpha", "beta");
    cx.simulate_keystrokes("alt-cmd-p");
    assert!(v.read_with(cx, |v, _| v.preserve_case));
    let shown: Vec<_> = v.read_with(cx, |v, cx| {
        let f = v.results().files.iter().find(|f| f.name == "case.txt").unwrap();
        f.matches.iter().map(|m| v.replacement_for(m, cx).unwrap()).collect()
    });
    assert_eq!(shown, ["beta", "Beta", "BETA"]);
    cx.simulate_keystrokes("alt-cmd-enter");
    click_selector(cx, "replace-confirm-ok");
    assert_eq!(read(&dir.join("case.txt")), "beta Beta BETA\n");
}

#[gpui_kit::test]
fn replace_keeps_crlf_and_handles_multibyte_text(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "replace-crlf");
    fs::write(dir.join("w.txt"), "h\u{e9}llo alpha\r\n\u{1F600} alpha \u{65E5}\r\nend\r\n").unwrap();
    let _v = search_and_type_replacement(&wb, cx, "alpha", "\u{3C9}");
    cx.simulate_keystrokes("alt-cmd-enter");
    click_selector(cx, "replace-confirm-ok");
    let bytes = fs::read(dir.join("w.txt")).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap(), "h\u{e9}llo \u{3C9}\r\n\u{1F600} \u{3C9} \u{65E5}\r\nend\r\n");
}
