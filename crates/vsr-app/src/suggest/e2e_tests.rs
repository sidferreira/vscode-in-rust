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
fn enter_a_selection_and_escape_end_the_session(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "cancel");
    let _ = fake(&wb, cx, &["main", "make"]);
    open(&wb, cx, &blank(&proj, "a.txt", ""));
    for ends in ["enter", "cmd-a", "escape"] {
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
