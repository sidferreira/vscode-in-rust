//! Process spawning and argument passing, against a fake `gh` shell script on a `PATH` of its own
//! (in a fresh temp folder). Never runs the real `gh`.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::Arc;

use vsr_github::{Github, NewPr, PrFilter, SystemRunner, locate_gh_in};

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("vsr-github-spawn-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `gh` that logs its arguments (one per line, a marker line between calls) and prints canned JSON.
fn fake_gh(dir: &PathBuf) -> PathBuf {
    let script = r#"#!/bin/sh
log="$(dirname "$0")/args.log"
{ echo "--- cwd=$(pwd)"; for a in "$@"; do printf '%s\n' "$a"; done; } >> "$log"
case "$1 $2" in
  "repo view") echo '{"defaultBranchRef":{"name":"trunk"},"nameWithOwner":"o/r","url":"https://github.com/o/r"}' ;;
  "pr list") echo '[{"number":7,"title":"Seven","state":"OPEN","author":{"login":"me"}}]' ;;
  "pr create") echo "https://github.com/o/r/pull/8" ;;
  "auth status") echo "no good" >&2; exit 1 ;;
  *) ;;
esac
"#;
    let path = dir.join("gh");
    fs::write(&path, script).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_gh_on_path_is_found_spawned_and_given_its_arguments() {
    let dir = scratch("path");
    let gh = fake_gh(&dir);
    let found = locate_gh_in(None, Some(dir.clone().into_os_string()), &[]).expect("gh on PATH");
    assert_eq!(found, gh);

    let work = scratch("work");
    let github = Github::new(Arc::new(SystemRunner::new(Some(found))), work.clone());
    let repo = github.repo().unwrap();
    assert_eq!((repo.name_with_owner.as_str(), repo.default_branch()), ("o/r", "trunk"));
    let prs = github.list_prs(PrFilter::ReviewRequested).unwrap();
    assert_eq!(prs[0].number, 7);
    let url = github
        .create_pr(&NewPr { title: "A title with spaces".into(), body: "two\nlines & \"quotes\"".into(), base: "trunk".into(), head: "feat".into(), draft: true })
        .unwrap();
    assert_eq!(url, "https://github.com/o/r/pull/8");

    let log = fs::read_to_string(dir.join("args.log")).unwrap();
    let calls: Vec<&str> = log.split("--- cwd=").skip(1).collect();
    assert_eq!(calls.len(), 3);
    let cwd_line = calls[0].lines().next().unwrap();
    assert_eq!(fs::canonicalize(cwd_line).unwrap(), fs::canonicalize(&work).unwrap(), "runs in the repository folder");
    assert!(calls[1].contains("\n--search\nreview-requested:@me\n"), "{}", calls[1]);
    // The body is one argument, newline and quotes intact.
    assert!(calls[2].contains("\n--title\nA title with spaces\n--body\ntwo\nlines & \"quotes\"\n--base\ntrunk\n--head\nfeat\n--draft\n"), "{}", calls[2]);
}

#[test]
fn a_nonzero_exit_carries_stderr_and_the_code() {
    let dir = scratch("fail");
    let gh = fake_gh(&dir);
    let github = Github::new(Arc::new(SystemRunner::new(Some(gh))), scratch("work2"));
    let err = github.auth_status().unwrap_err();
    assert_eq!(err, vsr_github::GhError::Failed { command: "gh auth status".into(), code: Some(1), message: "no good".into() });
}

#[test]
fn no_gh_anywhere_is_gh_missing() {
    let github = Github::new(Arc::new(SystemRunner::new(None)), scratch("work3"));
    assert_eq!(github.repo(), Err(vsr_github::GhError::GhMissing));
}
