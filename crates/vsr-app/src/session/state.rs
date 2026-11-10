//! The serialized session state (JSON), and the on-disk formats.
//!
//! # `User/workspaceStorage/<key>/state.json`
//!
//! ```json
//! {
//!   "version": 1,
//!   "folder": "/Users/me/project",
//!   "editors": [
//!     { "path": "/Users/me/project/a.rs", "preview": false,
//!       "view": { "top_line": 120, "selection": { "anchor": [4, 1], "active": [4, 9] }, "word_wrap": true } }
//!   ],
//!   "active": 0,                         // index into editors; absent when only the Welcome page was open
//!   "mru": ["/Users/me/project/a.rs"],   // activation order, most recent first
//!   "side_bar": { "visible": true, "width": 300.0, "view": "explorer" },
//!   "search": { "query": "foo", "include": "", "exclude": "", "match_case": false, "whole_word": false,
//!               "use_regex": false, "use_excludes": true, "details_open": false },
//!   "find_history": ["foo", "bar"],      // editor.find.history (newest first; absent with `never`)
//!   "replace_history": ["baz"]           // editor.find.replaceHistory
//! }
//! ```
//!
//! Positions are `[line, column]`, both 1-based as in the editor. `top_line` is the first visible view
//! line. Every field has a default, so a file from an older or newer build still loads (unknown fields
//! are ignored); `version` is for a future incompatible change.
//!
//! # `User/globalStorage/recents.json`
//!
//! `{ "version": 1, "files": ["/abs/path", ...], "folders": ["/abs/dir", ...] }`, most recent first.
//!
//! The hot exit backup format is in `backup.rs`.
//!
//! upstream: src/vs/workbench/common/editor/editorGroupModel.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/browser/parts/editor/editorPart.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const FORMAT_VERSION: u32 = 1;
/// How many recent files and folders are remembered.
pub const MAX_RECENT_FILES: usize = 200;
pub const MAX_RECENT_FOLDERS: usize = 50;

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkspaceState {
    pub version: u32,
    pub folder: String,
    pub editors: Vec<EditorState>,
    pub active: Option<usize>,
    pub mru: Vec<String>,
    pub side_bar: SideBarState,
    pub search: Option<SearchState>,
    /// The Find and Replace box histories, newest first (`editor.find.history`, `replaceHistory`).
    pub find_history: Vec<String>,
    pub replace_history: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditorState {
    pub path: String,
    /// The preview editor (shown in italics): replaced by the next preview open.
    pub preview: bool,
    pub view: ViewState,
}

/// What `workbench.editor.restoreViewState` brings back for one editor.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewState {
    /// The first visible view line (0-based).
    pub top_line: usize,
    pub selection: Option<SelectionState>,
    /// The Alt+Z override of `editor.wordWrap` for this editor.
    pub word_wrap: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionState {
    pub anchor: (usize, usize),
    pub active: (usize, usize),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SideBarState {
    pub visible: bool,
    pub width: f32,
    /// "explorer", "search", "scm" or "extensions".
    pub view: String,
}

impl Default for SideBarState {
    fn default() -> Self {
        Self { visible: true, width: 0.0, view: "explorer".into() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchState {
    pub query: String,
    pub include: String,
    pub exclude: String,
    pub match_case: bool,
    pub whole_word: bool,
    pub use_regex: bool,
    pub use_excludes: bool,
    pub details_open: bool,
}

impl Default for SearchState {
    fn default() -> Self {
        Self {
            query: String::new(),
            include: String::new(),
            exclude: String::new(),
            match_case: false,
            whole_word: false,
            use_regex: false,
            use_excludes: true,
            details_open: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GlobalState {
    pub version: u32,
    pub files: Vec<String>,
    pub folders: Vec<String>,
}

impl GlobalState {
    pub fn new(files: &[PathBuf], folders: &[PathBuf]) -> Self {
        let text = |p: &PathBuf| p.to_string_lossy().into_owned();
        Self {
            version: FORMAT_VERSION,
            files: files.iter().take(MAX_RECENT_FILES).map(text).collect(),
            folders: folders.iter().take(MAX_RECENT_FOLDERS).map(text).collect(),
        }
    }
}

/// Moves `path` to the front of `list` (no duplicates).
pub fn touch_recent(list: &mut Vec<PathBuf>, path: &Path, max: usize) {
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(max);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> WorkspaceState {
        WorkspaceState {
            version: FORMAT_VERSION,
            folder: "/w".into(),
            editors: vec![
                EditorState {
                    path: "/w/a.rs".into(),
                    preview: false,
                    view: ViewState {
                        top_line: 120,
                        selection: Some(SelectionState { anchor: (4, 1), active: (4, 9) }),
                        word_wrap: Some(true),
                    },
                },
                EditorState { path: "/w/b ü.txt".into(), preview: true, view: ViewState::default() },
            ],
            active: Some(1),
            mru: vec!["/w/b ü.txt".into(), "/w/a.rs".into()],
            side_bar: SideBarState { visible: false, width: 321.5, view: "search".into() },
            search: Some(SearchState { query: "fo\"o\n".into(), use_regex: true, ..SearchState::default() }),
            find_history: vec!["fo\"o".into(), "bar".into()],
            replace_history: vec!["baz".into()],
        }
    }

    #[test]
    fn workspace_state_round_trips() {
        let state = sample();
        let json = serde_json::to_string_pretty(&state).unwrap();
        assert_eq!(serde_json::from_str::<WorkspaceState>(&json).unwrap(), state);
    }

    #[test]
    fn unknown_and_missing_fields_are_tolerated() {
        let state: WorkspaceState =
            serde_json::from_str(r#"{ "version": 7, "future": [1, 2], "editors": [ { "path": "/w/a", "extra": true } ] }"#).unwrap();
        assert_eq!(state.version, 7);
        assert_eq!(state.editors.len(), 1);
        assert_eq!(state.editors[0].view, ViewState::default());
        assert!(state.side_bar.visible, "defaults apply");
        assert_eq!(state.side_bar.view, "explorer");
        assert_eq!(serde_json::from_str::<WorkspaceState>("{}").unwrap(), WorkspaceState::default());
    }

    #[test]
    fn global_state_round_trips_and_is_capped() {
        let files: Vec<PathBuf> = (0..MAX_RECENT_FILES + 5).map(|i| PathBuf::from(format!("/f/{i}"))).collect();
        let g = GlobalState::new(&files, &[PathBuf::from("/d")]);
        assert_eq!(g.files.len(), MAX_RECENT_FILES);
        let json = serde_json::to_string(&g).unwrap();
        assert_eq!(serde_json::from_str::<GlobalState>(&json).unwrap(), g);
    }

    #[test]
    fn touch_recent_moves_to_front_without_duplicates() {
        let mut list = vec![PathBuf::from("/a"), PathBuf::from("/b"), PathBuf::from("/c")];
        touch_recent(&mut list, Path::new("/c"), 3);
        touch_recent(&mut list, Path::new("/d"), 3);
        assert_eq!(list, [PathBuf::from("/d"), PathBuf::from("/c"), PathBuf::from("/a")]);
    }
}
