//! upstream: src/vs/platform/shell/node/shellEnv.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/electron-browser/localProcessExtensionHost.ts @ 50e1350fc78d
//!
//! The environment the extension host runs with (ADR-0100). An app launched from Finder or the
//! Dock inherits launchd's `PATH`, so a language server started by its bare name (`java`, or `node`
//! from a version manager) is not found. Upstream asks the user's login shell for its environment
//! and merges it under the host's own entries. Here the same: `$SHELL -i -l -c` prints `env -0`
//! between two markers, so rc-file output is discarded. Any failure falls back to the inherited
//! environment: a broken shell configuration must not stop extensions from starting.
//!
//! Unix only (rscode is macOS only, ADR-0019).

use std::io::Read;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

/// Set in the shell's environment while resolving. A resolution started with it set (from inside
/// one, say from a shell rc that starts rscode) is skipped. Removed from the result.
pub const GUARD: &str = "RSCODE_RESOLVING_ENVIRONMENT";

/// `application.shellEnvironmentResolutionTimeout`'s default, in seconds (upstream: 10).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

const POLL: Duration = Duration::from_millis(20);
static MARKS: AtomicU32 = AtomicU32::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellEnvError {
    /// Already resolving in this process tree (the guard is set).
    Guarded,
    /// The shell could not be started.
    Spawn(String),
    /// The shell did not finish in time; it was killed with its process group.
    Timeout,
    /// The shell exited without printing an environment between the markers.
    Failed(String),
}

impl std::fmt::Display for ShellEnvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShellEnvError::Guarded => write!(f, "already resolving ({GUARD} is set)"),
            ShellEnvError::Spawn(e) => write!(f, "could not start the shell: {e}"),
            ShellEnvError::Timeout => write!(f, "the shell did not answer in time"),
            ShellEnvError::Failed(e) => write!(f, "no environment from the shell: {e}"),
        }
    }
}

impl std::error::Error for ShellEnvError {}

/// The login shell: `$SHELL`, else `/bin/sh`.
pub fn default_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .filter(|s| !s.is_empty())
        .map_or_else(|| PathBuf::from("/bin/sh"), PathBuf::from)
}

/// The shell command that prints `mark`, the environment as `env -0` (NUL-separated), then `mark`.
pub fn command(mark: &str) -> String {
    format!("printf '%s' '{mark}'; env -0; printf '%s' '{mark}'")
}

/// A marker no shell output can contain by accident.
fn new_mark() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    format!("rscode-env-{}-{}-{nanos}", std::process::id(), MARKS.fetch_add(1, Ordering::SeqCst))
}

/// The variables between the first two `mark`s in `output`, `None` when there are not two.
/// Entries without a name are dropped, and so is [`GUARD`].
pub fn parse(output: &[u8], mark: &str) -> Option<Vec<(String, String)>> {
    let m = mark.as_bytes();
    let start = find(output, m)? + m.len();
    let end = start + find(&output[start..], m)?;
    let vars = output[start..end]
        .split(|b| *b == 0)
        .filter(|entry| !entry.is_empty())
        .filter_map(|entry| {
            let text = String::from_utf8_lossy(entry);
            let (key, value) = text.split_once('=')?;
            (!key.is_empty()).then(|| (key.to_string(), value.to_string()))
        })
        .filter(|(key, _)| key != GUARD)
        .collect();
    Some(vars)
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Runs `shell -i -l -c <command>` and returns the environment it printed. Gives up after
/// `timeout`, killing the shell and what it started.
pub fn resolve(shell: &Path, timeout: Duration) -> Result<Vec<(String, String)>, ShellEnvError> {
    resolve_with(shell, timeout, std::env::var_os(GUARD).is_some())
}

/// [`resolve`] with the guard given: `guarded` is whether this process is already resolving.
pub fn resolve_with(
    shell: &Path,
    timeout: Duration,
    guarded: bool,
) -> Result<Vec<(String, String)>, ShellEnvError> {
    if guarded {
        return Err(ShellEnvError::Guarded);
    }
    let mark = new_mark();
    let mut child = Command::new(shell)
        .args(["-i", "-l", "-c", &command(&mark)])
        .env(GUARD, "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        // Its own process group, so a timeout kills whatever the rc files started too.
        .process_group(0)
        .spawn()
        .map_err(|e| ShellEnvError::Spawn(e.to_string()))?;
    let (tx, rx) = channel();
    if let Some(out) = child.stdout.take() {
        read_to(out, tx);
    }
    let stderr_rx = child.stderr.take().map(|err| {
        let (etx, erx) = channel();
        read_to(err, etx);
        erx
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => return Err(ShellEnvError::Spawn(e.to_string())),
        }
        if Instant::now() >= deadline {
            kill_group(child.id());
            let _ = child.kill();
            let _ = child.wait();
            return Err(ShellEnvError::Timeout);
        }
        std::thread::sleep(POLL);
    };
    let stdout = receive(&rx);
    let stderr = stderr_rx.map(|r| receive(&r)).unwrap_or_default();
    match parse(&stdout, &mark) {
        Some(vars) if status.success() => Ok(vars),
        _ => Err(ShellEnvError::Failed(format!(
            "{status}; stderr: {}",
            String::from_utf8_lossy(&stderr).trim()
        ))),
    }
}

/// Reads a pipe to its end on a thread, then sends the bytes.
fn read_to(mut pipe: impl Read + Send + 'static, tx: std::sync::mpsc::Sender<Vec<u8>>) {
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
}

/// What the reader sent, or nothing if it has not finished within a second (a grandchild still
/// holds the pipe).
fn receive(rx: &Receiver<Vec<u8>>) -> Vec<u8> {
    rx.recv_timeout(Duration::from_secs(1)).unwrap_or_default()
}

/// `kill -KILL -- -<pgid>`: the shell leads its own group (`process_group(0)`).
fn kill_group(pgid: u32) {
    let _ = Command::new("kill")
        .args(["-KILL", "--", &format!("-{pgid}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

/// What the host adds to its inherited environment: the resolved login-shell variables, or
/// nothing when resolution fails (the reason is printed to stderr; the inherited one is used).
pub fn for_host(shell: &Path, timeout: Duration) -> Vec<(String, String)> {
    match resolve(shell, timeout) {
        Ok(vars) => vars,
        Err(ShellEnvError::Guarded) => Vec::new(),
        Err(e) => {
            eprintln!("rscode: shell environment not resolved ({e}); using the inherited one");
            Vec::new()
        }
    }
}
