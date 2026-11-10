//! Untitled text editors in the workbench (ADR-0058): File: New Untitled Text File (Cmd+N), Save As
//! of an untitled text through the Explorer's name box, and the session's handling of its text.
//! The rules (names, dirty state, titles, language) are `vsr_editor::untitled` (ADR-0057).
//!
//! A new text is an editor whose key is `untitled:Untitled-N` (`model::untitled_key`), so the open
//! list, the MRU list and the preview keep working with paths. Its text is kept by hot exit as a
//! backup under that key (`session::backup`), and comes back as an untitled text.

use std::path::{Path, PathBuf};

use gpui_kit::{Context, Window};
use vsr_editor::untitled::new_language;

use crate::Workbench;
use crate::config::AppConfig;
use crate::model::{OpenOptions, untitled_name_of};

impl Workbench {
    /// File: New Untitled Text File (Cmd+N): an empty text, pinned and focused. Its language is
    /// `files.defaultLanguage` (`${activeEditorLanguage}` is the active editor's language), else plain text.
    pub(crate) fn new_untitled(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_untitled_text(cx);
        self.editor_view.focus(window, cx);
    }

    /// An empty untitled text, shown and not focused: Cmd+N, and `workbench.startupEditor: newUntitledFile`.
    pub(crate) fn open_untitled_text(&mut self, cx: &mut Context<Self>) {
        let active = self.editors.active_editor().map(|e| e.language_id().to_string());
        let default = AppConfig::get(cx).configuration().get_str("files.defaultLanguage").map(str::to_string);
        let language = new_language(None, default.as_deref(), active.as_deref());
        self.settings_active = false;
        self.welcome_active = false;
        self.editors.open_untitled("", &language, OpenOptions::PINNED);
        cx.notify();
    }

    /// Save (Cmd+S, or Save from the palette) on the untitled text at `index`: the Explorer's name
    /// box asks for the file, in the workspace folder. Nothing else happens until the name is
    /// committed (Enter). A plain Save keeps the tab open once the file is written, and drops any Save
    /// As queue in flight.
    pub(crate) fn ask_save_untitled(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.abandon_save_as();
        self.open_save_as_box(index, window, cx);
    }

    /// Save As for `turns` (untitled texts with the flag "close the tab once written", in tab order):
    /// each name box opens in turn (ADR-0074). `then_quit`: the app quits once every name is given.
    pub(crate) fn start_save_as_queue(
        &mut self,
        turns: Vec<(PathBuf, bool)>,
        then_quit: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.session.save_as_queue = turns.into();
        self.session.save_as_quit = then_quit;
        self.next_save_as(window, cx);
    }

    /// Opens the name box of the next queued untitled text that is still open. With the queue empty,
    /// a quit that waited for the names goes on.
    pub(crate) fn next_save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        while let Some((key, _)) = self.session.save_as_queue.front().cloned() {
            if let Some(i) = self.editors.index_of(&key) {
                self.settings_active = false;
                self.welcome_active = false;
                self.editors.activate(i);
                self.open_save_as_box(i, window, cx);
                return;
            }
            self.session.save_as_queue.pop_front();
        }
        if std::mem::take(&mut self.session.save_as_quit) {
            self.finish_quit(cx);
        }
        cx.notify();
    }

    /// The name box of Save As is cancelled: the queue ends, the rest of it stays open and dirty, and
    /// a quit that waited for the names is aborted (ADR-0074).
    pub(crate) fn abandon_save_as(&mut self) {
        self.session.save_as_queue.clear();
        self.session.save_as_quit = false;
    }

    /// The name box of Save As for the untitled text at `index`, with its suggested name.
    fn open_save_as_box(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(e) = self.editors.open.get(index).filter(|e| e.is_untitled()) else { return };
        let key = e.path.clone();
        let name = self.editors.suggested_save_name(e);
        let parent = self.save_as_folder();
        self.show_explorer(cx);
        self.start_save_untitled_inline(key, &name, parent, window, cx);
    }

    /// The folder a Save As starts in (upstream `defaultFilePath`: the folder of the last active
    /// file): the folder of the most recent file editor when it is inside the workspace, else the
    /// workspace root (ADR-0074).
    fn save_as_folder(&self) -> PathBuf {
        let folder = self.editors.mru.iter().find(|p| untitled_name_of(p).is_none()).and_then(|p| p.parent());
        match folder {
            Some(dir) if dir.starts_with(&self.root) => dir.to_path_buf(),
            _ => self.root.clone(),
        }
    }

    /// Save As through the name box: creates `parent/name` with the text of the untitled text `key`.
    pub(crate) fn save_untitled_in(&mut self, key: &Path, parent: &Path, name: &str, cx: &mut Context<Self>) -> Result<PathBuf, String> {
        let target = parent.join(name);
        if target.exists() {
            return Err(format!("{name} already exists"));
        }
        self.save_untitled_to(key, &target, cx)?;
        self.expanded.insert(parent.to_path_buf());
        self.refresh_dir(parent);
        Ok(target)
    }

    /// Save the untitled text `key` to `target` (Save As; the `files.*` save transforms apply). The
    /// editor becomes the file's tab, and its hot exit backup goes: the text is in the file now.
    pub(crate) fn save_untitled_to(&mut self, key: &Path, target: &Path, cx: &mut Context<Self>) -> Result<(), String> {
        let Some(index) = self.editors.index_of(key) else {
            return Err("the untitled text is closed".to_string());
        };
        let transforms = self.editor_settings.save_transforms(false);
        self.editors.save_untitled_as(index, target, transforms).map_err(|e| format!("{}: {e}", target.display()))?;
        self.editor_view.forget(&key.to_path_buf());
        if let Some(backups) = self.session.backups.clone() {
            self.session.backed_up.remove(key);
            self.session.seen.remove(key);
            backups.remove_saved(key);
        }
        // The head of the queue is done: its tab closes when the close prompt asked for that.
        if self.session.save_as_queue.front().is_some_and(|(k, _)| k == key)
            && let Some((_, close_tab)) = self.session.save_as_queue.pop_front()
            && close_tab
            && let Some(i) = self.editors.index_of(target)
        {
            self.close_tab(i);
        }
        cx.notify();
        Ok(())
    }
}
