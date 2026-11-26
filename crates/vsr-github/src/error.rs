//! What can go wrong calling `gh`, in the terms the UI shows.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::fmt;

use crate::runner::{Output, SpawnError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GhError {
    /// There is no `gh` to run.
    GhMissing,
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
            GhError::NotSignedIn(_) => "Run `gh auth login`".into(),
            GhError::NotGitHubRepo(_) => "This folder is not a GitHub repository".into(),
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
            GhError::NotSignedIn(m) => write!(f, "gh is not signed in: {m}"),
            GhError::NotGitHubRepo(m) => write!(f, "not a GitHub repository: {m}"),
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
            SpawnError::Io(m) => GhError::Io(m),
        }
    }
}

/// Sorts a failed run by what gh said.
pub(crate) fn classify(command: &str, out: &Output) -> GhError {
    let text = out.stderr.trim();
    let lower = text.to_lowercase();
    if lower.contains("not a git repository")
        || lower.contains("none of the git remotes")
        || lower.contains("could not resolve to a repository")
        || lower.contains("no git remotes found")
    {
        return GhError::NotGitHubRepo(text.to_string());
    }
    if lower.contains("gh auth login") || lower.contains("not logged in") || lower.contains("authentication required") || lower.contains("bad credentials") {
        return GhError::NotSignedIn(text.to_string());
    }
    let message = if text.is_empty() { out.stdout.trim().to_string() } else { text.to_string() };
    GhError::Failed { command: command.to_string(), code: out.code, message }
}
