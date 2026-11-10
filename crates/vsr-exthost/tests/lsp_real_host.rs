//! upstream: src/vs/workbench/api/node/extHostMcpNode.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadDiagnostics.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostLanguageFeatures.ts @ 50e1350fc78d
//!
//! A language server over stdio, end to end (ADR-0078). The fixture extension
//! (`fixtures/lsp-fixture`, plain JS) spawns `fake-server.js` itself with `child_process`, then
//! sends `initialize`, `didOpen` and `completion` over LSP framing. The main side sees the
//! diagnostics through `MainThreadDiagnostics` and the completions through the query API. Needs Node
//! and a Code-OSS build (`RSCODE_CODE_OSS`); otherwise each test prints why and passes.

mod common;

use common::{T, scratch};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_exthost::language_features::dto::{Position, completion_item_kind_name};
use vsr_exthost::language_features::query::CompletionTrigger;
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::Launch;

const TEXT: &str = "fn alpha(a, b) {}\n// TODO: fix beta\nfn beta() {}\n";
const OWNER: &str = "lsp-fixture";

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lsp-fixture")
}

/// Boot a host with the LSP fixture and wait until its providers are registered, or print why not.
fn boot(name: &str) -> Option<(ExtensionHost, PathBuf)> {
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
        workspace: Some(("ws".into(), workspace.clone())),
        extensions: vec![ExtensionDescription::from_dir(&fixture_dir()).unwrap()],
        default_settings: json!({}),
        user_settings: json!({}),
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    assert!(
        host.main_thread.extensions.activation_errors.is_empty(),
        "{:?}",
        host.main_thread.extensions.activation_errors
    );
    common::wait_until(&mut host, |h| {
        !h.main_thread.language_features.registrations.is_empty()
    });
    let file = workspace.join("a.lsp");
    host.open_document(&file, TEXT, "lsplang", T).unwrap();
    Some((host, file))
}

/// The host's `lsp-fixture.info` command: where Node comes from and the two pids.
fn info(host: &mut ExtensionHost) -> Value {
    match host
        .execute_contributed_command("lsp-fixture.info", vec![], T)
        .unwrap()
    {
        Reply::Json(v) => v,
        other => panic!("{other:?}"),
    }
}

fn pid_of(v: &Value, key: &str) -> u32 {
    v[key].as_u64().unwrap_or_else(|| panic!("no {key} in {v}")) as u32
}

/// Alive, and not a zombie (a zombie has exited; only its parent has not reaped it yet).
fn alive(pid: u32) -> bool {
    let out = Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
        .expect("run ps");
    out.status.success() && !String::from_utf8_lossy(&out.stdout).trim().starts_with('Z')
}

/// Polls until the process is gone; `false` if it is still there after `timeout`.
fn gone_within(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while alive(pid) {
        if Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    true
}

/// Kills a process it guards if it is still alive when dropped: a failed assertion must not leave a
/// language server running.
struct Reaper(u32);

impl Drop for Reaper {
    fn drop(&mut self) {
        if alive(self.0) {
            let _ = Command::new("kill")
                .args(["-KILL", &self.0.to_string()])
                .status();
        }
    }
}

/// The key `MainThreadDiagnostics` files a document's markers under (`scheme://authority/path`).
fn resource_key(file: &Path) -> String {
    format!("file://{}", file.display())
}

#[test]
fn the_extension_runs_on_node_and_its_server_is_a_child_process() {
    let Some((mut host, _file)) = boot("lsp-spawn") else {
        return;
    };
    let v = info(&mut host);
    // Plain Node, not Electron: no `electron` version, and the same binary Rust launched.
    assert!(v["electron"].is_null(), "{v}");
    let node = Launch::from_env().unwrap().node;
    assert_eq!(
        std::fs::canonicalize(v["execPath"].as_str().unwrap()).unwrap(),
        std::fs::canonicalize(&node).unwrap(),
        "{v}"
    );
    assert_eq!(v["serverName"], "fake-server", "{v}");
    let server = pid_of(&v, "serverPid");
    let _reaper = Reaper(server);
    assert!(alive(server), "server {server} should be running");
    host.shutdown(Duration::from_secs(5));
    assert!(
        gone_within(server, Duration::from_secs(5)),
        "orphan {server}"
    );
}

#[test]
fn diagnostics_from_the_server_reach_the_main_thread() {
    let Some((mut host, file)) = boot("lsp-diagnostics") else {
        return;
    };
    let key = resource_key(&file);
    common::wait_until(&mut host, |h| {
        !h.main_thread.diagnostics.markers_for(&key).is_empty()
    });
    let markers = host.main_thread.diagnostics.markers_for(&key);
    assert_eq!(markers.len(), 1, "{markers:?}");
    let m = markers[0];
    // `TODO` on line 2 (1-based), columns 4 to 8; severity Error (MarkerSeverity 8).
    assert_eq!(m["message"], "fake: TODO found", "{m}");
    assert_eq!(m["severity"], 8, "{m}");
    assert_eq!(m["startLineNumber"], 2, "{m}");
    assert_eq!(m["startColumn"], 4, "{m}");
    assert_eq!(m["endColumn"], 8, "{m}");
    assert_eq!(host.main_thread.diagnostics.counts(), (1, 0));
    assert!(host.main_thread.diagnostics.by_owner.contains_key(OWNER));

    let server = pid_of(&info(&mut host), "serverPid");
    let _reaper = Reaper(server);
    host.shutdown(Duration::from_secs(5));
    assert!(
        gone_within(server, Duration::from_secs(5)),
        "orphan {server}"
    );
}

#[test]
fn completion_is_forwarded_to_the_server_and_back() {
    let Some((mut host, file)) = boot("lsp-completion") else {
        return;
    };
    // 1-based (2, 6) is 0-based (1, 5) on the wire; the server echoes the position it received.
    let got = host
        .provide_completion_items(&file, Position::new(2, 6), CompletionTrigger::Invoke, T)
        .unwrap();
    assert!(got.errors.is_empty(), "{:?}", got.errors);
    let (_, list) = got.results.into_iter().next().expect("one provider");
    let labels: Vec<_> = list.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["serverAlpha", "serverPos:1:5"]);
    assert!(!list.is_incomplete);
    assert_eq!(
        completion_item_kind_name(list.items[0].kind.unwrap()),
        "Function"
    );
    assert_eq!(list.items[0].insert_text, "serverAlpha");
    assert_eq!(list.items[0].detail.as_deref(), Some("from fake-server"));
    // LSP kinds are 1-based (6 is Variable); the extension API's enum starts at Text = 0, so the
    // fixture sends API kind 5, which names as Variable.
    assert_eq!(
        completion_item_kind_name(list.items[1].kind.unwrap()),
        "Variable"
    );

    let server = pid_of(&info(&mut host), "serverPid");
    let _reaper = Reaper(server);
    host.shutdown(Duration::from_secs(5));
    assert!(
        gone_within(server, Duration::from_secs(5)),
        "orphan {server}"
    );
}

#[test]
fn a_host_that_is_killed_takes_its_server_down() {
    // The risk with children of a process we do not control: no shutdown runs, so only the pipe
    // closing ends the server. Kill the host outright and check the server follows.
    let Some((mut host, _file)) = boot("lsp-killed") else {
        return;
    };
    let v = info(&mut host);
    let (server, host_pid) = (pid_of(&v, "serverPid"), pid_of(&v, "hostPid"));
    let _reaper = Reaper(server);
    assert!(alive(server));
    let status = Command::new("kill")
        .args(["-KILL", &host_pid.to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    assert!(
        gone_within(server, Duration::from_secs(5)),
        "server {server} outlived its killed host {host_pid}"
    );
    // The host is dead; dropping it must not hang.
    drop(host);
}
