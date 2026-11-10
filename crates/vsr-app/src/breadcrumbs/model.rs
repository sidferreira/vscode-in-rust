//! The breadcrumb path of the active file and the listing behind its dropdown. UI-free.
//!
//! upstream: src/vs/workbench/browser/parts/editor/breadcrumbsModel.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/parts/editor/breadcrumbsPicker.ts @ 50e1350fc78d
//!
//! Only the file part of upstream's model: the symbol part needs document symbols (ADR-0047).

use std::cmp::Ordering;
use std::path::{Component, Path, PathBuf};

use vsr_search::patterns::FileFilter;
use vsr_settings::Configuration;

/// `breadcrumbs.filePath`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilePathMode {
    /// Every folder and the file name.
    On,
    /// No file segments.
    Off,
    /// Only the file name.
    Last,
}

impl FilePathMode {
    /// Unknown values fall back to `on`, the default.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("off") => Self::Off,
            Some("last") => Self::Last,
            _ => Self::On,
        }
    }
}

/// The settings the breadcrumbs read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BreadcrumbsSettings {
    /// `breadcrumbs.enabled`.
    pub enabled: bool,
    /// `breadcrumbs.filePath`.
    pub file_path: FilePathMode,
    /// `breadcrumbs.icons`.
    pub icons: bool,
}

impl BreadcrumbsSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            enabled: c.get_bool("breadcrumbs.enabled").unwrap_or(true),
            file_path: FilePathMode::parse(c.get_str("breadcrumbs.filePath")),
            icons: c.get_bool("breadcrumbs.icons").unwrap_or(true),
        }
    }
}

/// One crumb: a folder above the file, or the file itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Segment {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// The crumbs of `file`. Inside `root` the path is relative to it (the root folder itself is not
/// a crumb, as upstream does for a single folder). Outside it, the path is absolute: every
/// component below `/`.
pub fn file_segments(root: &Path, file: &Path, mode: FilePathMode) -> Vec<Segment> {
    if mode == FilePathMode::Off {
        return Vec::new();
    }
    let (mut path, rest) = match file.strip_prefix(root) {
        Ok(rel) => (root.to_path_buf(), rel.to_path_buf()),
        Err(_) => (PathBuf::from("/"), file.strip_prefix("/").unwrap_or(file).to_path_buf()),
    };
    let names: Vec<String> = rest
        .components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();
    let last = names.len().saturating_sub(1);
    let mut out = Vec::with_capacity(names.len());
    for (i, name) in names.into_iter().enumerate() {
        path.push(&name);
        out.push(Segment { name, path: path.clone(), is_dir: i < last });
    }
    if mode == FilePathMode::Last {
        out = out.split_off(last.min(out.len()));
    }
    out
}

/// An entry of a folder, as the dropdown lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Child {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Folders first, then files, each by name (case-insensitive, then exact), as the Explorer
/// lists them with the default sort.
pub fn sort_children(children: &mut [Child]) {
    children.sort_by(|a, b| {
        b.is_dir.cmp(&a.is_dir).then_with(|| {
            let (la, lb) = (a.name.to_lowercase(), b.name.to_lowercase());
            match la.cmp(&lb) {
                Ordering::Equal => a.name.cmp(&b.name),
                o => o,
            }
        })
    });
}

/// The entries of `dir` that `files.exclude` leaves in. Hidden files are listed (the Explorer
/// shows them too). An unreadable folder lists nothing.
pub fn list_children(root: &Path, dir: &Path, exclude_globs: &[String]) -> Vec<Child> {
    let filter = FileFilter::new(&[], &[], &[], exclude_globs, false);
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
        let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
        let shown = if is_dir { !filter.prune_dir(&rel) } else { filter.accept_file(&rel) };
        if shown {
            out.push(Child { name, path, is_dir });
        }
    }
    sort_children(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(segs: &[Segment]) -> Vec<&str> {
        segs.iter().map(|s| s.name.as_str()).collect()
    }

    #[test]
    fn file_inside_the_root_is_relative_and_the_root_is_not_a_crumb() {
        let root = Path::new("/work/proj");
        let segs = file_segments(root, Path::new("/work/proj/src/main.rs"), FilePathMode::On);
        assert_eq!(names(&segs), ["src", "main.rs"]);
        assert_eq!(segs[0].path, Path::new("/work/proj/src"));
        assert!(segs[0].is_dir);
        assert!(!segs[1].is_dir);
        assert_eq!(segs[1].path, Path::new("/work/proj/src/main.rs"));
    }

    #[test]
    fn file_at_the_root_has_only_its_name() {
        let segs = file_segments(Path::new("/work/proj"), Path::new("/work/proj/Cargo.toml"), FilePathMode::On);
        assert_eq!(names(&segs), ["Cargo.toml"]);
    }

    #[test]
    fn file_outside_the_root_shows_its_absolute_path() {
        let root = Path::new("/work/proj");
        let segs = file_segments(root, Path::new("/tmp/notes/todo.txt"), FilePathMode::On);
        assert_eq!(names(&segs), ["tmp", "notes", "todo.txt"]);
        assert_eq!(segs[0].path, Path::new("/tmp"));
        assert_eq!(segs[2].path, Path::new("/tmp/notes/todo.txt"));
    }

    #[test]
    fn last_keeps_only_the_file_name() {
        let root = Path::new("/work/proj");
        let segs = file_segments(root, Path::new("/work/proj/src/main.rs"), FilePathMode::Last);
        assert_eq!(names(&segs), ["main.rs"]);
        assert!(!segs[0].is_dir);
        let outside = file_segments(root, Path::new("/tmp/a.txt"), FilePathMode::Last);
        assert_eq!(names(&outside), ["a.txt"]);
    }

    #[test]
    fn off_has_no_segments() {
        let root = Path::new("/work/proj");
        assert!(file_segments(root, Path::new("/work/proj/src/main.rs"), FilePathMode::Off).is_empty());
    }

    #[test]
    fn file_path_mode_parses_the_three_values_and_falls_back_to_on() {
        assert_eq!(FilePathMode::parse(Some("on")), FilePathMode::On);
        assert_eq!(FilePathMode::parse(Some("off")), FilePathMode::Off);
        assert_eq!(FilePathMode::parse(Some("last")), FilePathMode::Last);
        assert_eq!(FilePathMode::parse(Some("sometimes")), FilePathMode::On);
        assert_eq!(FilePathMode::parse(None), FilePathMode::On);
    }

    #[test]
    fn children_sort_folders_first_then_by_name() {
        let child = |n: &str, d: bool| Child { name: n.into(), path: PathBuf::from(n), is_dir: d };
        let mut v = vec![child("b.rs", false), child("Zed", true), child("a.rs", false), child("alpha", true)];
        sort_children(&mut v);
        let order: Vec<_> = v.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(order, ["alpha", "Zed", "a.rs", "b.rs"]);
    }

    #[test]
    fn list_children_applies_files_exclude() {
        // Only files this test creates are removed, one by one (no recursive delete).
        let dir = std::env::temp_dir().join(format!("rscode-crumbs-test-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::create_dir_all(dir.join("target")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.join("notes.log"), "").unwrap();
        let listed = list_children(&dir, &dir, &["target".to_string(), "*.log".to_string()]);
        let names: Vec<_> = listed.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["src", "Cargo.toml"]);
        for f in ["src", "target"] {
            std::fs::remove_dir(dir.join(f)).unwrap();
        }
        for f in ["Cargo.toml", "notes.log"] {
            std::fs::remove_file(dir.join(f)).unwrap();
        }
        std::fs::remove_dir(&dir).unwrap();
    }
}
