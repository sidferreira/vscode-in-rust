//! Delete in the Explorer (ADR-0041): the Delete key moves the selection to the Trash after an
//! inline confirmation bar (`explorer.confirmDelete`); Delete Permanently always asks. With
//! `files.enableTrash` off, Delete deletes permanently and asks the stronger question.
//!
//! upstream: src/vs/workbench/contrib/files/browser/fileActions.ts @ 50e1350fc78d (deleteFiles)

use std::path::Path;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::state::PendingDelete;
use crate::config::AppConfig;
use crate::ui::to_gpui;
use crate::Workbench;

/// `explorer.confirmDelete`, `files.enableTrash` in the configuration.
fn settings(cx: &App) -> (bool, bool) {
    let c = AppConfig::get(cx).configuration();
    (c.get_bool("explorer.confirmDelete").unwrap_or(true), c.get_bool("files.enableTrash").unwrap_or(true))
}

/// The question the bar asks about `path` (upstream `getConfirmMessage`, one element).
pub fn question(path: &Path, use_trash: bool) -> (String, String) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let folder = path.is_dir();
    let message = match (use_trash, folder) {
        (true, false) => format!("Are you sure you want to delete '{name}'?"),
        (true, true) => format!("Are you sure you want to delete '{name}' and its contents?"),
        (false, false) => format!("Are you sure you want to permanently delete '{name}'?"),
        (false, true) => format!("Are you sure you want to permanently delete '{name}' and its contents?"),
    };
    let detail = if use_trash { "You can restore this file from the Trash." } else { "This action is irreversible!" };
    (message, detail.to_string())
}

impl Workbench {
    /// Delete key (`moveFileToTrash`) and Cmd+Alt+Backspace (`deleteFile`, always permanent).
    pub(crate) fn request_delete(&mut self, permanent: bool, cx: &mut Context<Self>) {
        let Some(path) = self.explorer.selected.clone() else { return };
        let (confirm, enable_trash) = settings(cx);
        let pending = PendingDelete { path, use_trash: enable_trash && !permanent };
        if confirm {
            self.explorer.confirm = Some(pending);
            cx.notify();
        } else {
            self.run_delete(&pending, cx);
        }
    }

    /// The confirmation bar's button, or Enter while the bar is open.
    pub(crate) fn confirm_delete(&mut self, cx: &mut Context<Self>) {
        if let Some(pending) = self.explorer.confirm.take() {
            self.run_delete(&pending, cx);
        }
        cx.notify();
    }

    /// Escape or Cancel: the bar goes, nothing is deleted.
    pub(crate) fn cancel_delete(&mut self, cx: &mut Context<Self>) {
        if self.explorer.confirm.take().is_some() {
            cx.notify();
        }
    }

    fn run_delete(&mut self, pending: &PendingDelete, cx: &mut Context<Self>) {
        match self.delete_path(&pending.path, pending.use_trash, cx) {
            Ok(()) => {
                if self.explorer.selected.as_deref().is_some_and(|s| s.starts_with(&pending.path)) {
                    self.explorer.selected = None;
                }
            }
            Err(e) => self.error = Some(format!("Unable to delete '{}': {e}", pending.path.display())),
        }
        cx.notify();
    }

    /// The inline confirmation bar, shown above the tree while a delete waits.
    pub(crate) fn delete_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let pending = self.explorer.confirm.as_ref()?;
        let t = &self.theme;
        let (message, detail) = question(&pending.path, pending.use_trash);
        let button = |id: &'static str, label: &str, primary: bool| {
            div()
                .id(id)
                .px(px(8.))
                .h(px(22.))
                .flex()
                .items_center()
                .rounded(px(2.))
                .cursor_pointer()
                .text_size(px(12.))
                .when(primary, |d| d.bg(to_gpui(t.accent)).text_color(to_gpui(t.editor_bg)))
                .when(!primary, |d| d.border_1().border_color(to_gpui(t.border)))
                .child(label.to_string())
        };
        let confirm_label = if pending.use_trash { "Move to Trash" } else { "Delete Permanently" };
        Some(
            div()
                .id("explorer-delete-bar")
                .flex_none()
                .p(px(8.))
                .gap(px(6.))
                .flex()
                .flex_col()
                .text_size(px(12.))
                .bg(to_gpui(t.chrome_bg))
                .border_b_1()
                .border_color(to_gpui(t.border))
                .child(div().child(message))
                .child(div().text_color(to_gpui(t.description_fg)).child(detail))
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child(button("explorer-delete-confirm", confirm_label, true).on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| this.confirm_delete(cx),
                        )))
                        .child(button("explorer-delete-cancel", "Cancel", false).on_click(cx.listener(
                            |this, _: &ClickEvent, _, cx| this.cancel_delete(cx),
                        ))),
                )
                .into_any_element(),
        )
    }
}
