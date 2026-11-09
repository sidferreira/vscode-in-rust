//! The inline name input of the Explorer (ADR-0041): New File, New Folder and Rename type their
//! name in a box in the tree. Enter commits when the name is valid, Escape cancels, and losing
//! focus commits a valid name and cancels an invalid one (upstream `InlineEditInputBox`). The
//! validation message (`names::validate_file_name`) shows under the box.
//!
//! upstream: src/vs/workbench/browser/parts/views/inlineEditInputBox.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerViewer.ts @ 50e1350fc78d

use std::path::{Path, PathBuf};

use gpui_kit::component::input::{Enter as InputEnter, Escape as InputEscape, InputEvent, InputState};
use gpui_kit::*;

use super::actions::KEY_CONTEXT;
use super::names::{NameMessage, Severity, validate_file_name};
use crate::Workbench;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InlineKind {
    NewFile,
    NewFolder,
    Rename,
}

/// An inline name input in progress.
pub struct InlineEdit {
    pub kind: InlineKind,
    /// New File / New Folder: the folder the item goes into. Rename: the folder of the item.
    pub parent: PathBuf,
    /// Rename: the item being renamed.
    pub target: Option<PathBuf>,
    pub input: Entity<InputState>,
    /// The validation message under the box (an error keeps Enter from committing).
    pub message: Option<NameMessage>,
    _subs: Vec<Subscription>,
}

impl InlineEdit {
    pub fn is_new(&self) -> bool {
        self.kind != InlineKind::Rename
    }

    pub fn has_error(&self) -> bool {
        self.message.as_ref().is_some_and(|m| m.severity == Severity::Error)
    }

    pub fn value(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }
}

/// The byte length of the part of a file name that Rename selects: everything before the last dot
/// (`foo.tar.gz` selects `foo.tar`), all of it without a dot or for a folder (upstream
/// `renderInputBox`, which selects the name without its extension).
pub fn rename_selection(name: &str, is_dir: bool) -> usize {
    match name.rfind('.') {
        Some(dot) if dot > 0 && !is_dir => dot,
        _ => name.len(),
    }
}

impl Workbench {
    /// New File / New Folder / Rename: open the inline box. New items go into the selected folder
    /// (or the folder of the selected file, or the workspace root).
    pub(crate) fn start_inline(&mut self, kind: InlineKind, window: &mut Window, cx: &mut Context<Self>) {
        let selected = self.explorer.selected.clone();
        let (parent, target, initial, selection) = match kind {
            InlineKind::NewFile | InlineKind::NewFolder => {
                let parent = match selected {
                    Some(p) if p.is_dir() => p,
                    Some(p) => p.parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.clone()),
                    None => self.root.clone(),
                };
                self.expand_to(&parent);
                (parent, None, String::new(), 0)
            }
            InlineKind::Rename => {
                let Some(target) = selected else { return };
                if target == self.root {
                    return;
                }
                let Some(parent) = target.parent().map(Path::to_path_buf) else { return };
                self.expand_to(&parent);
                let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let selection = rename_selection(&name, target.is_dir());
                (parent, Some(target), name, selection)
            }
        };
        let placeholder = match kind {
            InlineKind::NewFolder => "Folder name",
            _ => "File name",
        };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let focus = input.read(cx).focus_handle(cx);
        let mut subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| {
            if matches!(ev, InputEvent::Change) {
                this.validate_inline(cx);
            }
        })];
        subs.push(cx.on_focus_out(&focus, window, |this, _ev, window, cx| this.blur_inline(window, cx)));
        input.update(cx, |s, cx| {
            if !initial.is_empty() {
                s.set_value(initial.clone(), window, cx);
                s.set_selected_range(0..selection, cx);
            }
            s.focus(window, cx);
        });
        self.explorer.inline = Some(InlineEdit { kind, parent, target, input, message: None, _subs: subs });
        self.validate_inline(cx);
        cx.notify();
    }

    /// The name in the box is checked on every change (upstream `inputValidation`).
    fn validate_inline(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.explorer.inline.as_ref() else { return };
        let name = edit.value(cx);
        let own = edit.target.as_ref().and_then(|t| t.file_name()).map(|n| n.to_string_lossy().into_owned());
        let parent = edit.parent.clone();
        let message = validate_file_name(&name, |n| own.as_deref() != Some(n) && parent.join(n).exists());
        if let Some(edit) = self.explorer.inline.as_mut() {
            edit.message = message;
        }
        cx.notify();
    }

    /// Enter: commit a valid name; an invalid one keeps the box open with its message.
    pub(crate) fn enter_inline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.explorer.inline.as_ref() {
            Some(edit) if !edit.has_error() => self.commit_inline(true, window, cx),
            Some(_) => cx.notify(),
            None => {}
        }
    }

    /// Escape: close the box, nothing is created or renamed.
    pub(crate) fn cancel_inline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.explorer.inline.take().is_some() {
            window.focus(&self.explorer.focus, cx);
            cx.notify();
        }
    }

    /// Losing focus: commit a valid name, cancel an invalid one (focus is already elsewhere, so the
    /// tree does not take it back).
    fn blur_inline(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.explorer.inline.as_ref() else { return };
        if edit.has_error() {
            self.explorer.inline = None;
            cx.notify();
        } else {
            self.commit_inline(false, window, cx);
        }
    }

    fn commit_inline(&mut self, refocus_tree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(edit) = self.explorer.inline.take() else { return };
        let name = edit.value(cx);
        let result = match (edit.kind, edit.target.clone()) {
            (InlineKind::NewFile, _) => self.create_entry(&edit.parent, &name, false, cx),
            (InlineKind::NewFolder, _) => self.create_entry(&edit.parent, &name, true, cx),
            (InlineKind::Rename, Some(target)) => self.rename_entry(&target, &name, cx),
            (InlineKind::Rename, None) => Err("nothing to rename".to_string()),
        };
        match result {
            Ok(path) => {
                self.explorer.selected = Some(path);
                if refocus_tree {
                    window.focus(&self.explorer.focus, cx);
                }
            }
            Err(text) => {
                // Keep the box open with the message the operation gave.
                let message = NameMessage { severity: Severity::Error, text };
                self.explorer.inline = Some(InlineEdit { message: Some(message), ..edit });
            }
        }
        cx.notify();
    }
}

/// The tree's key context: only while no name is being typed, so the explorer keys (Space,
/// arrows, Delete) reach the box as text.
pub fn tree_key_context(inline_open: bool) -> Option<&'static str> {
    if inline_open { None } else { Some(KEY_CONTEXT) }
}
