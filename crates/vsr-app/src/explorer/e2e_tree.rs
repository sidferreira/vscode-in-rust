//! Headless E2E tests for the Explorer tree: keyboard selection, and Delete with its confirmation
//! (ADR-0041; scenarios in tests/e2e-scenarios/explorer.md). Same harness as `e2e.rs`: a fake
//! watcher, a fake Trash that moves into a test-owned folder, and an in-memory configuration.

use std::fs;

use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::e2e::{start, workspace};
use crate::config::AppConfig;
use crate::Workbench;

fn focus_tree(wb: &gpui_kit::Entity<Workbench>, cx: &mut VisualTestContext) {
    let focus = wb.read_with(cx, |wb, _| wb.explorer.focus.clone());
    cx.update(|window, cx| window.focus(&focus, cx));
    cx.run_until_parked();
}

fn selected(wb: &gpui_kit::Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    wb.read_with(cx, |wb, _| {
        wb.explorer.selected.as_ref().map(|p| p.strip_prefix(&wb.root).unwrap_or(p).to_string_lossy().into_owned())
    })
}

#[gpui::test]
fn arrow_keys_move_the_selection_and_expand_and_collapse_folders(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("keys-select");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    focus_tree(&wb, cx);
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&wb, cx).as_deref(), Some("src"));
    cx.simulate_keystrokes("right");
    assert!(wb.read_with(cx, |wb, _| wb.expanded.contains(&wb.root.join("src"))), "Right expands the folder");
    cx.simulate_keystrokes("down");
    assert_eq!(selected(&wb, cx).as_deref(), Some("src/lib.rs"), "folders list first, then the files by name");
    cx.simulate_keystrokes("left");
    assert_eq!(selected(&wb, cx).as_deref(), Some("src"), "Left on a file selects its folder");
    cx.simulate_keystrokes("left");
    assert!(wb.read_with(cx, |wb, _| !wb.expanded.contains(&wb.root.join("src"))), "Left on an open folder collapses it");
}

#[gpui::test]
fn delete_asks_first_and_then_moves_the_file_to_the_trash(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("delete-ask");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let lib = ws.path().join("src/lib.rs");
    wb.update(cx, |wb, _| wb.select(&lib));
    focus_tree(&wb, cx);
    cx.simulate_keystrokes("delete");
    cx.run_until_parked();
    let pending = wb.read_with(cx, |wb, _| wb.explorer.confirm.clone()).expect("the confirmation bar is up");
    assert!(pending.use_trash);
    assert!(lib.exists(), "nothing is deleted before the confirmation");
    assert_eq!(
        super::delete::question(&lib, true),
        ("Are you sure you want to delete 'lib.rs'?".to_string(), "You can restore this file from the Trash.".to_string())
    );
    wb.update(cx, |wb, cx| wb.confirm_delete(cx));
    cx.run_until_parked();
    assert!(!lib.exists());
    assert!(bin.path().join("lib.rs").is_file(), "in the Trash");
    assert!(wb.read_with(cx, |wb, _| wb.explorer.confirm.is_none()));
}

#[gpui::test]
fn escape_cancels_the_delete_and_keeps_the_file(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("delete-cancel");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let lib = ws.path().join("src/lib.rs");
    wb.update(cx, |wb, _| wb.select(&lib));
    focus_tree(&wb, cx);
    cx.simulate_keystrokes("delete");
    assert!(wb.read_with(cx, |wb, _| wb.explorer.confirm.is_some()), "the bar is up before Escape");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(lib.is_file());
    assert!(wb.read_with(cx, |wb, _| wb.explorer.confirm.is_none()));
}

#[gpui::test]
fn confirm_delete_false_deletes_at_once(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("delete-noconfirm");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    cx.update(|_, cx| AppConfig::set_value(cx, "explorer.confirmDelete", Some(json!(false)))).unwrap();
    let lib = ws.path().join("src/lib.rs");
    wb.update(cx, |wb, _| wb.select(&lib));
    wb.update(cx, |wb, cx| wb.request_delete(false, cx));
    cx.run_until_parked();
    assert!(!lib.exists());
    assert!(bin.path().join("lib.rs").is_file());
}

#[gpui::test]
fn delete_permanently_never_goes_to_the_trash(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("delete-permanent");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let lib = ws.path().join("src/lib.rs");
    wb.update(cx, |wb, _| wb.select(&lib));
    wb.update(cx, |wb, cx| wb.request_delete(true, cx));
    let pending = wb.read_with(cx, |wb, _| wb.explorer.confirm.clone()).unwrap();
    assert!(!pending.use_trash);
    assert_eq!(
        super::delete::question(&lib, false).1,
        "This action is irreversible!"
    );
    wb.update(cx, |wb, cx| wb.confirm_delete(cx));
    cx.run_until_parked();
    assert!(!lib.exists());
    assert!(!bin.path().join("lib.rs").exists(), "not in the Trash");
}

#[gpui::test]
fn with_files_enable_trash_off_delete_is_permanent_and_asks_the_stronger_question(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("delete-notrash");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    cx.update(|_, cx| AppConfig::set_value(cx, "files.enableTrash", Some(json!(false)))).unwrap();
    let lib = ws.path().join("src/lib.rs");
    wb.update(cx, |wb, _| wb.select(&lib));
    wb.update(cx, |wb, cx| wb.request_delete(false, cx));
    let pending = wb.read_with(cx, |wb, _| wb.explorer.confirm.clone()).unwrap();
    assert!(!pending.use_trash, "the Delete key deletes permanently with the Trash off");
    assert_eq!(super::delete::question(&lib, false).0, "Are you sure you want to permanently delete 'lib.rs'?");
    wb.update(cx, |wb, cx| wb.confirm_delete(cx));
    cx.run_until_parked();
    assert!(!lib.exists());
    assert!(fs::read_dir(bin.path()).unwrap().next().is_none(), "the Trash stays empty");
}
