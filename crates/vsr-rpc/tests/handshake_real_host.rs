//! upstream: src/vs/workbench/api/node/extensionHostProcess.ts @ 50e1350fc78d
//!
//! Headless integration: upstream's own compiled `extensionHostProcess` under Node completes
//! `Ready` -> init data -> `Initialized` against our Rust main-thread side (ADR-0060).
//!
//! Skips (and passes) when Node or the Code-OSS build is missing. To run it:
//!
//! ```sh
//! RSCODE_CODE_OSS=/path/to/code-oss-checkout cargo test -p vsr-rpc --test handshake_real_host -- --nocapture
//! ```
//!
//! where the checkout has `out/` from `node build/next/index.ts transpile` (see ADR-0060).
//! No window is opened and no focus changes: the host is a plain Node process.

use std::path::PathBuf;
use std::time::Duration;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::handshake::{Host, InitParams, Launch, RecordingHandler, init_data};
use vsr_rpc::proxy_ids;

fn scratch(name: &str) -> PathBuf {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn real_extension_host_completes_the_handshake() {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP real_extension_host_completes_the_handshake: {why}");
            return;
        }
    };
    let storage = scratch("exthost-storage");
    let workspace = scratch("exthost-workspace");
    let socket = std::env::temp_dir().join(format!("rscode-rpc-{}.sock", std::process::id()));
    let script = launch.code_oss.join("out/bootstrap-fork.js");

    let mut host = Host::spawn(
        &launch.node,
        &script,
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
        workspace: Some(("ws".into(), workspace)),
    });
    let mut report = host
        .handshake(&init, Duration::from_secs(30))
        .unwrap_or_else(|e| panic!("handshake: {e}"));
    assert!(report.ready_seen && report.initialized_seen);

    // After Initialized the host starts talking to MainThread* proxies; answer them all with
    // `undefined` for a moment and check the proxy ids resolve through the generated table.
    let mut handler = RecordingHandler::default();
    host.serve(&mut handler, Duration::from_secs(3), &mut report)
        .unwrap_or_else(|e| panic!("serve: {e}"));

    // The other direction: requests we encode, decoded by upstream and answered by the real
    // ExtHostExtensionService (`$test_*` are upstream's own round-trip probes): JSON args and a
    // JSON reply, a buffer argument (mixed-args layout), and a buffer reply.
    let svc = proxy_ids::ext_host::EXT_HOST_EXTENSION_SERVICE;
    let t = Duration::from_secs(10);
    let done = host
        .request(
            svc,
            "$test_latency",
            &[Arg::Json(serde_json::json!(41))],
            &mut handler,
            &mut report,
            t,
        )
        .unwrap_or_else(|e| panic!("$test_latency: {e}"));
    assert_eq!(done.result, Ok(Reply::Json(serde_json::json!(41))));
    let done = host
        .request(
            svc,
            "$test_up",
            &[Arg::Buffer(vec![1, 2, 3, 4, 5])],
            &mut handler,
            &mut report,
            t,
        )
        .unwrap_or_else(|e| panic!("$test_up: {e}"));
    assert_eq!(done.result, Ok(Reply::Json(serde_json::json!(5))));
    let done = host
        .request(
            svc,
            "$test_down",
            &[Arg::Json(serde_json::json!(6))],
            &mut handler,
            &mut report,
            t,
        )
        .unwrap_or_else(|e| panic!("$test_down: {e}"));
    assert!(
        matches!(&done.result, Ok(Reply::Buffer(b)) if b.len() == 6),
        "{done:?}"
    );

    let exit = host.terminate(Duration::from_secs(5));
    for c in &report.calls {
        eprintln!(
            "{:>3} {:<28} {} {:.300}",
            c.rpc_id,
            c.proxy.unwrap_or("?"),
            c.method,
            format!("{:?}", c.args)
        );
    }
    eprintln!(
        "clean exit: {}\nstdout: {}\nstderr: {}",
        exit.exited_cleanly, exit.stdout, exit.stderr
    );

    // Nothing the host does at startup may be an unexpected error (the missing native `spdlog`
    // binding of a scripts-less install is only logged through MainThreadConsole).
    assert!(
        !report
            .calls
            .iter()
            .any(|c| c.method == "$onUnexpectedError"),
        "the host reported an unexpected error"
    );
    assert!(
        !report.calls.is_empty(),
        "the host made no request after Initialized"
    );
    for c in &report.calls {
        assert!(proxy_ids::is_main(c.rpc_id), "{c:?}: not a MainContext id");
        assert!(
            c.proxy.is_some_and(|n| n.starts_with("MainThread")),
            "{c:?}: id is not a MainThread* shape"
        );
    }
}
