//! The Explorer's view state: the tree's focus, its selection and the scroll position of its rows
//! (ADR-0041). The workbench owns one; the tree (`view.rs`) and the commands (`actions.rs`) read it.

use std::path::PathBuf;

use gpui_kit::*;

use super::settings::ExplorerSettings;
use crate::config::AppConfig;

pub struct ExplorerState {
    /// Focus of the tree: the key bindings of `actions::KEY_CONTEXT` apply while it has focus.
    pub focus: FocusHandle,
    /// Scroll position of the tree rows (`autoReveal` scrolls the active file into view).
    pub scroll: ScrollHandle,
    /// The selected row (a file or a folder), if any. The commands act on it.
    pub selected: Option<PathBuf>,
    /// The delete waiting for the inline confirmation bar (`explorer.confirmDelete`).
    pub confirm: Option<PendingDelete>,
    /// The inline name input (New File, New Folder, Rename), while one is open.
    pub inline: Option<super::inline::InlineEdit>,
    /// The `explorer.*` settings in effect (re-read when the configuration changes).
    pub settings: ExplorerSettings,
    /// `explorer.autoRevealExclude`, compiled (re-read with the settings).
    pub reveal_exclude: crate::config::glob::ExcludeMatcher,
}

/// A delete the user has to confirm: the path, and whether it goes to the Trash.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PendingDelete {
    pub path: PathBuf,
    pub use_trash: bool,
}

impl ExplorerState {
    pub fn new(cx: &mut App) -> Self {
        let configuration = AppConfig::get(cx).configuration();
        let settings = ExplorerSettings::from_configuration(configuration);
        Self {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            selected: None,
            confirm: None,
            inline: None,
            settings,
            reveal_exclude: super::settings::reveal_exclude(configuration),
        }
    }
}
