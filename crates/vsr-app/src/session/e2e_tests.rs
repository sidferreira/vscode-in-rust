//! Headless end-to-end tests of session restore, hot exit and the Welcome page (docs/testing.md,
//! tier 2). Our own tests; the scenarios are in tests/e2e-scenarios/workbench.md ("Session restore",
//! "Hot exit", "Welcome page").
//!
//! A "restart" is a second `Workbench` over the same folder in the same test app, with the same
//! injected user data directory; "quit" is `persist_on_quit` (the quit hook) or `request_quit`, never
//! the process exiting. The user data directory, the settings.json and the folder are all fresh temp
//! directories: the real `~/Library/Application Support/rscode` is never read or written.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use gpui_kit::{Entity, MouseButton, TestAppContext, VisualTestContext};
use vsr_editor::{Position, Selection};

use super::backup::BackupStore;
use super::state::{GlobalState, ViewState, WorkspaceState};
use super::store::testing::unique_dir;
use super::store::{SessionStorage, SessionStore, path_key};
use super::runtime::TICK;
use crate::Workbench;
use crate::config::AppConfig;

struct Env {
    root: PathBuf,
    data: PathBuf,
    settings: PathBuf,
}

fn env(name: &str, settings: Option<&str>) -> Env {
    let root = unique_dir(&format!("{name}-folder"));
    let data = unique_dir(&format!("{name}-userdata"));
    let settings_path = unique_dir(&format!("{name}-settings")).join("settings.json");
    if let Some(text) = settings {
        fs::write(&settings_path, text).unwrap();
    }
    let lines: String = (1..=400).map(|i| format!("line {i}\n")).collect();
    for f in ["a.txt", "b.txt", "c.txt"] {
        fs::write(root.join(f), &lines).unwrap();
    }
    Env { root, data, settings: settings_path }
}

impl Env {
    fn backups(&self) -> BackupStore {
        BackupStore::new(SessionStore::new(&self.data).backups_dir(&self.root))
    }
    fn state_file(&self) -> PathBuf {
        SessionStore::new(&self.data).workspace_state_path(&self.root)
    }
}

/// A workbench over the env's folder with its user data directory injected.
fn start<'a>(cx: &'a mut TestAppContext, env: &Env) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let (settings, data) = (env.settings.clone(), env.data.clone());
    cx.update(|cx| {
        if !cx.has_global::<SessionStorage>() {
            gpui_kit::init(cx);
            crate::icons::install(cx);
            crate::settings_view::install(cx);
            crate::editor_view::init(cx);
        }
        AppConfig::install(cx, Some(settings));
        cx.set_global(SessionStorage(Some(SessionStore::new(data))));
    });
    let root = env.root.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    (wb, vcx)
}

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
}

fn titles(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, _| wb.editors.open.iter().map(|e| e.title.clone()).collect())
}

fn active_title(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.title.clone()))
}

/// Selects `from..to` and scrolls so that `top` is the first visible line, in the active editor.
fn place(wb: &Entity<Workbench>, cx: &mut VisualTestContext, top: usize, from: (usize, usize), to: (usize, usize)) {
    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.unwrap();
        let view = ViewState { top_line: top, selection: None, word_wrap: None };
        wb.editor_view.apply_view_state(&mut wb.editors.open[i], &view);
        wb.editors.open[i].editor.set_selection(Selection { anchor: Position::new(from.0, from.1), active: Position::new(to.0, to.1) });
        cx.notify();
    });
    cx.run_until_parked();
}

fn view_of_active(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> ViewState {
    wb.read_with(cx, |wb, _| wb.editor_view.view_state(wb.editors.active_editor().unwrap()))
}

fn type_in_active(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &str) {
    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.unwrap();
        wb.editors.open[i].editor.type_text(text);
        cx.notify();
    });
    cx.run_until_parked();
}

fn quit(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    wb.update(cx, |wb, cx| wb.persist_on_quit(cx));
}

fn tick(cx: &mut VisualTestContext) {
    cx.executor().advance_clock(TICK);
    cx.run_until_parked();
}

fn value_of(wb: &Entity<Workbench>, cx: &mut VisualTestContext, title: &str) -> String {
    wb.read_with(cx, |wb, _| wb.editors.open.iter().find(|e| e.title == title).unwrap().buffer().value())
}

fn dirty_of(wb: &Entity<Workbench>, cx: &mut VisualTestContext, title: &str) -> bool {
    wb.read_with(cx, |wb, _| wb.editors.open.iter().find(|e| e.title == title).unwrap().is_dirty())
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} is not rendered"));
    cx.simulate_mouse_move(b.center(), None, Default::default());
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
}

// ---- session restore ----------------------------------------------------------------------------

#[gpui_kit::test]
fn quitting_and_restarting_restores_tabs_active_scroll_and_selection(cx: &mut TestAppContext) {
    let env = env("restore", None);
    let (wb, vcx) = start(cx, &env);
    for f in ["a.txt", "b.txt", "c.txt"] {
        open(&wb, vcx, &env.root.join(f));
    }
    place(&wb, vcx, 0, (7, 2), (9, 4)); // c.txt (active)
    wb.update(vcx, |wb, cx| {
        wb.editors.activate(0);
        cx.notify();
    });
    vcx.run_until_parked();
    place(&wb, vcx, 120, (30, 3), (30, 8)); // a.txt, scrolled and selected
    wb.update(vcx, |wb, cx| {
        wb.editors.activate(1);
        cx.notify();
    });
    vcx.run_until_parked();
    place(&wb, vcx, 55, (2, 1), (2, 1)); // b.txt, the active tab at quit
    let before = view_of_active(&wb, vcx);
    assert_eq!(before.top_line, 55, "the scroll position is readable back from a painted editor");
    quit(&wb, vcx);

    assert!(env.state_file().is_file(), "the state is under the injected user data dir");
    let (wb, vcx) = start(cx, &env);
    assert_eq!(titles(&wb, vcx), ["a.txt", "b.txt", "c.txt"], "tabs in order");
    assert_eq!(active_title(&wb, vcx).as_deref(), Some("b.txt"));
    assert_eq!(view_of_active(&wb, vcx).top_line, 55, "scroll");
    wb.update(vcx, |wb, cx| {
        wb.editors.activate(0);
        cx.notify();
    });
    vcx.run_until_parked();
    let a = view_of_active(&wb, vcx);
    assert_eq!(a.top_line, 120);
    let sel = a.selection.unwrap();
    assert_eq!((sel.anchor, sel.active), ((30, 3), (30, 8)), "selection");
    assert!(!wb.read_with(vcx, |wb, _| wb.welcome_open), "no Welcome page: editors were restored");
}

#[gpui_kit::test]
fn preview_flag_active_and_mru_survive_a_restart(cx: &mut TestAppContext) {
    let env = env("preview", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    open(&wb, vcx, &env.root.join("b.txt"));
    wb.update(vcx, |wb, cx| {
        wb.open_with(&env.root.join("c.txt"), crate::model::OpenOptions::PREVIEW);
        wb.editors.activate(0);
        cx.notify();
    });
    quit(&wb, vcx);
    let (wb, vcx) = start(cx, &env);
    assert_eq!(titles(&wb, vcx), ["a.txt", "b.txt", "c.txt"]);
    assert_eq!(active_title(&wb, vcx).as_deref(), Some("a.txt"));
    wb.read_with(vcx, |wb, _| {
        assert!(wb.editors.is_preview(2) && !wb.editors.is_preview(0));
        assert_eq!(wb.editors.mru.first().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).as_deref(), Some("a.txt"));
        assert_eq!(wb.editors.mru.len(), 3);
    });
}

#[gpui_kit::test]
fn files_deleted_since_are_skipped_on_restore(cx: &mut TestAppContext) {
    let env = env("gone", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    open(&wb, vcx, &env.root.join("b.txt"));
    quit(&wb, vcx);
    fs::remove_file(env.root.join("a.txt")).unwrap();
    let (wb, vcx) = start(cx, &env);
    assert_eq!(titles(&wb, vcx), ["b.txt"]);
    assert_eq!(active_title(&wb, vcx).as_deref(), Some("b.txt"));
}

#[gpui_kit::test]
fn the_side_bar_and_the_search_query_are_restored(cx: &mut TestAppContext) {
    let env = env("layout", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    wb.update(vcx, |wb, cx| {
        wb.view = crate::ActivityView::Search;
        wb.sidebar_width = 333.0;
        wb.session.pending_search = Some(super::state::SearchState { query: "line 3".into(), match_case: true, ..Default::default() });
        cx.notify();
    });
    vcx.run_until_parked(); // the render hook creates the Search view and applies the query
    wb.read_with(vcx, |wb, _| assert!(wb.search.is_some()));
    quit(&wb, vcx);
    let saved: WorkspaceState = serde_json::from_str(&fs::read_to_string(env.state_file()).unwrap()).unwrap();
    assert_eq!(saved.side_bar.view, "search");
    assert_eq!(saved.search.as_ref().map(|s| (s.query.as_str(), s.match_case)), Some(("line 3", true)));

    let (wb, vcx) = start(cx, &env);
    wb.read_with(vcx, |wb, cx| {
        assert!(wb.view == crate::ActivityView::Search);
        assert_eq!(wb.sidebar_width, 333.0);
        let s = wb.search.as_ref().expect("the Search view exists again").read(cx).session_state(cx);
        assert_eq!((s.query.as_str(), s.match_case), ("line 3", true));
    });
}

#[gpui_kit::test]
fn restore_windows_none_and_restore_editors_off_start_empty(cx: &mut TestAppContext) {
    for setting in [r#"{ "window.restoreWindows": "none" }"#, r#"{ "workbench.editor.restoreEditors": false }"#] {
        let env = env("norestore", Some("{}"));
        let (wb, vcx) = start(cx, &env);
        open(&wb, vcx, &env.root.join("a.txt"));
        quit(&wb, vcx);
        fs::write(&env.settings, setting).unwrap();
        let (wb, vcx) = start(cx, &env);
        assert!(titles(&wb, vcx).is_empty(), "{setting}");
        assert!(wb.read_with(vcx, |wb, _| wb.welcome_showing()), "the Welcome page shows instead");
    }
}

#[gpui_kit::test]
fn a_closed_file_reopens_at_its_view_state_and_restore_view_state_off_does_not(cx: &mut TestAppContext) {
    for (name, setting, kept) in [("viewstate-on", "{}", true), ("viewstate-off", r#"{ "workbench.editor.restoreViewState": false }"#, false)] {
        let env = env(name, Some(setting));
        let (wb, vcx) = start(cx, &env);
        open(&wb, vcx, &env.root.join("a.txt"));
        place(&wb, vcx, 90, (50, 2), (51, 5));
        wb.update(vcx, |wb, cx| {
            wb.close_tab(0);
            cx.notify();
        });
        vcx.run_until_parked();
        assert!(titles(&wb, vcx).is_empty());
        open(&wb, vcx, &env.root.join("a.txt"));
        let v = view_of_active(&wb, vcx);
        if kept {
            assert_eq!(v.top_line, 90, "scroll is back");
            let s = v.selection.unwrap();
            assert_eq!((s.anchor, s.active), ((50, 2), (51, 5)), "selection is back");
        } else {
            assert_eq!(v.top_line, 0);
            assert_eq!(v.selection.unwrap().active, (1, 1));
        }
    }
}

#[gpui_kit::test]
fn cmd_w_remembers_the_view_state_and_a_reopen_before_the_next_tick_does_not_delete_the_backup(cx: &mut TestAppContext) {
    let env = env("cmd-w", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    place(&wb, vcx, 90, (50, 2), (50, 2));
    type_in_active(&wb, vcx, "K");
    tick(vcx);
    tick(vcx);
    assert!(env.backups().exists(&env.root.join("a.txt")));
    vcx.simulate_keystrokes("cmd-w"); // the keyboard path, not the tab strip's close button
    assert_eq!(titles(&wb, vcx), ["a.txt"], "a dirty tab asks first and stays open");
    click(vcx, "close-prompt:discard"); // Don't Save: the backup is moved aside
    assert!(titles(&wb, vcx).is_empty());
    open(&wb, vcx, &env.root.join("a.txt")); // reopened before the next tick: clean, from disk
    assert!(!dirty_of(&wb, vcx, "a.txt"));
    let v = view_of_active(&wb, vcx);
    assert_eq!(v.top_line, 90, "scroll is back after a Cmd+W close");
    assert_eq!(v.selection.unwrap().active, (50, 3), "the caret after the typed K");
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")));
    let discarded = env.data.join(format!("Backups/{}/discarded", path_key(&env.root)));
    let kept: Vec<_> = fs::read_dir(discarded).unwrap().collect();
    assert_eq!(kept.len(), 1, "the dropped text was moved aside, not deleted by the clean reopened buffer");
    assert!(fs::read_to_string(kept[0].as_ref().unwrap().path()).unwrap().contains("lKine 50"));
}

#[gpui_kit::test]
fn a_damaged_state_file_is_kept_aside_and_the_start_is_clean(cx: &mut TestAppContext) {
    let env = env("corrupt-state", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    quit(&wb, vcx);
    let bytes = fs::read(env.state_file()).unwrap();
    fs::write(env.state_file(), &bytes[..bytes.len() / 2]).unwrap();
    let (wb, vcx) = start(cx, &env);
    assert!(titles(&wb, vcx).is_empty());
    let kept = env.state_file().with_file_name("state.json.corrupt-1");
    assert_eq!(fs::read(&kept).unwrap(), bytes[..bytes.len() / 2], "the damaged bytes are kept");
    assert!(wb.read_with(vcx, |wb, _| wb.error.as_deref().is_some_and(|e| e.contains("state.json.corrupt-1"))), "and the user is told");
}

// ---- hot exit -----------------------------------------------------------------------------------

#[gpui_kit::test]
fn a_dirty_buffer_survives_quit_and_restart_as_dirty_with_its_text(cx: &mut TestAppContext) {
    let env = env("hotexit", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    open(&wb, vcx, &env.root.join("b.txt"));
    type_in_active(&wb, vcx, "UNSAVED ");
    let edited = value_of(&wb, vcx, "b.txt");
    assert!(edited.starts_with("UNSAVED line 1"));
    // Cmd+Q with a dirty buffer and the default `files.hotExit: onExit`: no prompt, it may quit.
    assert!(wb.update(vcx, |wb, cx| wb.request_quit(false, cx)));
    assert!(!wb.read_with(vcx, |wb, _| wb.session.prompt));
    assert_eq!(fs::read_to_string(env.root.join("b.txt")).unwrap().lines().next(), Some("line 1"), "the file on disk is untouched");

    let (wb, vcx) = start(cx, &env);
    assert_eq!(titles(&wb, vcx), ["a.txt", "b.txt"]);
    assert_eq!(value_of(&wb, vcx, "b.txt"), edited, "the same text");
    assert!(dirty_of(&wb, vcx, "b.txt"), "still dirty");
    assert!(!dirty_of(&wb, vcx, "a.txt"));
    assert!(env.backups().exists(&env.root.join("b.txt")), "the backup stays until the buffer is saved");
    // Undo goes back to what is on disk.
    wb.update(vcx, |wb, _| {
        let i = wb.editors.index_of(&wb.root.join("b.txt")).unwrap();
        wb.editors.open[i].editor.undo();
    });
    assert!(value_of(&wb, vcx, "b.txt").starts_with("line 1"));
}

#[gpui_kit::test]
fn a_backup_is_written_after_a_quiet_delay_so_a_crash_loses_nothing(cx: &mut TestAppContext) {
    let env = env("crash", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "X");
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "not yet: the text must be stable for a tick");
    type_in_active(&wb, vcx, "Y");
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "still typing: the debounce restarts");
    tick(vcx);
    assert!(env.backups().exists(&env.root.join("a.txt")), "written once quiet");
    // No quit hook ran (a crash): a restart still brings the text back.
    let (wb, vcx) = start(cx, &env);
    assert!(value_of(&wb, vcx, "a.txt").starts_with("XYline 1"));
    assert!(dirty_of(&wb, vcx, "a.txt"));
}

#[gpui_kit::test]
fn saving_removes_the_backup_and_a_restart_shows_no_stale_text(cx: &mut TestAppContext) {
    let env = env("saved", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "EDIT ");
    tick(vcx);
    tick(vcx);
    assert!(env.backups().exists(&env.root.join("a.txt")));
    vcx.simulate_keystrokes("cmd-s");
    assert!(!dirty_of(&wb, vcx, "a.txt"));
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "saved: the backup is gone");
    quit(&wb, vcx);
    let (wb, vcx) = start(cx, &env);
    assert!(!dirty_of(&wb, vcx, "a.txt"));
    assert!(value_of(&wb, vcx, "a.txt").starts_with("EDIT line 1"), "the saved text, from disk");
    assert!(env.backups().load_all().backups.is_empty());
}

#[gpui_kit::test]
fn undoing_back_to_the_saved_text_drops_the_backup(cx: &mut TestAppContext) {
    let env = env("undo-clean", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "Z");
    tick(vcx);
    tick(vcx);
    assert!(env.backups().exists(&env.root.join("a.txt")));
    wb.update(vcx, |wb, _| {
        wb.editors.open[0].editor.undo();
    });
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "the buffer equals the file again");
}

#[gpui_kit::test]
fn a_tab_closed_with_unsaved_changes_moves_its_backup_aside_instead_of_deleting_it(cx: &mut TestAppContext) {
    let env = env("close-dirty", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "KEEPME ");
    tick(vcx);
    tick(vcx);
    wb.update(vcx, |wb, cx| wb.request_close_tab(0, cx));
    vcx.run_until_parked();
    click(vcx, "close-prompt:discard");
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "it does not come back as a tab");
    let discarded: Vec<_> = fs::read_dir(env.data.join(format!("Backups/{}/discarded", path_key(&env.root)))).unwrap().collect();
    assert_eq!(discarded.len(), 1, "but the text is still on disk");
    assert!(fs::read_to_string(discarded[0].as_ref().unwrap().path()).unwrap().contains("KEEPME"));
}

#[gpui_kit::test]
fn a_damaged_backup_is_kept_aside_and_the_rest_restore(cx: &mut TestAppContext) {
    let env = env("corrupt-backup", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    open(&wb, vcx, &env.root.join("b.txt"));
    type_in_active(&wb, vcx, "B-EDIT ");
    wb.update(vcx, |wb, cx| {
        wb.editors.activate(0);
        cx.notify();
    });
    type_in_active(&wb, vcx, "A-EDIT ");
    quit(&wb, vcx);
    let a_file = env.data.join(format!("Backups/{}/{}.json", path_key(&env.root), path_key(&env.root.join("a.txt"))));
    let full = fs::read(&a_file).unwrap();
    fs::write(&a_file, &full[..full.len() - 20]).unwrap(); // a truncated write
    let (wb, vcx) = start(cx, &env);
    assert!(value_of(&wb, vcx, "b.txt").starts_with("B-EDIT"), "the intact backup restored");
    assert!(!dirty_of(&wb, vcx, "a.txt"), "the damaged one did not");
    assert_eq!(fs::read(format!("{}.corrupt-1", a_file.display())).unwrap(), full[..full.len() - 20], "and its bytes are kept");
    assert!(wb.read_with(vcx, |wb, _| wb.error.as_deref().is_some_and(|e| e.contains("damaged backup"))));
}

#[gpui_kit::test]
fn a_backup_of_a_file_deleted_since_restores_into_a_buffer_that_can_save_it_again(cx: &mut TestAppContext) {
    let env = env("deleted", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("c.txt"));
    type_in_active(&wb, vcx, "PRECIOUS ");
    quit(&wb, vcx);
    fs::remove_file(env.root.join("c.txt")).unwrap();
    let (wb, vcx) = start(cx, &env);
    assert!(dirty_of(&wb, vcx, "c.txt"));
    assert!(value_of(&wb, vcx, "c.txt").starts_with("PRECIOUS line 1"));
}

#[gpui_kit::test]
fn unsaved_text_restores_even_when_editors_are_not_restored(cx: &mut TestAppContext) {
    let env = env("hotexit-norestore", Some("{}"));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "SAFE ");
    quit(&wb, vcx);
    fs::write(&env.settings, r#"{ "window.restoreWindows": "none" }"#).unwrap();
    let (wb, vcx) = start(cx, &env);
    assert_eq!(titles(&wb, vcx), ["a.txt"], "the dirty buffer is not skipped");
    assert!(value_of(&wb, vcx, "a.txt").starts_with("SAFE line 1"));
    assert!(!wb.read_with(vcx, |wb, _| wb.welcome_open), "an editor came back, so no Welcome page");
}

#[gpui_kit::test]
fn a_backup_equal_to_the_file_is_dropped(cx: &mut TestAppContext) {
    let env = env("same", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "Q");
    quit(&wb, vcx);
    // The file was changed on disk to exactly the backed-up text meanwhile.
    let text = value_of(&wb, vcx, "a.txt");
    fs::write(env.root.join("a.txt"), &text).unwrap();
    let (wb, vcx) = start(cx, &env);
    assert!(!dirty_of(&wb, vcx, "a.txt"));
    assert!(!env.backups().exists(&env.root.join("a.txt")));
}

// ---- files.hotExit and the save prompt ----------------------------------------------------------

#[gpui_kit::test]
fn hot_exit_off_asks_and_cancel_keeps_the_app_open(cx: &mut TestAppContext) {
    let env = env("off-cancel", Some(r#"{ "files.hotExit": "off" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    assert!(wb.update(vcx, |wb, cx| wb.request_quit(false, cx)), "nothing unsaved: it quits without asking");
    type_in_active(&wb, vcx, "W");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(false, cx)), "unsaved changes: the bar is raised instead");
    vcx.run_until_parked();
    assert!(vcx.debug_bounds("quit-prompt").is_some());
    click(vcx, "quit-prompt:cancel");
    assert!(vcx.debug_bounds("quit-prompt").is_none());
    assert!(dirty_of(&wb, vcx, "a.txt") && !wb.read_with(vcx, |wb, _| wb.session.quit_requested));
}

#[gpui_kit::test]
fn hot_exit_off_save_all_saves_then_quits(cx: &mut TestAppContext) {
    let env = env("off-save", Some(r#"{ "files.hotExit": "off" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "SAVED ");
    tick(vcx);
    tick(vcx);
    assert!(env.backups().exists(&env.root.join("a.txt")), "crash-safety backups are written with hot exit off too");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(false, cx)));
    vcx.run_until_parked();
    click(vcx, "quit-prompt:save");
    assert!(wb.read_with(vcx, |wb, _| wb.session.quit_requested));
    assert!(fs::read_to_string(env.root.join("a.txt")).unwrap().starts_with("SAVED line 1"));
    assert!(!env.backups().exists(&env.root.join("a.txt")), "saved: no backup left");
}

#[gpui_kit::test]
fn quit_save_all_on_a_file_changed_on_disk_keeps_the_app_open_and_the_disk_text(cx: &mut TestAppContext) {
    let env = env("quit-conflict", Some(r#"{ "files.hotExit": "off" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "MINE ");
    tick(vcx);
    tick(vcx);
    write_elsewhere(&env.root.join("a.txt"), "theirs\n");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(false, cx)), "unsaved changes: the bar is raised");
    vcx.run_until_parked();
    click(vcx, "quit-prompt:save");
    assert!(!wb.read_with(vcx, |wb, _| wb.session.quit_requested), "the app does not quit");
    assert!(vcx.debug_bounds("quit-prompt").is_none(), "the bar is gone");
    assert_eq!(wb.read_with(vcx, |wb, _| wb.editor_view.save_conflict.clone()), Some(env.root.join("a.txt")), "the conflict bar shows");
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap(), "theirs\n", "the external version is kept");
    assert!(dirty_of(&wb, vcx, "a.txt"), "the text is still there");
}

#[gpui_kit::test]
fn quit_save_all_saves_the_other_files_and_shows_the_refused_one(cx: &mut TestAppContext) {
    let env = env("quit-conflict-others", Some(r#"{ "files.hotExit": "off" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "MINE ");
    open(&wb, vcx, &env.root.join("b.txt"));
    type_in_active(&wb, vcx, "B ");
    write_elsewhere(&env.root.join("a.txt"), "theirs\n");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(false, cx)));
    vcx.run_until_parked();
    click(vcx, "quit-prompt:save");
    assert!(!wb.read_with(vcx, |wb, _| wb.session.quit_requested));
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap(), "theirs\n", "a.txt is not overwritten");
    assert!(fs::read_to_string(env.root.join("b.txt")).unwrap().starts_with("B line 1"), "b.txt is saved");
    assert!(!dirty_of(&wb, vcx, "b.txt"));
    assert_eq!(active_title(&wb, vcx).as_deref(), Some("a.txt"), "the refused editor is shown");
    assert_eq!(wb.read_with(vcx, |wb, _| wb.editor_view.save_conflict.clone()), Some(env.root.join("a.txt")));
}

#[gpui_kit::test]
fn quit_save_all_with_overwrite_file_on_disk_saves_the_changed_file_and_quits(cx: &mut TestAppContext) {
    let env = env("quit-overwrite", Some(r#"{ "files.hotExit": "off", "files.saveConflictResolution": "overwriteFileOnDisk" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "MINE ");
    write_elsewhere(&env.root.join("a.txt"), "theirs\n");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(false, cx)));
    vcx.run_until_parked();
    click(vcx, "quit-prompt:save");
    assert!(wb.read_with(vcx, |wb, _| wb.session.quit_requested), "saved, so it quits");
    assert!(fs::read_to_string(env.root.join("a.txt")).unwrap().starts_with("MINE line 1"), "the buffer overwrote the file");
}

#[gpui_kit::test]
fn hot_exit_off_dont_save_discards_but_keeps_the_text_aside(cx: &mut TestAppContext) {
    let env = env("off-discard", Some(r#"{ "files.hotExit": "off" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "DROP ");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(true, cx)));
    vcx.run_until_parked();
    click(vcx, "quit-prompt:discard");
    assert!(wb.read_with(vcx, |wb, _| wb.session.quit_requested));
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap().lines().next(), Some("line 1"), "disk untouched");
    assert!(!env.backups().exists(&env.root.join("a.txt")));
    let kept = fs::read_dir(env.data.join(format!("Backups/{}/discarded", path_key(&env.root)))).unwrap().count();
    assert_eq!(kept, 1, "moved aside, not deleted");
    let (wb, vcx) = start(cx, &env);
    assert!(!dirty_of(&wb, vcx, "a.txt"), "and it does not come back");
}

#[gpui_kit::test]
fn window_close_asks_with_on_exit_but_not_with_on_exit_and_window_close(cx: &mut TestAppContext) {
    let env = env("window-close", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "W");
    assert!(!wb.update(vcx, |wb, cx| wb.request_quit(true, cx)), "onExit: closing the window asks");
    click(vcx, "quit-prompt:cancel");
    assert!(wb.update(vcx, |wb, cx| wb.request_quit(false, cx)), "onExit: quitting keeps the text and goes");
    fs::write(&env.settings, r#"{ "files.hotExit": "onExitAndWindowClose" }"#).unwrap();
    let (wb, vcx) = start(cx, &env);
    assert!(dirty_of(&wb, vcx, "a.txt"), "restored from the backup");
    assert!(wb.update(vcx, |wb, cx| wb.request_quit(true, cx)), "onExitAndWindowClose: no prompt");
}

// ---- the Welcome page ---------------------------------------------------------------------------

#[gpui_kit::test]
fn with_nothing_restored_the_welcome_tab_shows_and_a_recent_file_opens_on_click(cx: &mut TestAppContext) {
    let env = env("welcome", None);
    let (wb, vcx) = start(cx, &env);
    assert!(wb.read_with(vcx, |wb, _| wb.welcome_showing()), "first start: the Welcome page");
    assert!(vcx.debug_bounds("welcome-tab").is_some() && vcx.debug_bounds("welcome-page").is_some());
    assert!(vcx.debug_bounds("welcome:open-file").is_some());
    assert!(vcx.debug_bounds("welcome:recent:0").is_none(), "no recent files yet");
    open(&wb, vcx, &env.root.join("b.txt"));
    wb.update(vcx, |wb, cx| {
        wb.close_tab(0);
        cx.notify();
    });
    quit(&wb, vcx);
    let recents: GlobalState = serde_json::from_str(&fs::read_to_string(SessionStore::new(&env.data).global_state_path()).unwrap()).unwrap();
    assert_eq!(recents.files.len(), 1);
    assert_eq!(recents.folders, [env.root.to_string_lossy()]);

    let (wb, vcx) = start(cx, &env);
    assert!(titles(&wb, vcx).is_empty());
    assert!(wb.read_with(vcx, |wb, _| wb.welcome_showing()), "nothing restored: Welcome");
    click(vcx, "welcome:recent:0");
    assert_eq!(titles(&wb, vcx), ["b.txt"], "clicking a recent file opens it");
    assert!(!wb.read_with(vcx, |wb, _| wb.welcome_showing()), "and the editor is shown");
    assert!(vcx.debug_bounds("welcome-tab").is_some(), "the Welcome tab stays open");
    click(vcx, "welcome-tab");
    assert!(wb.read_with(vcx, |wb, _| wb.welcome_showing()));
    click(vcx, "welcome-tab-close");
    assert!(vcx.debug_bounds("welcome-tab").is_none());
}

#[gpui_kit::test]
fn welcome_open_file_opens_quick_open(cx: &mut TestAppContext) {
    let env = env("welcome-open", None);
    let (wb, vcx) = start(cx, &env);
    click(vcx, "welcome:open-file");
    assert!(wb.read_with(vcx, |wb, _| wb.quick.picker().is_some()), "Open File... is Quick Open");
}

#[gpui_kit::test]
fn the_show_on_startup_checkbox_writes_workbench_startup_editor(cx: &mut TestAppContext) {
    let env = env("welcome-checkbox", Some("{}"));
    let (_wb, vcx) = start(cx, &env);
    click(vcx, "welcome:startup-checkbox");
    assert_eq!(vsr_settings::UserSettings::load(&env.settings).unwrap().get("workbench.startupEditor"), Some(&serde_json::json!("none")));
    click(vcx, "welcome:startup-checkbox");
    assert_eq!(vsr_settings::UserSettings::load(&env.settings).unwrap().get("workbench.startupEditor"), None, "back to the default");
}

#[gpui_kit::test]
fn help_welcome_is_a_registered_command_that_opens_the_page(cx: &mut TestAppContext) {
    let env = env("welcome-command", Some(r#"{ "workbench.startupEditor": "none" }"#));
    let (wb, vcx) = start(cx, &env);
    assert!(!wb.read_with(vcx, |wb, _| wb.welcome_open));
    let known = vcx.update(|_, cx| cx.global::<crate::commands::Commands>().registry.handler("workbench.action.openWalkthrough").is_some());
    assert!(known, "the command is in the registry");
    vcx.dispatch_action(crate::welcome::ShowWelcome);
    vcx.run_until_parked();
    assert!(wb.read_with(vcx, |wb, _| wb.welcome_showing()));
}

#[gpui_kit::test]
fn a_workbench_without_an_installed_store_writes_nothing(cx: &mut TestAppContext) {
    // The existing tests build a Workbench with no SessionStorage: nothing may be read or written.
    let root = unique_dir("nostore");
    fs::write(root.join("a.txt"), "x").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::editor_view::init(cx);
    });
    let r = root.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(r, cx));
    wb.update(vcx, |wb, cx| {
        wb.open(&root.join("a.txt"));
        wb.persist_on_quit(cx);
        assert!(wb.session.store.is_none() && wb.session.backups.is_none());
    });
    assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
}

// ---- closing a tab with unsaved changes (ADR-0040, session/close.rs) ----------------------------

fn bar_up(vcx: &mut VisualTestContext) -> bool {
    vcx.run_until_parked();
    vcx.debug_bounds("close-prompt").is_some()
}

fn discarded_count(env: &Env) -> usize {
    fs::read_dir(env.data.join(format!("Backups/{}/discarded", path_key(&env.root)))).map(|d| d.count()).unwrap_or(0)
}

/// Writes `text` to `path` as another program would, with a modification time after the editor's.
fn write_elsewhere(path: &Path, text: &str) {
    fs::write(path, text).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() + Duration::from_secs(20)).unwrap();
}

#[gpui_kit::test]
fn cmd_w_on_a_clean_tab_closes_it_without_asking(cx: &mut TestAppContext) {
    let env = env("close-clean", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    vcx.simulate_keystrokes("cmd-w");
    assert!(titles(&wb, vcx).is_empty());
    assert!(!bar_up(vcx));
}

#[gpui_kit::test]
fn cmd_w_at_once_after_typing_asks_and_save_writes_the_file_then_closes(cx: &mut TestAppContext) {
    let env = env("close-save", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "SAVE ");
    vcx.simulate_keystrokes("cmd-w"); // before any backup was written
    assert_eq!(titles(&wb, vcx), ["a.txt"], "still open");
    assert!(dirty_of(&wb, vcx, "a.txt"));
    assert!(bar_up(vcx), "the save bar asks");
    click(vcx, "close-prompt:save");
    assert!(titles(&wb, vcx).is_empty(), "saved, then closed");
    assert!(!bar_up(vcx));
    assert!(fs::read_to_string(env.root.join("a.txt")).unwrap().starts_with("SAVE line 1"), "the file on disk is updated");
    tick(vcx);
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "saved: no backup left");
}

#[gpui_kit::test]
fn dont_save_closes_the_tab_leaves_the_disk_alone_and_moves_the_backup_aside(cx: &mut TestAppContext) {
    let env = env("close-discard", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "DROP ");
    tick(vcx);
    tick(vcx);
    assert!(env.backups().exists(&env.root.join("a.txt")), "a backup was written");
    vcx.simulate_keystrokes("cmd-w");
    assert!(bar_up(vcx));
    click(vcx, "close-prompt:discard");
    assert!(titles(&wb, vcx).is_empty(), "closed");
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap().lines().next(), Some("line 1"), "disk untouched");
    tick(vcx);
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")), "no backup left in place");
    assert_eq!(discarded_count(&env), 1, "the text is kept aside, not deleted");
    let (wb, vcx) = start(cx, &env);
    assert!(titles(&wb, vcx).is_empty(), "and its tab does not come back on restart");
}

#[gpui_kit::test]
fn dont_save_before_any_backup_leaves_nothing_behind(cx: &mut TestAppContext) {
    let env = env("close-discard-early", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "EARLY ");
    vcx.simulate_keystrokes("cmd-w");
    click(vcx, "close-prompt:discard");
    assert!(titles(&wb, vcx).is_empty());
    tick(vcx);
    tick(vcx);
    assert!(!env.backups().exists(&env.root.join("a.txt")));
    assert_eq!(discarded_count(&env), 0);
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap().lines().next(), Some("line 1"));
}

#[gpui_kit::test]
fn cancel_keeps_the_dirty_tab_open_with_its_text(cx: &mut TestAppContext) {
    let env = env("close-cancel", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "KEEP ");
    vcx.simulate_keystrokes("cmd-w");
    assert!(bar_up(vcx));
    click(vcx, "close-prompt:cancel");
    assert!(!bar_up(vcx));
    assert_eq!(titles(&wb, vcx), ["a.txt"]);
    assert!(dirty_of(&wb, vcx, "a.txt"));
    assert!(value_of(&wb, vcx, "a.txt").starts_with("KEEP line 1"));
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap().lines().next(), Some("line 1"));
}

#[gpui_kit::test]
fn the_tab_close_button_and_a_middle_click_ask_too(cx: &mut TestAppContext) {
    let env = env("close-tab-button", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    open(&wb, vcx, &env.root.join("b.txt"));
    type_in_active(&wb, vcx, "B ");
    click(vcx, "tab-close:1");
    assert_eq!(titles(&wb, vcx), ["a.txt", "b.txt"], "the close button asks: b.txt stays");
    assert!(bar_up(vcx));
    click(vcx, "close-prompt:cancel");

    let b = vcx.debug_bounds("tab:1").expect("tab 1 is rendered").center();
    vcx.simulate_mouse_down(b, MouseButton::Middle, Default::default());
    vcx.simulate_mouse_up(b, MouseButton::Middle, Default::default());
    vcx.run_until_parked();
    assert_eq!(titles(&wb, vcx), ["a.txt", "b.txt"], "a middle click asks too");
    assert!(bar_up(vcx));
    click(vcx, "close-prompt:discard");
    assert_eq!(titles(&wb, vcx), ["a.txt"]);

    // A clean tab's close button closes at once.
    click(vcx, "tab-close:0");
    assert!(titles(&wb, vcx).is_empty());
    assert!(!bar_up(vcx));
}

#[gpui_kit::test]
fn a_dirty_tab_whose_file_changed_on_disk_stays_open_with_the_conflict_bar(cx: &mut TestAppContext) {
    let env = env("close-conflict", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "MINE ");
    write_elsewhere(&env.root.join("a.txt"), "theirs\n");
    vcx.simulate_keystrokes("cmd-w");
    assert!(bar_up(vcx));
    click(vcx, "close-prompt:save");
    assert_eq!(titles(&wb, vcx), ["a.txt"], "the save was refused: the tab stays");
    assert_eq!(wb.read_with(vcx, |wb, _| wb.editor_view.save_conflict.clone()), Some(env.root.join("a.txt")), "and the conflict bar shows");
    assert!(!bar_up(vcx));
    assert_eq!(fs::read_to_string(env.root.join("a.txt")).unwrap(), "theirs\n", "nothing overwritten");
    assert!(dirty_of(&wb, vcx, "a.txt"), "the text is still there");
}

#[gpui_kit::test]
fn auto_save_after_delay_saves_before_a_later_close_so_no_prompt_shows(cx: &mut TestAppContext) {
    let env = env("close-autosave", Some(r#"{ "files.autoSave": "afterDelay", "files.autoSaveDelay": 500 }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "AUTO ");
    vcx.simulate_keystrokes("cmd-w"); // within the delay: upstream asks
    assert!(bar_up(vcx));
    click(vcx, "close-prompt:cancel");
    vcx.executor().advance_clock(Duration::from_millis(600));
    vcx.run_until_parked();
    assert!(!dirty_of(&wb, vcx, "a.txt"), "auto save ran");
    vcx.simulate_keystrokes("cmd-w");
    assert!(titles(&wb, vcx).is_empty(), "saved: closes without a prompt");
    assert!(!bar_up(vcx));
    assert!(fs::read_to_string(env.root.join("a.txt")).unwrap().starts_with("AUTO line 1"));
}

#[gpui_kit::test]
fn after_cancel_the_keyboard_stays_in_the_editor(cx: &mut TestAppContext) {
    // A click on the bar's Cancel must not hand the keyboard to the workbench root.
    let env = env("close-refocus", None);
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "X ");
    vcx.simulate_keystrokes("cmd-w");
    click(vcx, "close-prompt:cancel");
    vcx.simulate_keystrokes("x");
    assert!(value_of(&wb, vcx, "a.txt").starts_with("X xline 1"), "typed keys reach the editor after Cancel");
    vcx.simulate_keystrokes("cmd-w");
    assert!(bar_up(vcx), "Cmd+W after Cancel asks again");
    click(vcx, "close-prompt:cancel");
}

#[gpui_kit::test]
fn on_focus_change_saves_a_dirty_tab_on_close_without_asking(cx: &mut TestAppContext) {
    let env = env("close-focus", Some(r#"{ "files.autoSave": "onFocusChange" }"#));
    let (wb, vcx) = start(cx, &env);
    open(&wb, vcx, &env.root.join("a.txt"));
    type_in_active(&wb, vcx, "FOCUS ");
    vcx.simulate_keystrokes("cmd-w");
    assert!(titles(&wb, vcx).is_empty(), "upstream saves on focus change instead of asking");
    assert!(!bar_up(vcx));
    assert!(fs::read_to_string(env.root.join("a.txt")).unwrap().starts_with("FOCUS line 1"));
}
