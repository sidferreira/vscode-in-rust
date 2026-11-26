//! Running `gh` and `git`: locating the binary, the [`GhRunner`] trait the rest of the crate and
//! the app's tests go through, and the real runner that spawns a process.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::io::Read;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
    /// More than the capture limit was written to stdout or stderr and the rest was dropped.
    pub truncated: bool,
}

impl Output {
    pub fn ok(stdout: impl Into<String>) -> Self {
        Self { success: true, code: Some(0), stdout: stdout.into(), stderr: String::new(), truncated: false }
    }

    pub fn fail(code: i32, stderr: impl Into<String>) -> Self {
        Self { success: false, code: Some(code), stdout: String::new(), stderr: stderr.into(), truncated: false }
    }
}

/// The program could not be started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpawnError {
    /// There is no such program.
    NotFound,
    /// `RSCODE_GH` is set to something unusable; carries the message.
    BadOverride(String),
    /// It ran past its time limit and was killed.
    Timeout,
    Io(String),
}

/// Runs a tool to completion, blocking. The app calls it off the UI thread; tests inject fakes.
pub trait GhRunner: Send + Sync {
    fn run(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<Output, SpawnError>;

    /// Starts a tool that outlives the call (`gh pr view --web` hands over to the browser) and
    /// returns without waiting for it. The default just runs it, which is what fakes want.
    fn spawn_detached(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<(), SpawnError> {
        self.run(tool, args, cwd).map(|_| ())
    }
}

/// The places `gh` is searched when the GUI was launched with a short `PATH`.
pub const FALLBACK_GH_PATHS: [&str; 2] = ["/opt/homebrew/bin/gh", "/usr/local/bin/gh"];
/// The same for `git`.
pub const FALLBACK_GIT_PATHS: [&str; 3] = ["/usr/bin/git", "/opt/homebrew/bin/git", "/usr/local/bin/git"];

/// How long a call may run before it is killed.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);
/// The limit for calls that move data (`gh pr checkout`, `git push`).
pub const LONG_TIMEOUT: Duration = Duration::from_secs(600);
/// How much of each of stdout and stderr is kept; the rest is read and dropped.
pub const MAX_CAPTURE: usize = 16 * 1024 * 1024;
/// How long a detached call (`gh pr view --web`) may keep running before it is killed.
pub const DETACHED_LIMIT: Duration = Duration::from_secs(30);

/// Finds `name` on `path`, then in `fallbacks`. Only absolute `PATH` entries count (an empty or
/// relative entry means the current directory, where a repository could plant its own program),
/// and the result is always an absolute path.
fn locate_in(name: &str, path: Option<OsString>, fallbacks: &[PathBuf]) -> Option<PathBuf> {
    if let Some(path) = path {
        for dir in std::env::split_paths(&path) {
            if !dir.is_absolute() {
                continue;
            }
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    fallbacks.iter().find(|p| p.is_absolute() && p.is_file()).cloned()
}

/// Finds `gh`: the `RSCODE_GH` override (taken as is when absolute, even when it does not exist, so
/// a wrong override is an error rather than a silent fallback; a relative one is an error too, as
/// it would depend on the current directory), else the first `gh` file in an absolute `path`
/// entry, else the fallback locations.
pub fn locate_gh_in(env_override: Option<OsString>, path: Option<OsString>, fallbacks: &[PathBuf]) -> Result<Option<PathBuf>, String> {
    if let Some(p) = env_override.filter(|p| !p.is_empty()).map(PathBuf::from) {
        return if p.is_absolute() {
            Ok(Some(p))
        } else {
            Err(format!("RSCODE_GH must be an absolute path (it is {:?})", p.display().to_string()))
        };
    }
    Ok(locate_in("gh", path, fallbacks))
}

/// [`locate_gh_in`] with the process environment and the standard fallbacks.
pub fn locate_gh() -> Result<Option<PathBuf>, String> {
    let fallbacks: Vec<PathBuf> = FALLBACK_GH_PATHS.iter().map(PathBuf::from).collect();
    locate_gh_in(std::env::var_os("RSCODE_GH"), std::env::var_os("PATH"), &fallbacks)
}

/// Finds `git` in an absolute `path` entry, else the fallback locations.
pub fn locate_git_in(path: Option<OsString>, fallbacks: &[PathBuf]) -> Option<PathBuf> {
    locate_in("git", path, fallbacks)
}

/// [`locate_git_in`] with the process environment and the standard fallbacks.
pub fn locate_git() -> Option<PathBuf> {
    let fallbacks: Vec<PathBuf> = FALLBACK_GIT_PATHS.iter().map(PathBuf::from).collect();
    locate_git_in(std::env::var_os("PATH"), &fallbacks)
}

/// Spawns the real `gh` and `git`, each by the absolute path it was found at (never by bare name,
/// so a program in the repository folder cannot run in their place).
#[derive(Debug, Clone)]
pub struct SystemRunner {
    gh: Option<PathBuf>,
    git: Option<PathBuf>,
    /// Why `gh` cannot be used although it was configured (a relative `RSCODE_GH`).
    gh_error: Option<String>,
    timeout: Duration,
    long_timeout: Duration,
    detached_limit: Duration,
    max_capture: usize,
}

impl SystemRunner {
    /// `gh` at the given path (or none when it was not found); `git` is looked up with [`locate_git`].
    pub fn new(gh: Option<PathBuf>) -> Self {
        Self::with_tools(gh, locate_git())
    }

    pub fn with_tools(gh: Option<PathBuf>, git: Option<PathBuf>) -> Self {
        Self { gh, git, gh_error: None, timeout: DEFAULT_TIMEOUT, long_timeout: LONG_TIMEOUT, detached_limit: DETACHED_LIMIT, max_capture: MAX_CAPTURE }
    }

    /// Looks `gh` and `git` up.
    pub fn locate() -> Self {
        match locate_gh() {
            Ok(gh) => Self::new(gh),
            Err(message) => Self::new(None).with_gh_error(message),
        }
    }

    /// Makes every `gh` call fail with this message (an unusable `RSCODE_GH`).
    pub fn with_gh_error(mut self, message: String) -> Self {
        self.gh_error = Some(message);
        self
    }

    /// Overrides how long a detached call may run before it is killed.
    pub fn with_detached_limit(mut self, limit: Duration) -> Self {
        self.detached_limit = limit;
        self
    }

    /// Whether a `gh` was found.
    pub fn has_gh(&self) -> bool {
        self.gh.is_some()
    }

    /// Overrides the time and size limits.
    pub fn with_limits(mut self, timeout: Duration, max_capture: usize) -> Self {
        self.timeout = timeout;
        self.long_timeout = timeout;
        self.max_capture = max_capture;
        self
    }

    fn program(&self, tool: Tool) -> Result<&Path, SpawnError> {
        if let (Tool::Gh, Some(message)) = (tool, &self.gh_error) {
            return Err(SpawnError::BadOverride(message.clone()));
        }
        let found = match tool {
            Tool::Gh => self.gh.as_deref(),
            Tool::Git => self.git.as_deref(),
        };
        found.ok_or(SpawnError::NotFound)
    }

    fn command(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<Command, SpawnError> {
        let mut cmd = Command::new(self.program(tool)?);
        cmd.args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            // Never wait on a prompt, a pager or colour codes: nobody can answer them.
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_PAGER", "cat")
            .env("NO_COLOR", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            // Its own process group, so a timeout can stop the grandchildren too (`gh pr checkout` runs git).
            .process_group(0);
        Ok(cmd)
    }

    fn timeout_for(&self, tool: Tool, args: &[String]) -> Duration {
        let moves_data = match tool {
            Tool::Gh => args.starts_with(&["pr".to_string(), "checkout".to_string()]),
            Tool::Git => args.first().is_some_and(|a| a == "push"),
        };
        if moves_data { self.long_timeout } else { self.timeout }
    }
}

fn spawn_failure(e: std::io::Error) -> SpawnError {
    if e.kind() == std::io::ErrorKind::NotFound { SpawnError::NotFound } else { SpawnError::Io(e.to_string()) }
}

/// Reads `r` to the end, keeping at most `cap` bytes and dropping the rest (so the child never
/// blocks on a full pipe). The flag says whether anything was dropped.
fn read_capped(mut r: impl Read, cap: usize) -> (Vec<u8>, bool) {
    let mut kept = Vec::new();
    let mut truncated = false;
    let mut buf = [0u8; 16 * 1024];
    loop {
        match r.read(&mut buf) {
            Ok(0) | Err(_) => return (kept, truncated),
            Ok(n) => {
                let room = cap.saturating_sub(kept.len());
                truncated |= n > room;
                kept.extend_from_slice(&buf[..n.min(room)]);
            }
        }
    }
}

type Captured = (Vec<u8>, bool);

fn reader(pipe: Option<impl Read + Send + 'static>, cap: usize) -> mpsc::Receiver<Captured> {
    let (tx, rx) = mpsc::channel();
    match pipe {
        Some(p) => {
            std::thread::spawn(move || {
                let _ = tx.send(read_capped(p, cap));
            });
        }
        None => {
            let _ = tx.send((Vec::new(), false));
        }
    }
    rx
}

/// How long the process group gets to exit after SIGTERM before SIGKILL.
const KILL_GRACE: Duration = Duration::from_millis(300);

/// Stops the child and everything it started: SIGTERM to its process group, SIGKILL after a short
/// grace, then reaps the child. The readers end by themselves once the group is gone and the pipes
/// close.
fn kill_group(child: &mut Child) {
    let pgid = child.id() as libc::pid_t;
    // SAFETY: killpg only sends a signal; the group was created by this child (process_group(0)).
    unsafe { libc::killpg(pgid, libc::SIGTERM) };
    let until = Instant::now() + KILL_GRACE;
    while Instant::now() < until && !matches!(child.try_wait(), Ok(Some(_))) {
        std::thread::sleep(Duration::from_millis(5));
    }
    // A grandchild may outlive the child that was reaped above: the group id stays valid while any member lives.
    unsafe { libc::killpg(pgid, libc::SIGKILL) };
    let _ = child.wait();
}

/// How long to wait for the pipes to close once the child has exited (a grandchild may hold them).
const PIPE_GRACE: Duration = Duration::from_secs(2);

impl GhRunner for SystemRunner {
    fn run(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<Output, SpawnError> {
        let mut child = self.command(tool, args, cwd)?.stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().map_err(spawn_failure)?;
        let out_rx = reader(child.stdout.take(), self.max_capture);
        let err_rx = reader(child.stderr.take(), self.max_capture);
        let deadline = Instant::now() + self.timeout_for(tool, args);
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() >= deadline => {
                    kill_group(&mut child);
                    return Err(SpawnError::Timeout);
                }
                Ok(None) => std::thread::sleep(Duration::from_millis(5)),
                Err(e) => {
                    kill_group(&mut child);
                    return Err(SpawnError::Io(e.to_string()));
                }
            }
        };
        let take = |rx: mpsc::Receiver<Captured>| rx.recv_timeout(PIPE_GRACE).unwrap_or_default();
        let ((stdout, out_cut), (stderr, err_cut)) = (take(out_rx), take(err_rx));
        Ok(Output {
            success: status.success(),
            code: status.code(),
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
            truncated: out_cut || err_cut,
        })
    }

    fn spawn_detached(&self, tool: Tool, args: &[String], cwd: &Path) -> Result<(), SpawnError> {
        let mut child = self.command(tool, args, cwd)?.stdout(Stdio::null()).stderr(Stdio::piped()).spawn().map_err(spawn_failure)?;
        let stderr = reader(child.stderr.take(), 64 * 1024);
        // A failure to even start (not signed in, no such pull request) shows up at once.
        let grace = Instant::now() + Duration::from_millis(1500);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return Ok(()),
                Ok(Some(status)) => {
                    let text = String::from_utf8_lossy(&stderr.recv_timeout(PIPE_GRACE).unwrap_or_default().0).trim().to_string();
                    return Err(SpawnError::Io(if text.is_empty() { format!("{tool:?} exited with {:?}", status.code()) } else { text }));
                }
                Ok(None) if Instant::now() < grace => std::thread::sleep(Duration::from_millis(5)),
                Ok(None) => break,
                Err(e) => {
                    kill_group(&mut child);
                    return Err(SpawnError::Io(e.to_string()));
                }
            }
        }
        // Still running: let it go, reap it when it ends, and stop it (and what it started) when it
        // runs past the limit.
        let deadline = Instant::now() + self.detached_limit;
        std::thread::spawn(move || {
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => return,
                    Ok(None) if Instant::now() >= deadline => return kill_group(&mut child),
                    Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                    Err(_) => return kill_group(&mut child),
                }
            }
        });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
        let dir = vsr_test_scratch::dir!().join(format!("vsr-github-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn the_env_override_wins_even_when_it_does_not_exist() {
        let found = locate_gh_in(Some("/nope/gh".into()), Some("/usr/bin".into()), &[]).unwrap();
        assert_eq!(found, Some(PathBuf::from("/nope/gh")));
    }

    #[test]
    fn an_empty_override_is_ignored() {
        let dir = scratch("empty-override");
        std::fs::write(dir.join("gh"), "").unwrap();
        let found = locate_gh_in(Some("".into()), Some(dir.clone().into_os_string()), &[]).unwrap();
        assert_eq!(found, Some(dir.join("gh")));
    }

    #[test]
    fn path_entries_are_searched_in_order() {
        let (a, b) = (scratch("path-a"), scratch("path-b"));
        std::fs::write(b.join("gh"), "").unwrap();
        let path = std::env::join_paths([&a, &b]).unwrap();
        assert_eq!(locate_gh_in(None, Some(path), &[]).unwrap(), Some(b.join("gh")));
    }

    #[test]
    fn the_fallbacks_are_used_when_path_has_no_gh() {
        let (empty, fallback) = (scratch("fb-empty"), scratch("fb"));
        std::fs::write(fallback.join("gh"), "").unwrap();
        let found = locate_gh_in(None, Some(empty.into_os_string()), &[PathBuf::from("/nope/gh"), fallback.join("gh")]).unwrap();
        assert_eq!(found, Some(fallback.join("gh")));
    }

    #[test]
    fn nothing_found_is_none() {
        assert_eq!(locate_gh_in(None, None, &[PathBuf::from("/nope/gh")]).unwrap(), None);
    }

    #[test]
    fn relative_and_empty_path_entries_are_skipped() {
        let repo = scratch("relative");
        std::fs::write(repo.join("gh"), "").unwrap();
        std::fs::write(repo.join("git"), "").unwrap();
        // The same folder as a relative path from wherever the tests run (`..` stops at the root).
        let relative = format!("{}{}", "../".repeat(40), repo.strip_prefix("/").unwrap().display());
        assert!(Path::new(&relative).join("gh").is_file(), "the relative path reaches the planted gh");
        for path in [relative.as_str(), ".", "", "bin"] {
            assert_eq!(locate_gh_in(None, Some(path.into()), &[]).unwrap(), None, "gh on {path:?}");
            assert_eq!(locate_git_in(Some(path.into()), &[]), None, "git on {path:?}");
        }
        let abs = scratch("absolute");
        std::fs::write(abs.join("gh"), "").unwrap();
        let path = std::env::join_paths([PathBuf::from("."), PathBuf::new(), abs.clone()]).unwrap();
        assert_eq!(locate_gh_in(None, Some(path), &[]).unwrap(), Some(abs.join("gh")));
    }

    #[test]
    fn a_relative_override_is_an_error_naming_rscode_gh() {
        let err = locate_gh_in(Some("./gh".into()), None, &[]).unwrap_err();
        assert!(err.contains("RSCODE_GH") && err.contains("absolute"), "{err}");
        // Not a silent fallback to a gh that is on PATH.
        let dir = scratch("relative-override");
        std::fs::write(dir.join("gh"), "").unwrap();
        assert!(locate_gh_in(Some("gh".into()), Some(dir.into_os_string()), &[]).is_err());
    }

    #[test]
    fn a_runner_with_a_bad_override_reports_it_instead_of_running() {
        let err = locate_gh_in(Some("./gh".into()), None, &[]).unwrap_err();
        let r = SystemRunner::with_tools(None, None).with_gh_error(err.clone());
        assert_eq!(r.run(Tool::Gh, &[], Path::new("/")), Err(SpawnError::BadOverride(err)));
    }

    #[test]
    fn read_capped_says_when_it_dropped_bytes() {
        assert_eq!(read_capped(&b"abcdef"[..], 4), (b"abcd".to_vec(), true));
        assert_eq!(read_capped(&b"abcd"[..], 4), (b"abcd".to_vec(), false));
    }

    #[test]
    fn git_is_found_on_path_then_in_the_fallbacks() {
        let (a, b) = (scratch("git-a"), scratch("git-b"));
        std::fs::write(a.join("git"), "").unwrap();
        std::fs::write(b.join("git"), "").unwrap();
        assert_eq!(locate_git_in(Some(a.clone().into_os_string()), &[b.join("git")]), Some(a.join("git")));
        assert_eq!(locate_git_in(None, &[PathBuf::from("/nope/git"), b.join("git")]), Some(b.join("git")));
        assert_eq!(locate_git_in(None, &[PathBuf::from("git")]), None, "a bare name is not a location");
    }

    #[test]
    fn a_missing_gh_is_not_found() {
        let r = SystemRunner::new(None);
        assert_eq!(r.run(Tool::Gh, &[], Path::new("/")), Err(SpawnError::NotFound));
        let r = SystemRunner::new(Some(PathBuf::from("/nope/does/not/exist")));
        assert_eq!(r.run(Tool::Gh, &[], Path::new("/")), Err(SpawnError::NotFound));
    }
}
