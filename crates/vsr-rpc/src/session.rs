//! upstream: src/vs/workbench/services/extensions/common/rpcProtocol.ts @ 50e1350fc78d
//! upstream: src/vs/base/parts/ipc/common/ipc.net.ts @ 50e1350fc78d
//!
//! A long-lived connection (ADR-0063). [`Session::start`] takes a [`Connection`] and runs a reader
//! thread that cuts frames, acknowledges them, answers keep-alives and turns what arrives into
//! [`Event`]s on a channel:
//!
//! * a request from the host becomes [`Event::Request`] with a [`Responder`]. The `Acknowledged`
//!   goes out at once (like `_receiveRequest`); the reply is sent whenever the owner of the
//!   `Responder` calls [`Responder::reply`], from any thread and in any order. This is how
//!   `MainThread*` handlers are served from app state: the app drains the channel on its own
//!   thread (or event loop) and replies when the state is ready;
//! * [`Event::Closed`] when the socket closes (the host exited or crashed).
//!
//! Our own requests go through [`Session::request`], which returns a [`Pending`] to wait on
//! (replies may arrive out of order). [`Session::shutdown`] sends `Terminate`, waits for the process
//! and joins the reader.

use crate::codec::{Arg, CodecError, Reply};
use crate::framing::{Frame, FrameReader};
use crate::handshake::{Connection, Exit, HostProcess, MSG_TERMINATE};
use crate::rpc::{Call, Completed, HandlerError, RpcPeer, reply_message};
use std::collections::HashMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// `ProtocolConstants.KeepAliveSendTime`.
const KEEP_ALIVE: Duration = Duration::from_secs(5);

/// What the reader thread reports.
#[derive(Debug)]
pub enum Event {
    /// A request from the host. Reply through the responder (or drop it: the host then waits).
    Request(Call, Responder),
    /// The host cancelled one of its requests.
    Cancelled(u32),
    /// A message that did not decode; the connection carries on.
    BadMessage(String),
    /// The socket closed. Always the last event.
    Closed,
}

struct Inner {
    stream: UnixStream,
    protocol: crate::framing::Protocol,
    rpc: RpcPeer,
    waiters: HashMap<u32, Sender<Completed>>,
}

impl Inner {
    fn write(&mut self, frame: &Frame) -> std::io::Result<()> {
        self.stream.write_all(&frame.encode())
    }

    fn send(&mut self, data: Vec<u8>) -> std::io::Result<()> {
        let frame = self.protocol.send(data);
        self.write(&frame)
    }
}

struct Shared {
    inner: Mutex<Inner>,
    closed: AtomicBool,
}

/// The right to answer one request of the host.
pub struct Responder {
    shared: Arc<Shared>,
    req: u32,
}

impl std::fmt::Debug for Responder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Responder({})", self.req)
    }
}

impl Responder {
    /// Send the reply. A closed connection is not an error (nobody is left to read it).
    pub fn reply(self, result: Result<Reply, HandlerError>) {
        let msg = reply_message(self.req, &result);
        let mut inner = self.shared.inner.lock().unwrap();
        let _ = inner.send(msg);
    }
}

/// One of our requests, waiting for the host's reply.
pub struct Pending {
    pub req: u32,
    rx: Receiver<Completed>,
}

/// Why [`Pending::wait`] gave no reply.
#[derive(Debug, PartialEq, Eq)]
pub enum WaitError {
    Timeout,
    /// The connection closed first.
    Closed,
}

impl Pending {
    pub fn wait(&self, timeout: Duration) -> Result<Completed, WaitError> {
        match self.rx.recv_timeout(timeout) {
            Ok(c) => Ok(c),
            Err(RecvTimeoutError::Timeout) => Err(WaitError::Timeout),
            Err(RecvTimeoutError::Disconnected) => Err(WaitError::Closed),
        }
    }
}

pub struct Session {
    shared: Arc<Shared>,
    process: Option<HostProcess>,
    reader: Option<JoinHandle<()>>,
}

impl Session {
    /// Start the reader thread on `conn`. Messages already received but not yet processed (the
    /// handshake's inbox) are processed first. `process` is the host's process, when there is one.
    pub fn start(conn: Connection, process: Option<HostProcess>) -> (Session, Receiver<Event>) {
        let Connection {
            stream,
            reader,
            protocol,
            rpc,
            inbox,
        } = conn;
        let read_stream = stream.try_clone().expect("clone the socket");
        let shared = Arc::new(Shared {
            inner: Mutex::new(Inner {
                stream,
                protocol,
                rpc,
                waiters: HashMap::new(),
            }),
            closed: AtomicBool::new(false),
        });
        let (tx, rx) = channel();
        let thread_shared = shared.clone();
        let handle = std::thread::Builder::new()
            .name("rpc-reader".into())
            .spawn(move || read_loop(thread_shared, read_stream, reader, inbox, tx))
            .expect("spawn the reader thread");
        (
            Session {
                shared,
                process,
                reader: Some(handle),
            },
            rx,
        )
    }

    /// Start on a host that finished its handshake.
    pub fn from_host(host: crate::handshake::Host) -> (Session, Receiver<Event>) {
        let (conn, process) = host.into_parts();
        Session::start(conn, Some(process))
    }

    /// Whether the reader saw the socket close.
    pub fn is_closed(&self) -> bool {
        self.shared.closed.load(Ordering::SeqCst)
    }

    /// Call a proxy on the host (an `ExtHostContext` id).
    pub fn request(&self, rpc_id: u8, method: &str, args: &[Arg]) -> Result<Pending, CodecError> {
        self.request_with(rpc_id, method, args, false)
    }

    pub fn request_with(
        &self,
        rpc_id: u8,
        method: &str,
        args: &[Arg],
        cancellable: bool,
    ) -> Result<Pending, CodecError> {
        let mut inner = self.shared.inner.lock().unwrap();
        let (req, msg) = inner.rpc.request(rpc_id, method, args, cancellable)?;
        let (tx, rx) = channel();
        inner.waiters.insert(req, tx);
        if inner.send(msg).is_err() {
            inner.waiters.remove(&req);
        }
        Ok(Pending { req, rx })
    }

    /// Cancel one of our requests (`CancellationToken`).
    pub fn cancel(&self, req: u32) {
        let mut inner = self.shared.inner.lock().unwrap();
        let msg = inner.rpc.cancel(req);
        let _ = inner.send(msg);
    }

    /// Our requests without a reply yet.
    pub fn pending(&self) -> usize {
        self.shared.inner.lock().unwrap().rpc.pending()
    }

    /// The host's stderr so far (empty without a process).
    pub fn stderr(&self) -> String {
        self.process
            .as_ref()
            .map(|p| p.stderr())
            .unwrap_or_default()
    }

    /// Send `Terminate` (the host exits by itself), wait up to `grace`, kill it after that, and
    /// join the reader. Without a process the socket is shut down instead.
    pub fn shutdown(mut self, grace: Duration) -> Option<Exit> {
        {
            let mut inner = self.shared.inner.lock().unwrap();
            let _ = inner.send(vec![MSG_TERMINATE]);
        }
        let exit = self.process.take().map(|p| p.finish(grace));
        if exit.is_none() {
            let inner = self.shared.inner.lock().unwrap();
            let _ = inner.stream.shutdown(std::net::Shutdown::Both);
        }
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
        exit
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        // Dropping without `shutdown`: close the socket so the reader exits; the process (if any)
        // is killed by `HostProcess::drop`.
        let _ = self
            .shared
            .inner
            .lock()
            .map(|i| i.stream.shutdown(std::net::Shutdown::Both));
        if let Some(h) = self.reader.take() {
            let _ = h.join();
        }
    }
}

fn read_loop(
    shared: Arc<Shared>,
    mut stream: UnixStream,
    mut frames: FrameReader,
    mut inbox: std::collections::VecDeque<Vec<u8>>,
    events: Sender<Event>,
) {
    let _ = stream.set_read_timeout(Some(Duration::from_millis(500)));
    let mut last_write = Instant::now();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        // Messages first (the handshake's leftovers), then the socket.
        while let Some(raw) = inbox.pop_front() {
            dispatch(&shared, &raw, &events);
        }
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for frame in frames.push(&buf[..n]) {
                    let mut messages = Vec::new();
                    {
                        let mut inner = shared.inner.lock().unwrap();
                        let got = inner.protocol.receive(frame);
                        for f in got.to_send {
                            let _ = inner.write(&f);
                        }
                        messages.extend(got.message);
                    }
                    for raw in messages {
                        dispatch(&shared, &raw, &events);
                    }
                }
                let mut inner = shared.inner.lock().unwrap();
                if inner.protocol.ack_due() {
                    let ack = inner.protocol.make_ack();
                    let _ = inner.write(&ack);
                }
                last_write = Instant::now();
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                if last_write.elapsed() >= KEEP_ALIVE {
                    let mut inner = shared.inner.lock().unwrap();
                    let f = inner.protocol.keepalive_frame();
                    let _ = inner.write(&f);
                    last_write = Instant::now();
                }
            }
            Err(_) => break,
        }
    }
    shared.closed.store(true, Ordering::SeqCst);
    // Wake everyone waiting on a reply: dropping the senders makes `Pending::wait` return Closed.
    shared.inner.lock().unwrap().waiters.clear();
    let _ = events.send(Event::Closed);
}

fn dispatch(shared: &Arc<Shared>, raw: &[u8], events: &Sender<Event>) {
    let mut inner = shared.inner.lock().unwrap();
    let handled = match inner.rpc.receive(raw) {
        Ok(h) => h,
        Err(e) => {
            drop(inner);
            let _ = events.send(Event::BadMessage(e.to_string()));
            return;
        }
    };
    for m in handled.send {
        let _ = inner.send(m);
    }
    if let Some(done) = handled.completed
        && let Some(w) = inner.waiters.remove(&done.req)
    {
        let _ = w.send(done);
    }
    drop(inner);
    if let Some(call) = handled.call {
        let responder = Responder {
            shared: shared.clone(),
            req: call.req,
        };
        let _ = events.send(Event::Request(call, responder));
    }
    if let Some(req) = handled.cancelled {
        let _ = events.send(Event::Cancelled(req));
    }
}
