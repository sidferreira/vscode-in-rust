//! Running `gh` and `git`: locating the binary, the [`GhRunner`] trait the rest of the crate and
//! the app's tests go through, and the real runner that spawns a process.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Which program a call runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tool {
    Gh,
    Git,
}

/// What a finished process left behind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self { success: true, code: Some(0), stdout: stdout.into(), stderr: String::new() }
    }

    pub fn fail(code: i32, stderr: impl Into<String>) -> Self {
        Self { success: false, code: Some(code), stdout: String::new(), stderr: stderr.into() }
    }
}

/// The program could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnError {
    /// There is no such program.
    NotFound,
    Io(String),
}

/// Runs a tool to completion, blocking. The app calls it off the UI thread; tests inject fakes.
pub trait GhRunner: Send + Sync {
    fn run(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<Output, SpawnError>;
}

/// The places `gh` is searched when the GUI was launched with a short `PATH`.
pub const FALLBACK_GH_PATHS: [&str; 2] = ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"];

/// Finds `gh`: the `RSCODE_GH` override (taken as is, even when it does not exist, so a wrong
/// override is an error rather than a silent fallback), else the first `gh` file on `path`, else
/// the fallback locations.
pub fn locate_gh_in(env_override: Option<OsString>, path: Option<OsString>, fallbacks: &[PathBuf]) -> Option<PathBuf> {
    if let Some(p) = env_override.filter(|p| !p.is_empty()) {
        return Some(PathBuf::from(p));
    }
    if let Some(path) = path {
        for dir in std::env::split_paths(&path) {
            let candidate = dir.join("gh");
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    fallbacks.iter().find(|p| p.is_file()).cloned()
}

/// [`locate_gh_in`] with the process environment and the standard fallbacks.
pub fn locate_gh() -> Option<PathBuf> {
    let fallbacks: Vec<PathBuf> = FALLBACK_GH_PATHS.iter().map(PathBuf::from).collect();
    locate_gh_in(std::env::var_os("RSCODE_GH"), std::env::var_os("PATH"), &fallbacks)
}

/// Spawns the real `gh` (at `gh`, or none when it was not found) and `git` (from `PATH`).
#[derive(Debug, Clone)]
pub struct SystemRunner {
    gh: Option<PathBuf>,
}

impl SystemRunner {
    pub fn new(gh: Option<PathBuf>) -> Self {
        Self { gh }
    }

    /// Looks `gh` up with [`locate_gh`].
    pub fn locate() -> Self {
        Self::new(locate_gh())
    }
}

impl GhRunner for SystemRunner {
    fn run(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<Output, SpawnError> {
        let program = match tool {
            Tool::Gh => self.gh.clone().ok_or(SpawnError::NotFound)?,
            Tool::Git => PathBuf::from("git"),
        };
        let out = Command::new(program)
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            // Never wait on a prompt, a pager or colour codes: nobody can answer them.
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_PAGER", "cat")
            .env("NO_COLOR", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .output()
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { SpawnError::NotFound } else { SpawnError::Io(e.to_string()) })?;
        Ok(Output {
            success: out.status.success(),
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = std::env::temp_dir().join(format!("vsr-github-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_env_override_wins_even_when_it_does_not_exist() {
        let found = locate_gh_in(Some("/nope/gh".into()), Some("/usr/bin".into()), &[]);
        assert_eq!(found, Some(PathBuf::from("/nope/gh")));
    }

    #[test]
    fn an_empty_override_is_ignored() {
        let dir = scratch("empty-override");
        std::fs::write(dir.join("gh"), "").unwrap();
        let found = locate_gh_in(Some("".into()), Some(dir.clone().into_os_string()), &[]);
        assert_eq!(found, Some(dir.join("gh")));
    }

    #[test]
    fn path_entries_are_searched_in_order() {
        let (a, b) = (scratch("path-a"), scratch("path-b"));
        std::fs::write(b.join("gh"), "").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(locate_gh_in(None, Some(path), &[]), Some(b.join("gh")));
    }

    #[test]
    fn the_fallbacks_are_used_when_path_has_no_gh() {
        let (empty, fallback) = (scratch("fb-empty"), scratch("fb"));
        std::fs::write(fallback.join("gh"), "").unwrap();
        let found = locate_gh_in(None, Some(empty.into_os_string()), &[PathBuf::from("/nope/gh"), fallback.join("gh")]);
        assert_eq!(found, Some(fallback.join("gh")));
    }

    #[test]
    fn nothing_found_is_none() {
        assert_eq!(locate_gh_in(None, None, &[PathBuf::from("/nope/gh")]), None);
    }

    #[test]
    fn a_missing_gh_is_not_found() {
        let r = SystemRunner::new(None);
        assert_eq!(r.run(Tool::Gh, &[], Path::new("/")), Err(SpawnError::NotFound));
        let r = SystemRunner::new(Some(PathBuf::from("/nope/does/not/exist")));
        assert_eq!(r.run(Tool::Gh, &[], Path::new("/")), Err(SpawnError::NotFound));
    }
}
