//! Headless tests of the completion seam (ADR-0082): requests are answered in-process (no
//! extension host runs in these tests), and only the answer to the newest request is kept.
//!
//! upstream: src/vs/editor/contrib/suggest/test/browser/suggestModel.test.ts @ 50e1350fc78d
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggest.ts

use std::fs;
use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use vsr_exthost::language_features::dto::CompletionList;
use vsr_exthost::language_features::query::CompletionTrigger;

use super::Answer;
use crate::Workbench;
use crate::config::AppConfig;

/// A workbench over `<tmp>/proj` (`src/main.rs`), settings in a temp file.
fn workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "rscode-suggest-e2e-{name}-{}-{nanos}",
        std::process::id()
    ));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    let settings = dir.join("settings.json");
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, Some(settings));
    });
    let root = proj.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    (wb, vcx, proj)
}

/// A list of one item labelled `label`, with the list's default ranges (an item needs a range,
/// its own or the list's, as upstream).
fn list(label: &str) -> CompletionList {
    let range = |col: u32| json!({ "startLineNumber": 1, "startColumn": col, "endLineNumber": 1, "endColumn": col });
    CompletionList::from_json(&json!({
        "a": { "insert": range(1), "replace": range(1) },
        "b": [{ "a": label }]
    }))
    .expect("a completion list")
}

#[gpui::test]
fn the_answer_to_the_newest_request_is_kept(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "newest");
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(|_, line, column, _| {
            vec![(3, list(&format!("at-{line}-{column}")))]
        }));
    });
    let main = proj.join("src/main.rs");
    let first = wb.update(cx, |wb, _| {
        wb.suggest_request(main.clone(), 1, 2, CompletionTrigger::Invoke)
    });
    let second = wb.update(cx, |wb, _| {
        wb.suggest_request(main.clone(), 1, 5, CompletionTrigger::Character('.'))
    });
    assert!(second > first, "request ids grow");

    let kept = wb
        .read_with(cx, |wb, _| wb.suggest.last.clone())
        .expect("the newest answered");
    assert_eq!(kept.id, second);
    assert_eq!((kept.line, kept.column), (1, 5));
    assert_eq!(kept.results[0].0, 3, "the provider's handle");
    assert_eq!(kept.results[0].1.items[0].label, "at-1-5");

    // A late answer to the first request is dropped and nothing changes.
    let late = Answer {
        id: first,
        path: main,
        line: 1,
        column: 2,
        results: vec![(3, list("stale"))],
    };
    let accepted = wb.update(cx, |wb, _| wb.suggest.deliver(late));
    assert!(!accepted, "the first request is no longer wanted");
    let still = wb
        .read_with(cx, |wb, _| wb.suggest.last.clone())
        .expect("kept");
    assert_eq!(still.id, second);
    assert_eq!(still.results[0].1.items[0].label, "at-1-5");
}

// ---- the trigger machine through the workbench (ADR-0083) ----------------------------------

use std::cell::RefCell;
use std::path::Path;
use std::rc::Rc;
use std::time::Duration;

use vsr_suggest::trigger::State;

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
}

/// A list of items labelled `labels`, each replacing line `line` from column 1 to `column`.
fn items_up_to(labels: &[&str], line: u32, column: u32) -> CompletionList {
    let items: Vec<_> = labels
        .iter()
        .map(|l| {
            json!({ "a": l, "j": { "startLineNumber": line, "startColumn": 1,
                                   "endLineNumber": line, "endColumn": column } })
        })
        .collect();
    CompletionList::from_json(&json!({ "b": items })).expect("a completion list")
}

/// Answers every request with `labels` and records the triggers.
fn fake(wb: &Entity<Workbench>, cx: &mut VisualTestContext, labels: &'static [&'static str]) -> Rc<RefCell<Vec<CompletionTrigger>>> {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&seen);
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(move |_, line, column, trigger| {
            log.borrow_mut().push(trigger);
            vec![(7, items_up_to(labels, line, column))]
        }));
    });
    seen
}

fn state(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> State {
    wb.read_with(cx, |wb, _| wb.suggest.session.state())
}

fn shown(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.update(cx, |wb, _| {
        wb.suggest
            .session
            .model()
            .map(|m| m.items().iter().map(|i| i.text_label.clone()).collect())
            .unwrap_or_default()
    })
}

fn pass(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn blank(proj: &Path, name: &str, text: &str) -> PathBuf {
    let path = proj.join(name);
    fs::write(&path, text).unwrap();
    path
}

// upstream: SuggestModel - TriggerAndCancelOracle > trigger - on type
#[gpui::test]
fn typing_a_word_asks_after_the_quick_suggest_delay(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "quick");
    let seen = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    assert!(seen.borrow().is_empty(), "nothing is asked before the delay");
    assert_eq!(state(&wb, cx), State::Idle);
    pass(cx, 20);
    assert_eq!(*seen.borrow(), [CompletionTrigger::Invoke]);
    assert_eq!(state(&wb, cx), State::Auto);
    assert_eq!(shown(&wb, cx), ["main", "make"]);
    assert!(wb.read_with(cx, |wb, _| wb.suggest.visible));
}

#[gpui::test]
fn typing_on_refilters_without_asking_again(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "refilter");
    let seen = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    cx.simulate_keystrokes("a i");
    assert_eq!(shown(&wb, cx), ["main"], "mai matches main only");
    assert_eq!(seen.borrow().len(), 1, "one request for the whole word");
    // Backspace narrows the word again and the list follows.
    cx.simulate_keystrokes("backspace");
    assert_eq!(shown(&wb, cx), ["main", "make"]);
    assert_eq!(seen.borrow().len(), 1);
}

#[gpui::test]
fn the_list_ends_when_the_cursor_leaves_the_word(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "leave");
    let _ = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    assert_eq!(state(&wb, cx), State::Auto);
    cx.simulate_keystrokes("left");
    assert_eq!(state(&wb, cx), State::Idle, "left of the word, no word to retrigger on");
    assert!(!wb.read_with(cx, |wb, _| wb.suggest.visible));
}

#[gpui::test]
fn a_selection_and_escape_end_the_session(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "cancel");
    let _ = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    for ends in ["cmd-a", "escape"] {
        cx.simulate_keystrokes("m");
        pass(cx, 20);
        assert_eq!(state(&wb, cx), State::Auto, "before {ends}");
        cx.simulate_keystrokes(ends);
        assert_eq!(state(&wb, cx), State::Idle, "{ends} ends the session");
        assert!(!wb.read_with(cx, |wb, _| wb.suggest.visible));
        cx.simulate_keystrokes("cmd-a backspace");
    }
}

#[gpui::test]
fn a_late_answer_after_the_session_ended_is_dropped(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "late");
    let _ = fake(&wb, cx, &["main"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    let id = wb.update(cx, |wb, _| wb.suggest.pending_id());
    assert_eq!(id, None, "the answer arrived");
    let request = wb.update(cx, |wb, _| wb.suggest.request());
    cx.simulate_keystrokes("escape");
    let late = Answer { id: request, path: proj.join("a.txt"), line: 1, column: 2, results: vec![(7, items_up_to(&["stale"], 1, 2))] };
    assert!(!wb.update(cx, |wb, _| wb.suggest.deliver(late)), "escape forgot the request");
}

#[gpui::test]
fn ctrl_space_asks_for_completions(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "manual");
    let seen = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", "ma"));
    cx.simulate_keystrokes("cmd-down ctrl-space");
    assert_eq!(*seen.borrow(), [CompletionTrigger::Invoke]);
    assert_eq!(state(&wb, cx), State::Manual);
    assert_eq!(shown(&wb, cx), ["main", "make"]);
}

#[gpui::test]
fn a_trigger_character_asks_with_its_character(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "char");
    let seen = fake(&wb, cx, &["foo.bar"]);
    open(&wb, cx, &blank(&proj, "a.txt", "foo"));
    cx.simulate_keystrokes("cmd-down .");
    assert_eq!(*seen.borrow(), [CompletionTrigger::Character('.')]);
    assert_eq!(state(&wb, cx), State::Auto);
    assert_eq!(shown(&wb, cx), ["foo.bar"]);
}

// upstream: SuggestModel - TriggerAndCancelOracle > #17400: Keep filtering suggestModel.ts after space
#[gpui::test]
fn the_list_keeps_filtering_after_a_space(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "space");
    let _ = fake(&wb, cx, &["My Table"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("M");
    pass(cx, 20);
    cx.simulate_keystrokes("y space");
    assert_eq!(state(&wb, cx), State::Auto);
    assert_eq!(shown(&wb, cx), ["My Table"]);
}

#[gpui::test]
fn without_a_host_the_words_of_the_open_documents_are_offered(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "words");
    let lib = proj.join("src/lib.rs");
    let main = proj.join("src/main.rs");
    fs::write(&main, "alpha alphabet\n\n").unwrap();
    fs::write(&lib, "pub fn lib() {}\n").unwrap();
    open(&wb, cx, &lib);
    open(&wb, cx, &main);
    cx.simulate_keystrokes("down al");
    pass(cx, 20);
    assert_eq!(state(&wb, cx), State::Auto);
    assert_eq!(shown(&wb, cx), ["alpha", "alphabet"], "words of the document, the word being typed left out");

    // Another document of the same language contributes (`matchingDocuments`).
    cx.simulate_keystrokes("escape cmd-a backspace");
    cx.simulate_keystrokes("l i");
    pass(cx, 20);
    assert_eq!(shown(&wb, cx), ["lib"]);
}

// ---- the list widget: keys, accept, snippets, release (ADR-0084) ------------------------------

fn text(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.buffer().value()).unwrap_or_default())
}

fn focused(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> usize {
    wb.read_with(cx, |wb, _| wb.suggest.list.focused)
}

#[gpui::test]
fn the_context_key_follows_the_list_and_arrows_move_the_focus_not_the_caret(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "keys");
    let _ = fake(&wb, cx, &["main", "make", "mask"]);
    open(&wb, cx, &blank(&proj, "a.txt", "x\n"));
    cx.simulate_keystrokes("cmd-down m");
    assert!(!wb.read_with(cx, |wb, _| wb.editor_view.suggest_visible), "not before the answer");
    pass(cx, 20);
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.suggest.visible));
    assert_eq!(focused(&wb, cx), 0);
    cx.simulate_keystrokes("down");
    assert_eq!(focused(&wb, cx), 1);
    cx.simulate_keystrokes("up up");
    assert_eq!(focused(&wb, cx), 2, "up wraps to the last item");
    cx.simulate_keystrokes("down");
    assert_eq!(focused(&wb, cx), 0, "down wraps to the first");
    cx.simulate_keystrokes("pagedown");
    assert_eq!(focused(&wb, cx), 2, "page down goes to the last visible row");
    let caret = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!((caret.line, caret.column), (2, 2), "the caret did not move");
    cx.simulate_keystrokes("escape");
    assert!(!wb.read_with(cx, |wb, _| wb.suggest.visible));
    assert_eq!(text(&wb, cx), "x\nm");
    // With the list gone the arrows move the caret again.
    cx.simulate_keystrokes("up");
    let caret = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!(caret.line, 1);
}

#[gpui::test]
fn enter_accepts_the_focused_item_as_one_undo_step(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "enter");
    let _ = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    cx.simulate_keystrokes("down enter");
    assert_eq!(text(&wb, cx), "make", "Enter inserts the item and no line break");
    assert!(!wb.read_with(cx, |wb, _| wb.suggest.visible));
    let caret = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active);
    assert_eq!(caret.column, 5);
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&wb, cx), "m", "one undo returns to what was typed");
}

#[gpui::test]
fn tab_accepts_and_typing_on_before_accepting_replaces_the_whole_word(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "tab");
    let _ = fake(&wb, cx, &["maker", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    cx.simulate_keystrokes("a k");
    assert_eq!(shown(&wb, cx), ["make", "maker"]);
    cx.simulate_keystrokes("down tab");
    assert_eq!(text(&wb, cx), "maker", "the range follows the cursor: mak became maker");
}

#[gpui::test]
fn a_snippet_is_expanded_with_its_first_placeholder_selected(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "snippet");
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(|_, line, column, _| {
            let l = CompletionList::from_json(&json!({"b": [{
                "a": "fn", "h": "fn ${1:name}(${2:args}) {\n\t$0\n}", "i": 4,
                "j": { "startLineNumber": line, "startColumn": column - 1, "endLineNumber": line, "endColumn": column } }]}))
            .unwrap();
            vec![(1, l)]
        }));
    });
    open(&wb, cx, &blank(&proj, "a.txt", "  "));
    cx.simulate_keystrokes("cmd-down f");
    pass(cx, 20);
    cx.simulate_keystrokes("enter");
    assert_eq!(text(&wb, cx), "  fn name(args) {\n  \t\n  }");
    let sel = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection());
    assert_eq!((sel.anchor.column, sel.active.column), (6, 10), "`name` is selected");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(text(&wb, cx), "  f");
}

#[gpui::test]
fn the_host_caches_are_held_for_the_session_and_released_with_it(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "release");
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(|_, line, column, _| {
            let mut l = items_up_to(&["main"], line, column);
            l.cache_id = Some(41);
            vec![(7, l)]
        }));
    });
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    assert_eq!(wb.read_with(cx, |wb, _| wb.suggest.held_lists()), [(7, 41)]);
    cx.simulate_keystrokes("escape");
    assert!(wb.read_with(cx, |wb, _| wb.suggest.held_lists().is_empty()), "escape releases the list");
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    cx.simulate_keystrokes("enter");
    assert!(wb.read_with(cx, |wb, _| wb.suggest.held_lists().is_empty()), "accepting releases it too");
}

#[gpui::test]
fn the_widget_rows_have_the_kind_icon_and_the_matched_characters(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "rows");
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(|_, line, column, _| {
            let l = CompletionList::from_json(&json!({"b": [
                {"a": {"label": "makeThing", "detail": "(x)"}, "b": 1, "j": {"startLineNumber": line, "startColumn": 1, "endLineNumber": line, "endColumn": column}},
            ]}))
            .unwrap();
            vec![(1, l)]
        }));
    });
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    cx.simulate_keystrokes("t");
    let data = wb.update(cx, |wb, _| wb.suggest_widget_data()).expect("a list shows");
    assert_eq!(data.rows.len(), 1);
    let row = &data.rows[0];
    assert_eq!((row.label.as_str(), row.icon, row.detail.as_str(), row.focused), ("makeThing", '\u{ea8c}', "(x)", true));
    assert_eq!(row.matches, [0..1, 4..5], "m and T");
}

// ---- the settings (ADR-0095) -------------------------------------------------------------------

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
    cx.update(|window, _| window.refresh());
    cx.run_until_parked();
}

#[gpui::test]
fn quick_suggestions_off_and_its_delay(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "quickset");
    let seen = fake(&wb, cx, &["main"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    set(cx, "editor.quickSuggestions", json!("off"));
    cx.simulate_keystrokes("m");
    pass(cx, 50);
    assert!(seen.borrow().is_empty(), "off: typing asks nothing");
    cx.simulate_keystrokes("ctrl-space");
    assert_eq!(seen.borrow().len(), 1, "Ctrl+Space still asks");
    cx.simulate_keystrokes("escape cmd-a backspace");

    set(cx, "editor.quickSuggestions", json!({ "other": true, "comments": false, "strings": false }));
    set(cx, "editor.quickSuggestionsDelay", json!(100));
    cx.simulate_keystrokes("m");
    pass(cx, 50);
    assert_eq!(seen.borrow().len(), 1, "before the delay");
    pass(cx, 100);
    assert_eq!(seen.borrow().len(), 2, "after the delay");
}

#[gpui::test]
fn trigger_characters_can_be_turned_off(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "charset");
    let seen = fake(&wb, cx, &["foo.bar"]);
    open(&wb, cx, &blank(&proj, "a.txt", "foo"));
    set(cx, "editor.suggestOnTriggerCharacters", json!(false));
    cx.simulate_keystrokes("cmd-down .");
    pass(cx, 50);
    assert!(seen.borrow().is_empty());
}

#[gpui::test]
fn word_based_suggestions_modes(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "wordset");
    let lib = proj.join("src/lib.rs");
    let main = proj.join("src/main.rs");
    fs::write(&main, "alpha alphabet\n\n").unwrap();
    fs::write(&lib, "libword\n").unwrap();
    open(&wb, cx, &lib);
    open(&wb, cx, &main);
    set(cx, "editor.wordBasedSuggestions", json!("off"));
    cx.simulate_keystrokes("down a l");
    pass(cx, 20);
    assert!(shown(&wb, cx).is_empty(), "off: no words");
    cx.simulate_keystrokes("cmd-a backspace");
    set(cx, "editor.wordBasedSuggestions", json!("currentDocument"));
    cx.simulate_keystrokes("l i");
    pass(cx, 20);
    assert!(shown(&wb, cx).is_empty(), "currentDocument: libword is in another document");
    cx.simulate_keystrokes("cmd-a backspace");
    set(cx, "editor.wordBasedSuggestions", json!("allDocuments"));
    cx.simulate_keystrokes("l i");
    pass(cx, 20);
    assert_eq!(shown(&wb, cx), ["libword"]);
}

#[gpui::test]
fn accept_suggestion_on_enter(cx: &mut TestAppContext) {
    for (mode, typed, want) in [("off", "m", "m\n"), ("on", "m", "main"), ("smart", "m", "main"), ("smart", "ma", "main"), ("smart", "main", "main\n")] {
        let (wb, cx, proj) = workbench(cx, "enterset");
        let _ = fake(&wb, cx, &["main"]);
        open(&wb, cx, &blank(&proj, "a.txt", ""));
        set(cx, "editor.acceptSuggestionOnEnter", json!(mode));
        for c in typed.chars() {
            cx.simulate_keystrokes(&c.to_string());
        }
        pass(cx, 20);
        cx.simulate_keystrokes("enter");
        assert_eq!(text(&wb, cx), want, "{mode} after typing {typed}");
    }
}

#[gpui::test]
fn snippet_suggestions_placement_and_none(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "snipset");
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(|_, line, column, _| {
            let range = json!({ "startLineNumber": line, "startColumn": 1, "endLineNumber": line, "endColumn": column });
            let l = CompletionList::from_json(&json!({"b": [
                {"a": "ma-snip", "b": 28, "j": range}, {"a": "ma-fn", "b": 1, "j": range}]}))
            .unwrap();
            vec![(1, l)]
        }));
    });
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    for (mode, want) in [("top", vec!["ma-snip", "ma-fn"]), ("bottom", vec!["ma-fn", "ma-snip"]), ("none", vec!["ma-fn"])] {
        set(cx, "editor.snippetSuggestions", json!(mode));
        cx.simulate_keystrokes("m a");
        pass(cx, 20);
        assert_eq!(shown(&wb, cx), want, "{mode}");
        cx.simulate_keystrokes("escape cmd-a backspace");
    }
}

#[gpui::test]
fn insert_mode_replace_takes_the_rest_of_the_word(cx: &mut TestAppContext) {
    for (mode, want) in [("insert", "makeer"), ("replace", "make")] {
        let (wb, cx, proj) = workbench(cx, "insertset");
        wb.update(cx, |wb, _| {
            wb.suggest.fake = Some(Box::new(|_, line, _, _| {
                let l = CompletionList::from_json(&json!({"b": [{"a": "make", "j": {
                    "insert": { "startLineNumber": line, "startColumn": 1, "endLineNumber": line, "endColumn": 3 },
                    "replace": { "startLineNumber": line, "startColumn": 1, "endLineNumber": line, "endColumn": 7 } }}]}))
                .unwrap();
                vec![(1, l)]
            }));
        });
        open(&wb, cx, &blank(&proj, "a.txt", "mamaer"));
        set(cx, "editor.suggest.insertMode", json!(mode));
        cx.simulate_keystrokes("cmd-up right right ctrl-space");
        cx.simulate_keystrokes("enter");
        assert_eq!(text(&wb, cx), if mode == "insert" { "makemaer" } else { want }, "{mode}");
    }
}

#[gpui::test]
fn graceful_filtering_keeps_a_typo(cx: &mut TestAppContext) {
    for (graceful, want) in [(true, vec!["main"]), (false, vec![])] {
        let (wb, cx, proj) = workbench(cx, "gracefulset");
        let _ = fake(&wb, cx, &["main"]);
        open(&wb, cx, &blank(&proj, "a.txt", ""));
        set(cx, "editor.suggest.filterGraceful", json!(graceful));
        cx.simulate_keystrokes("m");
        pass(cx, 20);
        cx.simulate_keystrokes("i a");
        assert_eq!(shown(&wb, cx), want, "graceful {graceful}");
    }
}

#[gpui::test]
fn kind_filters_icons_and_inline_details(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "kindset");
    wb.update(cx, |wb, _| {
        wb.suggest.fake = Some(Box::new(|_, line, column, _| {
            let range = json!({ "startLineNumber": line, "startColumn": 1, "endLineNumber": line, "endColumn": column });
            let items: Vec<_> = (0..=28u32).map(|k| json!({"a": format!("m{k:02}"), "b": k, "c": "the detail", "x": [9, k], "j": range})).collect();
            vec![(1, CompletionList::from_json(&json!({ "b": items })).unwrap())]
        }));
    });
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    // Every `show*` key hides its kind (kind 22 is `showCustomcolors`, 27 has no setting).
    let keys = [
        (0, "Methods"), (1, "Functions"), (2, "Constructors"), (3, "Fields"), (4, "Variables"), (5, "Classes"), (6, "Structs"),
        (7, "Interfaces"), (8, "Modules"), (9, "Properties"), (10, "Events"), (11, "Operators"), (12, "Units"), (13, "Values"),
        (14, "Constants"), (15, "Enums"), (16, "EnumMembers"), (17, "Keywords"), (18, "Words"), (19, "Colors"), (20, "Files"),
        (21, "References"), (22, "Customcolors"), (23, "Folders"), (24, "TypeParameters"), (25, "Users"), (26, "Issues"), (28, "Snippets"),
    ];
    for (k, name) in keys {
        let key = format!("editor.suggest.show{name}");
        set(cx, &key, json!(false));
        cx.simulate_keystrokes("m");
        pass(cx, 20);
        let labels = shown(&wb, cx);
        assert!(!labels.contains(&format!("m{k:02}")), "{key} hides kind {k}");
        assert_eq!(labels.len(), 28, "{key} hides only its kind");
        cx.simulate_keystrokes("escape cmd-a backspace");
        set(cx, &key, json!(true));
    }
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    let data = wb.update(cx, |wb, _| wb.suggest_widget_data()).unwrap();
    assert!(data.show_icons);
    cx.simulate_keystrokes("escape cmd-a backspace");
    set(cx, "editor.suggest.showIcons", json!(false));
    cx.simulate_keystrokes("m");
    pass(cx, 20);
    assert!(!wb.update(cx, |wb, _| wb.suggest_widget_data()).unwrap().show_icons);
    cx.simulate_keystrokes("escape cmd-a backspace m");
    pass(cx, 20);
    // The detail (here from the list, not a resolve) is on the row; `showInlineDetails` governs the resolved one.
    // A resolved detail shows on the focused row, unless `showInlineDetails` is off.
    wb.update(cx, |wb, _| {
        let mut resolved = *wb.suggest.session.model().unwrap().items()[0].source.clone().unwrap();
        resolved.detail = Some("resolved detail".into());
        wb.suggest.resolved.insert((9, 0), Box::new(resolved));
    });
    assert_eq!(wb.update(cx, |wb, _| wb.suggest_widget_data()).unwrap().focused_detail.as_deref(), Some("resolved detail"));
    set(cx, "editor.suggest.showInlineDetails", json!(false));
    assert!(wb.update(cx, |wb, _| wb.suggest_widget_data()).unwrap().focused_detail.is_none());
}
