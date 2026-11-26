//! Command-line arguments of the `rscode` binary (ADR-0110).
//!
//! `rscode [folder] [file ...]`, the way `code` reads its paths: the first folder becomes the
//! workspace root and every file opens in an editor. With only files, the root is the first
//! file's folder; with nothing, the current directory. A file that does not exist yet opens as a
//! new file when its folder exists (`code new.txt`), else it is reported. Options are handled by the `rode` shell
//! launcher, so anything starting with `-` is ignored here.
//!
//! Upstream source: src/vs/platform/environment/node/argv.ts (paths part only).

use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchArgs {
    /// The workspace folder.
    pub root: PathBuf,
    /// Files to open after the window is up, in the order given.
    pub files: Vec<LaunchFile>,
    /// Paths that are neither a folder nor a file (reported on stderr, then skipped).
    pub missing: Vec<PathBuf>,
    /// Folders after the first: rscode has one root per window, so these are ignored.
    pub extra_folders: Vec<PathBuf>,
}

/// A file argument. A file that does not exist yet is `new` when its folder exists: it opens as an
/// empty editor that the first save creates (`code new.txt`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchFile {
    pub path: PathBuf,
    pub new: bool,
}

/// Parses `args` (without the program name). Relative paths resolve against `cwd`.
pub fn parse<I, S>(args: I, cwd: &Path) -> LaunchArgs
where
    I: IntoIterator<Item = S>,
    S: Into<PathBuf>,
{
    let mut folders = Vec::new();
    let mut files = Vec::new();
    let mut missing = Vec::new();
    for arg in args {
        let arg: PathBuf = arg.into();
        if arg.as_os_str().to_string_lossy().starts_with('-') {
            continue;
        }
        let path = absolute(&cwd.join(arg));
        if path.is_dir() {
            folders.push(path);
        } else if path.is_file() {
            files.push(LaunchFile { path, new: false });
        } else if path.parent().is_some_and(Path::is_dir) {
            files.push(LaunchFile { path, new: true });
        } else {
            missing.push(path);
        }
    }
    let mut folders = folders.into_iter();
    let root = folders
        .next()
        .or_else(|| files.first().and_then(|f| f.path.parent()).map(Path::to_path_buf))
        .unwrap_or_else(|| absolute(cwd));
    LaunchArgs { root, files, missing, extra_folders: folders.collect() }
}

/// `canonicalize` when the path exists (resolves `..` and symlinks). A path that does not exist
/// yet keeps its canonical folder, so it matches the paths of the files in that folder.
fn absolute(path: &Path) -> PathBuf {
    if let Ok(full) = path.canonicalize() {
        return full;
    }
    match (path.parent().and_then(|p| p.canonicalize().ok()), path.file_name()) {
        (Some(parent), Some(name)) => parent.join(name),
        _ => path.to_path_buf(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rscode-cli-{name}-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        dir.canonicalize().unwrap()
    }

    fn file(path: PathBuf, new: bool) -> LaunchFile {
        LaunchFile { path, new }
    }

    #[test]
    fn no_args_opens_the_current_directory() {
        let cwd = tmp("none");
        let a = parse(Vec::<String>::new(), &cwd);
        assert_eq!(a.root, cwd);
        assert!(a.files.is_empty());
    }

    #[test]
    fn a_relative_folder_resolves_against_cwd() {
        let cwd = tmp("rel");
        fs::create_dir_all(cwd.join("proj")).unwrap();
        let a = parse(["proj"], &cwd);
        assert_eq!(a.root, cwd.join("proj"));
        let dot = parse(["."], &cwd.join("proj"));
        assert_eq!(dot.root, cwd.join("proj"));
    }

    #[test]
    fn only_a_file_uses_its_folder_as_root_and_opens_it() {
        let cwd = tmp("file");
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::write(cwd.join("src/main.rs"), "fn main() {}\n").unwrap();
        let a = parse(["src/main.rs"], &cwd);
        assert_eq!(a.root, cwd.join("src"));
        assert_eq!(a.files, vec![file(cwd.join("src/main.rs"), false)]);
    }

    #[test]
    fn a_folder_and_files_keep_the_folder_as_root_and_file_order() {
        let cwd = tmp("mixed");
        fs::write(cwd.join("b.txt"), "").unwrap();
        fs::write(cwd.join("a.txt"), "").unwrap();
        let a = parse(["b.txt", ".", "a.txt"], &cwd);
        assert_eq!(a.root, cwd);
        assert_eq!(a.files, vec![file(cwd.join("b.txt"), false), file(cwd.join("a.txt"), false)]);
    }

    #[test]
    fn a_missing_file_in_an_existing_folder_opens_as_a_new_file() {
        let cwd = tmp("new");
        let a = parse(["new.txt"], &cwd);
        assert_eq!(a.files, vec![file(cwd.join("new.txt"), true)]);
        assert_eq!(a.root, cwd);
        assert!(a.missing.is_empty());
    }

    #[test]
    fn a_missing_file_in_a_missing_folder_is_reported_and_skipped() {
        let cwd = tmp("nodir");
        let a = parse(["no-such-dir/new.txt"], &cwd);
        assert!(a.files.is_empty());
        assert_eq!(a.missing, vec![cwd.join("no-such-dir/new.txt")]);
        assert_eq!(a.root, cwd);
    }

    #[test]
    fn extra_folders_and_missing_paths_are_reported_not_opened() {
        let cwd = tmp("extra");
        fs::create_dir_all(cwd.join("one")).unwrap();
        fs::create_dir_all(cwd.join("two")).unwrap();
        let a = parse(["one", "two", "gone/nope.txt"], &cwd);
        assert_eq!(a.root, cwd.join("one"));
        assert_eq!(a.extra_folders, vec![cwd.join("two")]);
        assert_eq!(a.missing, vec![cwd.join("gone/nope.txt")]);
        assert!(a.files.is_empty());
    }

    #[test]
    fn options_are_ignored() {
        let cwd = tmp("opts");
        let a = parse(["--wait", "-n"], &cwd);
        assert_eq!(a.root, cwd);
        assert!(a.missing.is_empty());
    }
}
