//! upstream: src/vs/workbench/services/extensions/common/rpcProtocol.ts @ 50e1350fc78d
//!
//! The request / reply bookkeeping of `RPCProtocol`, sans-IO: [`RpcPeer`] numbers our requests,
//! decodes what arrives, answers requests through a [`Handler`] and reports replies to ours.
//!
//! Like upstream's `_receiveRequest`, a request is acknowledged (`Acknowledged`) before its reply,
//! and a handler error becomes an error reply. Handlers are synchronous here: a handler that has
//! to wait for the UI should be driven by the caller's event loop instead (phase P1).

use crate::codec::{self, Arg, CodecError, Message, Reply};
use serde_json::Value;
use std::collections::BTreeSet;

/// A request from the other side, as a [`Handler`] sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct Call {
    pub req: u32,
    pub rpc_id: u8,
    pub method: String,
    pub args: Vec<Arg>,
    /// The caller passed a cancellation token (upstream appends a token to `args`).
    pub cancellable: bool,
}

/// How a handler fails.
#[derive(Debug, Clone, PartialEq)]
pub enum HandlerError {
    /// `throw new Error(message)`: serialised as `{ $isError, name, message }`.
    Error { name: String, message: String },
    /// A thrown non-error value.
    Value(Value),
    /// `throw undefined` / a rejected `undefined`.
    Empty,
}

impl HandlerError {
    pub fn error(message: impl Into<String>) -> Self {
        HandlerError::Error {
            name: "Error".into(),
            message: message.into(),
        }
    }
}

/// The local actors (`RPCProtocol.set`): answers requests addressed to a proxy id we implement.
pub trait Handler {
    fn call(&mut self, call: &Call) -> Result<Reply, HandlerError>;
}

impl<F: FnMut(&Call) -> Result<Reply, HandlerError>> Handler for F {
    fn call(&mut self, call: &Call) -> Result<Reply, HandlerError> {
        self(call)
    }
}

/// The outcome of one of our own requests.
#[derive(Debug, Clone, PartialEq)]
pub struct Completed {
    pub req: u32,
    /// `Err(None)` is the empty error reply.
    pub result: Result<Reply, Option<Value>>,
}

/// What [`RpcPeer::handle`] decided for one incoming message.
#[derive(Debug, Default, PartialEq)]
pub struct Handled {
    /// Messages to send, in order (the ack, then the reply).
    pub send: Vec<Vec<u8>>,
    /// The request that was dispatched, if the message was one.
    pub call: Option<Call>,
    /// A reply to one of our requests.
    pub completed: Option<Completed>,
    /// The peer acknowledged our request (`Acknowledged`).
    pub acknowledged: Option<u32>,
    /// The peer cancelled one of its requests.
    pub cancelled: Option<u32>,
}

#[derive(Default)]
pub struct RpcPeer {
    last_req: u32,
    pending: BTreeSet<u32>,
}

impl RpcPeer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Our requests that have no reply yet.
    pub fn pending(&self) -> usize {
        self.pending.len()
    }

    /// Start a request: returns its id and the message to send.
    pub fn request(
        &mut self,
        rpc_id: u8,
        method: &str,
        args: &[Arg],
        cancellable: bool,
    ) -> Result<(u32, Vec<u8>), CodecError> {
        let msg = codec::encode_request(self.last_req + 1, rpc_id, method, args, cancellable)?;
        self.last_req += 1;
        self.pending.insert(self.last_req);
        Ok((self.last_req, msg))
    }

    /// The `Cancel` message for one of our requests.
    pub fn cancel(&self, req: u32) -> Vec<u8> {
        codec::encode_cancel(req)
    }

    /// Process one incoming message.
    pub fn handle(&mut self, raw: &[u8], handler: &mut dyn Handler) -> Result<Handled, CodecError> {
        let mut out = Handled::default();
        match codec::decode(raw)? {
            Message::Request {
                req,
                rpc_id,
                method,
                args,
                cancellable,
            } => {
                let call = Call {
                    req,
                    rpc_id,
                    method,
                    args,
                    cancellable,
                };
                let result = handler.call(&call);
                out.send.push(codec::encode_acknowledged(req));
                out.send.push(match result {
                    Ok(reply) => codec::encode_reply_ok(req, &reply),
                    Err(HandlerError::Error { name, message }) => {
                        codec::encode_reply_err(req, Some(&codec::error_value(&name, &message)))
                    }
                    Err(HandlerError::Value(v)) => codec::encode_reply_err(req, Some(&v)),
                    Err(HandlerError::Empty) => codec::encode_reply_err(req, None),
                });
                out.call = Some(call);
            }
            Message::Acknowledged { req } => out.acknowledged = Some(req),
            Message::Cancel { req } => out.cancelled = Some(req),
            Message::ReplyOk { req, reply } => {
                if self.pending.remove(&req) {
                    out.completed = Some(Completed {
                        req,
                        result: Ok(reply),
                    });
                }
            }
            Message::ReplyErr { req, error } => {
                if self.pending.remove(&req) {
                    out.completed = Some(Completed {
                        req,
                        result: Err(error),
                    });
                }
            }
        }
        Ok(out)
    }
}
