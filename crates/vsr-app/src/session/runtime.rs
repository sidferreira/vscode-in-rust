//! The workbench side of session restore and hot exit: snapshot and restore of the state in
//! `state.rs`, the backups in `backup.rs`, the debounced writer, and the quit path.
//!
//! # When things are written
//!
//! A timer ([`TICK`]) looks at the workbench every half second. A change must be seen on two ticks in
//! a row before it is written (a debounce, like the settings file watcher): the workspace state when
//! its JSON changed, and a dirty editor's backup when its text version changed ([`BACKUP_DELAY`] ticks
//! of quiet, upstream waits one second too). Everything pending is written at once, synchronously, by
//! [`Workbench::persist_on_quit`], which the quit hooks call and tests call directly.
//!
//! # Quit
//!
//! - Cmd+Q / `workbench.action.quit` is an exit; the red window button is a window close (rscode has one
//!   window, so both end the process, ADR-0019).
//! - `files.hotExit: onExit` (default): an exit keeps unsaved text in backups and quits without asking;
//!   a window close asks first (upstream asks on macOS when the app stays alive). `onExitAndWindowClose`
//!   never asks. `off` always asks. Asking is a bar above the status bar: Save All, Don't Save, Cancel.
//! - Don't Save moves the backups to `discarded/`; Save All saves, which removes them. Nothing is deleted
//!   before the user chose one of these (or saved by hand).
//!
//! # Restore
//!
//! At startup [`Workbench::session_init`] reads the workspace state of the folder (tabs in order, active,
//! preview, view state, side bar, search) unless `window.restoreWindows` is `none` or
//! `workbench.editor.restoreEditors` is off, then restores every backup of the folder as a dirty buffer
//! whatever those settings say: unsaved text is never skipped. A backup whose text equals the file on
//! disk is dropped (nothing to restore). The Welcome page shows only when no editor came back.
//!
//! upstream: src/vs/workbench/services/workingCopy/common/workingCopyBackupTracker.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/services/workingCopy/electron-browser/workingCopyBackupTracker.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::*;

use super::backup::{Backup, BackupStore};
use super::settings::SessionSettings;
use super::state::{
    EditorState, FORMAT_VERSION, GlobalState, MAX_RECENT_FOLDERS, SearchState, SideBarState, ViewState, WorkspaceState, touch_recent,
};
use super::store::{Loaded, SessionStorage, SessionStore, load_json, write_atomic};
use crate::config::AppConfig;
use crate::config::settings::{FindSettings, StartupEditor, startup_editor};
use crate::model::{OpenEditor, untitled_name_of};
use crate::theme::Theme;
use crate::ui::to_gpui;
use crate::{ActivityView, Workbench};

/// How often the workbench is looked at for changes to save.
pub const TICK: Duration = Duration::from_millis(500);

/// Decides when a changing text is stable enough to write: it must be seen twice in a row.
#[derive(Debug, Default)]
pub struct Debounced {
    last_written: Option<String>,
    pending: Option<String>,
}

impl Debounced {
    /// Offers the current text. Returns true when it should be written now (and records that it is).
    pub fn offer(&mut self, text: &str, flush: bool) -> bool {
        if self.last_written.as_deref() == Some(text) {
            self.pending = None;
            return false;
        }
        if flush || self.pending.as_deref() == Some(text) {
            self.last_written = Some(text.to_string());
            self.pending = None;
            return true;
        }
        self.pending = Some(text.to_string());
        false
    }

    /// A write failed: offer the text again next time.
    pub fn failed(&mut self) {
        self.last_written = None;
    }
}

/// The live workbenches, for the Quit action (which belongs to the app, not to one window).
#[derive(Default)]
pub struct OpenWorkbenches(pub Vec<WeakEntity<Workbench>>);

impl Global for OpenWorkbenches {}

/// Which action the save bar answers for: the quit (Cmd+Q, window close) or the close of a dirty tab.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum PromptKind {
    Quit,
    Close,
}

/// Cmd+Q: every workbench gets to keep or ask about its unsaved text; the app quits when none asks.
pub fn quit_app(cx: &mut App) {
    let all = cx.try_global::<OpenWorkbenches>().map(|o| o.0.clone()).unwrap_or_default();
    let mut proceed = true;
    for w in all {
        if let Ok(ok) = w.update(cx, |wb, cx| wb.request_quit(false, cx)) {
            proceed &= ok;
        }
    }
    if proceed {
        cx.quit();
    }
}

#[derive(Default)]
pub struct Runtime {
    pub store: Option<SessionStore>,
    pub backups: Option<BackupStore>,
    pub workspace_file: Option<PathBuf>,
    state_writer: Debounced,
    recents_writer: Debounced,
    /// The text version each file was last backed up at (and so has a backup file for).
    pub backed_up: HashMap<PathBuf, u64>,
    /// The version seen on the previous tick (a backup is written when seen twice).
    pub seen: HashMap<PathBuf, u64>,
    /// `workbench.editor.restoreViewState`, for reopening closed tabs (set at startup and on changes).
    pub restore_view_state: bool,
    pub recent_folders: Vec<PathBuf>,
    /// Search state read from disk, applied when the Search view exists.
    pub pending_search: Option<SearchState>,
    /// The "save the changes?" bar of the quit is showing.
    pub prompt: bool,
    /// The tabs the user is closing while the "save the changes?" bar of a close is showing
    /// (ADR-0040), in tab order. Set by `request_close` (close.rs).
    pub close_prompt: Option<Vec<PathBuf>>,
    /// Untitled texts waiting for their Save As, in order: each name box takes the first (ADR-0074).
    /// The flag: the tab closes once its file is written (the close prompt, ADR-0058).
    pub save_as_queue: VecDeque<(PathBuf, bool)>,
    /// A quit waits for the names: it goes on when the queue is empty (ADR-0074).
    pub save_as_quit: bool,
    /// The user chose at the prompt: the next quit goes through.
    pub confirmed: bool,
    /// Don't Save: no backups are written from now on.
    pub dropping: bool,
    /// A quit was accepted (tests read this instead of the process exiting).
    pub quit_requested: bool,
    tick: Option<Task<()>>,
    quit_hook: Option<Subscription>,
}

impl Workbench {
    fn session_settings(&self, cx: &App) -> SessionSettings {
        SessionSettings::from_configuration(AppConfig::get(cx).configuration())
    }

    /// Starts the session: reads the stored state and restores it, hooks the quit paths and starts
    /// the saving timer. Returns whether any editor was restored. Does nothing without an installed
    /// [`SessionStorage`] (tests that do not install one never touch the disk).
    pub(crate) fn session_init(&mut self, cx: &mut Context<Self>) -> bool {
        if !cx.has_global::<OpenWorkbenches>() {
            cx.set_global(OpenWorkbenches::default());
        }
        self.session_config_changed(cx);
        let me = cx.weak_entity();
        cx.update_global::<OpenWorkbenches, _>(|o, _| o.0.push(me));
        self.session.quit_hook = Some(cx.on_app_quit(|this, cx| {
            this.persist_on_quit(cx);
            async {}
        }));
        let Some(store) = cx.try_global::<SessionStorage>().and_then(|s| s.0.clone()) else { return false };
        let settings = self.session_settings(cx);
        let mut notices: Vec<String> = Vec::new();

        // Global state: the recent files seed the editor history (Quick Open, Welcome page).
        match load_json::<GlobalState>(&store.global_state_path()) {
            Loaded::Value(g) => {
                let mut files: Vec<PathBuf> = g.files.iter().map(PathBuf::from).collect();
                files.retain(|p| p.is_file());
                self.editors.history = files;
                self.session.recent_folders = g.folders.iter().map(PathBuf::from).collect();
            }
            Loaded::Corrupt { kept, error } => notices.push(format!("Recent files could not be read ({error}); kept at {}", shown(kept))),
            Loaded::Missing => {}
        }
        let root = self.root.clone();
        touch_recent(&mut self.session.recent_folders, &root, MAX_RECENT_FOLDERS);

        let backups = BackupStore::new(store.backups_dir(&root));
        let workspace_file = store.workspace_state_path(&root);
        self.session.store = Some(store);
        self.session.backups = Some(backups.clone());
        self.session.workspace_file = Some(workspace_file.clone());

        // Backups are read first: an untitled text's tab is placed from its backup (ADR-0074).
        let loaded = backups.load_all();
        notices.extend(loaded.problems.iter().cloned());
        let untitled_backups: HashMap<PathBuf, Backup> = loaded
            .backups
            .iter()
            .filter(|b| untitled_name_of(Path::new(&b.path)).is_some())
            .map(|b| (PathBuf::from(&b.path), b.clone()))
            .collect();

        // Workspace state.
        let mut restored_views: Vec<(PathBuf, ViewState)> = Vec::new();
        match load_json::<WorkspaceState>(&workspace_file) {
            Loaded::Value(state) => {
                if settings.restore_windows != super::settings::RestoreWindows::None {
                    self.restore_layout(&state);
                }
                if settings.restores_editors() {
                    restored_views = self.restore_tabs(&state, &untitled_backups);
                }
            }
            Loaded::Corrupt { kept, error } => notices.push(format!("The saved session could not be read ({error}); kept at {}", shown(kept))),
            Loaded::Missing => {}
        }

        // Backups: unsaved text always comes back.
        for backup in &loaded.backups {
            self.restore_backup(backup, &backups);
        }

        if settings.restore_view_state {
            for (path, view) in &restored_views {
                if let Some(i) = self.editors.index_of(path) {
                    self.editor_view.apply_view_state(&mut self.editors.open[i], view);
                }
            }
        }
        if self.editors.active.is_none() && !self.editors.open.is_empty() {
            let first = self.editors.mru.first().and_then(|p| self.editors.index_of(p)).unwrap_or(0);
            self.editors.activate(first);
        }
        // The limit applies to the whole restore, once the order and the active tab are known (ADR-0074).
        self.editors.enforce_limit();
        if !notices.is_empty() {
            self.error = Some(notices.join(" | "));
        }

        let task = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                if this.update(cx, |wb, cx| wb.session_tick(cx)).is_err() {
                    break;
                }
            }
        });
        self.session.tick = Some(task);
        !self.editors.open.is_empty()
    }

    fn restore_layout(&mut self, state: &WorkspaceState) {
        self.sidebar_visible = state.side_bar.visible;
        if state.side_bar.width > 0.0 {
            self.sidebar_width = state.side_bar.width.clamp(170., 600.);
        }
        self.view = match state.side_bar.view.as_str() {
            "search" => ActivityView::Search,
            "scm" => ActivityView::Scm,
            "extensions" => ActivityView::Extensions,
            _ => ActivityView::Explorer,
        };
        self.session.pending_search = state.search.clone();
        self.editor_view.restore_find_histories(state.find_history.clone(), state.replace_history.clone());
    }

    /// An editor for the file at `path` (read from disk), `None` when it cannot be read.
    fn build_editor(&self, path: &Path) -> Option<OpenEditor> {
        let bytes = fs::read(path).ok()?;
        let text = String::from_utf8_lossy(&bytes);
        let mut editor = OpenEditor::from_text(path, &text, self.editors.options, self.editors.default_eol, self.editors.read_only.is_read_only(path));
        self.editors.init_language(&mut editor, &text);
        Some(editor)
    }

    /// Opens the saved tabs in their order (files that are gone are skipped; an untitled text comes back
    /// from its backup, at its saved place, and one without a backup was clean and is not brought back).
    /// Returns their view states.
    fn restore_tabs(&mut self, state: &WorkspaceState, untitled: &HashMap<PathBuf, Backup>) -> Vec<(PathBuf, ViewState)> {
        let mut views = Vec::new();
        for saved in &state.editors {
            let path = PathBuf::from(&saved.path);
            if self.editors.index_of(&path).is_some() {
                continue;
            }
            let editor = match untitled_name_of(&path) {
                Some(name) => match untitled.get(&path) {
                    Some(backup) => self.editors.untitled_editor(name, &backup.text, backup.language.as_deref()),
                    None => continue,
                },
                None => match self.build_editor(&path) {
                    Some(editor) => editor,
                    None => continue,
                },
            };
            self.editors.open.push(editor);
            if saved.preview && self.editors.preview.is_none() {
                self.editors.preview = Some(path.clone());
            }
            views.push((path, saved.view.clone()));
        }
        // Activation order: the saved list for the paths that are open, then any other.
        let open: Vec<PathBuf> = self.editors.open.iter().map(|e| e.path.clone()).collect();
        let mut mru: Vec<PathBuf> = state.mru.iter().map(PathBuf::from).filter(|p| open.contains(p)).collect();
        for p in &open {
            if !mru.contains(p) {
                mru.push(p.clone());
            }
        }
        self.editors.mru = mru;
        let active_path = state.active.and_then(|i| state.editors.get(i)).map(|e| PathBuf::from(&e.path));
        if let Some(i) = active_path.and_then(|p| self.editors.index_of(&p)) {
            self.editors.active = Some(i);
        }
        views
    }

    /// Brings one backup back as a dirty buffer (opening a tab for it when the saved tabs have none).
    fn restore_backup(&mut self, backup: &super::backup::Backup, store: &BackupStore) {
        let path = PathBuf::from(&backup.path);
        if let Some(name) = untitled_name_of(&path).map(str::to_string) {
            // An untitled text has no file: its text comes back as the untitled text it was (ADR-0058).
            if self.editors.index_of(&path).is_none() {
                self.editors.restore_untitled(&name, &backup.text, backup.language.as_deref());
            }
            if let Some(i) = self.editors.index_of(&path) {
                self.session.backed_up.insert(path, self.editors.open[i].editor.version());
            }
            return;
        }
        let on_disk = fs::read(&path).ok().map(|b| String::from_utf8_lossy(&b).into_owned());
        if on_disk.as_deref() == Some(backup.text.as_str()) {
            // The file already has this text: nothing is lost by dropping the backup.
            store.remove_saved(&path);
            return;
        }
        let opened_here = self.editors.index_of(&path).is_none();
        if opened_here {
            let editor = match self.build_editor(&path) {
                Some(e) => e,
                // The file is gone from disk: the text comes back in a buffer that saves it again.
                None => {
                    let mut e = OpenEditor::from_text(&path, "", self.editors.options, self.editors.default_eol, false);
                    self.editors.init_language(&mut e, "");
                    e
                }
            };
            self.editors.open.push(editor);
            self.editors.mru.push(path.clone());
        }
        let Some(i) = self.editors.index_of(&path) else { return };
        let editor = &mut self.editors.open[i].editor;
        let full = editor.buffer().full_range();
        editor.replace_ranges(&[(full, backup.text.clone())]);
        self.session.backed_up.insert(path, self.editors.open[i].editor.version());
    }

    // ---- saving ------------------------------------------------------------------------------

    /// The state of this workbench as it would be saved now.
    pub(crate) fn session_snapshot(&self, cx: &App) -> WorkspaceState {
        // An untitled text is listed by its key: its text is in its backup, and its place is kept (ADR-0074).
        let editors = self
            .editors
            .open
            .iter()
            .enumerate()
            .map(|(i, e)| EditorState {
                path: e.path.to_string_lossy().into_owned(),
                preview: self.editors.is_preview(i),
                view: self.editor_view.view_state(e),
            })
            .collect();
        let find = FindSettings::from_configuration(AppConfig::get(cx).configuration());
        let (find_history, replace_history) = self.editor_view.find_histories(&find);
        WorkspaceState {
            version: FORMAT_VERSION,
            folder: self.root.to_string_lossy().into_owned(),
            find_history,
            replace_history,
            editors,
            active: self.editors.active,
            mru: self.editors.mru.iter().map(|p| p.to_string_lossy().into_owned()).collect(),
            side_bar: SideBarState {
                visible: self.sidebar_visible,
                width: self.sidebar_width,
                view: match self.view {
                    ActivityView::Explorer => "explorer",
                    ActivityView::Search => "search",
                    ActivityView::Scm => "scm",
                    ActivityView::Extensions => "extensions",
                }
                .into(),
            },
            search: self.search.as_ref().map(|s| s.read(cx).session_state(cx)).or_else(|| self.session.pending_search.clone()),
        }
    }

    /// The timer: backups of dirty editors, the workspace state and the recent files, each written
    /// once its change has been stable for a tick.
    fn session_tick(&mut self, cx: &mut Context<Self>) {
        // A close waiting for an answer whose tabs are no longer dirty (auto save saved them) asks nothing more.
        if self.session.close_prompt.as_ref().is_some_and(|paths| !self.any_dirty_in(paths)) {
            self.session.close_prompt = None;
            cx.notify();
        }
        self.session_sync(false, cx);
    }

    /// Writes what changed. `flush`: write everything now (quit).
    pub(crate) fn session_sync(&mut self, flush: bool, cx: &App) {
        if self.session.store.is_none() {
            return;
        }
        self.sync_backups(flush);
        self.sync_state(flush, cx);
    }

    fn sync_backups(&mut self, flush: bool) {
        let Some(backups) = self.session.backups.clone() else { return };
        let mut open: HashSet<PathBuf> = HashSet::new();
        let mut failure: Option<String> = None;
        for e in &self.editors.open {
            open.insert(e.path.clone());
            if e.is_dirty() {
                if self.session.dropping {
                    continue;
                }
                let version = e.editor.version();
                if self.session.backed_up.get(&e.path) == Some(&version) && backups.exists(&e.path) {
                    continue;
                }
                if flush || self.session.seen.get(&e.path) == Some(&version) {
                    match backups.write(&e.path, &e.buffer().value(), Some(e.language_id())) {
                        Ok(()) => {
                            self.session.backed_up.insert(e.path.clone(), version);
                        }
                        Err(err) => failure = Some(format!("Could not back up {}: {err}", e.path.display())),
                    }
                } else {
                    self.session.seen.insert(e.path.clone(), version);
                }
            } else {
                self.session.seen.remove(&e.path);
                // Saved, or undone back to the saved text: the backup holds nothing more.
                if self.session.backed_up.remove(&e.path).is_some() {
                    backups.remove_saved(&e.path);
                }
            }
        }
        // A tab closed with unsaved changes: its text stays on disk, aside.
        let gone: Vec<PathBuf> = self.session.backed_up.keys().filter(|p| !open.contains(*p)).cloned().collect();
        for path in gone {
            self.session.backed_up.remove(&path);
            self.session.seen.remove(&path);
            backups.discard(&path);
        }
        if let Some(message) = failure {
            self.error = Some(message);
        }
    }

    fn sync_state(&mut self, flush: bool, cx: &App) {
        let snapshot = self.session_snapshot(cx);
        if let (Some(path), Ok(json)) = (self.session.workspace_file.clone(), serde_json::to_string_pretty(&snapshot))
            && self.session.state_writer.offer(&json, flush)
            && let Err(e) = write_atomic(&path, json.as_bytes())
        {
            self.session.state_writer.failed();
            self.error = Some(format!("Could not save the session: {e}"));
        }
        if let Some(store) = self.session.store.clone() {
            let global = GlobalState::new(&self.editors.history, &self.session.recent_folders);
            if let Ok(json) = serde_json::to_string_pretty(&global)
                && self.session.recents_writer.offer(&json, flush)
                && let Err(e) = write_atomic(&store.global_state_path(), json.as_bytes())
            {
                self.session.recents_writer.failed();
                self.error = Some(format!("Could not save the recent files: {e}"));
            }
        }
    }

    // ---- closed tabs ---------------------------------------------------------------------------------

    /// The configuration changed: read the session settings the workbench keeps.
    pub(crate) fn session_config_changed(&mut self, cx: &App) {
        self.session.restore_view_state = self.session_settings(cx).restore_view_state;
    }

    /// A file was just opened in a new tab: put back the view state it had when it was closed.
    pub(crate) fn apply_remembered_view_state(&mut self, path: &Path) {
        let Some(view) = self.editor_view.take_remembered(path) else { return };
        if !self.session.restore_view_state {
            return;
        }
        if let Some(i) = self.editors.index_of(path) {
            self.editor_view.apply_view_state(&mut self.editors.open[i], &view);
        }
    }

    /// A tab was closed (by the tab strip or Cmd+W). A tab with unsaved changes was dropped by the user:
    /// its backup is moved aside, now, not at the next tick (a reopen of the file in between must not
    /// find it still counted as the open buffer's backup). A saved tab's backup is no longer needed.
    pub(crate) fn session_editor_closed(&mut self, path: &Path, dirty: bool) {
        let Some(backups) = self.session.backups.clone() else { return };
        self.session.seen.remove(path);
        if self.session.backed_up.remove(path).is_some() {
            if dirty {
                backups.discard(path);
            } else {
                backups.remove_saved(path);
            }
        }
    }

    // ---- quit -------------------------------------------------------------------------------------

    /// Whether quitting is allowed now. `window_close`: the red window button (an exit otherwise).
    /// Returns false when the "save the changes?" bar was raised instead (the caller must not quit).
    pub(crate) fn request_quit(&mut self, window_close: bool, cx: &mut Context<Self>) -> bool {
        if !self.session.confirmed {
            let dirty = self.editors.open.iter().any(OpenEditor::is_dirty);
            if dirty && self.session_settings(cx).asks_before_closing(window_close) {
                self.session.prompt = true;
                cx.notify();
                return false;
            }
        }
        self.persist_on_quit(cx);
        true
    }

    /// The shutdown hook: writes the state, the recent files and a backup of every dirty editor,
    /// synchronously. Safe to call more than once. Tests call it instead of quitting.
    pub(crate) fn persist_on_quit(&mut self, cx: &App) {
        self.session_sync(true, cx);
        self.ext_shutdown(); // the extension host exits with the app (ADR-0066)
    }

    pub(crate) fn quit_prompt_cancel(&mut self, cx: &mut Context<Self>) {
        self.session.prompt = false;
        self.session.confirmed = false;
        cx.notify();
    }

    /// Save All, then quit (ADR-0040). Each dirty file is saved as Cmd+S saves it (ADR-0039): a file
    /// changed on disk is not overwritten while `files.saveConflictResolution` is `askUser`, and a failed
    /// save is an error. The other files are still saved. When any file is refused the app stays open:
    /// the first refused editor is activated and shows its conflict bar (or the error). Each dirty
    /// untitled text then gets its name box in turn; the quit goes on once every name is given, and
    /// Cancel on a name box aborts the quit (upstream asks for each name through the save dialog, ADR-0074).
    pub(crate) fn quit_prompt_save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let transforms = self.editor_settings.save_transforms(false);
        let ask_on_conflict = self.editor_settings.save_conflict_ask;
        let mut shown: Option<usize> = None;
        let mut untitled: Vec<(PathBuf, bool)> = Vec::new();
        for i in 0..self.editors.open.len() {
            let e = &mut self.editors.open[i];
            if !e.is_dirty() {
                continue;
            }
            if e.is_untitled() {
                untitled.push((e.path.clone(), false));
            } else if e.disk_changed() && ask_on_conflict {
                if shown.is_none() {
                    self.editor_view.save_conflict = Some(e.path.clone());
                }
                shown.get_or_insert(i);
            } else if let Err(err) = e.save_with(transforms) {
                self.error = Some(format!("{}: {err}", e.path.display()));
                shown.get_or_insert(i);
            } else if self.editor_view.save_conflict.as_deref() == Some(e.path.as_path()) {
                self.editor_view.save_conflict = None;
            }
        }
        if shown.is_none() && !untitled.is_empty() {
            self.session.prompt = false;
            self.session.confirmed = false;
            self.settings_active = false;
            self.welcome_active = false;
            self.start_save_as_queue(untitled, true, window, cx);
            return;
        }
        let Some(i) = shown else {
            self.finish_quit(cx);
            return;
        };
        // The app stays open: the prompt is gone and the refused editor is on screen with its conflict bar.
        self.session.prompt = false;
        self.session.confirmed = false;
        self.editors.activate(i);
        self.settings_active = false;
        self.welcome_active = false;
        self.editor_view.focus(window, cx);
        cx.notify();
    }

    /// Don't Save, then quit: the unsaved text is moved aside (`discarded/`), not deleted.
    pub(crate) fn quit_prompt_dont_save(&mut self, cx: &mut Context<Self>) {
        self.session.dropping = true;
        if let Some(backups) = self.session.backups.clone() {
            for e in self.editors.open.iter().filter(|e| e.is_dirty()) {
                // Make sure the text is on disk first, then move it aside.
                if !backups.exists(&e.path) && backups.write(&e.path, &e.buffer().value(), Some(e.language_id())).is_err() {
                    self.error = Some(format!("Could not keep the changes of {}", e.path.display()));
                    self.session.dropping = false;
                    cx.notify();
                    return;
                }
                backups.discard(&e.path);
                self.session.backed_up.remove(&e.path);
            }
        }
        self.finish_quit(cx);
    }

    pub(crate) fn finish_quit(&mut self, cx: &mut Context<Self>) {
        self.session.prompt = false;
        self.session.confirmed = true;
        self.session.quit_requested = true;
        self.persist_on_quit(cx);
        #[cfg(not(test))]
        cx.quit();
        cx.notify();
    }

    /// The bar above the status bar while the quit is waiting for an answer.
    pub(crate) fn quit_prompt_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.session.prompt {
            return None;
        }
        let dirty = self.editors.open.iter().filter(|e| e.is_dirty()).map(|e| self.editors.title_of(e)).collect();
        Some(self.save_prompt_bar(PromptKind::Quit, dirty, cx))
    }

    /// The bar above the status bar while a close of a dirty tab waits for an answer (close.rs).
    pub(crate) fn close_prompt_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let paths = self.session.close_prompt.as_ref()?;
        let dirty: Vec<String> = paths
            .iter()
            .filter_map(|p| self.editors.index_of(p).map(|i| &self.editors.open[i]))
            .filter(|e| e.is_dirty())
            .map(|e| self.editors.title_of(e))
            .collect();
        if dirty.is_empty() {
            return None;
        }
        Some(self.save_prompt_bar(PromptKind::Close, dirty, cx))
    }

    /// The "Do you want to save the changes?" bar (upstream `showSaveConfirm`), shared by the quit and
    /// the close of a dirty tab. `dirty` are the titles of the files that would be lost.
    fn save_prompt_bar(&self, kind: PromptKind, dirty: Vec<String>, cx: &mut Context<Self>) -> AnyElement {
        let t: &Theme = &self.theme;
        let message = save_prompt_message(&dirty);
        let (bar, save, discard, cancel) = match kind {
            PromptKind::Quit => ("quit-prompt", "quit-prompt:save", "quit-prompt:discard", "quit-prompt:cancel"),
            PromptKind::Close => ("close-prompt", "close-prompt:save", "close-prompt:discard", "close-prompt:cancel"),
        };
        let button = |id: &'static str, label: &'static str, primary: bool| {
            div()
                .id(id)
                .debug_selector(|| id.to_string())
                .flex_none()
                .px(px(12.))
                .py(px(3.))
                .rounded(px(2.))
                .cursor_pointer()
                .text_size(px(12.))
                .border_1()
                .border_color(to_gpui(t.button_border))
                .bg(to_gpui(if primary { t.button_bg } else { t.button_secondary_bg }))
                .text_color(to_gpui(if primary { t.button_fg } else { t.button_secondary_fg }))
                .hover(|s| s.bg(to_gpui(if primary { t.button_hover_bg } else { t.button_secondary_hover_bg })))
                .child(label)
        };
        div()
            .id(bar)
            .debug_selector(|| bar.to_string())
            // A click on the bar must not move the keyboard to the workbench root (its focus handle
            // is the ancestor that takes focus on a click): the editor keeps it, as gpui does for the
            // editor's own clicks.
            .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
            .flex_none()
            .w_full()
            .flex()
            .items_center()
            .gap(px(10.))
            .px(px(12.))
            .py(px(6.))
            .bg(to_gpui(t.menu_bg))
            .border_t_1()
            .border_color(to_gpui(t.panel_border))
            .text_size(px(13.))
            .text_color(to_gpui(t.foreground))
            .child(div().flex_1().child(message))
            .child(div().text_color(to_gpui(t.description_fg)).child("Your changes will be lost if you don't save them."))
            .child(
                button(save, if dirty.len() > 1 { "Save All" } else { "Save" }, true).on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, window, cx| match kind {
                        PromptKind::Quit => this.quit_prompt_save_all(window, cx),
                        PromptKind::Close => this.close_prompt_save(window, cx),
                    }),
                ),
            )
            .child(button(discard, "Don't Save", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| match kind {
                    PromptKind::Quit => this.quit_prompt_dont_save(cx),
                    PromptKind::Close => this.close_prompt_dont_save(window, cx),
                }),
            ))
            .child(button(cancel, "Cancel", false).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| match kind {
                    PromptKind::Quit => this.quit_prompt_cancel(cx),
                    PromptKind::Close => this.close_prompt_cancel(window, cx),
                }),
            ))
            .into_any_element()
    }

    // ---- startup and render hooks ---------------------------------------------------------------

    /// `workbench.startupEditor`, for a start with no editor restored (upstream: only then).
    pub(crate) fn open_startup_editor(&mut self, cx: &mut Context<Self>) {
        if !self.editors.open.is_empty() {
            return;
        }
        match startup_editor(AppConfig::get(cx).configuration()) {
            StartupEditor::Readme => {
                for name in ["README.md", "README"] {
                    let p = self.root.join(name);
                    if p.is_file() {
                        self.open(&p);
                        break;
                    }
                }
            }
            StartupEditor::WelcomePage => self.open_welcome(cx),
            StartupEditor::NewUntitledFile => self.open_untitled_text(cx),
            _ => {}
        }
    }

    /// Called on every render: creates the Search view when it is the restored view and gives it
    /// the restored query and options.
    pub(crate) fn session_render_hook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.pending_search.is_none() {
            return;
        }
        if self.view == ActivityView::Search && self.search.is_none() {
            self.ensure_search(window, cx);
        }
        if let (Some(view), Some(state)) = (self.search.clone(), self.session.pending_search.take()) {
            view.update(cx, |v, cx| v.apply_session_state(&state, window, cx));
        }
    }
}

/// The question of the save bar (upstream `showSaveConfirm`, with the names of the files).
pub(crate) fn save_prompt_message(dirty: &[String]) -> String {
    match dirty {
        [one] => format!("Do you want to save the changes you made to {one}?"),
        many => format!("Do you want to save the changes you made to {} files?", many.len()),
    }
}

fn shown(path: Option<PathBuf>) -> String {
    path.map_or_else(|| "(could not be moved)".to_string(), |p| p.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::Debounced;

    #[test]
    fn debounced_writes_a_change_when_seen_twice() {
        let mut d = Debounced::default();
        assert!(!d.offer("a", false), "first sighting waits");
        assert!(d.offer("a", false), "second sighting writes");
        assert!(!d.offer("a", false), "unchanged is not written again");
        assert!(!d.offer("b", false));
        assert!(!d.offer("c", false), "a text that keeps changing is not written");
        assert!(d.offer("c", false));
        assert!(d.offer("d", true), "a flush writes at once");
        assert!(!d.offer("d", true));
    }

    #[test]
    fn save_prompt_names_the_file_or_counts_the_files() {
        use super::save_prompt_message;
        assert_eq!(save_prompt_message(&["a.txt".to_string()]), "Do you want to save the changes you made to a.txt?");
        assert_eq!(save_prompt_message(&["a.txt".to_string(), "b.txt".to_string()]), "Do you want to save the changes you made to 2 files?");
    }

    #[test]
    fn debounced_retries_after_a_failed_write() {
        let mut d = Debounced::default();
        assert!(d.offer("a", true));
        d.failed();
        assert!(!d.offer("a", false));
        assert!(d.offer("a", false));
    }
}
