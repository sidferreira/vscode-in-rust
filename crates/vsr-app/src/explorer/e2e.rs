//! Headless E2E tests for the file watcher and the explorer file operations (ADR-0039; scenarios in
//! tests/e2e-scenarios/explorer.md). A real `Workbench` in a headless window, a fake watcher whose
//! events the test pushes, and a fake Trash that moves into a test-owned folder. Nothing depends on
//! file system timing, and nothing touches the real Trash or the real settings file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};

use super::testutil::TempWorkspace;
use super::trash::FakeTrash;
use super::watcher::FakeWatcher;
use crate::Workbench;
use crate::config::AppConfig;

/// Mount a workbench on `ws` with a fake watcher and a fake Trash. Returns the sender that pushes
/// watcher events, the Trash folder, and the window.
fn start<'a>(
    cx: &'a mut TestAppContext,
    ws: &Path,
    bin: &Path,
) -> (Entity<Workbench>, &'a mut VisualTestContext, futures::channel::mpsc::UnboundedSender<Vec<PathBuf>>) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        crate::quick_open::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None); // in memory: never the real settings file
    });
    let watcher = FakeWatcher::new();
    let events = watcher.tx.clone();
    let root = ws.to_path_buf();
    let trash = FakeTrash { dir: bin.to_path_buf() };
    let (wb, cx) = cx.add_window_view(move |_, cx| {
        Workbench::with_services(root, cx, Box::new(watcher), Box::new(trash))
    });
    cx.run_until_parked();
    (wb, cx, events)
}

/// A workspace with `src/main.rs` and `src/lib.rs`, and an empty Trash folder beside it.
fn workspace(tag: &str) -> (TempWorkspace, TempWorkspace) {
    let ws = TempWorkspace::new(tag);
    fs::create_dir_all(ws.path().join("src")).unwrap();
    fs::write(ws.path().join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(ws.path().join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
    (ws, TempWorkspace::new(&format!("{tag}-trash")))
}

/// Change a file as another program would: new content, and a modification time that is surely
/// later than the one the editor recorded.
fn write_external(path: &Path, text: &str, seconds_later: u64) {
    fs::write(path, text).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(10 + seconds_later)).unwrap();
}

fn tree_names(wb: &Entity<Workbench>, cx: &mut VisualTestContext, dir: &Path) -> Vec<String> {
    wb.read_with(cx, |wb, _| {
        wb.children
            .get(dir)
            .map(|v| v.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    })
}

#[gpui::test]
fn a_new_file_appears_in_the_tree_and_opens(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("new-file");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let root = ws.path().to_path_buf();
    let created = wb.update(cx, |wb, cx| wb.create_entry(&root, "notes.txt", false, cx)).expect("created");
    cx.run_until_parked();
    assert!(created.is_file());
    assert!(tree_names(&wb, cx, &root).contains(&"notes.txt".to_string()));
    let active = wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.path.clone()));
    assert_eq!(active, Some(created));
}

#[gpui::test]
fn creating_a_name_that_exists_is_refused_with_the_upstream_message(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("exists");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let src = ws.path().join("src");
    let err = wb.update(cx, |wb, cx| wb.create_entry(&src, "lib.rs", false, cx)).unwrap_err();
    assert_eq!(err, "A file or folder lib.rs already exists at this location. Please choose a different name.");
    let empty = wb.update(cx, |wb, cx| wb.create_entry(&src, "  ", false, cx)).unwrap_err();
    assert_eq!(empty, "A file or folder name must be provided.");
}

#[gpui::test]
fn rename_moves_the_open_tab_to_the_new_path(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("rename");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let main = ws.path().join("src/main.rs");
    wb.update(cx, |wb, _| wb.open(&main));
    let src = ws.path().join("src");
    let to = wb.update(cx, |wb, cx| wb.rename_entry(&main, "app.rs", cx)).expect("renamed");
    cx.run_until_parked();
    assert!(!main.exists() && to.is_file());
    let titles = wb.read_with(cx, |wb, _| wb.editors.open.iter().map(|e| (e.path.clone(), e.title.clone())).collect::<Vec<_>>());
    assert_eq!(titles, vec![(src.join("app.rs"), "app.rs".to_string())]);
}

#[gpui::test]
fn delete_moves_to_the_trash_and_closes_the_clean_tab(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("delete");
    let (wb, cx, _events) = start(cx, ws.path(), bin.path());
    let lib = ws.path().join("src/lib.rs");
    wb.update(cx, |wb, _| wb.open(&lib));
    let src = ws.path().join("src");
    wb.update(cx, |wb, cx| wb.delete_entry(&lib, cx)).expect("deleted");
    cx.run_until_parked();
    assert!(!lib.exists(), "gone from the workspace");
    assert!(bin.path().join("lib.rs").is_file(), "in the Trash");
    assert!(!tree_names(&wb, cx, &src).contains(&"lib.rs".to_string()));
    assert!(wb.read_with(cx, |wb, _| wb.editors.open.is_empty()));
}

#[gpui::test]
fn an_external_create_refreshes_the_tree_and_keeps_expanded_folders(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("external-create");
    let (wb, cx, events) = start(cx, ws.path(), bin.path());
    let root = ws.path().to_path_buf();
    let src = root.join("src");
    wb.update(cx, |wb, _| wb.toggle_dir(&src));
    cx.run_until_parked();
    let added = src.join("new_module.rs");
    fs::write(&added, "").unwrap();
    events.unbounded_send(vec![added.clone()]).unwrap();
    cx.run_until_parked();
    assert!(tree_names(&wb, cx, &src).contains(&"new_module.rs".to_string()));
    assert!(wb.read_with(cx, |wb, _| wb.expanded.contains(&src)), "still expanded");
}

#[gpui::test]
fn a_change_under_watcher_exclude_is_ignored(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("excluded");
    let (wb, cx, events) = start(cx, ws.path(), bin.path());
    let root = ws.path().to_path_buf();
    let git_obj = root.join(".git/objects");
    fs::create_dir_all(&git_obj).unwrap();
    fs::write(git_obj.join("pack"), "x").unwrap();
    events.unbounded_send(vec![git_obj.join("pack")]).unwrap();
    cx.run_until_parked();
    // The `.git` folder is not in the tree's listing cache, and the change did not touch it.
    assert!(wb.read_with(cx, |wb, _| !wb.children.contains_key(&git_obj)));
}

#[gpui::test]
fn an_external_change_reloads_a_clean_editor(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("reload");
    let (wb, cx, events) = start(cx, ws.path(), bin.path());
    let main = ws.path().join("src/main.rs");
    wb.update(cx, |wb, _| wb.open(&main));
    cx.run_until_parked();
    write_external(&main, "fn main() { println!(\"changed\"); }\n", 1);
    events.unbounded_send(vec![main.clone()]).unwrap();
    cx.run_until_parked();
    let text = wb.read_with(cx, |wb, _| wb.editors.open[0].buffer().value().to_string());
    assert_eq!(text, "fn main() { println!(\"changed\"); }\n");
    assert!(wb.read_with(cx, |wb, _| !wb.editors.open[0].is_dirty()));
}

#[gpui::test]
fn a_dirty_editor_keeps_its_text_and_save_offers_overwrite_or_revert(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("conflict");
    let (wb, cx, events) = start(cx, ws.path(), bin.path());
    let main = ws.path().join("src/main.rs");
    wb.update(cx, |wb, _| wb.open(&main));
    cx.run_until_parked();
    cx.simulate_input("// mine\n");
    cx.run_until_parked();
    write_external(&main, "fn main() { /* theirs */ }\n", 2);
    events.unbounded_send(vec![main.clone()]).unwrap();
    cx.run_until_parked();
    let kept = wb.read_with(cx, |wb, _| wb.editors.open[0].buffer().value().to_string());
    assert!(kept.starts_with("// mine"), "the dirty buffer is not reloaded");

    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.editor_view.save_conflict.clone()), Some(main.clone()));
    assert_eq!(fs::read_to_string(&main).unwrap(), "fn main() { /* theirs */ }\n", "the save was refused");

    wb.update(cx, |wb, cx| wb.resolve_save_conflict(true, cx));
    cx.run_until_parked();
    assert_eq!(fs::read_to_string(&main).unwrap(), kept, "Overwrite writes the buffer");
    assert_eq!(wb.read_with(cx, |wb, _| wb.editor_view.save_conflict.clone()), None);
}

#[gpui::test]
fn revert_replaces_the_buffer_with_the_file_on_disk(cx: &mut TestAppContext) {
    let (ws, bin) = workspace("revert");
    let (wb, cx, events) = start(cx, ws.path(), bin.path());
    let main = ws.path().join("src/main.rs");
    wb.update(cx, |wb, _| wb.open(&main));
    cx.run_until_parked();
    cx.simulate_input("// mine\n");
    cx.run_until_parked();
    write_external(&main, "fn main() { /* theirs */ }\n", 3);
    events.unbounded_send(vec![main.clone()]).unwrap();
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-s");
    cx.run_until_parked();
    wb.update(cx, |wb, cx| wb.resolve_save_conflict(false, cx));
    cx.run_until_parked();
    let text = wb.read_with(cx, |wb, _| wb.editors.open[0].buffer().value().to_string());
    assert_eq!(text, "fn main() { /* theirs */ }\n");
    assert!(wb.read_with(cx, |wb, _| !wb.editors.open[0].is_dirty()));
}
