//! Unit tests with a fake runner. The JSON is copied from the shape of real `gh` output.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::sync::Arc;

use crate::fake::FakeRunner;
use crate::runner::{Output, SpawnError, Tool};
use crate::*;

pub const REPO: &str = r#"{"defaultBranchRef":{"name":"main"},"nameWithOwner":"octo/hello","url":"https://github.com/octo/hello"}"#;

pub const LIST: &str = r#"[
 {"author":{"id":"MDQ6VXNlcjE=","is_bot":false,"login":"octocat","name":"The Octocat"},"baseRefName":"main","headRefName":"feat/search","isDraft":false,"number":12,"reviewDecision":"REVIEW_REQUIRED","state":"OPEN","title":"Add search","updatedAt":"2026-11-20T10:00:00Z","url":"https://github.com/octo/hello/pull/12"},
 {"author":{"id":"BOT_1","is_bot":true,"login":"app/dependabot","name":""},"baseRefName":"main","headRefName":"dependabot/cargo/serde","isDraft":true,"number":9,"reviewDecision":"","state":"OPEN","title":"Bump serde","updatedAt":"2026-11-19T09:00:00Z","url":"https://github.com/octo/hello/pull/9"}
]"#;

pub const DETAILS: &str = r#"{
 "author":{"id":"MDQ6VXNlcjE=","is_bot":false,"login":"octocat","name":"The Octocat"},
 "baseRefName":"main","body":"Adds the search view.\n\nCloses #3","comments":[{"id":"IC_1","author":{"login":"hubot"},"authorAssociation":"MEMBER","body":"Looks good","createdAt":"2026-11-20T11:00:00Z","url":"https://github.com/octo/hello/pull/12#issuecomment-1"}],
 "files":[{"path":"src/search.rs","additions":120,"deletions":4},{"path":"README.md","additions":1,"deletions":0}],
 "headRefName":"feat/search","isDraft":false,"mergeable":"MERGEABLE","number":12,
 "reviews":[{"id":"PRR_1","author":{"login":"hubot"},"authorAssociation":"MEMBER","body":"","submittedAt":"2026-11-20T12:00:00Z","includesCreatedEdit":false,"reactionGroups":[],"state":"APPROVED"}],
 "state":"OPEN",
 "statusCheckRollup":[
  {"__typename":"CheckRun","completedAt":"2026-11-20T10:05:00Z","conclusion":"SUCCESS","detailsUrl":"https://github.com/octo/hello/actions/runs/1","name":"build","startedAt":"2026-11-20T10:01:00Z","status":"COMPLETED","workflowName":"CI"},
  {"__typename":"CheckRun","completedAt":"","conclusion":"FAILURE","detailsUrl":"","name":"lint","startedAt":"","status":"COMPLETED","workflowName":"CI"},
  {"__typename":"CheckRun","completedAt":"","conclusion":"","detailsUrl":"","name":"e2e","startedAt":"","status":"IN_PROGRESS","workflowName":"CI"},
  {"__typename":"CheckRun","completedAt":"","conclusion":"SKIPPED","detailsUrl":"","name":"docs","startedAt":"","status":"COMPLETED","workflowName":"CI"},
  {"__typename":"StatusContext","context":"ci/legacy","startedAt":"2026-11-20T10:00:00Z","state":"PENDING","targetUrl":""}
 ],
 "title":"Add search","url":"https://github.com/octo/hello/pull/12"}"#;

fn gh(fake: FakeRunner) -> (Github, Arc<FakeRunner>) {
    let fake = Arc::new(fake);
    (Github::new(fake.clone(), "/work/hello".into()), fake)
}

#[test]
fn repo_parses_the_default_branch() {
    let (g, fake) = gh(FakeRunner::new().gh(&["repo", "view"], REPO));
    let repo = g.repo().unwrap();
    assert_eq!(repo.name_with_owner, "octo/hello");
    assert_eq!(repo.default_branch(), "main");
    assert_eq!(repo.url, "https://github.com/octo/hello");
    assert_eq!(fake.calls(), ["gh repo view --json nameWithOwner,defaultBranchRef,url"]);
}

#[test]
fn a_repo_without_a_default_branch_falls_back_to_main() {
    let (g, _) = gh(FakeRunner::new().gh(&["repo", "view"], r#"{"nameWithOwner":"a/b","defaultBranchRef":null,"url":""}"#));
    assert_eq!(g.repo().unwrap().default_branch(), "main");
}

#[test]
fn list_prs_open_asks_for_open_pull_requests_with_the_fields() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "list"], LIST));
    let prs = g.list_prs(PrFilter::Open).unwrap();
    assert_eq!(fake.calls(), [format!("gh pr list --state open --limit 50 --json {LIST_FIELDS}")]);
    assert_eq!(prs.len(), 2);
    assert_eq!((prs[0].number, prs[0].title.as_str(), prs[0].author_login()), (12, "Add search", "octocat"));
    assert_eq!((prs[0].head_ref_name.as_str(), prs[0].base_ref_name.as_str()), ("feat/search", "main"));
    assert_eq!(prs[0].state, PrState::Open);
    assert!(!prs[0].is_draft && prs[1].is_draft);
    assert_eq!(prs[0].review_decision.as_deref(), Some("REVIEW_REQUIRED"));
    assert_eq!(prs[1].review_decision.as_deref(), Some(""));
    assert_eq!(prs[0].updated_at, "2026-11-20T10:00:00Z");
}

#[test]
fn list_prs_created_by_me_adds_author_me() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "list"], "[]"));
    assert!(g.list_prs(PrFilter::CreatedByMe).unwrap().is_empty());
    assert!(fake.calls()[0].ends_with("--author @me"), "{:?}", fake.calls());
}

#[test]
fn list_prs_review_requested_searches() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "list"], "[]"));
    g.list_prs(PrFilter::ReviewRequested).unwrap();
    assert!(fake.calls()[0].ends_with("--search review-requested:@me"), "{:?}", fake.calls());
}

#[test]
fn pr_details_parses_files_checks_reviews_and_comments() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "view"], DETAILS));
    let d = g.pr_details(12).unwrap();
    assert_eq!(fake.calls(), [format!("gh pr view 12 --json {VIEW_FIELDS}")]);
    assert_eq!((d.number, d.title.as_str(), d.author_login()), (12, "Add search", "octocat"));
    assert_eq!(d.body, "Adds the search view.\n\nCloses #3");
    assert_eq!(d.files.len(), 2);
    assert_eq!((d.files[0].path.as_str(), d.files[0].additions, d.files[0].deletions), ("src/search.rs", 120, 4));
    assert_eq!(d.mergeable, "MERGEABLE");
    assert_eq!(d.reviews.len(), 1);
    assert_eq!(d.reviews[0].state, "APPROVED");
    assert_eq!(d.comments[0].body, "Looks good");
    assert_eq!(d.status_check_rollup.len(), 5);
}

#[test]
fn checks_are_summarised_by_outcome() {
    let (g, _) = gh(FakeRunner::new().gh(&["pr", "view"], DETAILS));
    let d = g.pr_details(12).unwrap();
    let outcomes: Vec<_> = d.status_check_rollup.iter().map(Check::outcome).collect();
    assert_eq!(
        outcomes,
        [CheckOutcome::Passed, CheckOutcome::Failed, CheckOutcome::Pending, CheckOutcome::Skipped, CheckOutcome::Pending]
    );
    assert_eq!(d.status_check_rollup[4].label(), "ci/legacy");
    assert_eq!(d.checks(), ChecksSummary { passed: 1, failed: 1, pending: 2, skipped: 1 });
    assert_eq!(d.checks().text(), "1 passed, 1 failed, 2 pending, 1 skipped");
    assert_eq!(ChecksSummary::default().text(), "No checks");
}

#[test]
fn a_status_context_error_is_a_failure() {
    let c: Check = serde_json::from_str(r#"{"__typename":"StatusContext","context":"x","state":"ERROR"}"#).unwrap();
    assert_eq!(c.outcome(), CheckOutcome::Failed);
}

#[test]
fn a_merged_pull_request_parses() {
    let (g, _) = gh(FakeRunner::new().gh(&["pr", "view"], r#"{"number":1,"title":"t","state":"MERGED"}"#));
    let d = g.pr_details(1).unwrap();
    assert_eq!(d.state, PrState::Merged);
    assert!(d.files.is_empty() && d.body.is_empty());
}

#[test]
fn create_passes_every_argument_and_returns_the_url() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "create"], "https://github.com/octo/hello/pull/13\n"));
    let pr = NewPr { title: "Fix: a \"b\"".into(), body: "line 1\nline 2".into(), base: "main".into(), head: "fix".into(), draft: false };
    assert_eq!(g.create_pr(&pr).unwrap(), "https://github.com/octo/hello/pull/13");
    assert_eq!(fake.calls(), ["gh pr create --title Fix: a \"b\" --body line 1\nline 2 --base main --head fix"]);
}

#[test]
fn create_draft_adds_the_flag() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "create"], "u\n"));
    let pr = NewPr { title: "t".into(), body: String::new(), base: "main".into(), head: "h".into(), draft: true };
    g.create_pr(&pr).unwrap();
    assert!(fake.calls()[0].ends_with("--head h --draft"));
}

#[test]
fn checkout_and_browser_run_the_gh_commands() {
    let (g, fake) = gh(FakeRunner::new().gh(&["pr", "checkout"], "").gh(&["pr", "view"], ""));
    g.checkout(12).unwrap();
    g.open_in_browser(12).unwrap();
    assert_eq!(fake.calls(), ["gh pr checkout 12", "gh pr view 12 --web"]);
}

#[test]
fn auth_status_ok_and_signed_out() {
    let (g, _) = gh(FakeRunner::new().gh(&["auth", "status"], "Logged in to github.com"));
    assert_eq!(g.auth_status(), Ok(()));
    let (g, _) = gh(FakeRunner::new().gh_fails(
        &["auth", "status"],
        1,
        "You are not logged into any GitHub hosts. To log in, run: gh auth login\n",
    ));
    assert!(matches!(g.auth_status(), Err(GhError::NotSignedIn(m)) if m.contains("gh auth login")));
}

#[test]
fn a_missing_gh_is_reported() {
    let (g, _) = gh(FakeRunner::new().on(Tool::Gh, &[], Err(SpawnError::NotFound)));
    assert_eq!(g.repo(), Err(GhError::GhMissing));
    assert_eq!(GhError::GhMissing.headline(), "Install GitHub CLI");
}

#[test]
fn another_spawn_failure_is_io() {
    let (g, _) = gh(FakeRunner::new().on(Tool::Gh, &[], Err(SpawnError::Io("permission denied".into()))));
    assert_eq!(g.repo(), Err(GhError::Io("permission denied".into())));
}

#[test]
fn not_a_github_repo_is_classified() {
    for text in [
        "fatal: not a git repository (or any of the parent directories): .git",
        "GraphQL: Could not resolve to a Repository with the name 'a/b'. (repository)",
    ] {
        let (g, _) = gh(FakeRunner::new().gh_fails(&["repo"], 1, text));
        assert!(matches!(g.repo(), Err(GhError::NotGitHubRepo(_))), "{text}");
    }
}

#[test]
fn the_no_remotes_message_is_a_repo_problem_not_a_sign_in_problem() {
    let text = "none of the git remotes configured for this repository point to a known GitHub host. To tell gh about a new GitHub host, please use `gh auth login`";
    let (g, _) = gh(FakeRunner::new().gh_fails(&["repo"], 1, text));
    assert!(matches!(g.repo(), Err(GhError::NotGitHubRepo(_))));
}

#[test]
fn an_unrecognised_failure_keeps_gh_s_text() {
    let (g, _) = gh(FakeRunner::new().gh_fails(&["pr", "checkout"], 1, "error: Your local changes would be overwritten\n"));
    match g.checkout(3).unwrap_err() {
        GhError::Failed { command, code, message } => {
            assert_eq!(command, "gh pr checkout 3");
            assert_eq!(code, Some(1));
            assert_eq!(message, "error: Your local changes would be overwritten");
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn a_failure_with_empty_stderr_falls_back_to_stdout() {
    let fake = FakeRunner::new().on(Tool::Gh, &["pr"], Ok(Output { success: false, code: Some(1), stdout: "nope".into(), stderr: String::new() }));
    let (g, _) = gh(fake);
    assert!(matches!(g.checkout(1), Err(GhError::Failed { message, .. }) if message == "nope"));
}

#[test]
fn bad_json_is_a_parse_error() {
    let (g, _) = gh(FakeRunner::new().gh(&["pr", "list"], "not json"));
    assert!(matches!(g.list_prs(PrFilter::Open), Err(GhError::Parse { .. })));
    let (g, _) = gh(FakeRunner::new().gh(&["pr", "view"], "{}"));
    assert!(matches!(g.pr_details(1), Err(GhError::Parse { .. })), "title and number are required");
}

#[test]
fn snapshot_loads_repo_and_three_lists_in_order() {
    let fake = FakeRunner::new()
        .gh(&["auth", "status"], "")
        .gh(&["repo", "view"], REPO)
        .gh(&["pr", "list"], LIST);
    let (g, fake) = gh(fake);
    let s = g.snapshot().unwrap();
    assert_eq!(s.repo.name_with_owner, "octo/hello");
    assert_eq!((s.open.len(), s.created_by_me.len(), s.review_requested.len()), (2, 2, 2));
    let calls = fake.calls();
    assert_eq!(calls.len(), 5);
    assert_eq!(calls[0], "gh auth status");
    assert!(calls[3].ends_with("--author @me") && calls[4].ends_with("--search review-requested:@me"), "{calls:?}");
}

#[test]
fn snapshot_stops_at_the_first_failure() {
    let fake = FakeRunner::new().gh_fails(&["auth", "status"], 1, "run: gh auth login");
    let (g, fake) = gh(fake);
    assert!(matches!(g.snapshot(), Err(GhError::NotSignedIn(_))));
    assert_eq!(fake.calls(), ["gh auth status"], "nothing else was asked");
    let fake = FakeRunner::new().gh(&["auth", "status"], "").gh_fails(&["repo", "view"], 1, "fatal: not a git repository");
    let (g, _) = gh(fake);
    assert!(matches!(g.snapshot(), Err(GhError::NotGitHubRepo(_))));
}

#[test]
fn branch_info_reads_branch_upstream_and_subject() {
    let fake = FakeRunner::new()
        .git(&["rev-parse", "--abbrev-ref", "HEAD"], "feat/x\n")
        .git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name"], "origin/feat/x\n")
        .git(&["log"], "Add the thing\n");
    let (g, _) = gh(fake);
    assert_eq!(
        g.branch_info().unwrap(),
        BranchInfo { branch: "feat/x".into(), has_upstream: true, last_commit_subject: "Add the thing".into() }
    );
}

#[test]
fn branch_info_without_an_upstream() {
    let fake = FakeRunner::new()
        .git(&["rev-parse", "--abbrev-ref", "HEAD"], "feat/x\n")
        .git_fails(&["rev-parse", "--abbrev-ref", "--symbolic-full-name"], 128, "fatal: no upstream configured for branch 'feat/x'")
        .git_fails(&["log"], 128, "fatal: your current branch 'feat/x' does not have any commits yet");
    let (g, _) = gh(fake);
    let info = g.branch_info().unwrap();
    assert!(!info.has_upstream);
    assert_eq!(info.last_commit_subject, "");
}

#[test]
fn branch_info_outside_a_repo_fails() {
    let (g, _) = gh(FakeRunner::new().git_fails(&["rev-parse"], 128, "fatal: not a git repository (or any of the parent directories): .git"));
    assert!(matches!(g.branch_info(), Err(GhError::NotGitHubRepo(_))));
}

#[test]
fn push_upstream_runs_the_exact_git_command() {
    let (g, fake) = gh(FakeRunner::new().git(&["push"], ""));
    g.push_upstream().unwrap();
    assert_eq!(fake.calls(), ["git push -u origin HEAD"]);
    let (g, _) = gh(FakeRunner::new().git_fails(&["push"], 1, "error: failed to push some refs"));
    assert!(matches!(g.push_upstream(), Err(GhError::Failed { .. })));
}

#[test]
fn errors_have_readable_text() {
    let e = GhError::Failed { command: "gh pr list".into(), code: Some(1), message: "boom".into() };
    assert_eq!(e.to_string(), "`gh pr list` failed (1): boom");
    assert_eq!(e.headline(), "boom");
    assert_eq!(GhError::NotSignedIn("x".into()).headline(), "Run `gh auth login`");
    assert!(GhError::NotGitHubRepo("x".into()).headline().contains("not a GitHub repository"));
}

// ---- review fixes ------------------------------------------------------------------------------

/// `gh auth status` with github.com fine and a second, broken host (real gh layout).
const MULTI_HOST_BROKEN_OTHER: &str = "github.com
  ✓ Logged in to github.com account octocat (keyring)
  - Active account: true
  - Git operations protocol: https
  - Token: gho_************************************
ghe.corp.example
  X Failed to log in to ghe.corp.example account octocat (keyring)
  - Active account: true
  - The token in keyring is invalid.
  - To re-authenticate, run: gh auth login -h ghe.corp.example
  - To forget about this account, run: gh auth logout -h ghe.corp.example -u octocat
";

const MULTI_HOST_BROKEN_GITHUB: &str = "ghe.corp.example
  ✓ Logged in to ghe.corp.example account octocat (keyring)
  - Active account: true
github.com
  X Failed to log in to github.com account octocat (keyring)
  - Active account: true
  - The token in keyring is invalid.
  - To re-authenticate, run: gh auth login -h github.com
";

#[test]
fn a_broken_other_host_does_not_tell_a_github_com_user_to_log_in() {
    let (g, _) = gh(FakeRunner::new().gh_fails(&["auth", "status"], 1, MULTI_HOST_BROKEN_OTHER));
    match g.auth_status().unwrap_err() {
        GhError::Failed { message, .. } => assert!(message.contains("ghe.corp.example"), "{message}"),
        e => panic!("{e:?}"),
    }
}

#[test]
fn the_default_host_with_an_invalid_token_is_not_signed_in() {
    let (g, _) = gh(FakeRunner::new().gh_fails(&["auth", "status"], 1, MULTI_HOST_BROKEN_GITHUB));
    assert!(matches!(g.auth_status(), Err(GhError::NotSignedIn(_))));
}

#[test]
fn every_host_broken_is_not_signed_in() {
    let text = "github.com\n  X Failed to log in to github.com account a (keyring)\n  - The token in keyring is invalid.\n  - To re-authenticate, run: gh auth login -h github.com\n";
    let (g, _) = gh(FakeRunner::new().gh_fails(&["auth", "status"], 1, text));
    assert!(matches!(g.auth_status(), Err(GhError::NotSignedIn(_))));
}

#[test]
fn the_not_logged_in_message_from_other_commands_is_still_not_signed_in() {
    let text = "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.";
    let (g, _) = gh(FakeRunner::new().gh_fails(&["pr", "list"], 4, text));
    assert!(matches!(g.list_prs(PrFilter::Open), Err(GhError::NotSignedIn(_))));
}

#[test]
fn a_snapshot_goes_on_when_only_another_host_is_broken() {
    let fake = FakeRunner::new()
        .gh_fails(&["auth", "status"], 1, MULTI_HOST_BROKEN_OTHER)
        .gh(&["repo", "view"], REPO)
        .gh(&["pr", "list"], "[]");
    let (g, _) = gh(fake);
    assert!(g.snapshot().is_ok());
}

#[test]
fn a_check_run_completed_without_a_conclusion_is_not_a_failure() {
    let c: Check = serde_json::from_str(r#"{"__typename":"CheckRun","name":"x","status":"COMPLETED","conclusion":""}"#).unwrap();
    assert_eq!(c.outcome(), CheckOutcome::Skipped);
    let c: Check = serde_json::from_str(r#"{"__typename":"CheckRun","name":"x","status":"COMPLETED"}"#).unwrap();
    assert_eq!(c.outcome(), CheckOutcome::Skipped);
}

#[test]
fn check_conclusions_map_to_failed_or_neutral() {
    for (conclusion, want) in [
        ("FAILURE", CheckOutcome::Failed),
        ("TIMED_OUT", CheckOutcome::Failed),
        ("CANCELLED", CheckOutcome::Failed),
        ("ACTION_REQUIRED", CheckOutcome::Failed),
        ("STARTUP_FAILURE", CheckOutcome::Failed),
        ("STALE", CheckOutcome::Skipped),
        ("SKIPPED", CheckOutcome::Skipped),
        ("NEUTRAL", CheckOutcome::Skipped),
        ("SUCCESS", CheckOutcome::Passed),
    ] {
        let json = format!(r#"{{"name":"x","status":"COMPLETED","conclusion":"{conclusion}"}}"#);
        let c: Check = serde_json::from_str(&json).unwrap();
        assert_eq!(c.outcome(), want, "{conclusion}");
    }
}

#[test]
fn a_missing_git_is_not_reported_as_a_missing_gh() {
    let (g, _) = gh(FakeRunner::new().on(Tool::Git, &[], Err(SpawnError::NotFound)));
    assert_eq!(g.branch_info(), Err(GhError::GitMissing));
    assert_eq!(GhError::GitMissing.headline(), "Install Git");
    assert_ne!(GhError::GitMissing.headline(), GhError::GhMissing.headline());
}

#[test]
fn a_repository_that_cannot_be_resolved_may_be_an_access_problem() {
    let (g, _) = gh(FakeRunner::new().gh_fails(&["repo"], 1, "GraphQL: Could not resolve to a Repository with the name 'a/b'. (repository)"));
    let e = g.repo().unwrap_err();
    assert_eq!(e.headline(), "Could not find this repository on GitHub (not a GitHub repository, or no access)");
}

#[test]
fn a_timeout_is_its_own_error() {
    let (g, _) = gh(FakeRunner::new().on(Tool::Gh, &[], Err(SpawnError::Timeout)));
    assert!(matches!(g.repo(), Err(GhError::Timeout { .. })));
}
