//! What can go wrong calling `gh`, in the terms the UI shows.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::fmt;

use crate::runner::{Output, SpawnError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhError {
    /// There is no `gh` to run.
    GhMissing,
    /// There is no `git` to run.
    GitMissing,
    /// A call ran past its time limit and was stopped.
    Timeout { command: String },
    /// `gh` is installed but not signed in; carries gh's own text.
    NotSignedIn(String),
    /// The folder is not a git repository with a GitHub remote; carries gh's own text.
    NotGitHubRepo(String),
    /// `gh` (or `git`) ran and failed.
    Failed { command: String, code: Option<i32>, message: String },
    /// Its output was not what the models expect.
    Parse { command: String, message: String },
    /// The process could not be started for another reason.
    Io(String),
}

impl GhError {
    /// A one-line text for the UI.
    pub fn headline(&self) -> String {
        match self {
            GhError::GhMissing => "Install GitHub CLI".into(),
            GhError::GitMissing => "Install Git".into(),
            GhError::Timeout { command } => format!("`{command}` timed out"),
            GhError::NotSignedIn(_) => "Run `gh auth login`".into(),
            GhError::NotGitHubRepo(_) => "Could not find this repository on GitHub (not a GitHub repository, or no access)".into(),
            GhError::Failed { message, .. } => message.clone(),
            GhError::Parse { command, .. } => format!("Unexpected output from `{command}`"),
            GhError::Io(m) => m.clone(),
        }
    }
}

impl fmt::Display for GhError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GhError::GhMissing => write!(f, "the GitHub CLI (gh) was not found"),
            GhError::GitMissing => write!(f, "git was not found"),
            GhError::Timeout { command } => write!(f, "`{command}` did not finish in time and was stopped"),
            GhError::NotSignedIn(m) => write!(f, "gh is not signed in: {m}"),
            GhError::NotGitHubRepo(m) => write!(f, "repository not found on GitHub (not a GitHub repository, or no access): {m}"),
            GhError::Failed { command, code, message } => match code {
                Some(c) => write!(f, "`{command}` failed ({c}): {message}"),
                None => write!(f, "`{command}` was killed: {message}"),
            },
            GhError::Parse { command, message } => write!(f, "cannot read the output of `{command}`: {message}"),
            GhError::Io(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for GhError {}

impl From<SpawnError> for GhError {
    fn from(e: SpawnError) -> Self {
        match e {
            SpawnError::NotFound => GhError::GhMissing,
            SpawnError::Timeout => GhError::Timeout { command: String::new() },
            SpawnError::Io(m) => GhError::Io(m),
        }
    }
}

/// Sorts a failed run by what gh said.
pub(crate) fn classify(command: &str, out: &Output) -> GhError {
    let default_host = std::env::var("GH_HOST").ok().filter(|h| !h.is_empty()).unwrap_or_else(|| "github.com".into());
    classify_with(command, out, &default_host)
}

fn classify_with(command: &str, out: &Output, default_host: &str) -> GhError {
    let text = out.stderr.trim();
    let lower = text.to_lowercase();
    if lower.contains("not a git repository")
        || lower.contains("none of the git remotes")
        || lower.contains("could not resolve to a repository")
        || lower.contains("no git remotes found")
    {
        return GhError::NotGitHubRepo(text.to_string());
    }
    let signed_out = if command == "gh auth status" { auth_status_says_signed_out(&lower, default_host) } else { says_log_in(&lower) };
    if signed_out {
        return GhError::NotSignedIn(text.to_string());
    }
    let message = if text.is_empty() { out.stdout.trim().to_string() } else { text.to_string() };
    GhError::Failed { command: command.to_string(), code: out.code, message }
}

/// gh's hints that the host in use has no usable login.
fn says_log_in(lower: &str) -> bool {
    lower.contains("gh auth login") || lower.contains("not logged in") || lower.contains("authentication required") || lower.contains("bad credentials")
}

/// `gh auth status` exits non-zero when any configured host is broken, and then prints `gh auth
/// login -h <host>` for it, so the hint alone says nothing about the host in use. Signed out means:
/// no host is logged in; the default host (`GH_HOST`, else github.com) has a bad token; or no host
/// has a good login at all and gh points at `gh auth login`.
fn auth_status_says_signed_out(lower: &str, default_host: &str) -> bool {
    if lower.contains("not logged into any") {
        return true;
    }
    if host_block(lower, &default_host.to_lowercase()).is_some_and(|b| b.contains("failed to log in") || b.contains("invalid") || b.contains("bad credentials")) {
        return true;
    }
    let some_host_is_fine = lower.contains("logged in to ");
    !some_host_is_fine && says_log_in(lower)
}

/// The lines of one host's section of `gh auth status` (a header line with the host alone, then
/// indented lines).
fn host_block(lower: &str, host: &str) -> Option<String> {
    let mut block: Option<Vec<&str>> = None;
    for line in lower.lines() {
        let header = !line.is_empty() && !line.starts_with(char::is_whitespace);
        if header {
            if let Some(b) = block.take() {
                return Some(b.join("\n"));
            }
            if line.trim() == host {
                block = Some(Vec::new());
            }
        } else if let Some(b) = block.as_mut() {
            b.push(line);
        }
    }
    block.map(|b| b.join("\n"))
}
