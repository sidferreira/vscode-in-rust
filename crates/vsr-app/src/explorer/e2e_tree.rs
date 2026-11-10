//! Headless E2E tests for the Explorer tree: keyboard selection, and Delete with its confirmation
//! (ADR-0041; scenarios in tests/e2e-scenarios/explorer.md). Same harness as `e2e.rs`: a fake
//! watcher, a fake Trash that moves into a test-owned folder, and an in-memory configuration.

use std::fs;

use gpui_kit::{TestAppContext, VisualTestContext};
use serde_json::json;

use super::e2e::{start, tree_names, workspace};
use crate::explorer::testutil::TempWorkspace;
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

#[gpui::test]
fn new_file_from_the_command_takes_a_name_and_opens_the_file(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("new-file-inline");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    wb.update(cx, |wb, _| wb.select(&ws.path().join("src/main.rs")));
    cx.update(|window, cx| wb.update(cx, |wb, cx| wb.start_inline(super::inline::InlineKind::NewFile, window, cx)));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_some()), "the name box is open");
    cx.simulate_input("notes.txt");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let created = ws.path().join("src/notes.txt");
    assert!(created.is_file(), "the file exists after Enter");
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_none()));
    assert!(tree_names(&wb, cx, &ws.path().join("src")).contains(&"notes.txt".to_string()));
    let active = wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.path.clone()));
    assert_eq!(active, Some(created));
}

#[gpui::test]
fn an_invalid_name_shows_the_message_and_creates_nothing(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("new-invalid");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    wb.update(cx, |wb, _| wb.select(&ws.path().join("src")));
    cx.update(|window, cx| wb.update(cx, |wb, cx| wb.start_inline(super::inline::InlineKind::NewFile, window, cx)));
    cx.run_until_parked();
    cx.simulate_input("lib.rs");
    cx.run_until_parked();
    let msg = wb.read_with(cx, |wb, _| wb.explorer.inline.as_ref().and_then(|e| e.message.clone()));
    let msg = msg.expect("a message under the box");
    assert_eq!(msg.text, "A file or folder lib.rs already exists at this location. Please choose a different name.");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_some()), "Enter does not commit an invalid name");
    assert!(!ws.path().join("src/lib.rs").is_file() || std::fs::read_to_string(ws.path().join("src/lib.rs")).unwrap() == "pub fn lib() {}\n", "the existing file is untouched");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_none()), "Escape cancels");
}

#[gpui::test]
fn enter_starts_rename_and_the_name_commits_on_enter(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("rename-inline");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let main = ws.path().join("src/main.rs");
    wb.update(cx, |wb, _| wb.select(&main));
    focus_tree(&wb, cx);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_some()), "Enter starts Rename");
    // The box is open with the name; typing and Enter are the same path as New File.
    let typed = wb.update(cx, |wb, cx| wb.explorer.inline.as_ref().map(|e| e.value(cx)));
    assert_eq!(typed.as_deref(), Some("main.rs"));
    cx.update(|window, cx| wb.update(cx, |wb, cx| wb.cancel_inline(window, cx)));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_none()));
    assert!(main.is_file(), "Escape keeps the file");
}

#[gpui::test]
fn rename_commits_the_typed_name_and_moves_the_file(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("rename-commit");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let main = ws.path().join("src/main.rs");
    wb.update(cx, |wb, _| wb.select(&main));
    cx.update(|window, cx| wb.update(cx, |wb, cx| wb.start_inline(super::inline::InlineKind::Rename, window, cx)));
    cx.run_until_parked();
    cx.simulate_input("app");
    cx.run_until_parked();
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let renamed = ws.path().join("src/app.rs");
    assert!(renamed.is_file() && !main.exists());
    assert!(wb.read_with(cx, |wb, _| wb.explorer.inline.is_none()));
    assert_eq!(wb.read_with(cx, |wb, _| wb.explorer.selected.clone()), Some(renamed));
}

#[test]
fn rename_selects_the_name_without_the_last_extension() {
    use super::inline::rename_selection;
    assert_eq!(rename_selection("main.rs", false), 4);
    assert_eq!(rename_selection("foo.tar.gz", false), 7);
    assert_eq!(rename_selection(".env", false), 4);
    assert_eq!(rename_selection("Makefile", false), 8);
    assert_eq!(rename_selection("src.d", true), 5, "a folder selects all of its name");
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

fn row_names(wb: &gpui_kit::Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, _| wb.visible_rows().into_iter().map(|(e, _)| e.name).collect())
}

#[gpui::test]
fn sort_order_follows_explorer_sort_order_live(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("sort-order");
    fs::write(ws.path().join("a.txt"), "").unwrap();
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    assert_eq!(row_names(&wb, cx), ["src", "a.txt"], "default: folders first");
    set(cx, "explorer.sortOrder", json!("mixed"));
    assert_eq!(row_names(&wb, cx), ["a.txt", "src"], "mixed: by name");
    set(cx, "explorer.sortOrder", json!("filesFirst"));
    assert_eq!(row_names(&wb, cx), ["a.txt", "src"]);
    set(cx, "explorer.sortOrder", json!("foldersNestsFiles"));
    assert_eq!(row_names(&wb, cx), ["src", "a.txt"], "without file nesting it sorts as default");
}

#[gpui::test]
fn sort_reverse_and_lexicographic_options_follow_settings_live(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("sort-reverse");
    fs::write(ws.path().join("a.txt"), "").unwrap();
    fs::write(ws.path().join("B.txt"), "").unwrap();
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    set(cx, "explorer.sortOrder", json!("mixed"));
    assert_eq!(row_names(&wb, cx), ["a.txt", "B.txt", "src"], "mixed: names ignore case");
    set(cx, "explorer.sortOrderReverse", json!(true));
    assert_eq!(row_names(&wb, cx), ["src", "B.txt", "a.txt"], "reverse flips the whole order");
    set(cx, "explorer.sortOrderLexicographicOptions", json!("unicode"));
    assert_eq!(row_names(&wb, cx), ["src", "a.txt", "B.txt"], "unicode: code points, so B sorts before a");
    set(cx, "explorer.sortOrderReverse", json!(false));
    assert_eq!(row_names(&wb, cx), ["B.txt", "a.txt", "src"]);
}

#[gpui::test]
fn compact_folders_shows_a_single_child_chain_as_one_row(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("compact");
    fs::create_dir_all(ws.path().join("a/b/c")).unwrap();
    fs::write(ws.path().join("a/b/c/deep.txt"), "").unwrap();
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    assert_eq!(row_names(&wb, cx), ["a/b/c", "src"], "the chain is one row");
    // The row opens the deepest folder: Right expands it, and its file is the next row.
    wb.update(cx, |wb, _| wb.select(&ws.path().join("a/b/c")));
    wb.update(cx, |wb, cx| wb.expand_or_first_child(cx));
    cx.run_until_parked();
    assert_eq!(row_names(&wb, cx), ["a/b/c", "deep.txt", "src"]);
    set(cx, "explorer.compactFolders", json!(false));
    assert_eq!(row_names(&wb, cx), ["a", "src"], "without compaction each folder has its own row");
}

#[gpui::test]
fn auto_reveal_expands_and_selects_the_opened_file(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("auto-reveal");
    fs::create_dir_all(ws.path().join("src/deep/er")).unwrap();
    fs::write(ws.path().join("src/deep/er/x.rs"), "").unwrap();
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let file = ws.path().join("src/deep/er/x.rs");
    wb.update(cx, |wb, _| wb.open(&file));
    cx.run_until_parked();
    let (selected_now, src_open, deep_open) = wb.read_with(cx, |wb, _| {
        (wb.explorer.selected.clone(), wb.expanded.contains(&wb.root.join("src/deep")), wb.expanded.contains(&wb.root.join("src")))
    });
    assert_eq!(selected_now, Some(file));
    assert!(src_open && deep_open, "the folders above the file are expanded");
}

#[gpui::test]
fn auto_reveal_exclude_skips_matching_paths_live(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("reveal-exclude");
    fs::create_dir_all(ws.path().join("node_modules/pkg")).unwrap();
    fs::write(ws.path().join("node_modules/pkg/index.js"), "").unwrap();
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let file = ws.path().join("node_modules/pkg/index.js");
    // The default excludes node_modules (upstream `explorer.autoRevealExclude`): nothing is revealed.
    wb.update(cx, |wb, _| wb.open(&file));
    cx.run_until_parked();
    let (selected, expanded) = wb.read_with(cx, |wb, _| (wb.explorer.selected.clone(), wb.expanded.contains(&wb.root.join("node_modules"))));
    assert_eq!((selected, expanded), (None, false), "excluded: neither selected nor expanded");
    // A user value replaces the default; `false` turns the pattern off.
    set(cx, "explorer.autoRevealExclude", json!({ "**/node_modules": false }));
    wb.update(cx, |wb, _| wb.open(&file));
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.explorer.selected.clone()), Some(file), "no longer excluded: revealed");
}

/// A tree taller than the window: 120 files at the root, so the last one is out of view.
fn tall_workspace(tag: &str) -> (TempWorkspace, TempWorkspace) {
    let (ws, bin) = workspace(tag);
    for i in 0..120 {
        fs::write(ws.path().join(format!("f{i:03}.txt")), "").unwrap();
    }
    (ws, bin)
}

fn scroll_y(wb: &gpui_kit::Entity<Workbench>, cx: &mut VisualTestContext) -> f32 {
    wb.read_with(cx, |wb, _| f32::from(wb.explorer.scroll.offset().y))
}

#[gpui::test]
fn auto_reveal_focus_no_scroll_selects_without_scrolling(cx: &mut TestAppContext) {
    let (ws, bin) = tall_workspace("reveal-noscroll");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    set(cx, "explorer.autoReveal", json!("focusNoScroll"));
    wb.update(cx, |wb, _| wb.open(&ws.path().join("f119.txt")));
    cx.run_until_parked();
    assert_eq!(scroll_y(&wb, cx), 0.0, "not scrolled");
    assert_eq!(wb.read_with(cx, |wb, _| wb.explorer.selected.clone()), Some(ws.path().join("f119.txt")), "but selected");
}

#[gpui::test]
fn auto_reveal_false_leaves_the_selection_alone(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("reveal-off");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    set(cx, "explorer.autoReveal", json!(false));
    wb.update(cx, |wb, _| wb.open(&ws.path().join("src/main.rs")));
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.explorer.selected.clone()), None);
    assert!(!wb.read_with(cx, |wb, _| wb.expanded.contains(&wb.root.join("src"))));
}

#[gpui::test]
fn refresh_explorer_relists_and_collapse_folders_closes_them(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("refresh-collapse");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let src = ws.path().join("src");
    wb.update(cx, |wb, _| wb.toggle_dir(&src));
    fs::write(src.join("made_outside.rs"), "").unwrap();
    wb.update(cx, |wb, cx| wb.refresh_all(cx));
    cx.run_until_parked();
    assert!(row_names(&wb, cx).contains(&"made_outside.rs".to_string()));
    wb.update(cx, |wb, cx| wb.collapse_all(cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| wb.expanded.is_empty()));
    assert_eq!(row_names(&wb, cx), ["src"]);
}
