//! Headless tests of the completion seam (ADR-0082): requests are answered in-process (no
//! extension host runs in these tests), and only the answer to the newest request is kept.
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
