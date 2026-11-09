//! Closing a tab with unsaved changes (ADR-0040): Cmd+W, the tab's close button, a middle click and
//! the Close Editor command all come here. A dirty tab is not closed until the user answers the
//! "Do you want to save the changes you made to <file>?" bar: Save writes the file and closes the
//! tab (a tab that cannot be saved stays open with the conflict or error), Don't Save closes it and
//! moves its backup aside, Cancel keeps it open.
//!
//! upstream: src/vs/workbench/browser/parts/editor/editorGroupView.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/services/dialogs/browser/abstractFileDialogService.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//!
//! Upstream asks for a dirty editor unless `files.autoSave` is `onFocusChange` (saved without a
//! question, because a dialog would steal the focus) or, on macOS, `onWindowChange`, which asks.
//! `afterDelay` asks while the delay has not passed yet, as upstream does.

use std::path::PathBuf;

use gpui_kit::{Context, Window};

use crate::Workbench;
use crate::config::files::AutoSaveMode;

impl Workbench {
    /// The user closes the tab of `path`. A clean tab closes now; a dirty one asks first.
    pub(crate) fn request_close_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(path) = self.editors.open.get(index).map(|e| e.path.clone()) {
            self.request_close(vec![path], cx);
        }
    }

    /// The user closes the tabs of `paths` (in tab order). The tabs close at once when none is dirty;
    /// otherwise the save bar asks about the dirty ones, and the answer closes them (`close_prompt_*`).
    pub(crate) fn request_close(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let mode = self.editor_settings.auto_save;
        let mut asks = false;
        for path in &paths {
            let Some(i) = self.editors.index_of(path) else { continue };
            if self.editors.open[i].is_dirty() {
                // `onFocusChange`: saved without a question. A save that fails (or a changed file) asks.
                let saved = mode == AutoSaveMode::OnFocusChange && self.auto_save_editor(i);
                asks |= !saved;
            }
        }
        if !asks {
            self.close_tabs(&paths);
            cx.notify();
            return;
        }
        // Show the file being asked about, as upstream opens the editor before its dialog.
        if let Some(i) = paths.iter().filter_map(|p| self.editors.index_of(p)).find(|&i| self.editors.open[i].is_dirty()) {
            self.editors.activate(i);
        }
        self.settings_active = false;
        self.welcome_active = false;
        self.session.close_prompt = Some(paths);
        cx.notify();
    }

    /// The bar's Save (Save All for several): saves each dirty tab and closes it. A tab whose save
    /// is refused (its file changed on disk, ADR-0039) or fails stays open, and is shown.
    pub(crate) fn close_prompt_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(paths) = self.session.close_prompt.take() else { return };
        let transforms = self.editor_settings.save_transforms(false);
        let ask_on_conflict = self.editor_settings.save_conflict_ask;
        let mut kept: Option<PathBuf> = None;
        let mut closable = Vec::new();
        for path in &paths {
            let Some(i) = self.editors.index_of(path) else { continue };
            let e = &mut self.editors.open[i];
            if !e.is_dirty() {
                closable.push(path.clone());
                continue;
            }
            if e.disk_changed() && ask_on_conflict {
                self.editor_view.save_conflict = Some(path.clone());
                kept.get_or_insert_with(|| path.clone());
            } else if let Err(err) = e.save_with(transforms) {
                self.error = Some(format!("{}: {err}", path.display()));
                kept.get_or_insert_with(|| path.clone());
            } else {
                closable.push(path.clone());
            }
        }
        self.close_tabs(&closable);
        if let Some(path) = kept
            && let Some(i) = self.editors.index_of(&path)
        {
            self.editors.activate(i);
        }
        self.answered_close_prompt(window, cx);
    }

    /// The bar's Don't Save: the unsaved text is moved aside (`discarded/`, never deleted) and the
    /// tabs close.
    pub(crate) fn close_prompt_dont_save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(paths) = self.session.close_prompt.take() else { return };
        if let Some(backups) = self.session.backups.clone() {
            for path in &paths {
                if self.editors.index_of(path).is_some_and(|i| self.editors.open[i].is_dirty()) {
                    backups.discard(path);
                }
            }
        }
        self.close_tabs(&paths);
        self.answered_close_prompt(window, cx);
    }

    /// The bar's Cancel: the tabs stay open, dirty as they were.
    pub(crate) fn close_prompt_cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.session.close_prompt = None;
        self.answered_close_prompt(window, cx);
    }

    /// Whatever the answer, the editor takes the keyboard back (the bar's button had it), so Cmd+W
    /// and the other editor keys work again.
    fn answered_close_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor_view.focus(window, cx);
        cx.notify();
    }

    /// Whether one of `paths` is an open dirty tab.
    pub(crate) fn any_dirty_in(&self, paths: &[PathBuf]) -> bool {
        paths.iter().any(|p| self.editors.index_of(p).is_some_and(|i| self.editors.open[i].is_dirty()))
    }

    /// Closes the open tabs of `paths` (the ones that are still open), with their session bookkeeping.
    fn close_tabs(&mut self, paths: &[PathBuf]) {
        for path in paths {
            if let Some(i) = self.editors.index_of(path) {
                self.close_tab(i);
            }
        }
    }
}
