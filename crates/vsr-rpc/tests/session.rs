//! upstream: src/vs/workbench/services/extensions/common/rpcProtocol.ts @ 50e1350fc78d
//!
//! The long-lived connection (ADR-0063): a loopback between two `Session`s (no Node needed) and
//! the same against upstream's real extension host (skips without `RSCODE_CODE_OSS` / node).

use serde_json::json;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::handshake::{
    Connection, Host, InitParams, Launch, configuration_data, empty_delta, init_data,
    workspace_data,
};
use vsr_rpc::proxy_ids::ext_host;
use vsr_rpc::rpc::HandlerError;
use vsr_rpc::session::{Event, Session, WaitError};

const T: Duration = Duration::from_secs(10);

fn loopback() -> (
    (Session, std::sync::mpsc::Receiver<Event>),
    (Session, std::sync::mpsc::Receiver<Event>),
) {
    let (a, b) = UnixStream::pair().unwrap();
    (
        Session::start(Connection::new(a), None),
        Session::start(Connection::new(b), None),
    )
}

#[test]
fn replies_may_come_in_any_order() {
    let ((a, _a_events), (b, b_events)) = loopback();
    let p1 = a.request(1, "$one", &[Arg::Json(json!(1))]).unwrap();
    let p2 = a.request(1, "$two", &[Arg::Json(json!(2))]).unwrap();
    let p3 = a.request(1, "$three", &[]).unwrap();
    // The other side receives all three, then answers last-first, and one with an error.
    let mut got = Vec::new();
    for _ in 0..3 {
        match b_events.recv_timeout(T).unwrap() {
            Event::Request(call, responder) => got.push((call, responder)),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(
        got.iter()
            .map(|(c, _)| c.method.as_str())
            .collect::<Vec<_>>(),
        ["$one", "$two", "$three"]
    );
    let mut got = got.into_iter().rev();
    let (_, r3) = got.next().unwrap();
    r3.reply(Err(HandlerError::error("nope")));
    let (_, r2) = got.next().unwrap();
    r2.reply(Ok(Reply::Json(json!("two"))));
    let (_, r1) = got.next().unwrap();
    // The first reply comes from another thread, later.
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(100));
        r1.reply(Ok(Reply::Empty));
    });
    let d3 = p3.wait(T).unwrap();
    assert!(d3.result.is_err(), "{d3:?}");
    assert_eq!(p2.wait(T).unwrap().result, Ok(Reply::Json(json!("two"))));
    assert_eq!(p1.wait(T).unwrap().result, Ok(Reply::Empty));
    assert_eq!(a.pending(), 0);
    drop(b);
}

#[test]
fn a_reply_that_never_comes_times_out_and_a_closed_peer_is_reported() {
    let ((a, a_events), (b, b_events)) = loopback();
    let p = a.request(1, "$never", &[]).unwrap();
    assert_eq!(
        p.wait(Duration::from_millis(200)).unwrap_err(),
        WaitError::Timeout
    );
    // The request is acknowledged on arrival even though nobody replies.
    assert!(matches!(
        b_events.recv_timeout(T).unwrap(),
        Event::Request(..)
    ));
    b.shutdown(Duration::from_secs(1));
    assert_eq!(p.wait(T).unwrap_err(), WaitError::Closed);
    loop {
        match a_events.recv_timeout(T).unwrap() {
            Event::Closed => break,
            // `shutdown` sends the one-byte Terminate message, which is not an RPC message.
            Event::BadMessage(_) => {}
            other => panic!("{other:?}"),
        }
    }
    assert!(a.is_closed());
}

#[test]
fn a_message_that_does_not_decode_is_reported_and_the_connection_survives() {
    use std::io::Write;
    use vsr_rpc::codec::encode_request;
    use vsr_rpc::framing::{Frame, FrameType};
    let (ours, mut raw) = UnixStream::pair().unwrap();
    let (_session, events) = Session::start(Connection::new(ours), None);
    let write = |raw: &mut UnixStream, id: u32, data: Vec<u8>| {
        raw.write_all(&Frame::new(FrameType::Regular, id, 0, data).encode())
            .unwrap();
    };
    write(&mut raw, 1, vec![250, 1, 2]);
    assert!(matches!(
        events.recv_timeout(T).unwrap(),
        Event::BadMessage(_)
    ));
    write(
        &mut raw,
        2,
        encode_request(1, 7, "$ok", &[], false).unwrap(),
    );
    match events.recv_timeout(T).unwrap() {
        Event::Request(call, _) => assert_eq!((call.rpc_id, call.method.as_str()), (7, "$ok")),
        other => panic!("{other:?}"),
    }
    drop(raw);
    assert!(matches!(events.recv_timeout(T).unwrap(), Event::Closed));
}

fn scratch(name: &str) -> PathBuf {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn real_host_long_lived_session() {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP real_host_long_lived_session: {why}");
            return;
        }
    };
    let base = scratch("session-real");
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let socket = std::env::temp_dir().join(format!("rscode-session-{}.sock", std::process::id()));
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
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace.clone())),
        auto_start: false,
    });
    host.handshake(&init, Duration::from_secs(30))
        .unwrap_or_else(|e| panic!("handshake: {e}"));
    let (session, events) = Session::from_host(host);

    // The main-thread side runs on its own thread, like the app's would: it answers every host
    // request from a table (here: undefined, with the two values the host needs) and counts them.
    let (seen_tx, seen_rx) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let mut closed = false;
        for ev in events {
            match ev {
                Event::Request(call, responder) => {
                    let reply = match call.method.as_str() {
                        "$getInitialState" => {
                            Reply::Json(json!({ "isFocused": false, "isActive": false }))
                        }
                        "$getTools" => Reply::Json(json!([])),
                        _ => Reply::Empty,
                    };
                    let _ = seen_tx.send(call.method.clone());
                    responder.reply(Ok(reply));
                }
                Event::Closed => {
                    closed = true;
                    break;
                }
                _ => {}
            }
        }
        closed
    });

    let call = |id: u8, m: &str, args: &[Arg]| session.request(id, m, args).unwrap();
    let ws = call(
        ext_host::EXT_HOST_WORKSPACE,
        "$initializeWorkspace",
        &[
            Arg::Json(workspace_data("ws", &workspace)),
            Arg::Json(json!(true)),
        ],
    );
    let cfg = call(
        ext_host::EXT_HOST_CONFIGURATION,
        "$initializeConfiguration",
        &[Arg::Json(configuration_data(json!({}), json!({})))],
    );
    assert!(ws.wait(T).unwrap().result.is_ok());
    assert!(cfg.wait(T).unwrap().result.is_ok());
    let start = call(
        ext_host::EXT_HOST_EXTENSION_SERVICE,
        "$startExtensionHost",
        &[Arg::Json(empty_delta())],
    );
    // Several in flight at once, waited on in reverse.
    let svc = ext_host::EXT_HOST_EXTENSION_SERVICE;
    let lat: Vec<_> = (0..5)
        .map(|n| call(svc, "$test_latency", &[Arg::Json(json!(n))]))
        .collect();
    for (n, p) in lat.iter().enumerate().rev() {
        assert_eq!(p.wait(T).unwrap().result, Ok(Reply::Json(json!(n))));
    }
    assert_eq!(start.wait(T).unwrap().result, Ok(Reply::Empty));
    assert!(
        seen_rx.try_iter().any(|m| m == "$registerCommand"),
        "the host's startup requests reached the app thread"
    );

    let exit = session.shutdown(Duration::from_secs(5)).unwrap();
    assert!(exit.exited_cleanly, "{}", exit.stderr);
    assert!(
        server.join().unwrap(),
        "the events channel ended with Closed"
    );
}
