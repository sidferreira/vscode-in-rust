//! UI-independent workbench state: workspace tree listing and open editors.
//!
//! Kept free of egui so it can move into `vsr-workbench` and be reused by a
//! different front end (e.g. a webview-hosted workbench). ADR-0001/0006.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use vsr_editor::{Editor, EditorOptions};
use vsr_text::{DefaultEol, Eol, TextBuffer};

/// Editor options for opened files (macOS VS Code defaults: `editor.tabSize` 4,
/// `editor.insertSpaces` true). Becomes settings-driven with `vsr-settings`.
pub const EDITOR_OPTIONS: EditorOptions = EditorOptions { tab_size: 4, insert_spaces: true };

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
}

/// List a directory like the VS Code explorer's default sort: folders first,
/// then files, each case-insensitively by name. Hidden VCS folders are skipped
/// (mirrors the default `files.exclude`).
pub fn list_dir(dir: &Path) -> io::Result<Vec<DirEntry>> {
    const EXCLUDED: &[&str] = &[".git", ".svn", ".hg", ".DS_Store", "Thumbs.db"];
    let mut entries: Vec<DirEntry> = fs::read_dir(dir)?
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if EXCLUDED.contains(&name.as_str()) {
                return None;
            }
            let is_dir = e.file_type().ok()?.is_dir();
            Some(DirEntry { path: e.path(), name, is_dir })
        })
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
}

impl OpenEditor {
    pub fn from_text(path: &Path, text: &str) -> Self {
        let title = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Self {
            path: path.to_path_buf(),
            title,
            editor: Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EDITOR_OPTIONS),
        }
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

    /// Write the buffer to disk (`value()` already uses the file's EOL) and mark it clean.
    pub fn save(&mut self) -> io::Result<()> {
        fs::write(&self.path, self.editor.buffer().value())?;
        self.editor.mark_saved();
        Ok(())
    }
}

#[derive(Default)]
pub struct Editors {
    pub open: Vec<OpenEditor>,
    pub active: Option<usize>,
}

impl Editors {
    /// Open `path`, or focus it if it is already open. Like VS Code, the new
    /// tab is inserted right after the active one.
    pub fn open_file(&mut self, path: &Path) -> io::Result<()> {
        if let Some(i) = self.open.iter().position(|e| e.path == path) {
            self.active = Some(i);
            return Ok(());
        }
        let bytes = fs::read(path)?;
        let text = String::from_utf8_lossy(&bytes);
        let editor = OpenEditor::from_text(path, &text);
        let at = self.active.map_or(self.open.len(), |i| i + 1);
        self.open.insert(at, editor);
        self.active = Some(at);
        Ok(())
    }

    /// Close a tab; focus moves to the tab on the right, else the left.
    pub fn close(&mut self, index: usize) {
        if index >= self.open.len() {
            return;
        }
        self.open.remove(index);
        self.active = match self.active {
            _ if self.open.is_empty() => None,
            Some(a) if a > index => Some(a - 1),
            Some(a) if a == index => Some(index.min(self.open.len() - 1)),
            other => other,
        };
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
        let names: Vec<_> = list_dir(&dir).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["Docs", "src", "A.md", "b.txt", "c.rs"]);
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
}
