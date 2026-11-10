//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//!
//! The `MainThread*` models without a host process (always run): requests as the host would send
//! them, applied to `MainThread`.

use serde_json::{Value, json};
use vsr_exthost::main_thread::MainThread;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::proxy_ids::main;
use vsr_rpc::rpc::{Call, HandlerError};

fn send(
    mt: &mut MainThread,
    rpc_id: u8,
    method: &str,
    args: Vec<Value>,
) -> Result<Reply, HandlerError> {
    mt.handle(&Call {
        req: 1,
        rpc_id,
        method: method.into(),
        args: args.into_iter().map(Arg::Json).collect(),
        cancellable: false,
    })
}

#[test]
fn status_bar_entries_are_ordered_and_disposed() {
    let mut mt = MainThread::new();
    let set = |mt: &mut MainThread, id: &str, left: bool, prio: Option<i64>| {
        send(
            mt,
            main::MAIN_THREAD_STATUS_BAR,
            "$setEntry",
            vec![
                json!(id),
                json!("s"),
                json!("pub.ext"),
                json!("name"),
                json!(id),
                Value::Null,
                json!(false),
                Value::Null,
                Value::Null,
                Value::Null,
                json!(left),
                json!(prio),
                Value::Null,
            ],
        )
        .unwrap();
    };
    set(&mut mt, "r-low", false, Some(1));
    set(&mut mt, "r-high", false, Some(100));
    set(&mut mt, "l-low", true, Some(1));
    set(&mut mt, "l-high", true, Some(100));
    let order: Vec<_> = mt
        .status_bar
        .sorted()
        .iter()
        .map(|e| e.id.as_str())
        .collect();
    // Left side first, higher priority nearer the left edge; right side mirrored.
    assert_eq!(order, ["l-high", "l-low", "r-low", "r-high"]);
    send(
        &mut mt,
        main::MAIN_THREAD_STATUS_BAR,
        "$disposeEntry",
        vec![json!("l-high")],
    )
    .unwrap();
    assert_eq!(mt.status_bar.entries.len(), 3);
    assert!(mt.unhandled.is_empty());
}

#[test]
fn diagnostics_are_per_owner_and_resource() {
    let mut mt = MainThread::new();
    let uri = |p: &str| json!({ "$mid": 1, "scheme": "file", "authority": "", "path": p });
    let marker = |sev: u64| json!({ "severity": sev, "message": "m", "startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 2 });
    let d = main::MAIN_THREAD_DIAGNOSTICS;
    send(
        &mut mt,
        d,
        "$changeMany",
        vec![
            json!("a"),
            json!([
                [uri("/x"), [marker(8), marker(4)]],
                [uri("/y"), [marker(8)]]
            ]),
        ],
    )
    .unwrap();
    send(
        &mut mt,
        d,
        "$changeMany",
        vec![json!("b"), json!([[uri("/x"), [marker(8)]]])],
    )
    .unwrap();
    assert_eq!(mt.diagnostics.counts(), (3, 1));
    assert_eq!(
        mt.diagnostics.markers_for("file:///x").len(),
        3,
        "owners add up per resource"
    );
    // `undefined` markers (null on the wire) clear one resource of one owner.
    send(
        &mut mt,
        d,
        "$changeMany",
        vec![json!("a"), json!([[uri("/x"), null]])],
    )
    .unwrap();
    assert_eq!(mt.diagnostics.markers_for("file:///x").len(), 1);
    send(&mut mt, d, "$clear", vec![json!("a")]).unwrap();
    assert_eq!(mt.diagnostics.counts(), (1, 0));
}

#[test]
fn commands_without_an_executor_reject_and_registration_is_tracked() {
    let mut mt = MainThread::new();
    let c = main::MAIN_THREAD_COMMANDS;
    send(&mut mt, c, "$registerCommand", vec![json!("a.b")]).unwrap();
    assert_eq!(
        send(&mut mt, c, "$getCommands", vec![]),
        Ok(Reply::Json(json!(["a.b"])))
    );
    send(&mut mt, c, "$unregisterCommand", vec![json!("a.b")]).unwrap();
    assert!(mt.commands.registered.is_empty());
    let err = send(
        &mut mt,
        c,
        "$executeCommand",
        vec![json!("nope"), json!([]), json!(false)],
    );
    assert!(matches!(err, Err(HandlerError::Error { message, .. }) if message.contains("nope")));
}

#[test]
fn unknown_requests_are_recorded_and_answered_with_undefined() {
    let mut mt = MainThread::new();
    assert_eq!(
        send(
            &mut mt,
            main::MAIN_THREAD_TASK,
            "$registerTaskSystem",
            vec![]
        ),
        Ok(Reply::Empty)
    );
    assert_eq!(mt.unhandled.len(), 1);
    assert_eq!(mt.unhandled[0].method, "$registerTaskSystem");
    // The two the host needs a value for.
    assert!(matches!(
        send(
            &mut mt,
            main::MAIN_THREAD_WINDOW,
            "$getInitialState",
            vec![]
        ),
        Ok(Reply::Json(_))
    ));
}

#[test]
fn document_edits_make_ordered_changes_with_offsets() {
    use std::path::Path;
    use vsr_exthost::documents::{Documents, TextEdit};
    let mut docs = Documents::default();
    let p = Path::new("/ws/a.txt");
    let delta = docs.add_document(p, "abc\ndef\n", "plaintext");
    assert_eq!(
        delta["addedDocuments"][0]["lines"],
        json!(["abc", "def", ""])
    );
    let (_, event, dirty) = docs
        .edit(
            p,
            &[
                TextEdit {
                    start_line: 1,
                    start_column: 2,
                    end_line: 1,
                    end_column: 3,
                    text: "X".into(),
                },
                TextEdit {
                    start_line: 2,
                    start_column: 1,
                    end_line: 2,
                    end_column: 4,
                    text: "".into(),
                },
            ],
        )
        .unwrap();
    assert!(dirty);
    assert_eq!(docs.open["file:///ws/a.txt"].text, "aXc\n\n");
    assert_eq!(event["versionId"], 2);
    // From the end of the document to the beginning, with offsets into the old text.
    assert_eq!(event["changes"][0]["rangeOffset"], 4);
    assert_eq!(event["changes"][0]["rangeLength"], 3);
    assert_eq!(event["changes"][1]["rangeOffset"], 1);
}
