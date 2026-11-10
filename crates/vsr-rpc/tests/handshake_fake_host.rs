//! upstream: src/vs/workbench/api/node/extensionHostProcess.ts @ 50e1350fc78d
//!
//! Headless handshake against a Node stand-in (`tests/fixtures/fake_host.js`, an independent
//! implementation of the framing and the RPC messages). Runs wherever `node` is on PATH
//! (or `RSCODE_NODE` is set), no Code-OSS build needed; skips otherwise.

use std::path::PathBuf;
use std::time::Duration;
use vsr_rpc::handshake::{Host, InitParams, RecordingHandler, find_on_path, init_data};
use vsr_rpc::proxy_ids;

#[test]
fn handshake_and_one_request_against_the_node_stand_in() {
    let node = match std::env::var_os("RSCODE_NODE")
        .map(PathBuf::from)
        .or_else(|| find_on_path("node"))
    {
        Some(n) => n,
        None => {
            eprintln!(
                "SKIP handshake_and_one_request_against_the_node_stand_in: node is not on PATH"
            );
            return;
        }
    };
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fake_host.js");
    let socket = std::env::temp_dir().join(format!("rscode-fake-{}.sock", std::process::id()));
    let id = proxy_ids::main::MAIN_THREAD_COMMANDS.to_string();
    let mut host = Host::spawn(
        &node,
        &script,
        &PathBuf::from(env!("CARGO_MANIFEST_DIR")),
        &[("FAKE_RPC_ID", &id)],
        &socket,
        Duration::from_secs(15),
    )
    .unwrap_or_else(|e| panic!("spawn: {e}"));
    let init = init_data(&InitParams {
        version: "1.147.0".into(),
        app_root: "/app".into(),
        storage_root: "/storage".into(),
        workspace: None,
    });
    let mut report = host
        .handshake(&init, Duration::from_secs(15))
        .unwrap_or_else(|e| panic!("handshake: {e}"));
    assert!(report.ready_seen && report.initialized_seen);

    let mut handler = RecordingHandler::default();
    host.serve(&mut handler, Duration::from_millis(1500), &mut report)
        .unwrap();
    assert_eq!(report.calls.len(), 1, "{:?}", report.calls);
    assert_eq!(report.calls[0].proxy, Some("MainThreadCommands"));
    assert_eq!(report.calls[0].method, "$registerCommand");

    // The stand-in exits 0 only when it saw Acknowledged + ReplyOKEmpty, then Terminate.
    let exit = host.terminate(Duration::from_secs(5));
    assert!(exit.exited_cleanly, "stand-in failed: {}", exit.stderr);
}
