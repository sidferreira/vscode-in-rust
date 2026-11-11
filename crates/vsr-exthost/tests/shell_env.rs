//! upstream: src/vs/platform/shell/node/shellEnv.ts @ 50e1350fc78d
//!
//! The login-shell environment (ADR-0100), headless: a fake `$SHELL` script stands in for zsh. The
//! real-host proof, a bare-name spawn from the host, is in `shell_env_real_host.rs`.

#![cfg(unix)]

mod common;

use common::scratch;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vsr_exthost::shell_env::{self, ShellEnvError};

fn value<'a>(vars: &'a [(String, String)], key: &str) -> Option<&'a str> {
    vars.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

/// An executable `sh` script at `dir/name`.
fn script(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// A stand-in for `zsh -i -l -c`: rc-file noise, a login `PATH`, then the command (`$4`).
fn login_shell(dir: &Path) -> PathBuf {
    script(
        dir,
        "fake-login-shell",
        "echo 'Last login: motd noise'\n\
         export PATH=\"/rscode-fake/bin:/usr/bin:/bin\"\n\
         export FROM_RC=yes\n\
         eval \"$4\"\n\
         echo 'trailing noise'",
    )
}

#[test]
fn the_parser_keeps_only_what_is_between_the_markers() {
    let mark = "MARK";
    let out = b"noise MARK\x00PATH=/a:/b\x00EMPTY=\x00X=a=b\x00MARK after";
    let vars = shell_env::parse(out, mark).unwrap();
    assert_eq!(value(&vars, "PATH"), Some("/a:/b"));
    assert_eq!(value(&vars, "EMPTY"), Some(""));
    assert_eq!(value(&vars, "X"), Some("a=b"), "only the first '=' splits");
    assert_eq!(vars.len(), 3, "{vars:?}");
}

#[test]
fn the_parser_drops_the_guard_and_entries_without_a_name() {
    let out = format!("@@M@@\0{}=1\0=orphan\0NOEQUALS\0OK=yes\0@@M@@", shell_env::GUARD);
    let vars = shell_env::parse(out.as_bytes(), "@@M@@").unwrap();
    assert_eq!(vars, vec![("OK".to_string(), "yes".to_string())]);
}

#[test]
fn the_parser_needs_both_markers() {
    assert_eq!(shell_env::parse(b"PATH=/a\0", "MARK"), None);
    assert_eq!(shell_env::parse(b"MARK PATH=/a\0", "MARK"), None);
}

#[test]
fn the_command_prints_the_environment_between_two_marks() {
    let cmd = shell_env::command("ZZ");
    assert_eq!(cmd, "printf '%s' 'ZZ'; env -0; printf '%s' 'ZZ'");
}

#[test]
fn the_login_shell_environment_is_resolved_and_the_rc_noise_discarded() {
    let dir = scratch("shell-env-login");
    let shell = login_shell(&dir);
    let vars = shell_env::resolve(&shell, Duration::from_secs(10)).expect("resolves");
    assert_eq!(value(&vars, "PATH"), Some("/rscode-fake/bin:/usr/bin:/bin"));
    assert_eq!(value(&vars, "FROM_RC"), Some("yes"));
    assert_eq!(value(&vars, shell_env::GUARD), None, "the guard is not passed on");
    assert!(vars.iter().all(|(k, v)| !k.is_empty() && !v.contains("motd")), "{vars:?}");
}

#[test]
fn a_shell_that_takes_too_long_is_killed_and_the_call_gives_up() {
    let dir = scratch("shell-env-timeout");
    let shell = script(&dir, "slow-shell", "sleep 30");
    let started = Instant::now();
    let err = shell_env::resolve(&shell, Duration::from_millis(300)).unwrap_err();
    assert_eq!(err, ShellEnvError::Timeout);
    assert!(started.elapsed() < Duration::from_secs(5), "{:?}", started.elapsed());
}

#[test]
fn a_shell_that_fails_without_markers_is_an_error_with_its_stderr() {
    let dir = scratch("shell-env-fail");
    let shell = script(&dir, "broken-shell", "echo 'bad rc' >&2\nexit 3");
    match shell_env::resolve(&shell, Duration::from_secs(10)) {
        Err(ShellEnvError::Failed(why)) => assert!(why.contains("bad rc"), "{why}"),
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_missing_shell_is_a_spawn_error() {
    let err = shell_env::resolve(Path::new("/nonexistent/rscode-shell"), Duration::from_secs(1)).unwrap_err();
    assert!(matches!(err, ShellEnvError::Spawn(_)), "{err:?}");
}

#[test]
fn a_resolution_started_inside_one_is_skipped_and_the_shell_never_runs() {
    let dir = scratch("shell-env-guard");
    let ran = dir.join("ran");
    let shell = script(&dir, "guarded-shell", &format!("touch '{}'", ran.display()));
    let err = shell_env::resolve_with(&shell, Duration::from_secs(10), true).unwrap_err();
    assert_eq!(err, ShellEnvError::Guarded);
    assert!(!ran.exists(), "the shell must not run when guarded");
    // Unguarded, the same shell runs (and fails: it prints no markers).
    assert!(shell_env::resolve_with(&shell, Duration::from_secs(10), false).is_err());
    assert!(ran.exists());
}

#[test]
fn for_host_falls_back_to_nothing_when_resolution_fails() {
    assert!(shell_env::for_host(Path::new("/nonexistent/rscode-shell"), Duration::from_secs(1)).is_empty());
}

#[test]
fn the_default_shell_is_the_login_shell_or_sh() {
    let shell = shell_env::default_shell();
    assert!(shell.is_absolute(), "{shell:?}");
}
