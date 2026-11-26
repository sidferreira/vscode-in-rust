//! Process spawning and argument passing, against a fake `gh` shell script on a `PATH` of its own
//! (in a fresh temp folder). Never runs the real `gh`.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use vsr_github::{Github, NewPr, PrFilter, SystemRunner, locate_gh_in};

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("vsr-github-spawn-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A `gh` that logs its arguments (one per line, a marker line between calls) and prints canned JSON.
fn fake_gh(dir: &Path) -> PathBuf {
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
    let found = locate_gh_in(None, Some(dir.clone().into_os_string()), &[]).unwrap().expect("gh on PATH");
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

fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn a_hung_gh_is_killed_after_the_timeout() {
    let dir = scratch("hang");
    let gh = script(&dir, "gh", "echo $$ > \"$(dirname \"$0\")/pid\"\nexec sleep 30");
    let runner = SystemRunner::new(Some(gh)).with_limits(std::time::Duration::from_millis(300), 1024);
    let started = std::time::Instant::now();
    let err = Github::new(Arc::new(runner), scratch("hang-work")).repo().unwrap_err();
    assert!(matches!(err, vsr_github::GhError::Timeout { ref command } if command == "gh repo view --json nameWithOwner,defaultBranchRef,url"), "{err:?}");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    // The child is gone (`kill -0` fails for a process that does not exist).
    let pid = fs::read_to_string(dir.join("pid")).unwrap();
    let alive = std::process::Command::new("kill").args(["-0", pid.trim()]).status().unwrap().success();
    assert!(!alive, "the hung gh was killed");
}

#[test]
fn captured_output_is_capped() {
    use vsr_github::{GhRunner, Tool};
    let dir = scratch("cap");
    let gh = script(&dir, "gh", "head -c 200000 /dev/zero | tr '\\0' x\nhead -c 200000 /dev/zero | tr '\\0' y >&2");
    let runner = SystemRunner::new(Some(gh)).with_limits(std::time::Duration::from_secs(30), 1000);
    let out = runner.run(Tool::Gh, &[], &dir).unwrap();
    assert_eq!((out.stdout.len(), out.stderr.len()), (1000, 1000));
}

#[test]
fn spawn_detached_does_not_wait_for_a_long_running_gh() {
    use vsr_github::{GhRunner, Tool};
    let dir = scratch("detached");
    let gh = script(&dir, "gh", "sleep 5");
    let runner = SystemRunner::new(Some(gh));
    let started = std::time::Instant::now();
    runner.spawn_detached(Tool::Gh, &["pr".into(), "view".into(), "1".into(), "--web".into()], &dir).unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(4));
}

#[test]
fn spawn_detached_reports_an_immediate_failure() {
    use vsr_github::{GhRunner, SpawnError, Tool};
    let dir = scratch("detached-fail");
    let gh = script(&dir, "gh", "echo 'not signed in' >&2\nexit 4");
    let err = SystemRunner::new(Some(gh)).spawn_detached(Tool::Gh, &[], &dir).unwrap_err();
    assert_eq!(err, SpawnError::Io("not signed in".into()));
}

#[test]
fn git_runs_by_its_absolute_path_not_a_repo_local_git() {
    use vsr_github::{GhRunner, SpawnError, Tool};
    // A `git` planted in the working directory must not run, even with "." on PATH semantics.
    let repo = scratch("planted");
    script(&repo, "git", "echo planted > \"$(dirname \"$0\")/ran\"");
    let runner = SystemRunner::with_tools(None, None);
    assert_eq!(runner.run(Tool::Git, &["--version".into()], &repo), Err(SpawnError::NotFound));
    assert!(!repo.join("ran").exists());
    let real = SystemRunner::with_tools(None, vsr_github::locate_git());
    let out = real.run(Tool::Git, &["--version".into()], &repo).unwrap();
    assert!(out.stdout.starts_with("git version"));
    assert!(!repo.join("ran").exists());
}

fn alive(pid: &str) -> bool {
    std::process::Command::new("kill").args(["-0", pid.trim()]).status().unwrap().success()
}

fn wait_gone(pid: &str) -> bool {
    for _ in 0..100 {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    false
}

#[test]
fn a_timeout_kills_the_whole_process_group_not_just_the_child() {
    use vsr_github::{GhRunner, SpawnError, Tool};
    let dir = scratch("group");
    // Like `gh pr checkout` spawning git: a grandchild that keeps the pipes open.
    let gh = script(&dir, "gh", "echo $$ > \"$(dirname \"$0\")/child\"\nsleep 600 &\necho $! > \"$(dirname \"$0\")/grandchild\"\nwait");
    let runner = SystemRunner::new(Some(gh)).with_limits(std::time::Duration::from_millis(400), 1024);
    let started = std::time::Instant::now();
    assert_eq!(runner.run(Tool::Gh, &[], &dir), Err(SpawnError::Timeout));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    for name in ["child", "grandchild"] {
        let pid = fs::read_to_string(dir.join(name)).unwrap();
        assert!(wait_gone(&pid), "{name} {pid} is gone");
    }
}

#[test]
fn spawn_detached_kills_a_gh_that_runs_too_long() {
    use vsr_github::{GhRunner, Tool};
    let dir = scratch("detached-long");
    let gh = script(&dir, "gh", "sleep 600 &\necho $! > \"$(dirname \"$0\")/grandchild\"\nwait");
    let runner = SystemRunner::new(Some(gh)).with_detached_limit(std::time::Duration::from_millis(2500));
    runner.spawn_detached(Tool::Gh, &[], &dir).unwrap();
    let pid = fs::read_to_string(dir.join("grandchild")).unwrap();
    assert!(alive(&pid), "still running right after the call returns");
    assert!(wait_gone(&pid), "killed after the detached limit");
}

#[test]
fn a_huge_output_is_reported_as_too_large() {
    let dir = scratch("huge");
    let gh = script(&dir, "gh", "head -c 200000 /dev/zero | tr '\\0' x");
    let runner = SystemRunner::new(Some(gh)).with_limits(std::time::Duration::from_secs(30), 1000);
    let err = Github::new(Arc::new(runner), scratch("huge-work")).repo().unwrap_err();
    assert!(matches!(err, vsr_github::GhError::OutputTooLarge { .. }), "{err:?}");
}
