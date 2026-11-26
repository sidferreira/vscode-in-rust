//! Headless E2E tests of the Pull Requests view, the PR tab and the GitHub commands, with an
//! injected fake runner (docs/testing.md tier 2). The real `gh` is never started.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::fs;
use std::sync::Arc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_github::fake::FakeRunner;
use vsr_github::{GhError, SpawnError, Tool};
use vsr_quickinput::pick::PickRow;

use super::{CheckoutPullRequest, CreatePullRequest, PrBody, RefreshPullRequests};
use crate::Workbench;
use crate::config::AppConfig;
use crate::quick_input::QuickPick;

const REPO: &str = r#"{"defaultBranchRef":{"name":"main"},"nameWithOwner":"octo/hello","url":"https://github.com/octo/hello"}"#;

fn pr(n: u64, title: &str, author: &str, draft: bool) -> String {
    format!(
        r#"{{"author":{{"id":"x","is_bot":false,"login":"{author}","name":""}},"baseRefName":"main","headRefName":"feat/{n}","isDraft":{draft},"number":{n},"reviewDecision":"","state":"OPEN","title":"{title}","updatedAt":"2026-11-20T10:00:00Z","url":"https://github.com/octo/hello/pull/{n}"}}"#
    )
}

const DETAILS: &str = r#"{"author":{"login":"octocat","name":"The Octocat"},"baseRefName":"main","body":"Adds the search view.","comments":[],
 "files":[{"path":"src/search.rs","additions":120,"deletions":4},{"path":"README.md","additions":1,"deletions":0}],
 "headRefName":"feat/12","isDraft":false,"mergeable":"MERGEABLE","number":12,"reviews":[],"state":"OPEN",
 "statusCheckRollup":[{"__typename":"CheckRun","conclusion":"SUCCESS","name":"build","status":"COMPLETED"},{"__typename":"CheckRun","conclusion":"FAILURE","name":"lint","status":"COMPLETED"}],
 "title":"Add search","url":"https://github.com/octo/hello/pull/12"}"#;

/// A signed-in `gh` with one pull request in each list.
fn signed_in() -> FakeRunner {
    FakeRunner::new()
        .gh(&["auth", "status"], "Logged in")
        .gh(&["repo", "view"], REPO)
        .gh(&["pr", "list"], &format!("[{}]", pr(12, "Add search", "octocat", false)))
        .gh_with(&["pr", "list"], &["--author"], &format!("[{}]", pr(14, "Mine", "me", true)))
        .gh_with(&["pr", "list"], &["--search"], &format!("[{}]", pr(15, "Please review", "hubot", false)))
        .gh(&["pr", "view", "12"], DETAILS)
        .gh(&["pr", "checkout"], "")
}

fn start<'a>(cx: &'a mut TestAppContext, name: &str, fake: FakeRunner) -> (Entity<Workbench>, &'a mut VisualTestContext, Arc<FakeRunner>) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-github-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("a.txt"), "x").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None); // in memory
    });
    let fake = Arc::new(fake);
    let runner = fake.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| {
        let mut wb = Workbench::new(dir, cx);
        wb.github.set_runner(runner); // before anything can start the real gh
        wb
    });
    vcx.run_until_parked();
    (wb, vcx, fake)
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let b = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} is not rendered"));
    cx.simulate_mouse_move(b.center(), None, Default::default());
    cx.simulate_click(b.center(), Default::default());
    cx.run_until_parked();
}

fn picker(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Entity<QuickPick> {
    wb.read_with(cx, |wb, _| wb.quick.picker().cloned()).expect("a picker is open")
}

fn has_picker(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    wb.read_with(cx, |wb, _| wb.quick.picker().is_some())
}

fn labels(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    picker(wb, cx).read_with(cx, |p, _| {
        p.list().rows().iter().filter_map(|r| if let PickRow::Item(i) = r { Some(i.label.clone()) } else { None }).collect()
    })
}

fn type_text(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &'static str) {
    let p = picker(wb, cx);
    cx.update(|window, cx| p.update(cx, |p, cx| p.set_value(text, window, cx)));
    cx.run_until_parked();
}

fn accept(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    let p = picker(wb, cx);
    p.update(cx, |p, cx| p.accept(false, cx));
    cx.run_until_parked();
}

fn show_view(cx: &mut VisualTestContext) {
    click(cx, "activity:4");
}

// ---- the view ----------------------------------------------------------------------------------

#[gpui_kit::test]
fn the_view_lists_the_three_sections(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "sections", signed_in());
    assert!(cx.debug_bounds("github:view").is_none(), "nothing is loaded before the view is shown");
    assert_eq!(fake.calls().len(), 0, "and gh is not asked either");
    show_view(cx);
    for sel in ["github:section:open", "github:section:mine", "github:section:review", "github:repo"] {
        assert!(cx.debug_bounds(sel).is_some(), "{sel}");
    }
    assert!(cx.debug_bounds("github:pr:open:12").is_some());
    assert!(cx.debug_bounds("github:pr:mine:14").is_some());
    assert!(cx.debug_bounds("github:pr:review:15").is_some());
    assert!(cx.debug_bounds("github:error").is_none());
    let snapshot = wb.read_with(cx, |wb, _| wb.github.load.clone()).unwrap().unwrap();
    assert_eq!(snapshot.repo.name_with_owner, "octo/hello");
    assert!(snapshot.created_by_me[0].is_draft);
    // Showing the view again does not load again.
    let before = fake.calls().len();
    show_view(cx);
    show_view(cx);
    assert_eq!(fake.calls().len(), before);
}

#[gpui_kit::test]
fn refresh_loads_again_from_the_button_and_the_command(cx: &mut TestAppContext) {
    let (_wb, cx, fake) = start(cx, "refresh", signed_in());
    show_view(cx);
    let once = fake.count("gh auth status");
    assert_eq!(once, 1);
    click(cx, "github:refresh");
    assert_eq!(fake.count("gh auth status"), 2);
    cx.dispatch_action(RefreshPullRequests);
    cx.run_until_parked();
    assert_eq!(fake.count("gh auth status"), 3);
}

#[gpui_kit::test]
fn empty_lists_say_so(cx: &mut TestAppContext) {
    let fake = FakeRunner::new().gh(&["auth", "status"], "").gh(&["repo", "view"], REPO).gh(&["pr", "list"], "[]");
    let (_wb, cx, _) = start(cx, "empty", fake);
    show_view(cx);
    assert!(cx.debug_bounds("github:section:open").is_some());
    assert!(cx.debug_bounds("github:empty").is_some());
    assert!(cx.debug_bounds("github:pr:open:12").is_none());
}

#[gpui_kit::test]
fn gh_missing_shows_install_github_cli(cx: &mut TestAppContext) {
    let fake = FakeRunner::new().on(Tool::Gh, &[], Err(SpawnError::NotFound));
    let (wb, cx, _) = start(cx, "missing", fake);
    show_view(cx);
    assert!(cx.debug_bounds("github:error").is_some());
    let err = wb.read_with(cx, |wb, _| wb.github.load.clone()).unwrap().unwrap_err();
    assert_eq!(err, GhError::GhMissing);
    assert_eq!(super::view::error_lines(&err).0, "Install GitHub CLI");
    assert!(cx.debug_bounds("github:section:open").is_none());
}

#[gpui_kit::test]
fn signed_out_shows_run_gh_auth_login(cx: &mut TestAppContext) {
    let fake = FakeRunner::new().gh_fails(&["auth", "status"], 1, "You are not logged into any GitHub hosts. To log in, run: gh auth login");
    let (wb, cx, fake) = start(cx, "signed-out", fake);
    show_view(cx);
    assert!(cx.debug_bounds("github:error").is_some());
    let err = wb.read_with(cx, |wb, _| wb.github.load.clone()).unwrap().unwrap_err();
    assert!(matches!(err, GhError::NotSignedIn(_)));
    assert_eq!(super::view::error_lines(&err).0, "Run `gh auth login`");
    assert_eq!(fake.calls(), ["gh auth status"]);
}

#[gpui_kit::test]
fn a_folder_that_is_not_a_github_repo_says_so(cx: &mut TestAppContext) {
    let fake = FakeRunner::new().gh(&["auth", "status"], "").gh_fails(&["repo", "view"], 1, "fatal: not a git repository");
    let (wb, cx, _) = start(cx, "not-a-repo", fake);
    show_view(cx);
    assert!(cx.debug_bounds("github:error").is_some());
    let err = wb.read_with(cx, |wb, _| wb.github.load.clone()).unwrap().unwrap_err();
    assert!(matches!(err, GhError::NotGitHubRepo(_)));
}

// ---- the PR tab --------------------------------------------------------------------------------

#[gpui_kit::test]
fn clicking_a_pull_request_opens_the_read_only_detail_tab(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "tab", signed_in());
    show_view(cx);
    click(cx, "github:pr:open:12");
    for sel in ["github-tab", "github-pr-page", "github:pr-title", "github:pr-meta", "github:pr-body", "github:pr-checks", "github:pr-checkout", "github:pr-open-web", "github:pr-file:0", "github:pr-file:1"] {
        assert!(cx.debug_bounds(sel).is_some(), "{sel}");
    }
    assert!(cx.debug_bounds("github:pr-file:2").is_none());
    assert!(fake.calls().iter().any(|c| c.starts_with("gh pr view 12 --json ")));
    let tab = wb.read_with(cx, |wb, _| wb.github.pr.clone()).unwrap();
    assert_eq!(tab.number, 12);
    let PrBody::Loaded(d) = tab.body else { panic!("loaded") };
    assert_eq!((d.title.as_str(), d.author_login(), d.base_ref_name.as_str(), d.head_ref_name.as_str()), ("Add search", "octocat", "main", "feat/12"));
    assert_eq!(d.checks().text(), "1 passed, 1 failed");
    assert!(wb.read_with(cx, |wb, _| wb.github.pr_showing()));

    // Another tab takes over and the PR tab stays; clicking it brings it back; its close button closes it.
    wb.update(cx, |wb, cx| {
        wb.open(&wb.root.join("a.txt"));
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!wb.read_with(cx, |wb, _| wb.github.pr_showing()), "opening a file shows the file");
    assert!(cx.debug_bounds("github-pr-page").is_none());
    click(cx, "github-tab");
    assert!(wb.read_with(cx, |wb, _| wb.github.pr_showing()));
    click(cx, "github-tab-close");
    assert!(cx.debug_bounds("github-tab").is_none());
    assert!(wb.read_with(cx, |wb, _| wb.github.pr.is_none()));
}

#[gpui_kit::test]
fn a_failed_detail_load_shows_the_error_in_the_tab(cx: &mut TestAppContext) {
    let fake = signed_in().gh_fails(&["pr", "view", "12"], 1, "GraphQL: Could not resolve to a PullRequest with the number of 12.");
    let (wb, cx, _) = start(cx, "tab-error", fake);
    show_view(cx);
    click(cx, "github:pr:open:12");
    assert!(cx.debug_bounds("github:pr-error").is_some());
    assert!(matches!(wb.read_with(cx, |wb, _| wb.github.pr.clone()).unwrap().body, PrBody::Failed(_)));
}

#[gpui_kit::test]
fn the_tab_buttons_call_the_runner(cx: &mut TestAppContext) {
    let (_wb, cx, fake) = start(cx, "buttons", signed_in().gh(&["pr", "view", "12", "--web"], ""));
    show_view(cx);
    click(cx, "github:pr:open:12");
    assert_eq!(fake.count("gh pr checkout"), 0);
    click(cx, "github:pr-checkout");
    assert_eq!(fake.count("gh pr checkout 12"), 1);
    assert!(cx.debug_bounds("github:pr-notice").is_some(), "the result is shown");
    click(cx, "github:pr-open-web");
    assert_eq!(fake.count("gh pr view 12 --web"), 1);
}

#[gpui_kit::test]
fn a_failed_checkout_is_reported(cx: &mut TestAppContext) {
    let fake = signed_in().gh_fails(&["pr", "checkout"], 1, "error: Your local changes would be overwritten");
    let (wb, cx, _) = start(cx, "checkout-fail", fake);
    show_view(cx);
    click(cx, "github:pr:open:12");
    click(cx, "github:pr-checkout");
    let notice = wb.read_with(cx, |wb, _| wb.github.notice.clone()).unwrap();
    assert!(notice.error && notice.text.contains("local changes would be overwritten"), "{notice:?}");
}

// ---- commands ----------------------------------------------------------------------------------

#[gpui_kit::test]
fn the_commands_are_in_the_palette(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "palette", signed_in());
    cx.simulate_keystrokes("cmd-shift-p");
    cx.run_until_parked();
    type_text(&wb, cx, ">GitHub:");
    let rows = labels(&wb, cx);
    for title in ["Create Pull Request", "Checkout Pull Request", "Refresh Pull Requests"] {
        assert!(rows.iter().any(|l| l.contains(title)), "{title} in {rows:?}");
    }
    assert_eq!(fake.calls().len(), 0, "listing the commands runs nothing");
}

#[gpui_kit::test]
fn checkout_command_picks_from_the_open_pull_requests(cx: &mut TestAppContext) {
    let fake = signed_in().gh(
        &["pr", "list"],
        &format!("[{},{}]", pr(12, "Add search", "octocat", false), pr(13, "Fix crash", "hubot", false)),
    );
    let (wb, cx, fake) = start(cx, "checkout-cmd", fake);
    cx.dispatch_action(CheckoutPullRequest);
    cx.run_until_parked();
    assert_eq!(labels(&wb, cx), ["#12 Add search", "#13 Fix crash"]);
    assert_eq!(fake.count("gh pr checkout"), 0, "choosing comes first");
    // Down once, accept the second.
    let p = picker(&wb, cx);
    p.update(cx, |p, cx| p.move_next(cx));
    accept(&wb, cx);
    assert!(!has_picker(&wb, cx));
    assert_eq!(fake.count("gh pr checkout 13"), 1);
    assert_eq!(fake.count("gh pr checkout"), 1);
}

#[gpui_kit::test]
fn checkout_command_with_no_open_pull_requests_says_so(cx: &mut TestAppContext) {
    let (wb, cx, _) = start(cx, "checkout-none", FakeRunner::new().gh(&["pr", "list"], "[]"));
    cx.dispatch_action(CheckoutPullRequest);
    cx.run_until_parked();
    assert!(!has_picker(&wb, cx));
    assert!(wb.read_with(cx, |wb, _| wb.github.notice.clone()).unwrap().text.contains("no open pull requests"));
}

fn create_fake(has_upstream: bool) -> FakeRunner {
    let fake = FakeRunner::new()
        .git(&["rev-parse", "--abbrev-ref", "HEAD"], "feat/new\n")
        .git(&["log"], "Add the new thing\n")
        .gh(&["repo", "view"], REPO)
        .gh(&["pr", "create"], "https://github.com/octo/hello/pull/21\n")
        .gh(&["auth", "status"], "")
        .gh(&["pr", "list"], "[]")
        .git(&["push"], "");
    if has_upstream {
        fake.git(&["rev-parse", "--abbrev-ref", "--symbolic-full-name"], "origin/feat/new\n")
    } else {
        fake.git_fails(&["rev-parse", "--abbrev-ref", "--symbolic-full-name"], 128, "fatal: no upstream configured for branch 'feat/new'")
    }
}

#[gpui_kit::test]
fn create_asks_title_body_base_and_draft_then_runs_gh(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "create", create_fake(true));
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    // Title: prefilled from the last commit subject.
    assert_eq!(picker(&wb, cx).read_with(cx, |p, cx| p.query(cx)), "Add the new thing");
    accept(&wb, cx);
    // Body.
    type_text(&wb, cx, "Does the thing.");
    accept(&wb, cx);
    // Base: the repository's default branch.
    assert_eq!(picker(&wb, cx).read_with(cx, |p, cx| p.query(cx)), "main");
    type_text(&wb, cx, "develop");
    accept(&wb, cx);
    // Draft: yes (second row).
    assert_eq!(labels(&wb, cx).len(), 2);
    picker(&wb, cx).update(cx, |p, _| p.set_active_item(1));
    accept(&wb, cx);
    assert!(!has_picker(&wb, cx));
    assert_eq!(fake.count("git push"), 0, "the branch already had an upstream");
    let create: Vec<String> = fake.calls().into_iter().filter(|c| c.starts_with("gh pr create")).collect();
    assert_eq!(create, ["gh pr create --title Add the new thing --body Does the thing. --base develop --head feat/new --draft"]);
    let notice = wb.read_with(cx, |wb, _| wb.github.notice.clone()).unwrap();
    assert!(notice.text.contains("https://github.com/octo/hello/pull/21"), "{notice:?}");
    assert!(fake.count("gh pr list") >= 3, "the lists reload");
}

#[gpui_kit::test]
fn create_not_as_draft_leaves_the_flag_out(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "create-ready", create_fake(true));
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    for _ in 0..3 {
        accept(&wb, cx);
    }
    accept(&wb, cx); // Draft: the first row, ready for review
    let create: Vec<String> = fake.calls().into_iter().filter(|c| c.starts_with("gh pr create")).collect();
    assert_eq!(create, ["gh pr create --title Add the new thing --body  --base main --head feat/new"]);
}

#[gpui_kit::test]
fn create_without_an_upstream_asks_before_pushing_and_declining_pushes_nothing(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "create-decline", create_fake(false));
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    assert_eq!(labels(&wb, cx).len(), 2, "the push question");
    assert_eq!(fake.count("git push"), 0, "asking does not push");
    // "No": the second row.
    picker(&wb, cx).update(cx, |p, _| p.set_active_item(1));
    accept(&wb, cx);
    assert!(!has_picker(&wb, cx));
    assert_eq!(fake.count("git push"), 0);
    assert_eq!(fake.count("gh pr create"), 0);
}

#[gpui_kit::test]
fn escape_on_the_push_question_pushes_nothing(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "create-escape", create_fake(false));
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    picker(&wb, cx).update(cx, |p, cx| p.dismiss(cx));
    cx.run_until_parked();
    assert!(!has_picker(&wb, cx));
    assert_eq!(fake.count("git push"), 0);
    assert_eq!(fake.count("gh pr create"), 0);
}

#[gpui_kit::test]
fn create_without_an_upstream_pushes_only_after_confirmation(cx: &mut TestAppContext) {
    let (wb, cx, fake) = start(cx, "create-push", create_fake(false));
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    accept(&wb, cx); // "Yes": the first row
    assert_eq!(fake.count("git push -u origin HEAD"), 1);
    assert_eq!(fake.count("git push"), 1);
    // The flow goes on with the title.
    assert_eq!(picker(&wb, cx).read_with(cx, |p, cx| p.query(cx)), "Add the new thing");
    for _ in 0..4 {
        accept(&wb, cx);
    }
    assert_eq!(fake.count("gh pr create"), 1);
    // Pushed before the pull request was created.
    let calls = fake.calls();
    let (push, create) = (calls.iter().position(|c| c.starts_with("git push")).unwrap(), calls.iter().position(|c| c.starts_with("gh pr create")).unwrap());
    assert!(push < create);
}

#[gpui_kit::test]
fn a_failed_push_stops_the_flow(cx: &mut TestAppContext) {
    let fake = create_fake(false).git_fails(&["push"], 1, "error: failed to push some refs");
    let (wb, cx, fake) = start(cx, "create-push-fail", fake);
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    accept(&wb, cx);
    assert!(!has_picker(&wb, cx));
    assert_eq!(fake.count("gh pr create"), 0);
    let notice = wb.read_with(cx, |wb, _| wb.github.notice.clone()).unwrap();
    assert!(notice.error && notice.text.contains("failed to push"), "{notice:?}");
}

#[gpui_kit::test]
fn create_outside_a_repository_reports_it(cx: &mut TestAppContext) {
    let fake = FakeRunner::new().git_fails(&["rev-parse"], 128, "fatal: not a git repository (or any of the parent directories): .git");
    let (wb, cx, _) = start(cx, "create-no-repo", fake);
    cx.dispatch_action(CreatePullRequest);
    cx.run_until_parked();
    assert!(!has_picker(&wb, cx));
    assert!(wb.read_with(cx, |wb, _| wb.github.notice.clone()).unwrap().error);
}
