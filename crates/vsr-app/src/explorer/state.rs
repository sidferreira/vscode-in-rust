//! The Explorer's view state: the tree's focus, its selection and the scroll position of its rows
//! (ADR-0041). The workbench owns one; the tree (`view.rs`) and the commands (`actions.rs`) read it.

use std::path::PathBuf;

use gpui_kit::*;

pub struct ExplorerState {
    /// Focus of the tree: the key bindings of `actions::KEY_CONTEXT` apply while it has focus.
    pub focus: FocusHandle,
    /// Scroll position of the tree rows (`autoReveal` scrolls the active file into view).
    pub scroll: ScrollHandle,
    /// The selected row (a file or a folder), if any. The commands act on it.
    pub selected: Option<PathBuf>,
}

impl ExplorerState {
    pub fn new(cx: &mut App) -> Self {
        Self { focus: cx.focus_handle(), scroll: ScrollHandle::new(), selected: None }
    }
}
