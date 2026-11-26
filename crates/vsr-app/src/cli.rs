//! Command-line arguments of the `rscode` binary (ADR-0110).
//!
//! `rscode [folder] [file ...]`, the way `code` reads its paths: the first folder becomes the
//! workspace root and every file opens in an editor. With only files, the root is the first
//! file's folder; with nothing, the current directory. A file that does not exist yet opens as a
//! new file when its folder exists (`code new.txt`), else it is reported. With `-g`/`--goto`, each
//! file argument may end in `:line[:column]`, and the caret goes there. Other options are handled
//! by the `rode` shell launcher, so anything else starting with `-` is ignored here.
//!
//! Upstream source: src/vs/platform/environment/node/argv.ts and src/vs/base/common/extpath.ts
//! (`parseLineAndColumnAware`, the paths part only).

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
    /// The 1-based `(line, column)` from `-g file:line[:column]`, if given.
    pub position: Option<(usize, usize)>,
}

/// Parses `args` (without the program name). Relative paths resolve against `cwd`.
pub fn parse<I, S>(args: I, cwd: &Path) -> LaunchArgs
where
    I: IntoIterator<Item = S>,
    S: Into<PathBuf>,
{
    let args: Vec<PathBuf> = args.into_iter().map(Into::into).collect();
    let goto = args.iter().any(|a| a == "-g" || a == "--goto");
    let mut folders = Vec::new();
    let mut files = Vec::new();
    let mut missing = Vec::new();
    for arg in args {
        if arg.as_os_str().to_string_lossy().starts_with('-') {
            continue;
        }
        // An argument that exists as it is wins (`report:2024`); only then is `:line[:column]` read.
        let literal = cwd.join(&arg);
        let (arg, position) = if !goto || literal.exists() {
            (arg, None)
        } else {
            match arg.to_str().map(parse_goto) {
                Some(Ok(Some((path, position)))) => (PathBuf::from(path), position),
                // `main.rs:12x` for an existing `main.rs` is a mistyped position: report it. When the
                // part before the suffix is not a file, the argument is a plain name such as
                // `log:2024-01-05.txt` and opens as given.
                Some(Err(prefix)) if cwd.join(&prefix).is_file() => {
                    missing.push(literal);
                    continue;
                }
                _ => (arg, None),
            }
        };
        let path = absolute(&cwd.join(arg));
        if path.is_dir() {
            folders.push(path);
        } else if path.is_file() {
            files.push(LaunchFile { path, new: false, position });
        } else if path.parent().is_some_and(Path::is_dir) {
            files.push(LaunchFile { path, new: true, position });
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

/// `-g` syntax (upstream `parseLineAndColumnAware`): the path and the optional `(line, column)`.
/// Only trailing all-digit segments are a position: `:line` (the column is 1) or `:line:column`; a
/// line or column of 0 is clamped to 1. A colon elsewhere is part of the path. `Ok(None)` when no
/// path is left (`:3`). `Err(prefix)` when the segment after the path looks like a number but is not
/// one (`:12x`, `:-3`), with the path in front of it: the caller reports the argument when that path
/// is an existing file (a mistyped position) and otherwise opens the argument as a plain name.
fn parse_goto(text: &str) -> Result<Option<(String, Option<(usize, usize)>)>, String> {
    let is_number = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    // A segment that starts like a number (`12x`, `-3`, `+4`) but is not all digits.
    let is_malformed = |s: &str| {
        let digits = s.strip_prefix(['-', '+']).unwrap_or(s);
        digits.as_bytes().first().is_some_and(u8::is_ascii_digit) && !is_number(s)
    };
    let mut segments: Vec<&str> = text.split(':').collect();
    let mut numbers = Vec::new();
    while numbers.len() < 2 && segments.len() > 1 && segments.last().is_some_and(|s| is_number(s)) {
        numbers.push(segments.pop().unwrap_or_default().parse::<usize>().unwrap_or(usize::MAX));
    }
    if segments.len() > 1 && segments.last().is_some_and(|s| is_malformed(s)) {
        // The path in front: drop a `:line` before the bad segment too (`f.rs:3:4x` → `f.rs`).
        let mut path = segments[..segments.len() - 1].to_vec();
        if path.len() > 1 && path.last().is_some_and(|s| is_number(s)) {
            path.pop();
        }
        return Err(path.join(":"));
    }
    let path = segments.join(":");
    if path.is_empty() {
        return Ok(None);
    }
    numbers.reverse();
    let position = match numbers.as_slice() {
        [line] => Some((*line.max(&1), 1)),
        [line, column] => Some((*line.max(&1), *column.max(&1))),
        _ => None,
    };
    Ok(Some((path, position)))
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
        LaunchFile { path, new, position: None }
    }

    fn at(path: PathBuf, new: bool, line: usize, column: usize) -> LaunchFile {
        LaunchFile { path, new, position: Some((line, column)) }
    }

    #[test]
    fn goto_takes_line_and_column_from_the_file_argument() {
        let cwd = tmp("goto-both");
        fs::write(cwd.join("a.txt"), "x\n").unwrap();
        let a = parse(["-g", "a.txt:3:4"], &cwd);
        assert_eq!(a.files, vec![at(cwd.join("a.txt"), false, 3, 4)]);
        assert!(a.missing.is_empty());
    }

    #[test]
    fn goto_with_only_a_line_puts_the_column_at_1() {
        let cwd = tmp("goto-line");
        fs::write(cwd.join("a.txt"), "x\n").unwrap();
        let a = parse(["--goto", "a.txt:7"], &cwd);
        assert_eq!(a.files, vec![at(cwd.join("a.txt"), false, 7, 1)]);
    }

    #[test]
    fn goto_applies_to_a_file_that_does_not_exist_yet_and_keeps_the_folder_as_root() {
        let cwd = tmp("goto-new");
        let a = parse(["-g", "new.txt:2"], &cwd);
        assert_eq!(a.files, vec![at(cwd.join("new.txt"), true, 2, 1)]);
        assert_eq!(a.root, cwd);
    }

    #[test]
    fn without_goto_a_colon_is_part_of_the_file_name() {
        let cwd = tmp("no-goto");
        fs::write(cwd.join("a.txt"), "x\n").unwrap();
        let a = parse(["a.txt:3"], &cwd);
        assert_eq!(a.files, vec![file(cwd.join("a.txt:3"), true)]);
    }

    #[test]
    fn goto_keeps_colons_that_are_not_followed_by_numbers_in_the_path() {
        let cwd = tmp("goto-colon");
        fs::write(cwd.join("a:b.txt"), "x\n").unwrap();
        let a = parse(["-g", "a:b.txt:5"], &cwd);
        assert_eq!(a.files, vec![at(cwd.join("a:b.txt"), false, 5, 1)]);
    }

    #[test]
    fn parse_goto_follows_upstream_parse_line_and_column_aware() {
        assert_eq!(parse_goto("f.rs:3:4"), Ok(Some(("f.rs".to_string(), Some((3, 4))))));
        assert_eq!(parse_goto("f.rs"), Ok(Some(("f.rs".to_string(), None))));
        assert_eq!(parse_goto("f.rs:9"), Ok(Some(("f.rs".to_string(), Some((9, 1))))));
        assert_eq!(parse_goto("a:b:2"), Ok(Some(("a:b".to_string(), Some((2, 1))))));
        assert_eq!(parse_goto(":3"), Ok(None));
        assert_eq!(parse_goto("f.rs:12x"), Err("f.rs".to_string()));
    }

    #[test]
    fn goto_uses_an_existing_file_as_it_is_even_when_it_ends_in_a_number() {
        let cwd = tmp("goto-literal");
        fs::write(cwd.join("report:2024"), "x\n").unwrap();
        fs::write(cwd.join("report"), "x\n").unwrap();
        let a = parse(["-g", "report:2024"], &cwd);
        assert_eq!(a.files, vec![file(cwd.join("report:2024"), false)]);
    }

    #[test]
    fn goto_with_a_malformed_suffix_reports_the_argument_and_creates_nothing() {
        let cwd = tmp("goto-bad");
        fs::create_dir_all(cwd.join("src")).unwrap();
        fs::write(cwd.join("src/main.rs"), "x\n").unwrap();
        fs::write(cwd.join("f.rs"), "x\n").unwrap();
        for arg in ["src/main.rs:12x", "f.rs:-3", "f.rs:3:4x", "f.rs:3x:4"] {
            let a = parse(["-g", arg], &cwd);
            assert!(a.files.is_empty(), "{arg} opens nothing");
            assert_eq!(a.missing, vec![cwd.join(arg)], "{arg} is reported as given");
        }
    }

    #[test]
    fn goto_opens_a_new_name_with_a_colon_and_digits_when_no_file_is_in_front() {
        // Review finding: `log:2024-01-05.txt` is a plain new name, not a mistyped position.
        let cwd = tmp("goto-dated");
        for arg in ["log:2024-01-05.txt", "meeting:10am.md"] {
            let a = parse(["-g", arg], &cwd);
            assert_eq!(a.files, vec![file(cwd.join(arg), true)], "{arg}");
            assert!(a.missing.is_empty(), "{arg}");
        }
    }

    #[test]
    fn goto_keeps_a_name_with_colons_and_no_number_at_the_end() {
        let cwd = tmp("goto-colons");
        let a = parse(["-g", "a:1:b.txt"], &cwd);
        assert_eq!(a.files, vec![file(cwd.join("a:1:b.txt"), true)]);
        assert_eq!(parse_goto("a:1:b.txt"), Ok(Some(("a:1:b.txt".to_string(), None))));
    }

    #[test]
    fn goto_line_or_column_zero_is_clamped_to_1() {
        let cwd = tmp("goto-zero");
        fs::write(cwd.join("f.rs"), "x\n").unwrap();
        assert_eq!(parse(["-g", "f.rs:0"], &cwd).files, vec![at(cwd.join("f.rs"), false, 1, 1)]);
        assert_eq!(parse(["-g", "f.rs:5:0"], &cwd).files, vec![at(cwd.join("f.rs"), false, 5, 1)]);
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
