//! upstream: src/vs/workbench/api/common/extHostExtensionService.ts @ 50e1350fc78d
//!
//! Why `$startExtensionHost` was acknowledged but never answered (ADR-0062). Two things in
//! `ExtHostExtensionService`:
//!
//! * `initialize()` awaits `ExtHostWorkspace.waitForInitializeCall()` before it opens
//!   `_readyToStartExtensionHost`; that barrier opens only when the main thread calls
//!   `ExtHostWorkspace.$initializeWorkspace`. `_startExtensionHost()` waits on it, so
//!   `$startExtensionHost` hangs until the workspace is initialised.
//! * With `autoStart: true`, `initialize()` calls `_startExtensionHost()` itself and sets
//!   `_started`; a later `$startExtensionHost` then throws "Extension host is already started!".
//!
//! Skips (and passes) when Node or the Code-OSS build is missing.

use std::path::PathBuf;
use std::time::Duration;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::handshake::{
    Host, InitParams, Launch, RecordingHandler, Report, configuration_data, empty_delta, init_data,
    workspace_data,
};
use vsr_rpc::proxy_ids::ext_host;

struct Rig {
    host: Host,
    report: Report,
    handler: RecordingHandler,
    workspace: PathBuf,
}

fn rig(name: &str, auto_start: bool) -> Option<Rig> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    let storage = base.join("storage");
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&storage).unwrap();
    let socket = std::env::temp_dir().join(format!("rscode-{name}-{}.sock", std::process::id()));
    let mut host = Host::spawn(
        &launch.node,
        &launch.code_oss.join("out/bootstrap-fork.js"),
        &launch.code_oss,
        &[],
        &socket,
        Duration::from_secs(30),
    )
    .unwrap_or_else(|e| panic!("spawn: {e}"));
    let init = init_data(&InitParams {
        version: "1.147.0".into(),
        app_root: launch.code_oss.clone(),
        storage_root: storage,
        workspace: Some(("ws".into(), workspace.clone())),
        auto_start,
    });
    let mut report = host
        .handshake(&init, Duration::from_secs(30))
        .unwrap_or_else(|e| panic!("handshake: {e}"));
    let mut handler = RecordingHandler::default();
    host.serve(&mut handler, Duration::from_secs(1), &mut report)
        .unwrap();
    Some(Rig {
        host,
        report,
        handler,
        workspace,
    })
}

fn start(
    r: &mut Rig,
    t: Duration,
) -> Result<vsr_rpc::rpc::Completed, vsr_rpc::handshake::HandshakeError> {
    r.host.request(
        ext_host::EXT_HOST_EXTENSION_SERVICE,
        "$startExtensionHost",
        &[Arg::Json(empty_delta())],
        &mut r.handler,
        &mut r.report,
        t,
    )
}

fn init_workspace(r: &mut Rig) {
    let ws = workspace_data("ws", &r.workspace);
    let done = r
        .host
        .request(
            ext_host::EXT_HOST_WORKSPACE,
            "$initializeWorkspace",
            &[Arg::Json(ws), Arg::Json(serde_json::json!(true))],
            &mut r.handler,
            &mut r.report,
            Duration::from_secs(10),
        )
        .unwrap_or_else(|e| panic!("$initializeWorkspace: {e}"));
    assert!(done.result.is_ok(), "{done:?}");
    let done = r
        .host
        .request(
            ext_host::EXT_HOST_CONFIGURATION,
            "$initializeConfiguration",
            &[Arg::Json(configuration_data(
                serde_json::json!({}),
                serde_json::json!({}),
            ))],
            &mut r.handler,
            &mut r.report,
            Duration::from_secs(10),
        )
        .unwrap_or_else(|e| panic!("$initializeConfiguration: {e}"));
    assert!(done.result.is_ok(), "{done:?}");
}

// Upstream source: src/vs/workbench/api/common/extHostExtensionService.ts (no upstream test covers the barriers)
#[test]
fn start_extension_host_is_not_answered_until_workspace_and_configuration_are_initialised() {
    let Some(mut r) = rig("start-no-workspace", false) else {
        return;
    };
    let err = start(&mut r, Duration::from_secs(3));
    assert!(
        matches!(err, Err(vsr_rpc::handshake::HandshakeError::Timeout(_))),
        "expected the hang, got {err:?}"
    );
    r.host.terminate(Duration::from_secs(3));
}

#[test]
fn start_extension_host_is_answered_once_workspace_and_configuration_are_initialised() {
    let Some(mut r) = rig("start-with-workspace", false) else {
        return;
    };
    init_workspace(&mut r);
    let done = start(&mut r, Duration::from_secs(15));
    let done = done.unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(done.result, Ok(Reply::Empty), "{done:?}");
    let exit = r.host.terminate(Duration::from_secs(5));
    assert!(exit.exited_cleanly, "{}", exit.stderr);
}

#[test]
fn auto_start_makes_a_later_start_extension_host_an_error() {
    let Some(mut r) = rig("start-autostart", true) else {
        return;
    };
    init_workspace(&mut r);
    let done = start(&mut r, Duration::from_secs(15)).unwrap_or_else(|e| panic!("{e}"));
    let text = format!("{done:?}");
    assert!(text.contains("already started"), "{text}");
    r.host.terminate(Duration::from_secs(3));
}
