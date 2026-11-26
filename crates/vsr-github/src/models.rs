//! Typed views of `gh ... --json` output. Unknown fields are ignored and missing ones default, so
//! a newer gh does not break the parse.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use serde::Deserialize;

/// `gh repo view --json nameWithOwner,defaultBranchRef,url`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Repo {
    pub name_with_owner: String,
    #[serde(default)]
    pub default_branch_ref: Option<BranchRef>,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct BranchRef {
    pub name: String,
}

impl Repo {
    /// The default branch, `main` when gh did not say.
    pub fn default_branch(&self) -> &str {
        self.default_branch_ref.as_ref().map_or("main", |b| b.name.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Author {
    #[serde(default)]
    pub login: String,
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PrState {
    Open,
    Closed,
    Merged,
}

impl PrState {
    pub fn label(self) -> &'static str {
        match self {
            PrState::Open => "Open",
            PrState::Closed => "Closed",
            PrState::Merged => "Merged",
        }
    }
}

/// One row of `gh pr list`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrSummary {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub head_ref_name: String,
    #[serde(default)]
    pub base_ref_name: String,
    pub state: PrState,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub updated_at: String,
    /// `APPROVED`, `CHANGES_REQUESTED`, `REVIEW_REQUIRED` or empty.
    #[serde(default)]
    pub review_decision: Option<String>,
}

impl PrSummary {
    pub fn author_login(&self) -> &str {
        self.author.as_ref().map_or("", |a| a.login.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PrFile {
    pub path: String,
    #[serde(default)]
    pub additions: u64,
    #[serde(default)]
    pub deletions: u64,
}

/// An entry of `statusCheckRollup`: a `CheckRun` (Actions and apps) or a `StatusContext`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub context: Option<String>,
    /// `CheckRun`: `COMPLETED`, `IN_PROGRESS`, `QUEUED`...
    #[serde(default)]
    pub status: Option<String>,
    /// `CheckRun`: `SUCCESS`, `FAILURE`, `NEUTRAL`, `SKIPPED`...
    #[serde(default)]
    pub conclusion: Option<String>,
    /// `StatusContext`: `SUCCESS`, `FAILURE`, `PENDING`, `ERROR`.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckOutcome {
    Passed,
    Failed,
    Pending,
    Skipped,
}

impl Check {
    pub fn label(&self) -> &str {
        self.name.as_deref().or(self.context.as_deref()).unwrap_or("check")
    }

    pub fn outcome(&self) -> CheckOutcome {
        let word = |s: &Option<String>| s.as_deref().unwrap_or("").to_ascii_uppercase();
        if self.state.is_some() {
            return match word(&self.state).as_str() {
                "SUCCESS" => CheckOutcome::Passed,
                "FAILURE" | "ERROR" => CheckOutcome::Failed,
                _ => CheckOutcome::Pending,
            };
        }
        if word(&self.status) != "COMPLETED" {
            return CheckOutcome::Pending;
        }
        match word(&self.conclusion).as_str() {
            "SUCCESS" => CheckOutcome::Passed,
            "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => CheckOutcome::Failed,
            // NEUTRAL, SKIPPED, STALE, and a completed run that reports no conclusion (yet).
            _ => CheckOutcome::Skipped,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChecksSummary {
    pub passed: usize,
    pub failed: usize,
    pub pending: usize,
    pub skipped: usize,
}

impl ChecksSummary {
    pub fn of(checks: &[Check]) -> Self {
        let mut s = Self::default();
        for c in checks {
            match c.outcome() {
                CheckOutcome::Passed => s.passed += 1,
                CheckOutcome::Failed => s.failed += 1,
                CheckOutcome::Pending => s.pending += 1,
                CheckOutcome::Skipped => s.skipped += 1,
            }
        }
        s
    }

    /// `3 passed, 1 failed, 2 pending`, or `No checks`.
    pub fn text(&self) -> String {
        let parts: Vec<String> = [(self.passed, "passed"), (self.failed, "failed"), (self.pending, "pending"), (self.skipped, "skipped")]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, w)| format!("{n} {w}"))
            .collect();
        if parts.is_empty() { "No checks".into() } else { parts.join(", ") }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    #[serde(default)]
    pub author: Option<Author>,
    /// `APPROVED`, `CHANGES_REQUESTED`, `COMMENTED`, `DISMISSED`...
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub submitted_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Comment {
    #[serde(default)]
    pub author: Option<Author>,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub created_at: Option<String>,
}

/// `gh pr view N --json ...`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PrDetails {
    pub number: u64,
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: Option<Author>,
    pub state: PrState,
    #[serde(default)]
    pub is_draft: bool,
    #[serde(default)]
    pub head_ref_name: String,
    #[serde(default)]
    pub base_ref_name: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub files: Vec<PrFile>,
    #[serde(default)]
    pub status_check_rollup: Vec<Check>,
    #[serde(default)]
    pub reviews: Vec<Review>,
    #[serde(default)]
    pub comments: Vec<Comment>,
    /// `MERGEABLE`, `CONFLICTING` or `UNKNOWN`.
    #[serde(default)]
    pub mergeable: String,
}

impl PrDetails {
    pub fn author_login(&self) -> &str {
        self.author.as_ref().map_or("", |a| a.login.as_str())
    }

    pub fn checks(&self) -> ChecksSummary {
        ChecksSummary::of(&self.status_check_rollup)
    }
}

/// Which pull requests to list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PrFilter {
    /// Every open pull request.
    Open,
    /// Open, created by the signed-in user (`--author @me`).
    CreatedByMe,
    /// Open, asking the signed-in user for a review (`--search review-requested:@me`).
    ReviewRequested,
}

/// What `gh pr create` needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewPr {
    pub title: String,
    pub body: String,
    pub base: String,
    pub head: String,
    pub draft: bool,
}
