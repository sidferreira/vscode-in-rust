//! Pull requests on top of the `gh` CLI. No UI and no async: every call blocks, the app runs them
//! on a background executor. A [`GhRunner`] does the process spawning, so tests inject a
//! [`fake::FakeRunner`] with canned output.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

pub mod error;
pub mod fake;
pub mod models;
pub mod runner;

use std::path::PathBuf;
use std::sync::Arc;

use serde::de::DeserializeOwned;

pub use error::GhError;
pub use models::{
    Author, Check, CheckOutcome, ChecksSummary, NewPr, PrDetails, PrFile, PrFilter, PrState, PrSummary, Repo, Review,
};
pub use runner::{FALLBACK_GH_PATHS, GhRunner, Output, SpawnError, SystemRunner, Tool, locate_gh, locate_gh_in};

const LIST_FIELDS: &str = "number,title,author,headRefName,baseRefName,state,isDraft,url,updatedAt,reviewDecision";
const VIEW_FIELDS: &str = "number,title,body,author,state,isDraft,headRefName,baseRefName,url,files,statusCheckRollup,reviews,comments,mergeable";
/// How many pull requests a list asks for.
pub const LIST_LIMIT: u32 = 50;

/// What the Pull Requests view shows: the repository and its three lists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snapshot {
    pub repo: Repo,
    pub open: Vec<PrSummary>,
    pub created_by_me: Vec<PrSummary>,
    pub review_requested: Vec<PrSummary>,
}

/// Where the current branch stands for creating a pull request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchInfo {
    pub branch: String,
    pub has_upstream: bool,
    pub last_commit_subject: String,
}

/// A repository folder and the runner that serves it.
#[derive(Clone)]
pub struct Github {
    runner: Arc<dyn GhRunner>,
    cwd: PathBuf,
}

impl Github {
    pub fn new(runner: Arc<dyn GhRunner>, cwd: PathBuf) -> Self {
        Self { runner, cwd }
    }

    fn exec(&self, tool: Tool, args: &[&str]) -> Result<String, GhError> {
        let args: Vec<String> = args.iter().map(|s| s.to_string()).collect();
        let command = format!("{} {}", if tool == Tool::Gh { "gh" } else { "git" }, args.join(" "));
        let out = self.runner.run(tool, &args, &self.cwd)?;
        if out.success { Ok(out.stdout) } else { Err(error::classify(&command, &out)) }
    }

    fn json<T: DeserializeOwned>(&self, args: &[&str]) -> Result<T, GhError> {
        let text = self.exec(Tool::Gh, args)?;
        serde_json::from_str(&text)
            .map_err(|e| GhError::Parse { command: format!("gh {}", args.join(" ")), message: e.to_string() })
    }

    /// `gh auth status`: `Ok` when signed in to a host.
    pub fn auth_status(&self) -> Result<(), GhError> {
        self.exec(Tool::Gh, &["auth", "status"]).map(|_| ())
    }

    /// The repository of the folder.
    pub fn repo(&self) -> Result<Repo, GhError> {
        self.json(&["repo", "view", "--json", "nameWithOwner,defaultBranchRef,url"])
    }

    pub fn list_prs(&self, filter: PrFilter) -> Result<Vec<PrSummary>, GhError> {
        let limit = LIST_LIMIT.to_string();
        let mut args = vec!["pr", "list", "--state", "open", "--limit", &limit, "--json", LIST_FIELDS];
        match filter {
            PrFilter::Open => {}
            PrFilter::CreatedByMe => args.extend(["--author", "@me"]),
            PrFilter::ReviewRequested => args.extend(["--search", "review-requested:@me"]),
        }
        self.json(&args)
    }

    pub fn pr_details(&self, number: u64) -> Result<PrDetails, GhError> {
        let n = number.to_string();
        self.json(&["pr", "view", &n, "--json", VIEW_FIELDS])
    }

    /// Creates the pull request; returns its URL.
    pub fn create_pr(&self, pr: &NewPr) -> Result<String, GhError> {
        let mut args = vec!["pr", "create", "--title", &pr.title, "--body", &pr.body, "--base", &pr.base, "--head", &pr.head];
        if pr.draft {
            args.push("--draft");
        }
        Ok(self.exec(Tool::Gh, &args)?.trim().to_string())
    }

    /// `gh pr checkout N`.
    pub fn checkout(&self, number: u64) -> Result<(), GhError> {
        let n = number.to_string();
        self.exec(Tool::Gh, &["pr", "checkout", &n]).map(|_| ())
    }

    /// `gh pr view N --web`: gh opens the browser.
    pub fn open_in_browser(&self, number: u64) -> Result<(), GhError> {
        let n = number.to_string();
        self.exec(Tool::Gh, &["pr", "view", &n, "--web"]).map(|_| ())
    }

    /// Signed in, then the repository and the three lists. The first failure is the answer.
    pub fn snapshot(&self) -> Result<Snapshot, GhError> {
        self.auth_status()?;
        let repo = self.repo()?;
        Ok(Snapshot {
            repo,
            open: self.list_prs(PrFilter::Open)?,
            created_by_me: self.list_prs(PrFilter::CreatedByMe)?,
            review_requested: self.list_prs(PrFilter::ReviewRequested)?,
        })
    }

    // ---- git, for creating a pull request ------------------------------------------------------

    /// The branch, whether it has an upstream and the last commit's subject.
    pub fn branch_info(&self) -> Result<BranchInfo, GhError> {
        let branch = self.exec(Tool::Git, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
        let has_upstream = self.exec(Tool::Git, &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).is_ok();
        let last_commit_subject = self.exec(Tool::Git, &["log", "-1", "--format=%s"]).map(|s| s.trim().to_string()).unwrap_or_default();
        Ok(BranchInfo { branch, has_upstream, last_commit_subject })
    }

    /// `git push -u origin HEAD`. Only ever called after the user confirmed.
    pub fn push_upstream(&self) -> Result<(), GhError> {
        self.exec(Tool::Git, &["push", "-u", "origin", "HEAD"]).map(|_| ())
    }
}

#[cfg(test)]
mod tests;
