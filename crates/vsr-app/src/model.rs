//! UI-independent workbench state: workspace tree listing and open editors.
//!
//! Kept free of egui so it can move into `vsr-workbench` and be reused by a
//! different front end (e.g. a webview-hosted workbench). ADR-0001/0006.
//!
//! The editor tabs follow upstream's `EditorGroupModel` (preview, MRU, placement, close focus)
//! for the one group rscode has (ADR-0033).
//!
//! upstream: src/vs/workbench/test/browser/parts/editor/editorGroupModel.test.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use vsr_editor::{Editor, EditorOptions};
use vsr_text::{DefaultEol, Eol, Position, TextBuffer};

use crate::config::glob::ExcludeMatcher;
use crate::config::files::ReadonlyRules;
use crate::config::settings::{EditorGroupSettings, LabelFormat, OpenPositioning};

/// Editor options before any setting is read (the macOS VS Code defaults: `editor.tabSize` 4,
/// `editor.insertSpaces` true). The workbench sets `Editors::options` from the configuration.
pub const EDITOR_OPTIONS: EditorOptions = EditorOptions { tab_size: 4, insert_spaces: true };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

/// List a directory like the VS Code explorer's default sort: folders first, then files, each
/// case-insensitively by name. Entries matching `files.exclude` (`exclude`, matched against the
/// path relative to `root`) are hidden.
pub fn list_dir(dir: &Path, root: &Path, exclude: &ExcludeMatcher) -> io::Result<Vec<DirEntry>> {
    let all: Vec<(String, PathBuf, bool)> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let is_dir = e.file_type().ok()?.is_dir();
            Some((name, e.path(), is_dir))
        })
        .collect();
    let siblings: HashSet<&str> = all.iter().map(|(n, _, _)| n.as_str()).collect();
    let has_sibling = |name: &str| siblings.contains(name);
    let mut entries: Vec<DirEntry> = all
        .iter()
        .filter(|(_, path, _)| {
            let rel = path.strip_prefix(root).unwrap_or(path).to_string_lossy().into_owned();
            exclude.is_empty() || !exclude.is_excluded(&rel, &has_sibling)
        })
        .map(|(name, path, is_dir)| DirEntry { path: path.clone(), name: name.clone(), is_dir: *is_dir })
        .collect();
    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    Ok(entries)
}

pub struct OpenEditor {
    pub path: PathBuf,
    pub title: String,
    pub editor: Editor,
    /// Read-only by `files.readonlyInclude` / `readonlyExclude` / `readonlyFromPermissions`.
    config_read_only: bool,
    /// Set for this session by the toggle commands; wins over the settings (upstream
    /// `sessionReadonlyOverrides`).
    session_read_only: Option<bool>,
}

impl OpenEditor {
    /// An editor for `text`. `default_eol` is the line break of a text with no line break yet
    /// (`files.eol`); a text that has line breaks keeps its own.
    pub fn from_text(path: &Path, text: &str, options: EditorOptions, default_eol: DefaultEol, config_read_only: bool) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path: path.to_path_buf(),
            title,
            editor: Editor::new(TextBuffer::new(text, default_eol, true), options),
            config_read_only,
            session_read_only: None,
        }
    }

    /// Whether edits are refused: the session override, else the settings.
    pub fn is_read_only(&self) -> bool {
        self.session_read_only.unwrap_or(self.config_read_only)
    }

    pub fn buffer(&self) -> &TextBuffer {
        self.editor.buffer()
    }

    pub fn eol_label(&self) -> &'static str {
        match self.buffer().eol() {
            Eol::Lf => "LF",
            Eol::CrLf => "CRLF",
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.editor.is_dirty()
    }

    /// Save without the `files.*` save transforms.
    #[cfg(test)]
    pub fn save(&mut self) -> io::Result<()> {
        self.save_with(SaveTransforms::default())
    }

    /// Apply the save transforms (as undoable edits, in upstream's participant order: trim
    /// trailing whitespace, insert the final newline, trim the final newlines), write the buffer
    /// to disk (`value()` already uses the file's EOL) and mark it clean. An auto save leaves the
    /// lines the caret is on alone, as upstream does.
    pub fn save_with(&mut self, transforms: SaveTransforms) -> io::Result<()> {
        let cursors: Vec<Position> = if transforms.auto_save { vec![self.editor.selection().active] } else { Vec::new() };
        if transforms.trim_trailing_whitespace {
            self.editor.trim_trailing_whitespace(&cursors);
        }
        if transforms.insert_final_newline {
            self.editor.insert_final_newline();
        }
        if transforms.trim_final_newlines {
            self.editor.trim_final_newlines(&cursors);
        }
        fs::write(&self.path, self.editor.buffer().value())?;
        self.editor.mark_saved();
        Ok(())
    }
}

/// The `files.*` save participants (`files.trimTrailingWhitespace`, `files.insertFinalNewline`,
/// `files.trimFinalNewlines`), and whether the save is an auto save (ADR-0034).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SaveTransforms {
    pub trim_trailing_whitespace: bool,
    pub insert_final_newline: bool,
    pub trim_final_newlines: bool,
    pub auto_save: bool,
}

/// How an editor open places its editor (upstream `IEditorOpenOptions`, the parts rscode uses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenOptions {
    /// A pinned editor stays open. A preview is replaced by the next preview open.
    pub pinned: bool,
    pub active: bool,
}

impl OpenOptions {
    /// `{ pinned: true, active: true }`: an ordinary open.
    pub const PINNED: Self = Self { pinned: true, active: true };
    /// `{ pinned: false, active: true }`: a preview, focused.
    pub const PREVIEW: Self = Self { pinned: false, active: true };
}

/// A tab's label: the file name, and a description when the format asks for one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TabLabel {
    pub title: String,
    pub description: Option<String>,
}

pub struct Editors {
    pub open: Vec<OpenEditor>,
    pub active: Option<usize>,
    /// Options for editors opened from now on (`editor.tabSize`, `editor.insertSpaces`).
    pub options: EditorOptions,
    /// Every file opened so far, most recent first (the editor history of Quick Open).
    pub history: Vec<PathBuf>,
    /// The open editors in activation order, most recent first (upstream `EditorGroupModel.mru`).
    /// Holds exactly the open paths.
    pub mru: Vec<PathBuf>,
    /// The preview editor, if any (upstream `preview`): at most one, and a pinned editor is
    /// never it. Editing or a double click pins it.
    pub preview: Option<PathBuf>,
    /// The `workbench.editor.*` settings that place, pin, close and label editors.
    pub settings: EditorGroupSettings,
    /// `files.eol`: the line break of a new text with none.
    pub default_eol: DefaultEol,
    /// `files.readonly*`: which files open read-only.
    pub read_only: ReadonlyRules,
}

impl Default for Editors {
    fn default() -> Self {
        Self {
            open: Vec::new(),
            active: None,
            options: EDITOR_OPTIONS,
            history: Vec::new(),
            mru: Vec::new(),
            preview: None,
            settings: EditorGroupSettings::default(),
            default_eol: DefaultEol::Lf,
            read_only: ReadonlyRules::default(),
        }
    }
}

impl Editors {
    /// Open `path` as a pinned, active editor, or focus it if it is already open. The new tab is
    /// placed by `workbench.editor.openPositioning` (right of the active one by default).
    #[cfg(test)]
    pub fn open_file(&mut self, path: &Path) -> io::Result<()> {
        self.open_with(path, OpenOptions::PINNED)
    }

    /// Open `path` (upstream `EditorGroupModel.openEditor`). A pinned open of a preview makes it
    /// pinned; a preview open of a file that is already open leaves it as it is. A new preview
    /// replaces the current preview in its place. Opening a file limits the open editors when
    /// `workbench.editor.limit.*` asks for it.
    pub fn open_with(&mut self, path: &Path, options: OpenOptions) -> io::Result<()> {
        self.pin_dirty_preview();
        if let Some(i) = self.index_of(path) {
            if options.pinned {
                self.pin(path);
            }
            if options.active {
                self.activate(i);
            }
            self.touch_history(path);
            return Ok(());
        }
        let bytes = fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let editor = OpenEditor::from_text(path, &text, self.options, self.default_eol, self.read_only.is_read_only(path));

        let target = self.target_index();
        let preview_index = self.preview.as_deref().and_then(|p| self.index_of(p));
        let make_active =
            options.active || self.active.is_none() || (!options.pinned && preview_index.is_some() && preview_index == self.active);
        let mut at = target;
        if !options.pinned && let Some(pi) = preview_index {
            // The preview is removed first, so a target after it moves left by one.
            if at > pi {
                at -= 1;
            }
            self.remove_at(pi);
        }
        at = at.min(self.open.len());
        if self.active.is_some_and(|a| at <= a) {
            self.active = self.active.map(|a| a + 1);
        }
        let new_path = editor.path.clone();
        self.open.insert(at, editor);
        if make_active {
            self.active = Some(at);
            self.mru.insert(0, new_path.clone());
        } else {
            // A new editor that is not focused goes second in the MRU list.
            self.mru.insert(self.mru.len().min(1), new_path.clone());
        }
        if !options.pinned {
            self.preview = Some(new_path);
        }
        self.enforce_limit();
        self.touch_history(path);
        Ok(())
    }

    /// Where a new editor goes (`workbench.editor.openPositioning`, upstream `openEditor`).
    fn target_index(&self) -> usize {
        match self.settings.open_positioning {
            OpenPositioning::Left => match self.active {
                Some(a) if a > 0 => a,
                _ => 0,
            },
            OpenPositioning::Right => self.active.map_or(0, |a| a + 1),
            OpenPositioning::First => 0,
            OpenPositioning::Last => self.open.len(),
        }
    }

    pub fn index_of(&self, path: &Path) -> Option<usize> {
        self.open.iter().position(|e| e.path == path)
    }

    /// Whether the tab at `index` is the preview editor (shown in italics).
    pub fn is_preview(&self, index: usize) -> bool {
        self.open.get(index).is_some_and(|e| self.preview.as_deref() == Some(e.path.as_path()))
    }

    /// Make the editor at `index` the active one and move it to the front of the MRU list.
    pub fn activate(&mut self, index: usize) {
        let Some(path) = self.open.get(index).map(|e| e.path.clone()) else { return };
        self.active = Some(index);
        self.touch_mru(&path);
    }

    fn touch_mru(&mut self, path: &Path) {
        self.mru.retain(|p| p != path);
        self.mru.insert(0, path.to_path_buf());
    }

    /// Pin the preview editor if `path` is it (a double click on the tab or in the Explorer).
    pub fn pin(&mut self, path: &Path) {
        if self.preview.as_deref() == Some(path) {
            self.preview = None;
        }
    }

    /// A preview that became dirty stays open (upstream pins a dirty editor). Idempotent, so the
    /// workbench calls it on every render.
    pub fn pin_dirty_preview(&mut self) {
        let dirty = self.preview.as_deref().and_then(|p| self.index_of(p)).is_some_and(|i| self.open[i].is_dirty());
        if dirty {
            self.preview = None;
        }
    }

    /// `workbench.editor.enablePreview` turned off: the current preview stays open.
    pub fn apply_settings(&mut self, settings: EditorGroupSettings) {
        if !settings.enable_preview {
            self.preview = None;
        }
        self.settings = settings;
        self.enforce_limit();
    }

    /// Remove the editor at `index` without moving focus (the caller decides). The active index
    /// follows the removal; the active editor itself becomes `None`.
    fn remove_at(&mut self, index: usize) -> OpenEditor {
        let editor = self.open.remove(index);
        self.mru.retain(|p| *p != editor.path);
        if self.preview.as_deref() == Some(editor.path.as_path()) {
            self.preview = None;
        }
        self.active = match self.active {
            Some(a) if a == index => None,
            Some(a) if a > index => Some(a - 1),
            other => other,
        };
        editor
    }

    /// Close the editors whose files are gone (`workbench.editor.closeOnFileDelete`). Editors with
    /// unsaved changes stay open. Returns the paths that were closed.
    pub fn close_missing(&mut self, exists: impl Fn(&Path) -> bool) -> Vec<PathBuf> {
        let missing: Vec<usize> = (0..self.open.len())
            .filter(|&i| !self.open[i].is_dirty() && !exists(&self.open[i].path))
            .collect();
        let mut closed = Vec::new();
        for &i in missing.iter().rev() {
            closed.push(self.open[i].path.clone());
            self.close(i);
        }
        closed
    }

    /// Activate the tab `delta` places away from the active one (scrolling over the tab strip with
    /// `workbench.editor.scrollToSwitchTabs`). Clamped at both ends, no wrap.
    pub fn step_active(&mut self, delta: isize) {
        let Some(a) = self.active else { return };
        let last = self.open.len() as isize - 1;
        let next = (a as isize + delta).clamp(0, last.max(0)) as usize;
        if next != a {
            self.activate(next);
        }
    }

    /// Applies `files.readonly*` to the open editors (a session override is kept).
    pub fn apply_read_only(&mut self, rules: ReadonlyRules) {
        for e in &mut self.open {
            e.config_read_only = rules.is_read_only(&e.path);
        }
        self.read_only = rules;
    }

    /// Sets (`Some`) or clears (`None`) the session override of the editor at `index`
    /// (`workbench.action.files.toggleActiveEditorReadonlyInSession` and its siblings).
    pub fn set_session_read_only(&mut self, index: usize, value: Option<bool>) {
        if let Some(e) = self.open.get_mut(index) {
            e.session_read_only = value;
        }
    }

    /// `workbench.editor.limit.*`: close the least recently used editors (never the active one,
    /// never a dirty one) until no more than `limit.value` editors are open. With
    /// `limit.excludeDirty`, dirty editors do not count towards the limit.
    pub fn enforce_limit(&mut self) {
        let s = self.settings;
        if !s.limit_enabled {
            return;
        }
        let counted = self.open.iter().filter(|e| !(s.limit_exclude_dirty && e.is_dirty())).count();
        let mut excess = counted.saturating_sub(s.limit_value);
        while excess > 0 {
            let lru: Vec<PathBuf> = self.mru.iter().rev().cloned().collect();
            let victim = lru
                .iter()
                .filter_map(|p| self.index_of(p))
                .find(|&i| Some(i) != self.active && !self.open[i].is_dirty());
            let Some(i) = victim else { break };
            self.remove_at(i);
            excess -= 1;
        }
    }

    fn touch_history(&mut self, path: &Path) {
        self.history.retain(|p| p != path);
        self.history.insert(0, path.to_path_buf());
    }

    /// The label of every open tab, as `workbench.editor.labelFormat` asks. `root` is the opened
    /// folder (`medium` shows the directory relative to it).
    ///
    /// `short`: the name and the directory's own name. `medium`: the name and the directory
    /// relative to the root. `long`: the name and the directory's absolute path. `default`: the
    /// name, with the relative directory added when two open tabs share a name.
    pub fn tab_labels(&self, root: &Path, format: LabelFormat) -> Vec<TabLabel> {
        self.open
            .iter()
            .enumerate()
            .map(|(i, e)| {
                let title = e.title.clone();
                let dir = e.path.parent();
                let relative = || {
                    dir.and_then(|d| d.strip_prefix(root).ok())
                        .filter(|r| !r.as_os_str().is_empty())
                        .map(|r| r.display().to_string())
                };
                let description = match format {
                    LabelFormat::Short => dir.and_then(|d| d.file_name()).map(|n| n.to_string_lossy().into_owned()),
                    LabelFormat::Medium => relative(),
                    LabelFormat::Long => dir.map(|d| d.display().to_string()).filter(|d| !d.is_empty()),
                    LabelFormat::Default => {
                        let shared = self.open.iter().enumerate().any(|(j, o)| j != i && o.title == e.title);
                        shared.then(relative).flatten()
                    }
                };
                TabLabel { title, description }
            })
            .collect()
    }

    /// The files to offer as "recently opened": the active editor first, then the history,
    /// most recent first (`IHistoryService.getHistory`).
    pub fn recent_files(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = Vec::new();
        if let Some(a) = self.active_editor() {
            out.push(a.path.clone());
        }
        for p in &self.history {
            if !out.contains(p) {
                out.push(p.clone());
            }
        }
        out
    }

    /// Close a tab. Closing the active tab moves focus to the most recently used other editor
    /// (`workbench.editor.focusRecentEditorAfterClose`), else to the tab on the right, else the
    /// left. Closing another tab leaves focus where it is.
    pub fn close(&mut self, index: usize) {
        if index >= self.open.len() {
            return;
        }
        let closed_active = self.active == Some(index);
        let next = (closed_active && self.open.len() > 1).then(|| self.next_after_close(index));
        self.remove_at(index);
        if let Some(j) = next.and_then(|path| self.index_of(&path)) {
            self.activate(j);
        }
    }

    /// The editor that takes focus when the active editor at `index` closes.
    fn next_after_close(&self, index: usize) -> PathBuf {
        let closing = &self.open[index].path;
        if self.settings.focus_recent_editor_after_close && let Some(p) = self.mru.iter().find(|p| *p != closing) {
            return p.clone();
        }
        let right = self.open.get(index + 1).or_else(|| index.checked_sub(1).and_then(|i| self.open.get(i)));
        right.map_or_else(|| closing.clone(), |e| e.path.clone())
    }

    pub fn active_editor(&self) -> Option<&OpenEditor> {
        self.active.and_then(|i| self.open.get(i))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_workspace(name: &str) -> PathBuf {
        // Unique per call (no recursive cleanup of old runs; the OS clears temp).
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("vsr-model-{name}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::create_dir_all(dir.join(".git")).unwrap();
        fs::create_dir_all(dir.join("Docs")).unwrap();
        fs::write(dir.join("b.txt"), "b").unwrap();
        fs::write(dir.join("A.md"), "a\r\nb\r\n").unwrap();
        fs::write(dir.join("c.rs"), "fn main() {}\n").unwrap();
        dir
    }

    #[test]
    fn list_dir_sorts_folders_first_case_insensitive_and_excludes_vcs() {
        let dir = tmp_workspace("list");
        let default_exclude = vsr_settings::SettingsRegistry::load_default()
            .get("files.exclude")
            .map(|s| ExcludeMatcher::from_value(&s.default))
            .unwrap();
        let names: Vec<_> = list_dir(&dir, &dir, &default_exclude).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Docs", "src", "A.md", "b.txt", "c.rs"]);
    }

    #[test]
    fn list_dir_honours_a_custom_exclude_map() {
        let dir = tmp_workspace("exclude");
        let ex = ExcludeMatcher::from_value(&serde_json::json!({"**/*.txt": true, "Docs": true}));
        let names: Vec<_> = list_dir(&dir, &dir, &ex).unwrap().into_iter().map(|e| e.name).collect();
        // .git is shown now (no default entry), b.txt and Docs are hidden.
        assert_eq!(names, [".git", "src", "A.md", "c.rs"]);
    }

    #[test]
    fn save_transforms_edit_through_the_editor_and_undo() {
        let dir = tmp_workspace("transforms");
        fs::write(dir.join("t.txt"), "a  \nb").unwrap();
        let mut ed = Editors::default();
        ed.open_file(&dir.join("t.txt")).unwrap();
        let e = &mut ed.open[0];
        e.save_with(SaveTransforms { trim_trailing_whitespace: true, insert_final_newline: true, ..Default::default() }).unwrap();
        assert_eq!(fs::read_to_string(dir.join("t.txt")).unwrap(), "a\nb\n");
        assert!(!e.is_dirty());
        e.editor.undo();
        e.editor.undo();
        assert_eq!(e.buffer().value(), "a  \nb", "both transforms undo");
    }

    #[test]
    fn opening_same_file_twice_focuses_existing_tab() {
        let dir = tmp_workspace("dedupe");
        let mut ed = Editors::default();
        ed.open_file(&dir.join("b.txt")).unwrap();
        ed.open_file(&dir.join("c.rs")).unwrap();
        ed.open_file(&dir.join("b.txt")).unwrap();
        assert_eq!(ed.open.len(), 2);
        assert_eq!(ed.active_editor().unwrap().title, "b.txt");
    }

    #[test]
    fn new_tab_opens_right_of_active() {
        let dir = tmp_workspace("order");
        let mut ed = Editors::default();
        ed.open_file(&dir.join("b.txt")).unwrap();
        ed.open_file(&dir.join("c.rs")).unwrap();
        ed.active = Some(0);
        ed.open_file(&dir.join("A.md")).unwrap();
        let titles: Vec<_> = ed.open.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["b.txt", "A.md", "c.rs"]);
        assert_eq!(ed.active, Some(1));
    }

    #[test]
    fn recent_files_are_most_recent_first_with_the_active_editor_on_top() {
        let dir = tmp_workspace("history");
        let mut ed = Editors::default();
        for f in ["b.txt", "c.rs", "A.md", "b.txt"] {
            ed.open_file(&dir.join(f)).unwrap();
        }
        let names = |ed: &Editors| -> Vec<String> {
            ed.recent_files().iter().map(|p| p.file_name().unwrap().to_string_lossy().into_owned()).collect()
        };
        assert_eq!(names(&ed), ["b.txt", "A.md", "c.rs"]);
        ed.close(ed.active.unwrap()); // closed files stay in the history
        ed.active = Some(0);
        assert_eq!(names(&ed), ["c.rs", "b.txt", "A.md"]);
    }

    #[test]
    fn closing_tabs_moves_focus_like_vscode() {
        let dir = tmp_workspace("close");
        let mut ed = Editors::default();
        for f in ["b.txt", "c.rs", "A.md"] {
            ed.open_file(&dir.join(f)).unwrap();
        }
        ed.active = Some(1);
        ed.close(1); // closing active: focus the tab that slid into its place
        assert_eq!(ed.active_editor().unwrap().title, "A.md");
        ed.close(1); // closing last+active: focus the left neighbour
        assert_eq!(ed.active_editor().unwrap().title, "b.txt");
        ed.close(0);
        assert!(ed.active_editor().is_none());
    }

    #[test]
    fn file_content_goes_through_vsr_text() {
        let dir = tmp_workspace("eol");
        let mut ed = Editors::default();
        ed.open_file(&dir.join("A.md")).unwrap();
        let e = ed.active_editor().unwrap();
        assert_eq!(e.buffer().line_count(), 3);
        assert_eq!(e.eol_label(), "CRLF");
    }

    #[test]
    fn edit_makes_dirty_and_undo_to_saved_state_is_clean() {
        let dir = tmp_workspace("dirty");
        let mut ed = Editors::default();
        ed.open_file(&dir.join("c.rs")).unwrap();
        let e = &mut ed.open[0];
        assert!(!e.is_dirty());
        e.editor.type_text("x");
        assert!(e.is_dirty());
        e.editor.undo();
        assert!(!e.is_dirty());
    }

    #[test]
    fn save_round_trips_and_preserves_crlf() {
        let dir = tmp_workspace("save");
        let mut ed = Editors::default();
        ed.open_file(&dir.join("A.md")).unwrap();
        let e = &mut ed.open[0];
        e.editor.type_text("hi");
        e.editor.enter();
        assert!(e.is_dirty());
        e.save().unwrap();
        assert!(!e.is_dirty());
        assert_eq!(fs::read_to_string(dir.join("A.md")).unwrap(), "hi\r\na\r\nb\r\n");
        // Edit after save is dirty again; undoing back to the saved state is clean.
        e.editor.type_text("z");
        assert!(e.is_dirty());
        e.editor.undo();
        assert!(!e.is_dirty());
    }

    // ---- preview, MRU, positioning, close focus, limit, labels -------------------------------

    use crate::config::settings::OpenSource;

    /// A temp folder with `count` files `1.txt`, `2.txt`, ... (the content is the number).
    fn numbered_files(tag: &str, count: usize) -> (PathBuf, Vec<PathBuf>) {
        let dir = tmp_workspace(tag);
        let paths = (1..=count)
            .map(|n| {
                let p = dir.join(format!("{n}.txt"));
                fs::write(&p, n.to_string()).unwrap();
                p
            })
            .collect();
        (dir, paths)
    }

    /// The file stems of the open editors, in tab order (`1`, `2`, ... become `"123"`).
    fn order(ed: &Editors) -> String {
        ed.open.iter().map(|e| e.path.file_stem().unwrap().to_string_lossy().into_owned()).collect()
    }

    fn mru_order(ed: &Editors) -> String {
        ed.mru.iter().map(|p| p.file_stem().unwrap().to_string_lossy().into_owned()).collect()
    }

    fn active_stem(ed: &Editors) -> Option<String> {
        ed.active_editor().map(|e| e.path.file_stem().unwrap().to_string_lossy().into_owned())
    }

    fn index_named(ed: &Editors, stem: &str) -> usize {
        ed.open.iter().position(|e| e.path.file_stem().unwrap() == stem).unwrap()
    }

    fn pinned_active() -> OpenOptions {
        OpenOptions::PINNED
    }

    // upstream: EditorGroupModel > Multiple Editors - Pinned and Active
    #[test]
    fn multiple_editors_pinned_and_active() {
        let (_d, p) = numbered_files("pinned-active", 3);
        let mut ed = Editors::default();
        ed.open_with(&p[0], pinned_active()).unwrap();
        ed.open_with(&p[0], pinned_active()).unwrap(); // the existing one is focused, not added twice
        for f in &p[1..] {
            ed.open_with(f, pinned_active()).unwrap();
        }
        assert_eq!(order(&ed), "123");
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
        assert_eq!(mru_order(&ed), "321");
        assert!((0..3).all(|i| !ed.is_preview(i)));
    }

    // upstream: EditorGroupModel > Multiple Editors - Preview editor moves to the side of the active one
    #[test]
    fn multiple_editors_preview_moves_to_the_side_of_the_active_one() {
        let (_d, p) = numbered_files("preview-side", 4);
        let mut ed = Editors::default();
        ed.open_with(&p[0], OpenOptions::PREVIEW).unwrap();
        ed.open_with(&p[1], pinned_active()).unwrap();
        ed.open_with(&p[2], pinned_active()).unwrap();
        assert_eq!(order(&ed), "123");
        ed.open_with(&p[3], OpenOptions::PREVIEW).unwrap(); // the preview 1 closes, 4 goes after 3
        assert_eq!(order(&ed), "234");
        assert_eq!(index_named(&ed, "4"), 2);
        assert!(ed.is_preview(2));
    }

    // upstream: EditorGroupModel > Multiple Editors - Pinned and Active (DEFAULT_OPEN_EDITOR_DIRECTION = Direction.LEFT)
    #[test]
    fn multiple_editors_pinned_and_active_opening_to_the_left() {
        let (_d, p) = numbered_files("pinned-left", 3);
        let mut ed = Editors { settings: EditorGroupSettings { open_positioning: OpenPositioning::Left, ..Default::default() }, ..Default::default() };
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
        }
        assert_eq!(order(&ed), "321");
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
    }

    // upstream: EditorGroupModel > Multiple Editors - Pinned and Not Active
    #[test]
    fn multiple_editors_pinned_and_not_active() {
        let (_d, p) = numbered_files("pinned-not-active", 3);
        let mut ed = Editors::default();
        let not_active = OpenOptions { pinned: true, active: false };
        ed.open_with(&p[0], pinned_active()).unwrap();
        ed.open_with(&p[1], not_active).unwrap();
        ed.open_with(&p[2], not_active).unwrap();
        assert_eq!(active_stem(&ed).as_deref(), Some("1"));
        assert_eq!(mru_order(&ed), "132", "a new editor that is not focused goes second in the MRU list");
        assert!((0..3).all(|i| !ed.is_preview(i)));
    }

    // upstream: EditorGroupModel > Multiple Editors - Preview gets overwritten
    #[test]
    fn multiple_editors_preview_gets_overwritten() {
        let (_d, p) = numbered_files("preview-overwritten", 3);
        let mut ed = Editors::default();
        for f in &p {
            ed.open_with(f, OpenOptions::PREVIEW).unwrap();
        }
        assert_eq!(order(&ed), "3");
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
        assert!(ed.is_preview(0));
        assert_eq!(mru_order(&ed), "3");
    }

    // upstream: EditorGroupModel > Preview tab does not have a stable position (https://github.com/microsoft/vscode/issues/8245)
    #[test]
    fn preview_tab_does_not_have_a_stable_position() {
        let (_d, p) = numbered_files("preview-stable", 3);
        let mut ed = Editors::default();
        ed.open_with(&p[0], pinned_active()).unwrap();
        ed.open_with(&p[1], OpenOptions::PREVIEW).unwrap();
        ed.activate(0);
        ed.open_with(&p[2], OpenOptions::PREVIEW).unwrap();
        assert_eq!(index_named(&ed, "3"), 1);
    }

    // upstream: EditorGroupModel > Multiple Editors - closing picks next from MRU list
    #[test]
    fn multiple_editors_closing_picks_next_from_mru_list() {
        let (_d, p) = numbered_files("close-mru", 5);
        let mut ed = Editors::default();
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
        }
        assert_eq!(active_stem(&ed).as_deref(), Some("5"));
        ed.close(index_named(&ed, "5"));
        assert_eq!(active_stem(&ed).as_deref(), Some("4"));
        ed.activate(index_named(&ed, "1"));
        ed.activate(index_named(&ed, "4"));
        ed.close(index_named(&ed, "4"));
        assert_eq!(active_stem(&ed).as_deref(), Some("1"));
        ed.close(index_named(&ed, "1"));
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
        ed.activate(index_named(&ed, "2"));
        ed.close(index_named(&ed, "2"));
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
        ed.close(index_named(&ed, "3"));
        assert!(ed.active_editor().is_none());
        assert_eq!(ed.open.len(), 0);
    }

    #[test]
    fn closing_the_active_tab_focuses_the_mru_editor_or_the_right_neighbour() {
        let (_d, p) = numbered_files("close-focus", 4);
        let mut ed = Editors::default();
        let mut ltr = Editors { settings: EditorGroupSettings { focus_recent_editor_after_close: false, ..Default::default() }, ..Default::default() };
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
            ltr.open_with(f, pinned_active()).unwrap();
        }
        for e in [&mut ed, &mut ltr] {
            e.activate(0); // mru: 1 4 3 2
            e.activate(2); // mru: 3 1 4 2, focus on 3
        }
        assert_eq!(mru_order(&ed), "3142");
        ed.close(2);
        assert_eq!(active_stem(&ed).as_deref(), Some("1"), "most recently used");
        ltr.close(2);
        assert_eq!(active_stem(&ltr).as_deref(), Some("4"), "the right neighbour");
    }

    #[test]
    fn opening_to_the_first_and_last_positions() {
        let (_d, p) = numbered_files("positions", 4);
        let mut ed = Editors { settings: EditorGroupSettings { open_positioning: OpenPositioning::First, ..Default::default() }, ..Default::default() };
        ed.open_with(&p[0], pinned_active()).unwrap();
        ed.open_with(&p[1], pinned_active()).unwrap();
        assert_eq!(order(&ed), "21");
        ed.settings.open_positioning = OpenPositioning::Last;
        ed.open_with(&p[2], pinned_active()).unwrap();
        assert_eq!(order(&ed), "213");
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
    }

    #[test]
    fn a_dirty_preview_is_pinned_and_a_double_click_pins_one() {
        let (_d, p) = numbered_files("dirty-preview", 3);
        let mut ed = Editors::default();
        ed.open_with(&p[0], OpenOptions::PREVIEW).unwrap();
        ed.open[0].editor.type_text("x");
        ed.pin_dirty_preview();
        assert!(!ed.is_preview(0), "the first edit pins the preview");
        ed.open_with(&p[1], OpenOptions::PREVIEW).unwrap();
        assert!(ed.is_preview(1));
        ed.pin(&p[1]); // double click on the tab
        assert!(!ed.is_preview(1));
        assert_eq!(order(&ed), "12");
    }

    #[test]
    fn turning_previews_off_pins_the_current_one() {
        let (_d, p) = numbered_files("previews-off", 2);
        let mut ed = Editors::default();
        ed.open_with(&p[0], OpenOptions::PREVIEW).unwrap();
        ed.apply_settings(EditorGroupSettings { enable_preview: false, ..Default::default() });
        assert!(!ed.is_preview(0));
        ed.open_with(&p[1], OpenOptions::PREVIEW).unwrap();
        assert_eq!(order(&ed), "12", "nothing is a preview any more after the setting is off");
    }

    #[test]
    fn opens_pinned_follows_the_source_and_the_preview_settings() {
        let on = EditorGroupSettings::default();
        assert!(!on.opens_pinned(OpenSource::Explorer { double_click: false }), "a single click previews");
        assert!(on.opens_pinned(OpenSource::Explorer { double_click: true }), "a double click pins");
        assert!(on.opens_pinned(OpenSource::QuickOpen), "Quick Open pins unless enablePreviewFromQuickOpen");
        assert!(!on.opens_pinned(OpenSource::Search));
        let quick = EditorGroupSettings { enable_preview_from_quick_open: true, ..on };
        assert!(!quick.opens_pinned(OpenSource::QuickOpen));
        let off = EditorGroupSettings { enable_preview: false, enable_preview_from_quick_open: true, ..on };
        assert!(off.opens_pinned(OpenSource::Explorer { double_click: false }), "all opens pin when previews are off");
        assert!(off.opens_pinned(OpenSource::QuickOpen));
    }

    #[test]
    fn scrolling_over_tabs_steps_the_active_tab_without_wrapping() {
        let (_d, p) = numbered_files("scroll-tabs", 3);
        let mut ed = Editors::default();
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
        }
        ed.step_active(-1);
        assert_eq!(active_stem(&ed).as_deref(), Some("2"));
        ed.step_active(5);
        assert_eq!(active_stem(&ed).as_deref(), Some("3"), "clamped at the last tab");
        ed.step_active(-9);
        assert_eq!(active_stem(&ed).as_deref(), Some("1"));
    }

    #[test]
    fn limit_closes_the_least_recently_used_editors() {
        let (_d, p) = numbered_files("limit", 3);
        let mut ed = Editors { settings: EditorGroupSettings { limit_enabled: true, limit_value: 2, ..Default::default() }, ..Default::default() };
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
        }
        assert_eq!(order(&ed), "23", "1 was the least recently used");
        assert_eq!(active_stem(&ed).as_deref(), Some("3"));
    }

    #[test]
    fn limit_never_closes_dirty_editors() {
        let (_d, p) = numbered_files("limit-dirty", 3);
        let mut ed = Editors { settings: EditorGroupSettings { limit_enabled: true, limit_value: 2, ..Default::default() }, ..Default::default() };
        ed.open_with(&p[0], pinned_active()).unwrap();
        ed.open[0].editor.type_text("x");
        ed.open_with(&p[1], pinned_active()).unwrap();
        ed.open_with(&p[2], pinned_active()).unwrap();
        assert_eq!(order(&ed), "13", "the dirty 1 stays, 2 is closed");
    }

    #[test]
    fn limit_exclude_dirty_leaves_dirty_editors_out_of_the_count() {
        let (_d, p) = numbered_files("limit-exclude", 3);
        let settings = EditorGroupSettings { limit_enabled: true, limit_value: 2, limit_exclude_dirty: true, ..Default::default() };
        let mut ed = Editors { settings, ..Default::default() };
        ed.open_with(&p[0], pinned_active()).unwrap();
        ed.open[0].editor.type_text("x");
        ed.open_with(&p[1], pinned_active()).unwrap();
        ed.open_with(&p[2], pinned_active()).unwrap();
        assert_eq!(order(&ed), "123", "two clean editors are within the limit");
    }

    #[test]
    fn lowering_the_limit_closes_editors_at_once() {
        let (_d, p) = numbered_files("limit-lower", 3);
        let mut ed = Editors::default();
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
        }
        ed.apply_settings(EditorGroupSettings { limit_enabled: true, limit_value: 1, ..Default::default() });
        assert_eq!(order(&ed), "3");
    }

    #[test]
    fn files_that_are_gone_close_unless_dirty() {
        let (_d, p) = numbered_files("deleted", 3);
        let mut ed = Editors::default();
        for f in &p {
            ed.open_with(f, pinned_active()).unwrap();
        }
        ed.open[0].editor.type_text("x");
        let gone = p[1].clone();
        let closed = ed.close_missing(|path| path != gone && path != p[0]);
        assert_eq!(closed, vec![gone]);
        assert_eq!(order(&ed), "13", "the dirty 1 is missing too but stays open");
    }

    #[test]
    fn tab_labels_follow_the_label_format() {
        let dir = tmp_workspace("labels");
        fs::create_dir_all(dir.join("a")).unwrap();
        fs::create_dir_all(dir.join("b")).unwrap();
        fs::write(dir.join("a").join("mod.rs"), "").unwrap();
        fs::write(dir.join("b").join("mod.rs"), "").unwrap();
        fs::write(dir.join("c.rs"), "").unwrap();
        let mut ed = Editors::default();
        for f in [dir.join("a/mod.rs"), dir.join("b/mod.rs"), dir.join("c.rs")] {
            ed.open_file(&f).unwrap();
        }
        let label = |format| ed.tab_labels(&dir, format);
        let desc = |format| label(format).into_iter().map(|t| t.description.unwrap_or_default()).collect::<Vec<_>>();
        // Default: only the duplicated names get their directory.
        assert_eq!(desc(LabelFormat::Default), ["a", "b", ""]);
        let root_name = dir.file_name().unwrap().to_string_lossy().into_owned();
        assert_eq!(desc(LabelFormat::Short), vec!["a".to_string(), "b".into(), root_name]);
        assert_eq!(desc(LabelFormat::Medium), ["a", "b", ""], "relative to the opened folder");
        assert_eq!(desc(LabelFormat::Long)[0], dir.join("a").display().to_string());
        assert_eq!(label(LabelFormat::Medium)[2].title, "c.rs");
    }
}
