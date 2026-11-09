//! The `explorer.*` settings the tree reads (ADR-0041), parsed from the configuration and kept in
//! the Explorer state. They take effect live: `Workbench::apply_explorer_settings` re-reads them
//! whenever the configuration changes.
//!
//! upstream: src/vs/workbench/contrib/files/browser/files.contribution.ts @ 50e1350fc78d

use vsr_settings::Configuration;

/// `explorer.sortOrder`. `foldersNestsFiles` is `Default` here: nested files need file nesting
/// (`explorer.fileNesting.enabled`), which rscode does not have, so the two sort the same.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortOrder {
    #[default]
    Default,
    Mixed,
    FilesFirst,
    Type,
    Modified,
}

/// `explorer.autoReveal`: `true` selects and scrolls the active file into view, `focusNoScroll`
/// selects it without scrolling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AutoReveal {
    #[default]
    On,
    Off,
    FocusNoScroll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExplorerSettings {
    pub sort_order: SortOrder,
    pub compact_folders: bool,
    pub auto_reveal: AutoReveal,
    pub confirm_delete: bool,
    /// `files.enableTrash`: Delete moves to the Trash rather than deleting permanently.
    pub enable_trash: bool,
}

impl Default for ExplorerSettings {
    fn default() -> Self {
        Self {
            sort_order: SortOrder::Default,
            compact_folders: true,
            auto_reveal: AutoReveal::On,
            confirm_delete: true,
            enable_trash: true,
        }
    }
}

impl ExplorerSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let d = Self::default();
        let sort_order = match c.get_str("explorer.sortOrder") {
            Some("mixed") => SortOrder::Mixed,
            Some("filesFirst") => SortOrder::FilesFirst,
            Some("type") => SortOrder::Type,
            Some("modified") => SortOrder::Modified,
            Some(_) | None => SortOrder::Default,
        };
        let auto_reveal = match c.get_value("explorer.autoReveal") {
            Some(serde_json::Value::Bool(false)) => AutoReveal::Off,
            Some(serde_json::Value::String(s)) if s == "focusNoScroll" => AutoReveal::FocusNoScroll,
            _ => d.auto_reveal,
        };
        Self {
            sort_order,
            compact_folders: c.get_bool("explorer.compactFolders").unwrap_or(d.compact_folders),
            auto_reveal,
            confirm_delete: c.get_bool("explorer.confirmDelete").unwrap_or(d.confirm_delete),
            enable_trash: c.get_bool("files.enableTrash").unwrap_or(d.enable_trash),
        }
    }
}
