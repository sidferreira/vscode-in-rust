//! The `explorer.*` settings the tree reads (ADR-0041), parsed from the configuration and kept in
//! the Explorer state. They take effect live: `Workbench::apply_explorer_settings` re-reads them
//! whenever the configuration changes.
//!
//! upstream: src/vs/workbench/contrib/files/browser/files.contribution.ts @ 50e1350fc78d

use std::path::Path;

use vsr_settings::Configuration;

use crate::config::glob::ExcludeMatcher;

/// `explorer.autoRevealExclude`: the paths `autoReveal` skips (the default excludes `node_modules`
/// and `bower_components`).
pub fn reveal_exclude(c: &Configuration) -> ExcludeMatcher {
    c.get_value("explorer.autoRevealExclude").map(ExcludeMatcher::from_value).unwrap_or_default()
}

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

/// `explorer.sortOrderLexicographicOptions`: how names compare (see `sort.rs`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lexicographic {
    #[default]
    Default,
    Upper,
    Lower,
    Unicode,
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

/// `explorer.copyPathSeparator` and `explorer.copyRelativePathSeparator`: the character copied
/// paths use between folders. `Auto` is the platform's (`/` on macOS).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PathSeparator {
    #[default]
    Auto,
    Slash,
    Backslash,
}

impl PathSeparator {
    fn from_value(value: Option<&str>) -> Self {
        match value {
            Some("/") => Self::Slash,
            Some("\\") => Self::Backslash,
            _ => Self::Auto,
        }
    }

    pub fn char(self) -> char {
        match self {
            Self::Auto => std::path::MAIN_SEPARATOR,
            Self::Slash => '/',
            Self::Backslash => '\\',
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerSettings {
    pub sort_order: SortOrder,
    pub lexicographic: Lexicographic,
    /// `explorer.sortOrderReverse`.
    pub sort_reverse: bool,
    pub compact_folders: bool,
    pub auto_reveal: AutoReveal,
    pub confirm_delete: bool,
    /// `files.enableTrash`: Delete moves to the Trash rather than deleting permanently.
    pub enable_trash: bool,
    /// `workbench.list.openMode` `doubleClick`: a file opens on a double click only.
    pub open_on_double_click: bool,
    /// `workbench.tree.renderIndentGuides`: a line per ancestor level of each row.
    pub indent_guides: IndentGuides,
    /// `workbench.tree.expandMode` `doubleClick`: a folder expands on a double click only (its chevron
    /// always toggles it).
    pub expand_on_double_click: bool,
    /// `explorer.excludeGitIgnore`: entries that a `.gitignore` excludes are hidden.
    pub exclude_git_ignore: bool,
    pub copy_path_separator: PathSeparator,
    pub copy_relative_path_separator: PathSeparator,
    /// `explorer.fileNesting.enabled`: related files are shown under their parent (ADR-0052).
    pub file_nesting: bool,
    /// `explorer.fileNesting.expand`: the nests of a parent are shown until it is collapsed.
    pub file_nesting_expand: bool,
    /// `explorer.fileNesting.patterns`: parent pattern and its child patterns (see `nesting.rs`).
    pub file_nesting_patterns: Vec<(String, Vec<String>)>,
}

impl Default for ExplorerSettings {
    fn default() -> Self {
        Self {
            sort_order: SortOrder::Default,
            lexicographic: Lexicographic::Default,
            sort_reverse: false,
            compact_folders: true,
            auto_reveal: AutoReveal::On,
            confirm_delete: true,
            enable_trash: true,
            open_on_double_click: false,
            expand_on_double_click: false,
            indent_guides: IndentGuides::OnHover,
            exclude_git_ignore: false,
            copy_path_separator: PathSeparator::Auto,
            copy_relative_path_separator: PathSeparator::Auto,
            file_nesting: false,
            file_nesting_expand: true,
            file_nesting_patterns: super::nesting::default_rules(),
        }
    }
}

/// The text Copy Path (`relative: false`) or Copy Relative Path puts on the clipboard: the path
/// (or its path relative to the workspace `root`), with `separator` between the folders.
pub fn copied_path_text(path: &Path, root: &Path, relative: bool, separator: PathSeparator) -> String {
    let shown = if relative { path.strip_prefix(root).unwrap_or(path) } else { path };
    let text = shown.to_string_lossy();
    let sep = separator.char();
    if sep == std::path::MAIN_SEPARATOR { text.into_owned() } else { text.replace(std::path::MAIN_SEPARATOR, &sep.to_string()) }
}

/// `workbench.tree.renderIndentGuides`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndentGuides {
    None,
    OnHover,
    Always,
}

/// How many indent guides a row at `depth` draws (one per ancestor level; none when off).
pub fn indent_guide_count(mode: IndentGuides, depth: usize) -> usize {
    match mode {
        IndentGuides::None => 0,
        IndentGuides::OnHover | IndentGuides::Always => depth,
    }
}

/// Whether a click on a row activates it (opens a file, expands a folder): always on a single click,
/// and in a `doubleClick` mode (`workbench.list.openMode`, `workbench.tree.expandMode`) only from the
/// second click of a double click.
pub fn click_activates(double_click_mode: bool, click_count: usize) -> bool {
    !double_click_mode || click_count >= 2
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
        let lexicographic = match c.get_str("explorer.sortOrderLexicographicOptions") {
            Some("upper") => Lexicographic::Upper,
            Some("lower") => Lexicographic::Lower,
            Some("unicode") => Lexicographic::Unicode,
            Some(_) | None => Lexicographic::Default,
        };
        let auto_reveal = match c.get_value("explorer.autoReveal") {
            Some(serde_json::Value::Bool(false)) => AutoReveal::Off,
            Some(serde_json::Value::String(s)) if s == "focusNoScroll" => AutoReveal::FocusNoScroll,
            _ => d.auto_reveal,
        };
        Self {
            sort_order,
            lexicographic,
            sort_reverse: c.get_bool("explorer.sortOrderReverse").unwrap_or(d.sort_reverse),
            compact_folders: c.get_bool("explorer.compactFolders").unwrap_or(d.compact_folders),
            auto_reveal,
            confirm_delete: c.get_bool("explorer.confirmDelete").unwrap_or(d.confirm_delete),
            enable_trash: c.get_bool("files.enableTrash").unwrap_or(d.enable_trash),
            open_on_double_click: c.get_str("workbench.list.openMode") == Some("doubleClick"),
            expand_on_double_click: c.get_str("workbench.tree.expandMode") == Some("doubleClick"),
            indent_guides: match c.get_str("workbench.tree.renderIndentGuides") {
                Some("none") => IndentGuides::None,
                Some("always") => IndentGuides::Always,
                _ => IndentGuides::OnHover,
            },
            exclude_git_ignore: c.get_bool("explorer.excludeGitIgnore").unwrap_or(d.exclude_git_ignore),
            copy_path_separator: PathSeparator::from_value(c.get_str("explorer.copyPathSeparator")),
            copy_relative_path_separator: PathSeparator::from_value(c.get_str("explorer.copyRelativePathSeparator")),
            file_nesting: c.get_bool("explorer.fileNesting.enabled").unwrap_or(d.file_nesting),
            file_nesting_expand: c.get_bool("explorer.fileNesting.expand").unwrap_or(d.file_nesting_expand),
            file_nesting_patterns: super::nesting::rules_from_value(c.get_value("explorer.fileNesting.patterns")),
        }
    }
}

#[cfg(test)]
mod tests {
    use vsr_settings::{SettingsRegistry, UserSettings};

    use super::*;

    fn config(user: &str) -> Configuration {
        Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap())
    }

    #[test]
    fn defaults_and_values_are_read() {
        let d = ExplorerSettings::from_configuration(&config(""));
        assert_eq!(d, ExplorerSettings::default());
        assert!(!d.exclude_git_ignore);
        let s = ExplorerSettings::from_configuration(&config(
            r#"{ "explorer.excludeGitIgnore": true, "explorer.copyPathSeparator": "\\", "explorer.copyRelativePathSeparator": "/" }"#,
        ));
        assert!(s.exclude_git_ignore);
        assert_eq!((s.copy_path_separator, s.copy_relative_path_separator), (PathSeparator::Backslash, PathSeparator::Slash));
        let bad = ExplorerSettings::from_configuration(&config(r#"{ "explorer.copyPathSeparator": ":" }"#));
        assert_eq!(bad.copy_path_separator, PathSeparator::Auto);
    }

    #[test]
    fn open_mode_decides_which_clicks_open_a_file() {
        assert!(!ExplorerSettings::from_configuration(&config("")).open_on_double_click);
        assert!(ExplorerSettings::from_configuration(&config(r#"{ "workbench.list.openMode": "doubleClick" }"#)).open_on_double_click);
        assert!(click_activates(false, 1), "singleClick: a single click opens");
        assert!(!click_activates(true, 1), "doubleClick: a single click only selects");
        assert!(click_activates(true, 2), "doubleClick: the second click opens");
    }

    #[test]
    fn indent_guides_follow_the_setting_and_depth() {
        assert_eq!(ExplorerSettings::from_configuration(&config("")).indent_guides, IndentGuides::OnHover);
        let always = ExplorerSettings::from_configuration(&config(r#"{ "workbench.tree.renderIndentGuides": "always" }"#));
        assert_eq!(always.indent_guides, IndentGuides::Always);
        assert_eq!(indent_guide_count(IndentGuides::Always, 2), 2);
        assert_eq!(indent_guide_count(IndentGuides::OnHover, 0), 0, "a root row has no guide");
        assert_eq!(indent_guide_count(IndentGuides::None, 3), 0);
    }

    #[test]
    fn expand_mode_is_read_from_the_tree_setting() {
        assert!(!ExplorerSettings::from_configuration(&config("")).expand_on_double_click);
        assert!(ExplorerSettings::from_configuration(&config(r#"{ "workbench.tree.expandMode": "doubleClick" }"#)).expand_on_double_click);
    }

    #[test]
    fn copied_paths_follow_the_separator_setting() {
        let root = Path::new("/ws/project");
        let file = Path::new("/ws/project/src/main.rs");
        assert_eq!(copied_path_text(file, root, false, PathSeparator::Auto), "/ws/project/src/main.rs");
        assert_eq!(copied_path_text(file, root, true, PathSeparator::Auto), "src/main.rs");
        assert_eq!(copied_path_text(file, root, true, PathSeparator::Backslash), "src\\main.rs");
        assert_eq!(copied_path_text(file, root, false, PathSeparator::Slash), "/ws/project/src/main.rs");
        // A path outside the workspace is copied whole.
        assert_eq!(copied_path_text(Path::new("/elsewhere/a.txt"), root, true, PathSeparator::Auto), "/elsewhere/a.txt");
    }
}
