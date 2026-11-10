//! Headless E2E tests of the editor Find widget (docs/testing.md tier 2, ADR-0027).
//!
//! A real `Workbench` in gpui's test platform: real key dispatch (Cmd+F, Enter, Escape, Alt+Cmd+C
//! ...), real text inputs, an in-memory configuration (the user's settings file is never read or
//! written). No pixel metrics are asserted (the test text system is a no-op). Our own scenarios:
//! upstream's smoke tests have no editor find area.

use std::fs;
use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

const TEXT: &str = "alpha beta alpha\ngamma alpha\ndelta\nalpha ALPHA\n";

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-e2e-find-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workbench with `a.txt` (= `text`) open and focused; settings in memory.
fn start<'a>(cx: &'a mut TestAppContext, name: &str, text: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let dir = unique_dir(name);
    fs::write(dir.join("a.txt"), text).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        AppConfig::install(cx, None);
    });
    let root = dir.clone();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    let path = dir.join("a.txt");
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
    (wb, cx)
}

fn set_setting(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

fn label(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, _| wb.editor_view.find.label())
}

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    wb.read_with(cx, |wb, _| wb.editor_view.find.is_open())
}

fn find_text(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, cx| {
        wb.editor_view.find.find_input().map_or_else(String::new, |i| i.read(cx).value().to_string())
    })
}

fn value(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().value())
}

/// The selection as (start line, start column, end line, end column).
fn sel(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (usize, usize, usize, usize) {
    wb.read_with(cx, |wb, _| {
        let r = wb.editors.active_editor().unwrap().editor.selection().range();
        (r.start.line, r.start.column, r.end.line, r.end.column)
    })
}

fn caret_to(wb: &Entity<Workbench>, cx: &mut VisualTestContext, line: usize, column: usize) {
    wb.update(cx, |wb, cx| {
        let e = wb.editors.active.and_then(|i| wb.editors.open.get_mut(i)).unwrap();
        e.editor.set_cursor(vsr_editor::Position::new(line, column), false);
        cx.notify();
    });
    cx.run_until_parked();
}

fn focus_replace_box(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    wb.update_in(cx, |wb, window, cx| {
        let input = wb.editor_view.find.replace_input().cloned().expect("replace input exists");
        input.update(cx, |s, cx| s.focus(window, cx));
    });
    cx.run_until_parked();
}

fn highlighted_matches(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> usize {
    wb.read_with(cx, |wb, _| wb.editor_view.find.model().map_or(0, |m| m.matches().len()))
}

#[gpui_kit::test]
fn cmd_f_seeds_the_word_at_the_caret_and_typing_counts_matches(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "type", TEXT);
    caret_to(&wb, cx, 1, 8); // inside "beta"
    assert!(!open(&wb, cx));

    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    assert!(open(&wb, cx));
    assert_eq!(find_text(&wb, cx), "beta", "seeded from the word at the caret");
    assert_eq!(label(&wb, cx), "1 of 1");

    // The seed is selected, so typing replaces it; the first match after where the search began
    // (the caret in "beta") becomes the current one: the second "alpha" of line 1.
    cx.simulate_input("alpha");
    cx.run_until_parked();
    assert_eq!(find_text(&wb, cx), "alpha");
    assert_eq!(label(&wb, cx), "2 of 5");
    assert_eq!(sel(&wb, cx), (1, 12, 1, 17));
    assert_eq!(highlighted_matches(&wb, cx), 5, "every match is highlighted");
}

#[gpui_kit::test]
fn enter_and_shift_enter_move_the_selection_and_wrap(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "enter", TEXT);
    caret_to(&wb, cx, 3, 1); // "delta" (seeds "delta")
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    cx.run_until_parked();
    assert_eq!(label(&wb, cx), "4 of 5", "first match after the caret");
    assert_eq!(sel(&wb, cx), (4, 1, 4, 6));

    cx.simulate_keystrokes("enter");
    assert_eq!(label(&wb, cx), "5 of 5");
    assert_eq!(sel(&wb, cx), (4, 7, 4, 12));
    cx.simulate_keystrokes("enter"); // wraps (editor.find.loop)
    assert_eq!(label(&wb, cx), "1 of 5");
    assert_eq!(sel(&wb, cx), (1, 1, 1, 6));
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(label(&wb, cx), "5 of 5", "previous wraps back");
    cx.simulate_keystrokes("shift-enter");
    assert_eq!(label(&wb, cx), "4 of 5");
    assert!(open(&wb, cx));
}

#[gpui_kit::test]
fn escape_closes_the_widget_and_returns_focus_to_the_editor(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "esc", TEXT);
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("gamma");
    cx.simulate_keystrokes("enter");
    assert_eq!(sel(&wb, cx), (2, 1, 2, 6));

    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(!open(&wb, cx));
    assert_eq!(sel(&wb, cx), (2, 1, 2, 6), "the match stays selected");
    // The editor has the focus again: typing replaces the selection in the document.
    cx.simulate_input("X");
    assert_eq!(value(&wb, cx), "alpha beta alpha\nX alpha\ndelta\nalpha ALPHA\n");
    // And Cmd+F brings the widget back with the previous search (here seeded from the caret word).
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    assert!(open(&wb, cx));
}

#[gpui_kit::test]
fn cmd_g_navigates_without_the_widget_and_opens_it(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "cmdg", TEXT);
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    cx.simulate_keystrokes("escape");
    assert!(!open(&wb, cx));
    caret_to(&wb, cx, 1, 1);

    cx.simulate_keystrokes("cmd-g"); // opens the widget (no focus change) and moves on
    cx.run_until_parked();
    assert!(open(&wb, cx));
    assert_eq!(find_text(&wb, cx), "alpha");
    // With the caret at the start of a match, "next" selects that match (matchAfterPosition).
    assert_eq!(sel(&wb, cx), (1, 1, 1, 6));
    cx.simulate_keystrokes("cmd-g");
    assert_eq!(sel(&wb, cx), (1, 12, 1, 17));
    cx.simulate_keystrokes("cmd-g");
    assert_eq!(sel(&wb, cx), (2, 7, 2, 12));
    cx.simulate_keystrokes("shift-cmd-g");
    assert_eq!(sel(&wb, cx), (1, 12, 1, 17));
}

#[gpui_kit::test]
fn alt_cmd_f_replace_all_is_one_undo_step(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "replaceall", TEXT);
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("alt-cmd-f");
    cx.run_until_parked();
    assert!(open(&wb, cx));
    // No selection to seed: the Find box has the focus (the search string was seeded from the word).
    cx.simulate_input("alpha");
    focus_replace_box(&wb, cx);
    cx.simulate_input("omega");
    assert_eq!(label(&wb, cx).as_str(), "4 of 5");
    assert_eq!(value(&wb, cx), TEXT, "typing the replacement does not edit");

    cx.simulate_keystrokes("alt-cmd-enter");
    cx.run_until_parked();
    assert_eq!(value(&wb, cx), "omega beta omega\ngamma omega\ndelta\nomega omega\n");
    assert_eq!(label(&wb, cx), "No results");

    // Back in the editor, one Cmd+Z undoes all five replacements.
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(value(&wb, cx), TEXT);
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(value(&wb, cx), "omega beta omega\ngamma omega\ndelta\nomega omega\n");
}

#[gpui_kit::test]
fn enter_in_the_replace_box_replaces_one_and_cmd_enter_replaces_all(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "replaceone", TEXT);
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("alt-cmd-f");
    cx.simulate_input("alpha"); // cursorMoveOnType selected the first match after the caret
    assert_eq!(sel(&wb, cx), (4, 1, 4, 6));
    focus_replace_box(&wb, cx);
    cx.simulate_input("A");
    assert_eq!(value(&wb, cx), TEXT);

    // The selection is on a match: Enter replaces it and moves to the next one.
    cx.simulate_keystrokes("enter");
    assert_eq!(value(&wb, cx), "alpha beta alpha\ngamma alpha\ndelta\nA ALPHA\n");
    assert_eq!(sel(&wb, cx), (4, 3, 4, 8));
    assert_eq!(label(&wb, cx), "4 of 4");

    // With the caret elsewhere, Enter only selects the next match.
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("enter");
    assert_eq!(sel(&wb, cx), (4, 3, 4, 8));
    assert_eq!(value(&wb, cx), "alpha beta alpha\ngamma alpha\ndelta\nA ALPHA\n");

    cx.simulate_keystrokes("cmd-enter"); // Replace All
    cx.run_until_parked();
    assert_eq!(value(&wb, cx), "A beta A\ngamma A\ndelta\nA A\n");
}

#[gpui_kit::test]
fn regex_toggle_and_capture_groups_in_the_replacement(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "regex", "john smith\nada lovelace\n");
    cx.simulate_keystrokes("alt-cmd-f");
    cx.simulate_keystrokes("alt-cmd-r"); // Use Regular Expression
    cx.simulate_input(r"(\w+) (\w+)");
    assert_eq!(label(&wb, cx), "1 of 2");
    focus_replace_box(&wb, cx);
    cx.simulate_input(r"\u$2, \u$1");
    cx.simulate_keystrokes("alt-cmd-enter");
    cx.run_until_parked();
    assert_eq!(value(&wb, cx), "Smith, John\nLovelace, Ada\n");

    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(value(&wb, cx), "john smith\nada lovelace\n");
}

#[gpui_kit::test]
fn option_toggles_change_the_matches(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "toggles", TEXT);
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    assert_eq!(highlighted_matches(&wb, cx), 5);
    cx.simulate_keystrokes("alt-cmd-c"); // Match Case
    assert_eq!(highlighted_matches(&wb, cx), 4, "ALPHA no longer matches");
    cx.simulate_keystrokes("alt-cmd-c");
    assert_eq!(highlighted_matches(&wb, cx), 5);
    cx.simulate_keystrokes("alt-cmd-w"); // Whole Word: "alpha" is always a whole word here
    assert_eq!(highlighted_matches(&wb, cx), 5);
    cx.simulate_keystrokes("alt-cmd-w alt-cmd-r"); // regex: `alpha` still
    assert_eq!(highlighted_matches(&wb, cx), 5);
    // An invalid regular expression has no matches and no panic.
    cx.simulate_input("(");
    assert_eq!(label(&wb, cx), "No results");
    assert!(wb.read_with(cx, |wb, _| wb.editor_view.find.model().unwrap().error().is_some()));
}

#[gpui_kit::test]
fn find_in_selection_restricts_the_search(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "scope", TEXT);
    // Select lines 2..3 (line 3 from its start): Alt+Cmd+L turns on Find in Selection.
    wb.update(cx, |wb, cx| {
        let e = wb.editors.active.and_then(|i| wb.editors.open.get_mut(i)).unwrap();
        e.editor.set_cursor(vsr_editor::Position::new(2, 1), false);
        e.editor.set_cursor(vsr_editor::Position::new(4, 1), true);
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-f"); // multi-line selection: no seed
    cx.simulate_input("alpha");
    assert_eq!(highlighted_matches(&wb, cx), 5);
    cx.simulate_keystrokes("alt-cmd-l");
    cx.run_until_parked();
    assert_eq!(highlighted_matches(&wb, cx), 1, "only line 2 and 3 are searched");
    assert!(wb.read_with(cx, |wb, _| wb.editor_view.find.model().unwrap().state().search_scope.is_some()));
    cx.simulate_keystrokes("alt-cmd-l");
    assert_eq!(highlighted_matches(&wb, cx), 5);
}

#[gpui_kit::test]
fn seeding_follows_the_setting(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "seed", TEXT);
    // A selection seeds with the default setting ("always"); a multi-line one does not.
    wb.update(cx, |wb, cx| {
        let e = wb.editors.active.and_then(|i| wb.editors.open.get_mut(i)).unwrap();
        e.editor.set_cursor(vsr_editor::Position::new(2, 1), false);
        e.editor.set_cursor(vsr_editor::Position::new(2, 6), true);
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-f");
    assert_eq!(find_text(&wb, cx), "gamma");
    cx.simulate_keystrokes("escape");

    // "selection": only a non-empty selection seeds, not the word at the caret.
    set_setting(cx, "editor.find.seedSearchStringFromSelection", json!("selection"));
    caret_to(&wb, cx, 1, 2);
    cx.simulate_keystrokes("cmd-f");
    assert_eq!(find_text(&wb, cx), "gamma", "no seed: the previous search stays");
    cx.simulate_keystrokes("escape");

    // "never": even a selection is not used.
    set_setting(cx, "editor.find.seedSearchStringFromSelection", json!("never"));
    wb.update(cx, |wb, cx| {
        let e = wb.editors.active.and_then(|i| wb.editors.open.get_mut(i)).unwrap();
        e.editor.set_cursor(vsr_editor::Position::new(3, 1), false);
        e.editor.set_cursor(vsr_editor::Position::new(3, 6), true);
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-f");
    assert_eq!(find_text(&wb, cx), "gamma");
}

#[gpui_kit::test]
fn loop_setting_stops_at_the_ends(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "loop", TEXT);
    set_setting(cx, "editor.find.loop", json!(false));
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    assert_eq!(label(&wb, cx), "4 of 5");
    cx.simulate_keystrokes("enter enter enter");
    assert_eq!(label(&wb, cx), "5 of 5", "no wrap to the first match");
    assert_eq!(sel(&wb, cx), (4, 7, 4, 12));
}

#[gpui_kit::test]
fn cursor_move_on_type_and_find_on_type_settings(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "typing", TEXT);
    set_setting(cx, "editor.find.cursorMoveOnType", json!(false));
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    assert_eq!(highlighted_matches(&wb, cx), 5, "still searches");
    assert_eq!(sel(&wb, cx), (3, 1, 3, 1), "but the caret does not move");
    assert_eq!(label(&wb, cx), "3 of 5", "(the nearest match before the caret is the current one)");
    cx.simulate_keystrokes("escape");

    // findOnType off: nothing is searched until Enter.
    set_setting(cx, "editor.find.cursorMoveOnType", json!(true));
    set_setting(cx, "editor.find.findOnType", json!(false));
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("cmd-f");
    assert_eq!(find_text(&wb, cx), "delta", "seeded from the caret word");
    cx.simulate_input("gamma");
    assert_eq!(label(&wb, cx), "1 of 1", "still the seed's matches: typing does not search");
    assert_eq!(sel(&wb, cx), (3, 1, 3, 1));
    cx.simulate_keystrokes("enter"); // searches (and goes to the first match after the start)
    assert_eq!(sel(&wb, cx), (2, 1, 2, 6));
}

#[gpui_kit::test]
fn close_on_result_closes_after_enter_lands_on_a_match(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "closeonresult", TEXT);
    set_setting(cx, "editor.find.closeOnResult", json!(true));
    caret_to(&wb, cx, 3, 1);
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    assert!(open(&wb, cx), "typing alone does not close it");
    assert_eq!(sel(&wb, cx), (4, 1, 4, 6));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!open(&wb, cx));
    assert_eq!(sel(&wb, cx), (4, 7, 4, 12));
}

#[gpui_kit::test]
fn auto_find_in_selection_always_scopes_a_selection(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "autoscope", TEXT);
    set_setting(cx, "editor.find.autoFindInSelection", json!("always"));
    wb.update(cx, |wb, cx| {
        let e = wb.editors.active.and_then(|i| wb.editors.open.get_mut(i)).unwrap();
        e.editor.set_cursor(vsr_editor::Position::new(1, 1), false);
        e.editor.set_cursor(vsr_editor::Position::new(2, 13), true);
        cx.notify();
    });
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("alpha");
    assert_eq!(highlighted_matches(&wb, cx), 3, "lines 1 and 2 only");
}

// ---- editor.find.history and editor.find.globalFindClipboard -------------------------------------

/// Replaces the text of the Find box without pressing Enter (the box keeps the focus).
fn set_find_box(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &str) {
    wb.update_in(cx, |wb, window, cx| {
        let input = wb.editor_view.find.find_input().cloned().expect("find input exists");
        input.update(cx, |s, cx| s.set_value(text.to_string(), window, cx));
    });
    cx.run_until_parked();
}

/// Types `text` into the Find box and presses Enter.
fn submit_find(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &str) {
    set_find_box(wb, cx, text);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
}

#[gpui_kit::test]
fn up_and_down_walk_the_find_history_entered_with_enter(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "history", TEXT);
    cx.simulate_keystrokes("cmd-f");
    submit_find(&wb, cx, "beta");
    submit_find(&wb, cx, "gamma");
    set_find_box(&wb, cx, "dra");
    cx.simulate_keystrokes("up");
    assert_eq!(find_text(&wb, cx), "gamma", "the newest entry");
    cx.simulate_keystrokes("up");
    assert_eq!(find_text(&wb, cx), "beta");
    cx.simulate_keystrokes("up");
    assert_eq!(find_text(&wb, cx), "beta", "the oldest stays");
    cx.simulate_keystrokes("down");
    assert_eq!(find_text(&wb, cx), "gamma");
    cx.simulate_keystrokes("down");
    assert_eq!(find_text(&wb, cx), "dra", "down past the newest gives back what was typed");
}

#[gpui_kit::test]
fn find_history_never_recalls_nothing(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "history-never", TEXT);
    set_setting(cx, "editor.find.history", json!("never"));
    cx.simulate_keystrokes("cmd-f");
    submit_find(&wb, cx, "beta");
    set_find_box(&wb, cx, "");
    cx.simulate_keystrokes("up");
    assert_eq!(find_text(&wb, cx), "", "no history: the caret moves, the box keeps its text");
}

#[gpui_kit::test]
fn closing_the_widget_records_both_boxes_and_the_history_is_kept_in_the_session(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "history-session", TEXT);
    cx.simulate_keystrokes("cmd-f");
    submit_find(&wb, cx, "beta");
    cx.simulate_keystrokes("escape");
    let settings = crate::config::settings::FindSettings::default();
    let (find, replace) = wb.read_with(cx, |wb, _| wb.editor_view.find_histories(&settings));
    assert_eq!(find, ["beta"], "recorded on close");
    assert!(replace.is_empty());
    wb.update(cx, |wb, _| wb.editor_view.restore_find_histories(vec!["one".into()], vec!["two".into()]));
    let (find, replace) = wb.read_with(cx, |wb, _| wb.editor_view.find_histories(&settings));
    assert_eq!((find, replace), (vec!["one".to_string()], vec!["two".to_string()]));
}

#[gpui_kit::test]
fn the_global_find_clipboard_seeds_the_box_and_receives_the_search(cx: &mut TestAppContext) {
    use super::find::clipboard::MemoryFindPasteboard;
    let (wb, cx) = start(cx, "clipboard", TEXT);
    let pasteboard = MemoryFindPasteboard::default();
    *pasteboard.0.borrow_mut() = Some("from another app".into());
    wb.update(cx, |wb, _| wb.editor_view.set_find_pasteboard(pasteboard.clone()));
    set_setting(cx, "editor.find.globalFindClipboard", json!(true));
    set_setting(cx, "editor.find.seedSearchStringFromSelection", json!("never"));

    cx.simulate_keystrokes("cmd-f");
    assert_eq!(find_text(&wb, cx), "from another app", "the find pasteboard seeds the box");
    cx.simulate_keystrokes("escape");

    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("gamma"); // typed: the seed is selected, so it is replaced
    cx.run_until_parked();
    assert_eq!(pasteboard.0.borrow().as_deref(), Some("gamma"), "the search string is written to it");
}

#[gpui_kit::test]
fn without_the_global_clipboard_the_find_pasteboard_is_not_touched(cx: &mut TestAppContext) {
    use super::find::clipboard::MemoryFindPasteboard;
    let (wb, cx) = start(cx, "clipboard-off", TEXT);
    let pasteboard = MemoryFindPasteboard::default();
    *pasteboard.0.borrow_mut() = Some("kept".into());
    wb.update(cx, |wb, _| wb.editor_view.set_find_pasteboard(pasteboard.clone()));
    set_setting(cx, "editor.find.seedSearchStringFromSelection", json!("never"));
    cx.simulate_keystrokes("cmd-f");
    cx.simulate_input("gamma");
    cx.run_until_parked();
    assert_eq!(find_text(&wb, cx), "gamma");
    assert_eq!(pasteboard.0.borrow().as_deref(), Some("kept"));
}

#[gpui_kit::test]
fn the_widget_is_laid_out_at_the_top_right_of_the_editor(cx: &mut TestAppContext) {
    let (wb, cx) = start(cx, "layout", TEXT);
    cx.simulate_keystrokes("cmd-f");
    cx.run_until_parked();
    let w = cx.debug_bounds("find-widget").expect("the widget is drawn");
    let group = cx.debug_bounds("editor-group").expect("editor group");
    assert_eq!(f32::from(w.size.width), 419.0, "FIND_WIDGET_INITIAL_WIDTH");
    assert!(w.right() <= group.right() && w.top() >= group.top(), "inside the editor, at its top right");
    assert!(f32::from(group.right() - w.right()) < 40.0, "near the right edge");
    let one_row = f32::from(w.size.height);
    assert!(cx.debug_bounds("find-count").is_some());

    // The chevron reveals the Replace row: the widget grows.
    cx.simulate_keystrokes("alt-cmd-f");
    cx.run_until_parked();
    let two_rows = f32::from(cx.debug_bounds("find-widget").unwrap().size.height);
    assert!(two_rows > one_row + 20.0, "{one_row} -> {two_rows}");
    drop(wb);
}
