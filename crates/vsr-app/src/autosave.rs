//! `files.autoSave` (ADR-0034): saves dirty editors when the setting asks for it.
//!
//! upstream: src/vs/workbench/browser/parts/editor/editorAutoSave.ts @ 50e1350fc78d
//!
//! The editor notifies the workbench on every edit, so the workbench re-renders after each one.
//! `render` calls [`Workbench::auto_save_tick`], which compares each dirty editor's content version
//! with the version its `afterDelay` timer was started for. A changed version starts a new timer,
//! and dropping the old `Task` cancels it, so the delay restarts on every edit as upstream's does.
//! The focus and window triggers run on the same render. Every save goes through the save path
//! (`EditorSettings::save_transforms`) with the auto save reason, so the caret lines are left alone.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use gpui_kit::{Context, Task, Window};

use crate::Workbench;
use crate::config::files::AutoSaveMode;

#[derive(Default)]
pub struct AutoSaveState {
    /// The `afterDelay` timer of each editor, with the content version it was started for.
    timers: HashMap<PathBuf, (u64, Task<()>)>,
    /// The active editor at the last render (the `onFocusChange` trigger).
    last_active: Option<PathBuf>,
    /// Whether the window was active at the last render (the window triggers). `None` before the
    /// first render, so starting inactive does not save anything.
    window_active: Option<bool>,
}

impl Workbench {
    /// Runs the `files.autoSave` triggers. Called from `render`.
    pub(crate) fn auto_save_tick(&mut self, window: &Window, cx: &mut Context<Self>) {
        let mode = self.editor_settings.auto_save;
        let active = self.editors.active_editor().map(|e| e.path.clone());
        let window_active = window.is_window_active();
        let mut saved = false;

        let was_active = self.auto_save.window_active.replace(window_active);
        if was_active == Some(true)
            && !window_active
            && matches!(
                mode,
                AutoSaveMode::OnFocusChange | AutoSaveMode::OnWindowChange
            )
        {
            for i in 0..self.editors.open.len() {
                saved |= self.auto_save_editor(i);
            }
        }

        if active != self.auto_save.last_active {
            let previous = std::mem::replace(&mut self.auto_save.last_active, active);
            if let (AutoSaveMode::OnFocusChange, Some(path)) = (mode, previous)
                && let Some(i) = self.editors.index_of(&path)
            {
                saved |= self.auto_save_editor(i);
            }
        }

        if mode == AutoSaveMode::AfterDelay {
            self.schedule_after_delay(cx);
        } else {
            self.auto_save.timers.clear();
        }
        if saved {
            cx.notify();
        }
    }

    /// Starts (or restarts) the timer of every dirty editor whose content changed since its timer
    /// started, and forgets the timers of editors that are clean or gone.
    fn schedule_after_delay(&mut self, cx: &mut Context<Self>) {
        let delay = Duration::from_millis(self.editor_settings.auto_save_delay_ms);
        let workspace_only = self.editor_settings.auto_save_workspace_only;
        let timers = &mut self.auto_save.timers;
        for e in &self.editors.open {
            // `autoSaveWorkspaceFilesOnly`: files outside the opened folder are not saved after a delay.
            let eligible = e.is_dirty()
                && !e.is_read_only()
                && !(workspace_only && !e.path.starts_with(&self.root));
            if !eligible {
                timers.remove(&e.path);
                continue;
            }
            let version = e.editor.version();
            if timers.get(&e.path).is_some_and(|(v, _)| *v == version) {
                continue;
            }
            let path = e.path.clone();
            let task = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(delay).await;
                let _ = this.update(cx, |wb, cx| wb.auto_save_due(&path, version, cx));
            });
            timers.insert(e.path.clone(), (version, task));
        }
        let open: HashSet<&Path> = self.editors.open.iter().map(|e| e.path.as_path()).collect();
        timers.retain(|p, _| open.contains(p.as_path()));
    }

    /// A timer fired. Saves the editor if it is still dirty at the version the timer was started
    /// for (an edit since then started a newer timer, which replaced this one).
    fn auto_save_due(&mut self, path: &Path, version: u64, cx: &mut Context<Self>) {
        if self.editor_settings.auto_save != AutoSaveMode::AfterDelay {
            return;
        }
        let Some(i) = self.editors.index_of(path) else {
            return;
        };
        if self.editors.open[i].editor.version() == version && self.auto_save_editor(i) {
            cx.notify();
        }
    }

    /// Saves the editor at `index` if it is dirty and writable. Returns whether it was saved.
    fn auto_save_editor(&mut self, index: usize) -> bool {
        let transforms = self.editor_settings.save_transforms(true);
        let Some(e) = self.editors.open.get_mut(index) else {
            return false;
        };
        if !e.is_dirty() || e.is_read_only() {
            return false;
        }
        match e.save_with(transforms) {
            Ok(()) => true,
            Err(err) => {
                eprintln!("rscode: auto save {}: {err}", e.path.display());
                false
            }
        }
    }
}
