//! upstream: src/vs/workbench/services/extensions/test/common/rpcProtocol.test.ts @ 50e1350fc78d
//!
//! Ported as codec and dispatch round trips between two [`RpcPeer`]s joined in memory, one acting
//! as `A` (the caller) and one as `B` (the actor, with the handler). Upstream's `delegate` becomes
//! the handler closure; a JavaScript `Promise` that rejects with `undefined` is `HandlerError::Empty`.
//!
//! Not ported: "cancelling a call via CancellationToken before" / "quickly", "passing
//! CancellationToken.None", "releases cancellation handler ..." and "does not track uncancellable
//! calls ..." (there are no cancellation tokens in this sans-IO layer; the `Cancel` message itself
//! is covered in `cancel_message_round_trip`), "issue #60450: Converting circular structure to
//! JSON" and "issue #81424: SerializeRequest should throw ..." (a `serde_json::Value` cannot be
//! circular).

use serde_json::{Value, json};
use vsr_rpc::codec::{self, Arg, Reply};
use vsr_rpc::rpc::{Call, Completed, HandlerError, RpcPeer};

const BB: u8 = 1; // upstream: `new ProxyIdentifier('bb')`

/// A calls `$m(args)` on B; B answers through `delegate`.
fn call(args: &[Arg], mut delegate: impl FnMut(&Call) -> Result<Reply, HandlerError>) -> Completed {
    let mut a = RpcPeer::new();
    let mut b = RpcPeer::new();
    let (_, raw) = a.request(BB, "$m", args, false).unwrap();
    let handled = b.handle(&raw, &mut delegate).unwrap();
    assert_eq!(handled.send.len(), 2, "an ack, then the reply");
    let mut noop = |_: &Call| -> Result<Reply, HandlerError> { panic!("A is not called") };
    let mut completed = None;
    for msg in &handled.send {
        let h = a.handle(msg, &mut noop).unwrap();
        if h.completed.is_some() {
            completed = h.completed;
        }
    }
    assert_eq!(a.pending(), 0);
    completed.expect("the reply arrived")
}

fn num(v: &Value) -> i64 {
    v.as_i64().unwrap()
}

fn json_arg(call: &Call, i: usize) -> Value {
    match &call.args[i] {
        Arg::Json(v) => v.clone(),
        other => panic!("argument {i} is {other:?}"),
    }
}

// upstream: RPCProtocol > simple call
#[test]
fn simple_call() {
    let done = call(&[Arg::Json(json!(4)), Arg::Json(json!(1))], |c| {
        Ok(Reply::Json(json!(
            num(&json_arg(c, 0)) + num(&json_arg(c, 1))
        )))
    });
    assert_eq!(done.result, Ok(Reply::Json(json!(5))));
}

// upstream: RPCProtocol > simple call without result
#[test]
fn simple_call_without_result() {
    let done = call(&[Arg::Json(json!(4)), Arg::Json(json!(1))], |_| {
        Ok(Reply::Empty)
    });
    assert_eq!(done.result, Ok(Reply::Empty));
}

// upstream: RPCProtocol > passing buffer as argument
#[test]
fn passing_buffer_as_argument() {
    let done = call(&[Arg::Buffer(vec![1, 2, 3, 4]), Arg::Json(json!(2))], |c| {
        let Arg::Buffer(b) = &c.args[0] else {
            panic!("not a buffer: {:?}", c.args[0])
        };
        Ok(Reply::Json(json!(b[num(&json_arg(c, 1)) as usize])))
    });
    assert_eq!(done.result, Ok(Reply::Json(json!(3))));
}

// upstream: RPCProtocol > returning a buffer
#[test]
fn returning_a_buffer() {
    let done = call(&[Arg::Json(json!(4)), Arg::Json(json!(1))], |_| {
        Ok(Reply::Buffer(vec![1, 2, 3, 4]))
    });
    assert_eq!(done.result, Ok(Reply::Buffer(vec![1, 2, 3, 4])));
}

// upstream: RPCProtocol > throwing an error
#[test]
fn throwing_an_error() {
    let done = call(&[Arg::Json(json!(4)), Arg::Json(json!(1))], |_| {
        Err(HandlerError::error("nope"))
    });
    let Err(Some(err)) = done.result else {
        panic!("{done:?}")
    };
    assert_eq!(err["message"], "nope");
    assert_eq!(err["$isError"], true);
    assert_eq!(err["name"], "Error");
}

// upstream: RPCProtocol > error promise
#[test]
fn error_promise() {
    let done = call(&[Arg::Json(json!(4)), Arg::Json(json!(1))], |_| {
        Err(HandlerError::Empty)
    });
    assert_eq!(done.result, Err(None));
}

// upstream: RPCProtocol > issue #72798: null errors are hard to digest
#[test]
fn thrown_non_error_values_arrive_as_they_are() {
    let done = call(&[Arg::Json(json!(4)), Arg::Json(json!(1))], |_| {
        Err(HandlerError::Value(json!({ "what": "what" })))
    });
    let Err(Some(err)) = done.result else {
        panic!("{done:?}")
    };
    assert_eq!(err["what"], "what");
}

// upstream: RPCProtocol > undefined arguments arrive as null
#[test]
fn undefined_arguments_arrive_as_null() {
    // `bProxy.$m(undefined, null)`: undefined forces the mixed layout; null stays JSON `null`.
    let done = call(&[Arg::Undefined, Arg::Json(Value::Null)], |c| {
        assert_eq!(c.args[0], Arg::Undefined);
        assert_eq!(c.args[1], Arg::Json(Value::Null));
        Ok(Reply::Json(json!(7)))
    });
    assert_eq!(done.result, Ok(Reply::Json(json!(7))));
}

// upstream: RPCProtocol > SerializableObjectWithBuffers is correctly transferred
#[test]
fn serializable_object_with_buffers_is_correctly_transferred() {
    let obj = json!({ "string": "hello", "buff": { "$$ref$$": 0 } });
    let done = call(
        &[
            Arg::ObjectWithBuffers {
                json: obj,
                buffers: vec![vec![1, 2, 3, 4]],
            },
            Arg::Undefined,
        ],
        |c| {
            let Arg::ObjectWithBuffers { json, buffers } = &c.args[0] else {
                panic!("{:?}", c.args[0])
            };
            assert_eq!(c.args[1], Arg::Undefined);
            let s = format!("{} world", json["string"].as_str().unwrap());
            Ok(Reply::JsonWithBuffers {
                json: json!({ "string": s, "buff": json["buff"] }),
                buffers: buffers.clone(),
            })
        },
    );
    let Ok(Reply::JsonWithBuffers { json, buffers }) = done.result else {
        panic!("{done:?}")
    };
    assert_eq!(json["string"], "hello world");
    assert_eq!(buffers, vec![vec![1, 2, 3, 4]]);
}

#[test]
fn request_layouts_match_the_wire_format() {
    // JSON args: TYPE=1, REQ, rpcId, method (short string), args (long string).
    let raw = codec::encode_request(
        7,
        3,
        "$m",
        &[Arg::Json(json!(1)), Arg::Json(json!("x"))],
        false,
    )
    .unwrap();
    let mut want = vec![1, 0, 0, 0, 7, 3, 2, b'$', b'm'];
    want.extend_from_slice(&7u32.to_be_bytes());
    want.extend_from_slice(b"[1,\"x\"]");
    assert_eq!(raw, want);
    // With a cancellation token the type is 2; mixed args are 3 / 4.
    assert_eq!(codec::encode_request(7, 3, "$m", &[], true).unwrap()[0], 2);
    assert_eq!(
        codec::encode_request(7, 3, "$m", &[Arg::Undefined], false).unwrap()[0],
        3
    );
    assert_eq!(
        codec::encode_request(7, 3, "$m", &[Arg::Undefined], true).unwrap()[0],
        4
    );
    // Mixed: count u8, then argType (1 string, 2 buffer, 3 object with buffers, 4 undefined).
    let raw =
        codec::encode_request(1, 9, "m", &[Arg::Undefined, Arg::Buffer(vec![5])], false).unwrap();
    assert_eq!(raw, [3, 0, 0, 0, 1, 9, 1, b'm', 2, 4, 2, 0, 0, 0, 1, 5]);
}

#[test]
fn reply_and_ack_layouts_match_the_wire_format() {
    assert_eq!(codec::encode_acknowledged(5), [5, 0, 0, 0, 5]);
    assert_eq!(codec::encode_cancel(5), [6, 0, 0, 0, 5]);
    assert_eq!(codec::encode_reply_ok(5, &Reply::Empty), [7, 0, 0, 0, 5]);
    assert_eq!(
        codec::encode_reply_ok(5, &Reply::Buffer(vec![9])),
        [8, 0, 0, 0, 5, 0, 0, 0, 1, 9]
    );
    assert_eq!(
        codec::encode_reply_ok(5, &Reply::Json(json!(true))),
        [9, 0, 0, 0, 5, 0, 0, 0, 4, b't', b'r', b'u', b'e']
    );
    assert_eq!(codec::encode_reply_err(5, None), [12, 0, 0, 0, 5]);
    let e = codec::encode_reply_err(5, Some(&json!("x")));
    assert_eq!(e, [11, 0, 0, 0, 5, 0, 0, 0, 3, b'"', b'x', b'"']);
}

#[test]
fn cancel_message_round_trip() {
    let peer = RpcPeer::new();
    let mut other = RpcPeer::new();
    let mut h = |_: &Call| -> Result<Reply, HandlerError> { unreachable!() };
    let handled = other.handle(&peer.cancel(12), &mut h).unwrap();
    assert_eq!(handled.cancelled, Some(12));
    assert!(handled.send.is_empty());
}

#[test]
fn truncated_and_unknown_messages_are_errors_not_panics() {
    use codec::CodecError;
    assert_eq!(codec::decode(&[]), Err(CodecError::Truncated));
    assert_eq!(codec::decode(&[1, 0, 0]), Err(CodecError::Truncated));
    assert_eq!(
        codec::decode(&[99, 0, 0, 0, 1]),
        Err(CodecError::UnknownMessageType(99))
    );
    // A request whose method claims 200 bytes.
    assert_eq!(
        codec::decode(&[1, 0, 0, 0, 1, 3, 200, b'x']),
        Err(CodecError::Truncated)
    );
    // Mixed request with an unknown argument type.
    assert_eq!(
        codec::decode(&[3, 0, 0, 0, 1, 3, 1, b'm', 1, 77]),
        Err(CodecError::UnknownArgType(77))
    );
}
