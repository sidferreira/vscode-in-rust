//! upstream: src/vs/workbench/services/extensions/electron-browser/localProcessExtensionHost.ts @ 50e1350fc78d
//!
//! ADR-0100 on the real host: the environment given to the host is what its extensions see. The
//! fixture spawns `rscode-probe` by its bare name (as a language server started from `command`
//! does). Without the login-shell environment the probe is not on the host's `PATH`; with it, the
//! probe runs. Needs Node and `RSCODE_CODE_OSS`; otherwise each test prints why and passes.

#![cfg(unix)]

mod common;

use common::{T, scratch};
use serde_json::json;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_exthost::shell_env;
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::Launch;

fn lsp_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp-fixture")
}

/// An executable `rscode-probe` in `dir` that prints `found`.
fn probe_in(dir: &Path) {
    let path = dir.join("rscode-probe");
    std::fs::write(&path, "#!/bin/sh\necho found\n").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

fn boot(name: &str, env: Vec<(String, String)>) -> Option<ExtensionHost> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = scratch(name);
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace)),
        extensions: vec![ExtensionDescription::from_dir(&lsp_fixture()).unwrap()],
        default_settings: json!({}),
        user_settings: json!({}),
        env,
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    Some(host)
}

fn probe(host: &mut ExtensionHost) -> String {
    match host
        .execute_contributed_command("lsp-fixture.probe", vec![], T)
        .unwrap()
    {
        Reply::Json(v) => v.as_str().expect("a string").to_string(),
        other => panic!("{other:?}"),
    }
}

/// `PATH` with `dir` first, as the login shell would set it.
fn path_with(dir: &Path) -> String {
    format!("{}:/usr/bin:/bin", dir.display())
}

#[test]
fn without_the_shell_environment_a_bare_name_on_the_login_path_is_not_found() {
    let dir = scratch("shell-env-host-missing").join("login-bin");
    std::fs::create_dir_all(&dir).unwrap();
    probe_in(&dir);
    let Some(mut host) = boot("shell-env-host-missing", vec![]) else {
        return;
    };
    // The test runner's PATH does not contain `dir`.
    assert_eq!(probe(&mut host), "missing");
    host.shutdown(Duration::from_secs(5));
}

#[test]
fn the_login_shell_environment_reaches_the_host_and_its_children() {
    let base = scratch("shell-env-host-resolved");
    let dir = base.join("login-bin");
    std::fs::create_dir_all(&dir).unwrap();
    probe_in(&dir);
    // A fake login shell whose PATH includes `dir`, resolved the way the app resolves $SHELL.
    let shell = base.join("fake-login-shell");
    std::fs::write(
        &shell,
        format!(
            "#!/bin/sh\necho 'rc noise'\nexport PATH='{}'\neval \"$4\"\n",
            path_with(&dir)
        ),
    )
    .unwrap();
    std::fs::set_permissions(&shell, std::fs::Permissions::from_mode(0o755)).unwrap();
    let env = shell_env::resolve(&shell, Duration::from_secs(10)).expect("resolves");
    let Some(mut host) = boot("shell-env-host-resolved", env) else {
        return;
    };
    assert_eq!(probe(&mut host), "found");
    host.shutdown(Duration::from_secs(5));
}
