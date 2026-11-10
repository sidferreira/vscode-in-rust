//! `explorer.excludeGitIgnore`: the entries that a `.gitignore` excludes are not listed in the
//! Explorer. The rules that apply to a folder are those of every `.gitignore` from the workspace
//! root down to it; a deeper file overrides a shallower one (a `!` pattern there re-includes).
//!
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerViewer.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/common/ignoreFile.ts @ 50e1350fc78d

use std::path::{Path, PathBuf};

use ignore::Match;
use ignore::gitignore::Gitignore;

use crate::model::DirEntry;

/// A parsed `.gitignore` and the folder it applies to (the folder that holds it).
pub type Matcher = (PathBuf, Gitignore);

/// The `.gitignore` matchers that apply to the entries of `dir`, shallowest first. Only folders
/// from `root` down to `dir` are read.
pub fn matchers_for(root: &Path, dir: &Path) -> Vec<Matcher> {
    let mut folders: Vec<&Path> = dir.ancestors().take_while(|d| d.starts_with(root)).collect();
    folders.reverse();
    folders
        .into_iter()
        .filter_map(|folder| {
            let file = folder.join(".gitignore");
            file.is_file().then(|| (folder.to_path_buf(), Gitignore::new(&file).0))
        })
        .collect()
}

/// Whether `path` is excluded by the deepest matcher that has a rule for it. A folder that is
/// excluded hides everything inside it, so the listing never reaches its children.
pub fn is_ignored(matchers: &[Matcher], path: &Path, is_dir: bool) -> bool {
    for (folder, gitignore) in matchers.iter().rev() {
        if !path.starts_with(folder) {
            continue;
        }
        match gitignore.matched_path_or_any_parents(path, is_dir) {
            Match::None => {}
            Match::Ignore(_) => return true,
            Match::Whitelist(_) => return false,
        }
    }
    false
}

/// Drops the entries of `dir` (listed from disk) that the `.gitignore` rules exclude.
pub fn retain_visible(entries: &mut Vec<DirEntry>, root: &Path, dir: &Path) {
    let matchers = matchers_for(root, dir);
    if matchers.is_empty() {
        return;
    }
    entries.retain(|e| !is_ignored(&matchers, &e.path, e.is_dir));
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::explorer::testutil::TempWorkspace;

    #[test]
    fn deeper_gitignore_files_override_shallower_ones() {
        let ws = TempWorkspace::new("gitignore");
        let root = ws.path();
        fs::write(root.join(".gitignore"), "target/\n*.log\n").unwrap();
        fs::create_dir_all(root.join("sub")).unwrap();
        fs::write(root.join("sub").join(".gitignore"), "!keep.log\n").unwrap();
        let top = matchers_for(root, root);
        assert_eq!(top.len(), 1);
        assert!(is_ignored(&top, &root.join("target"), true));
        assert!(is_ignored(&top, &root.join("a.log"), false));
        assert!(!is_ignored(&top, &root.join("src.rs"), false));

        let sub = matchers_for(root, &root.join("sub"));
        assert_eq!(sub.len(), 2, "the root and the sub folder");
        assert!(is_ignored(&sub, &root.join("sub").join("b.log"), false), "the root rule still applies");
        assert!(!is_ignored(&sub, &root.join("sub").join("keep.log"), false), "the deeper negation wins");
        assert!(is_ignored(&sub, &root.join("target"), true));
    }

    #[test]
    fn retain_visible_hides_ignored_entries_and_keeps_the_gitignore_itself() {
        let ws = TempWorkspace::new("gitignore-list");
        let root = ws.path();
        fs::write(root.join(".gitignore"), "*.tmp\nbuild/\n").unwrap();
        let entry = |name: &str, is_dir: bool| DirEntry {
            path: root.join(name),
            name: name.to_string(),
            is_dir,
            modified: None,
        };
        let mut entries = vec![entry("build", true), entry("notes.tmp", false), entry("main.rs", false), entry(".gitignore", false)];
        retain_visible(&mut entries, root, root);
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["main.rs", ".gitignore"]);
    }

    #[test]
    fn no_gitignore_hides_nothing() {
        let ws = TempWorkspace::new("gitignore-none");
        assert!(matchers_for(ws.path(), ws.path()).is_empty());
        assert!(!is_ignored(&[], &ws.path().join("x"), false));
    }
}
