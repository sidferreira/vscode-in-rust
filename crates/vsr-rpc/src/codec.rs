//! upstream: src/vs/workbench/services/extensions/common/rpcProtocol.ts @ 50e1350fc78d
//!
//! The message codec of `RPCProtocol` (`rpcProtocol.ts`: `MessageBuffer` and `MessageIO`).
//!
//! A message is `TYPE (u8) | REQ (u32be) | body`. Bodies:
//!
//! * request (JSON args): `rpcId u8 | method short-string | args long-string (a JSON array)`;
//! * request (mixed args): `rpcId u8 | method short-string | count u8 | { argType u8 | value }*`
//!   where a string arg is a long string of JSON, a buffer is `len u32 | bytes`, an object with
//!   buffers is `bufferCount u32 | json long-string | buffers`, and `Undefined` has no value;
//! * replies: empty, a buffer, JSON, JSON with buffers, or an error (`$isError` JSON, or empty);
//! * `Acknowledged` and `Cancel`: no body.
//!
//! A short string has a one-byte length, a long string a four-byte one. Objects with buffers carry
//! `$$ref$$` placeholders in their JSON upstream (`parseJsonAndRestoreBufferRefs`); this codec
//! keeps the JSON and the buffer list apart and does not substitute them.

use serde_json::Value;
use std::fmt;

/// `MessageType` of `rpcProtocol.ts`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    RequestJsonArgs = 1,
    RequestJsonArgsWithCancellation = 2,
    RequestMixedArgs = 3,
    RequestMixedArgsWithCancellation = 4,
    Acknowledged = 5,
    Cancel = 6,
    ReplyOkEmpty = 7,
    ReplyOkVsBuffer = 8,
    ReplyOkJson = 9,
    ReplyOkJsonWithBuffers = 10,
    ReplyErrError = 11,
    ReplyErrEmpty = 12,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Option<MessageType> {
        use MessageType::*;
        Some(match v {
            1 => RequestJsonArgs,
            2 => RequestJsonArgsWithCancellation,
            3 => RequestMixedArgs,
            4 => RequestMixedArgsWithCancellation,
            5 => Acknowledged,
            6 => Cancel,
            7 => ReplyOkEmpty,
            8 => ReplyOkVsBuffer,
            9 => ReplyOkJson,
            10 => ReplyOkJsonWithBuffers,
            11 => ReplyErrError,
            12 => ReplyErrEmpty,
            _ => return None,
        })
    }
}

/// `ArgType`.
const ARG_STRING: u8 = 1;
const ARG_VSBUFFER: u8 = 2;
const ARG_OBJECT_WITH_BUFFERS: u8 = 3;
const ARG_UNDEFINED: u8 = 4;

/// One request argument.
#[derive(Debug, Clone, PartialEq)]
pub enum Arg {
    /// Anything JSON can carry (`null` included).
    Json(Value),
    /// A `VSBuffer`.
    Buffer(Vec<u8>),
    /// JavaScript `undefined` (JSON has no way to say it).
    Undefined,
    /// A `SerializableObjectWithBuffers`.
    ObjectWithBuffers { json: Value, buffers: Vec<Vec<u8>> },
}

/// A successful reply.
#[derive(Debug, Clone, PartialEq)]
pub enum Reply {
    /// `undefined`.
    Empty,
    Buffer(Vec<u8>),
    Json(Value),
    JsonWithBuffers {
        json: Value,
        buffers: Vec<Vec<u8>>,
    },
}

/// A decoded message.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Request {
        req: u32,
        rpc_id: u8,
        method: String,
        args: Vec<Arg>,
        cancellable: bool,
    },
    Acknowledged {
        req: u32,
    },
    Cancel {
        req: u32,
    },
    ReplyOk {
        req: u32,
        reply: Reply,
    },
    /// `None` is the empty error reply (`undefined` was thrown).
    ReplyErr {
        req: u32,
        error: Option<Value>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodecError {
    Truncated,
    UnknownMessageType(u8),
    UnknownArgType(u8),
    InvalidUtf8,
    InvalidJson(String),
    /// A method name over 255 bytes, or more than 255 arguments.
    TooLong(&'static str),
}

impl fmt::Display for CodecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodecError::Truncated => write!(f, "message is truncated"),
            CodecError::UnknownMessageType(t) => write!(f, "unknown RPC message type {t}"),
            CodecError::UnknownArgType(t) => write!(f, "unknown RPC argument type {t}"),
            CodecError::InvalidUtf8 => write!(f, "string is not valid UTF-8"),
            CodecError::InvalidJson(e) => write!(f, "invalid JSON: {e}"),
            CodecError::TooLong(what) => write!(f, "{what} is too long for the wire format"),
        }
    }
}

impl std::error::Error for CodecError {}

// ---- writing -------------------------------------------------------------------------------

struct Writer(Vec<u8>);

impl Writer {
    fn new(ty: MessageType, req: u32) -> Writer {
        let mut w = Writer(Vec::new());
        w.0.push(ty as u8);
        w.0.extend_from_slice(&req.to_be_bytes());
        w
    }
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_be_bytes());
    }
    fn short_string(&mut self, s: &str) -> Result<(), CodecError> {
        let len = u8::try_from(s.len()).map_err(|_| CodecError::TooLong("method name"))?;
        self.u8(len);
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
    fn long_string(&mut self, s: &str) {
        self.u32(s.len() as u32);
        self.0.extend_from_slice(s.as_bytes());
    }
    fn buffer(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
    }
}

fn needs_mixed(args: &[Arg]) -> bool {
    args.iter().any(|a| !matches!(a, Arg::Json(_)))
}

/// `serializeRequest`: JSON args unless an argument is a buffer, an object with buffers or
/// `undefined`, in which case the mixed layout is used (as `_useMixedArgSerialization` decides).
pub fn encode_request(
    req: u32,
    rpc_id: u8,
    method: &str,
    args: &[Arg],
    cancellable: bool,
) -> Result<Vec<u8>, CodecError> {
    if args.len() > 255 {
        return Err(CodecError::TooLong("argument list"));
    }
    if needs_mixed(args) {
        let ty = if cancellable {
            MessageType::RequestMixedArgsWithCancellation
        } else {
            MessageType::RequestMixedArgs
        };
        let mut w = Writer::new(ty, req);
        w.u8(rpc_id);
        w.short_string(method)?;
        w.u8(args.len() as u8);
        for arg in args {
            match arg {
                Arg::Json(v) => {
                    w.u8(ARG_STRING);
                    w.long_string(&v.to_string());
                }
                Arg::Buffer(b) => {
                    w.u8(ARG_VSBUFFER);
                    w.buffer(b);
                }
                Arg::ObjectWithBuffers { json, buffers } => {
                    w.u8(ARG_OBJECT_WITH_BUFFERS);
                    w.u32(buffers.len() as u32);
                    w.long_string(&json.to_string());
                    for b in buffers {
                        w.buffer(b);
                    }
                }
                Arg::Undefined => w.u8(ARG_UNDEFINED),
            }
        }
        Ok(w.0)
    } else {
        let ty = if cancellable {
            MessageType::RequestJsonArgsWithCancellation
        } else {
            MessageType::RequestJsonArgs
        };
        let mut w = Writer::new(ty, req);
        w.u8(rpc_id);
        w.short_string(method)?;
        let values: Vec<&Value> = args
            .iter()
            .map(|a| match a {
                Arg::Json(v) => v,
                _ => unreachable!("needs_mixed is false"),
            })
            .collect();
        w.long_string(&serde_json::to_string(&values).expect("a Value always serializes"));
        Ok(w.0)
    }
}

pub fn encode_acknowledged(req: u32) -> Vec<u8> {
    Writer::new(MessageType::Acknowledged, req).0
}

pub fn encode_cancel(req: u32) -> Vec<u8> {
    Writer::new(MessageType::Cancel, req).0
}

pub fn encode_reply_ok(req: u32, reply: &Reply) -> Vec<u8> {
    match reply {
        Reply::Empty => Writer::new(MessageType::ReplyOkEmpty, req).0,
        Reply::Buffer(b) => {
            let mut w = Writer::new(MessageType::ReplyOkVsBuffer, req);
            w.buffer(b);
            w.0
        }
        Reply::Json(v) => {
            let mut w = Writer::new(MessageType::ReplyOkJson, req);
            w.long_string(&v.to_string());
            w.0
        }
        Reply::JsonWithBuffers { json, buffers } => {
            let mut w = Writer::new(MessageType::ReplyOkJsonWithBuffers, req);
            w.u32(buffers.len() as u32);
            w.long_string(&json.to_string());
            for b in buffers {
                w.buffer(b);
            }
            w.0
        }
    }
}

/// `serializeReplyErr`: `None` (or JSON `null`, which `JSON.stringify` of a falsy error maps to
/// the empty form upstream) is the empty error reply.
pub fn encode_reply_err(req: u32, error: Option<&Value>) -> Vec<u8> {
    match error {
        Some(v) if !v.is_null() => {
            let mut w = Writer::new(MessageType::ReplyErrError, req);
            w.long_string(&v.to_string());
            w.0
        }
        _ => Writer::new(MessageType::ReplyErrEmpty, req).0,
    }
}

/// `transformErrorForSerialization` for an `Error`: the value of an error reply.
pub fn error_value(name: &str, message: &str) -> Value {
    serde_json::json!({ "$isError": true, "name": name, "message": message })
}

// ---- reading -------------------------------------------------------------------------------

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let end = self.pos.checked_add(n).ok_or(CodecError::Truncated)?;
        if end > self.buf.len() {
            return Err(CodecError::Truncated);
        }
        let s = &self.buf[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, CodecError> {
        Ok(self.take(1)?[0])
    }
    fn u32(&mut self) -> Result<u32, CodecError> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn string(&mut self, len: usize) -> Result<String, CodecError> {
        String::from_utf8(self.take(len)?.to_vec()).map_err(|_| CodecError::InvalidUtf8)
    }
    fn short_string(&mut self) -> Result<String, CodecError> {
        let n = self.u8()? as usize;
        self.string(n)
    }
    fn long_string(&mut self) -> Result<String, CodecError> {
        let n = self.u32()? as usize;
        self.string(n)
    }
    fn buffer(&mut self) -> Result<Vec<u8>, CodecError> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
    fn json(&mut self) -> Result<Value, CodecError> {
        let s = self.long_string()?;
        parse_json(&s)
    }
}

fn parse_json(s: &str) -> Result<Value, CodecError> {
    serde_json::from_str(s).map_err(|e| CodecError::InvalidJson(e.to_string()))
}

/// Decode one RPC message (the payload of one regular frame).
pub fn decode(raw: &[u8]) -> Result<Message, CodecError> {
    let mut r = Reader { buf: raw, pos: 0 };
    let ty_byte = r.u8()?;
    let req = r.u32()?;
    let ty = MessageType::from_u8(ty_byte).ok_or(CodecError::UnknownMessageType(ty_byte))?;
    use MessageType::*;
    Ok(match ty {
        RequestJsonArgs | RequestJsonArgsWithCancellation => {
            let rpc_id = r.u8()?;
            let method = r.short_string()?;
            let args = match r.json()? {
                Value::Array(items) => items.into_iter().map(Arg::Json).collect(),
                other => {
                    return Err(CodecError::InvalidJson(format!(
                        "arguments are not an array: {other}"
                    )));
                }
            };
            Message::Request {
                req,
                rpc_id,
                method,
                args,
                cancellable: ty == RequestJsonArgsWithCancellation,
            }
        }
        RequestMixedArgs | RequestMixedArgsWithCancellation => {
            let rpc_id = r.u8()?;
            let method = r.short_string()?;
            let count = r.u8()? as usize;
            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                let t = r.u8()?;
                args.push(match t {
                    ARG_STRING => Arg::Json(r.json()?),
                    ARG_VSBUFFER => Arg::Buffer(r.buffer()?),
                    ARG_OBJECT_WITH_BUFFERS => {
                        let n = r.u32()? as usize;
                        let json = r.json()?;
                        let mut buffers = Vec::new();
                        for _ in 0..n {
                            buffers.push(r.buffer()?);
                        }
                        Arg::ObjectWithBuffers { json, buffers }
                    }
                    ARG_UNDEFINED => Arg::Undefined,
                    other => return Err(CodecError::UnknownArgType(other)),
                });
            }
            Message::Request {
                req,
                rpc_id,
                method,
                args,
                cancellable: ty == RequestMixedArgsWithCancellation,
            }
        }
        Acknowledged => Message::Acknowledged { req },
        Cancel => Message::Cancel { req },
        ReplyOkEmpty => Message::ReplyOk {
            req,
            reply: Reply::Empty,
        },
        ReplyOkVsBuffer => Message::ReplyOk {
            req,
            reply: Reply::Buffer(r.buffer()?),
        },
        ReplyOkJson => Message::ReplyOk {
            req,
            reply: Reply::Json(r.json()?),
        },
        ReplyOkJsonWithBuffers => {
            let n = r.u32()? as usize;
            let json = r.json()?;
            let mut buffers = Vec::new();
            for _ in 0..n {
                buffers.push(r.buffer()?);
            }
            Message::ReplyOk {
                req,
                reply: Reply::JsonWithBuffers { json, buffers },
            }
        }
        ReplyErrError => Message::ReplyErr {
            req,
            error: Some(r.json()?),
        },
        ReplyErrEmpty => Message::ReplyErr { req, error: None },
    })
}
