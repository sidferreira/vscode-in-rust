//! upstream: src/vs/workbench/api/node/extensionHostProcess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/extensionHostProtocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/extensionHostEnv.ts @ 50e1350fc78d
//!
//! Spawn upstream's `extensionHostProcess` under Node and complete the handshake:
//!
//! ```text
//! host -> us   Resume, Ready (regular message [2])
//! us -> host   IExtensionHostInitData (a regular message holding JSON)
//! host -> us   Initialized (regular message [1])
//! ```
//!
//! then the host starts calling `MainThread*` proxies over [`crate::rpc`].
//!
//! The host is started the way `bootstrap-fork.ts` expects (`VSCODE_ESM_ENTRYPOINT`) with
//! `VSCODE_EXTHOST_IPC_HOOK` set: it connects to a Unix socket we listen on (the `IPCExtHostConnection`
//! branch of `extensionHostProcess.ts`), so no Electron and no `process.send` handle passing.
//! Needs a Code-OSS build of the pinned commit (`out/` from `node build/next/index.ts transpile`,
//! see ADR-0060); this module never builds one. Set `RSCODE_RPC_TRACE=1` to print every message
//! [`Host::request`] receives.

use crate::codec::{Arg, Reply};
use crate::framing::{Frame, FrameReader, Protocol};
use crate::proxy_ids;
use crate::rpc::{Call, Completed, Handler, HandlerError, RpcPeer};
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// `ExtensionHostMessageType` of `extensionHostProtocol.ts`: one-byte regular messages.
pub const MSG_INITIALIZED: u8 = 1;
pub const MSG_READY: u8 = 2;
pub const MSG_TERMINATE: u8 = 3;

/// Where to find Node and a Code-OSS build. See [`Launch::from_env`].
#[derive(Debug, Clone)]
pub struct Launch {
    pub node: PathBuf,
    /// The Code-OSS checkout whose `out/` holds the transpiled sources (its `product.json` is
    /// read from there).
    pub code_oss: PathBuf,
}

impl Launch {
    /// `RSCODE_NODE` (else `node` on `PATH`) and `RSCODE_CODE_OSS`. `Err` carries the reason that
    /// something is missing, for tests to print when they skip.
    pub fn from_env() -> Result<Launch, String> {
        let node = match std::env::var_os("RSCODE_NODE") {
            Some(p) => PathBuf::from(p),
            None => find_on_path("node").ok_or("node is not on PATH (set RSCODE_NODE)")?,
        };
        let code_oss = std::env::var_os("RSCODE_CODE_OSS")
            .map(PathBuf::from)
            .ok_or("RSCODE_CODE_OSS is not set (a Code-OSS checkout with out/ transpiled)")?;
        if !code_oss.join("out/bootstrap-fork.js").is_file() {
            return Err(format!(
                "{} has no out/bootstrap-fork.js",
                code_oss.display()
            ));
        }
        Ok(Launch { node, code_oss })
    }
}

pub fn find_on_path(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH")?
        .to_str()?
        .split(':')
        .map(|d| Path::new(d).join(name))
        .find(|p| p.is_file())
}

/// A `file` URI as `UriComponents` JSON.
fn uri(path: &Path) -> Value {
    json!({ "$mid": 1, "scheme": "file", "authority": "", "path": path.to_string_lossy(), "query": "", "fragment": "" })
}

/// What the init data needs from us. Storage folders must exist and be writable.
#[derive(Debug, Clone)]
pub struct InitParams {
    pub version: String,
    pub app_root: PathBuf,
    pub storage_root: PathBuf,
    /// The workspace folder, when one is open.
    pub workspace: Option<(String, PathBuf)>,
}

/// `IExtensionHostInitData` (`extensionHostProtocol.ts`) for a host with no extensions: what the
/// host needs to reach `Initialized` and to start `ExtensionHostMain`.
pub fn init_data(p: &InitParams) -> Value {
    json!({
        "version": p.version,
        // No `commit`: the host skips its version-mismatch check.
        // 0 disables the parent-pid polling and the native watchdog.
        "parentPid": 0,
        "environment": {
            "isExtensionDevelopmentDebug": false,
            "appName": "rscode",
            "appHost": "desktop",
            "appRoot": uri(&p.app_root),
            "appLanguage": "en",
            "isExtensionTelemetryLoggingOnly": false,
            "appUriScheme": "rscode",
            "globalStorageHome": uri(&p.storage_root.join("globalStorage")),
            "workspaceStorageHome": uri(&p.storage_root.join("workspaceStorage")),
        },
        "workspace": p.workspace.as_ref().map(|(name, path)| json!({ "id": name, "name": name, "configuration": null, "isUntitled": false, "transient": false, "folders": [{ "uri": uri(path), "name": name, "index": 0 }] })),
        "extensions": { "versionId": 0, "allExtensions": [], "activationEvents": {}, "myExtensions": [] },
        "telemetryInfo": { "sessionId": "rscode-session", "machineId": "rscode-machine", "sqmId": "", "devDeviceId": "", "firstSessionDate": "2026-01-01T00:00:00.000Z" },
        "logLevel": 3,
        "loggers": [],
        "logsLocation": uri(&p.storage_root.join("logs")),
        "autoStart": true,
        "remote": { "isRemote": false, "authority": null, "connectionData": null },
        "consoleForward": { "includeStack": false, "logNative": false },
        "uiKind": 1,
    })
}

/// A request the host made, with the proxy name resolved from the generated table.
#[derive(Debug, Clone, PartialEq)]
pub struct ObservedCall {
    pub rpc_id: u8,
    /// `None` when the id is not in the table: an off-by-one in the numbering shows up here.
    pub proxy: Option<&'static str>,
    pub method: String,
    pub args: Vec<Arg>,
}

/// The default main thread: records what was asked and answers every request with `undefined`,
/// except the two the host needs a value for at startup (`MainThreadWindow.$getInitialState`,
/// `MainThreadChatSkills.$getTools`, whose absence makes the host log an unhandled rejection).
#[derive(Default)]
pub struct RecordingHandler {
    pub calls: Vec<ObservedCall>,
}

impl Handler for RecordingHandler {
    fn call(&mut self, call: &Call) -> Result<Reply, HandlerError> {
        self.calls.push(ObservedCall {
            rpc_id: call.rpc_id,
            proxy: proxy_ids::name_of(call.rpc_id),
            method: call.method.clone(),
            args: call.args.clone(),
        });
        Ok(match call.method.as_str() {
            "$getInitialState" => Reply::Json(json!({ "isFocused": false, "isActive": false })),
            "$getTools" => Reply::Json(json!([])),
            _ => Reply::Empty,
        })
    }
}

#[derive(Debug)]
pub enum HandshakeError {
    Io(std::io::Error),
    Timeout(&'static str),
    /// The host sent something the handshake does not allow.
    Protocol(String),
    /// The host exited before the handshake finished.
    HostExited(String),
}

impl std::fmt::Display for HandshakeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HandshakeError::Io(e) => write!(f, "I/O error: {e}"),
            HandshakeError::Timeout(what) => write!(f, "timed out waiting for {what}"),
            HandshakeError::Protocol(m) => write!(f, "protocol error: {m}"),
            HandshakeError::HostExited(m) => write!(f, "the host exited early: {m}"),
        }
    }
}

impl std::error::Error for HandshakeError {}

impl From<std::io::Error> for HandshakeError {
    fn from(e: std::io::Error) -> Self {
        HandshakeError::Io(e)
    }
}

/// One connection to a host: the socket, the framing state and the RPC bookkeeping.
pub struct Connection {
    stream: UnixStream,
    reader: FrameReader,
    pub protocol: Protocol,
    pub rpc: RpcPeer,
    inbox: std::collections::VecDeque<Vec<u8>>,
}

impl Connection {
    pub fn new(stream: UnixStream) -> Connection {
        Connection {
            stream,
            reader: FrameReader::new(),
            protocol: Protocol::new(),
            rpc: RpcPeer::new(),
            inbox: Default::default(),
        }
    }

    fn write_frame(&mut self, frame: &Frame) -> std::io::Result<()> {
        self.stream.write_all(&frame.encode())
    }

    /// Send a regular message (numbered, acked, kept for replay).
    pub fn send(&mut self, data: Vec<u8>) -> std::io::Result<()> {
        let frame = self.protocol.send(data);
        self.write_frame(&frame)
    }

    /// The next regular message, waiting up to `timeout`. `Ok(None)` on timeout; `Err` when the
    /// peer closed the socket.
    pub fn recv(&mut self, timeout: Duration) -> std::io::Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        loop {
            if let Some(m) = self.inbox.pop_front() {
                return Ok(Some(m));
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(None);
            }
            self.stream.set_read_timeout(Some(left))?;
            let mut buf = [0u8; 16 * 1024];
            match self.stream.read(&mut buf) {
                Ok(0) => {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::UnexpectedEof,
                        "socket closed",
                    ));
                }
                Ok(n) => {
                    for frame in self.reader.push(&buf[..n]) {
                        let got = self.protocol.receive(frame);
                        for f in got.to_send {
                            self.write_frame(&f)?;
                        }
                        if let Some(m) = got.message {
                            self.inbox.push_back(m);
                        }
                    }
                    if self.protocol.ack_due() && self.inbox.is_empty() {
                        let ack = self.protocol.make_ack();
                        self.write_frame(&ack)?;
                    }
                }
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
        }
    }
}

/// What the handshake saw.
#[derive(Debug, Default)]
pub struct Report {
    pub ready_seen: bool,
    pub initialized_seen: bool,
    /// Requests the host made after `Initialized`, in order.
    pub calls: Vec<ObservedCall>,
    pub stderr: String,
    pub stdout: String,
}

/// A running host and the connection to it.
pub struct Host {
    pub conn: Connection,
    child: Child,
    socket_path: PathBuf,
    out: Arc<Mutex<String>>,
    err: Arc<Mutex<String>>,
}

fn pump<R: Read + Send + 'static>(mut r: R, into: Arc<Mutex<String>>) {
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        while let Ok(n) = r.read(&mut buf) {
            if n == 0 {
                break;
            }
            into.lock()
                .unwrap()
                .push_str(&String::from_utf8_lossy(&buf[..n]));
        }
    });
}

impl Host {
    /// Listen on `socket_path`, spawn `node` running `script` with the host's environment, and
    /// accept its connection. `script` is `<code_oss>/out/bootstrap-fork.js` for the real host.
    pub fn spawn(
        node: &Path,
        script: &Path,
        cwd: &Path,
        env: &[(&str, &str)],
        socket_path: &Path,
        connect_timeout: Duration,
    ) -> Result<Host, HandshakeError> {
        let _ = std::fs::remove_file(socket_path);
        let listener = UnixListener::bind(socket_path)?;
        listener.set_nonblocking(true)?;
        let mut cmd = Command::new(node);
        cmd.arg(script)
            .arg("--type=extensionHost")
            .current_dir(cwd)
            .env("VSCODE_EXTHOST_IPC_HOOK", socket_path)
            .env(
                "VSCODE_ESM_ENTRYPOINT",
                "vs/workbench/api/node/extensionHostProcess",
            )
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (k, v) in env {
            cmd.env(k, v);
        }
        let mut child = cmd.spawn()?;
        let out = Arc::new(Mutex::new(String::new()));
        let err = Arc::new(Mutex::new(String::new()));
        pump(child.stdout.take().unwrap(), out.clone());
        pump(child.stderr.take().unwrap(), err.clone());

        let deadline = Instant::now() + connect_timeout;
        let stream = loop {
            match listener.accept() {
                Ok((s, _)) => break s,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Some(status) = child.try_wait()? {
                        std::thread::sleep(Duration::from_millis(100));
                        let _ = std::fs::remove_file(socket_path);
                        return Err(HandshakeError::HostExited(format!(
                            "{status}; stderr: {}",
                            err.lock().unwrap()
                        )));
                    }
                    if Instant::now() > deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        let _ = std::fs::remove_file(socket_path);
                        return Err(HandshakeError::Timeout("the host to connect"));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(e) => return Err(e.into()),
            }
        };
        stream.set_nonblocking(false)?;
        Ok(Host {
            conn: Connection::new(stream),
            child,
            socket_path: socket_path.to_path_buf(),
            out,
            err,
        })
    }

    /// Ready -> init data -> Initialized. Messages other than the expected one are an error.
    pub fn handshake(&mut self, init: &Value, timeout: Duration) -> Result<Report, HandshakeError> {
        let mut report = Report::default();
        let deadline = Instant::now() + timeout;
        let resume = self.conn.protocol.resume_frame();
        self.conn.write_frame(&resume)?;

        let msg = self.wait_message(deadline, "Ready")?;
        if msg != [MSG_READY] {
            return Err(HandshakeError::Protocol(format!(
                "expected Ready, got {msg:?}"
            )));
        }
        report.ready_seen = true;
        self.conn
            .send(serde_json::to_vec(init).expect("init data serializes"))?;

        let msg = self.wait_message(deadline, "Initialized")?;
        if msg != [MSG_INITIALIZED] {
            return Err(HandshakeError::Protocol(format!(
                "expected Initialized, got {msg:?}"
            )));
        }
        report.initialized_seen = true;
        Ok(report)
    }

    fn wait_message(
        &mut self,
        deadline: Instant,
        what: &'static str,
    ) -> Result<Vec<u8>, HandshakeError> {
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.conn.recv(left.min(Duration::from_millis(200))) {
                Ok(Some(m)) => return Ok(m),
                Ok(None) => {
                    if let Some(status) = self.child.try_wait()? {
                        return Err(HandshakeError::HostExited(format!(
                            "{status}; stderr: {}",
                            self.err.lock().unwrap()
                        )));
                    }
                    if Instant::now() >= deadline {
                        return Err(HandshakeError::Timeout(what));
                    }
                }
                Err(e) => {
                    return Err(HandshakeError::HostExited(format!(
                        "{e}; stderr: {}",
                        self.err.lock().unwrap()
                    )));
                }
            }
        }
    }

    /// Call a proxy on the host (an `ExtHostContext` id) and wait for the reply, serving the
    /// host's own requests with `handler` meanwhile.
    pub fn request(
        &mut self,
        rpc_id: u8,
        method: &str,
        args: &[Arg],
        handler: &mut dyn Handler,
        report: &mut Report,
        timeout: Duration,
    ) -> Result<Completed, HandshakeError> {
        let (req, msg) = self
            .conn
            .rpc
            .request(rpc_id, method, args, false)
            .map_err(|e| HandshakeError::Protocol(e.to_string()))?;
        self.conn.send(msg)?;
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let raw = self.wait_message(Instant::now() + left, "a reply")?;
            let handled = self
                .conn
                .rpc
                .handle(&raw, handler)
                .map_err(|e| HandshakeError::Protocol(e.to_string()))?;
            if std::env::var_os("RSCODE_RPC_TRACE").is_some() {
                eprintln!(
                    "rpc <- {:?} call={:?} done={:?}",
                    handled.acknowledged,
                    handled.call.as_ref().map(|c| (&c.method, c.rpc_id)),
                    handled.completed
                );
            }
            if let Some(call) = handled.call {
                report.calls.push(ObservedCall {
                    rpc_id: call.rpc_id,
                    proxy: proxy_ids::name_of(call.rpc_id),
                    method: call.method,
                    args: call.args,
                });
            }
            for m in handled.send {
                self.conn.send(m)?;
            }
            if let Some(done) = handled.completed.filter(|c| c.req == req) {
                return Ok(done);
            }
        }
    }

    /// Serve the host's requests with `handler` for `duration` (or until it closes the socket),
    /// recording each into `report.calls`.
    pub fn serve(
        &mut self,
        handler: &mut dyn Handler,
        duration: Duration,
        report: &mut Report,
    ) -> Result<(), HandshakeError> {
        let deadline = Instant::now() + duration;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            match self.conn.recv(left) {
                Ok(Some(raw)) => {
                    let handled = self
                        .conn
                        .rpc
                        .handle(&raw, handler)
                        .map_err(|e| HandshakeError::Protocol(e.to_string()))?;
                    if let Some(call) = handled.call {
                        report.calls.push(ObservedCall {
                            rpc_id: call.rpc_id,
                            proxy: proxy_ids::name_of(call.rpc_id),
                            method: call.method,
                            args: call.args,
                        });
                    }
                    for m in handled.send {
                        self.conn.send(m)?;
                    }
                }
                Ok(None) => return Ok(()),
                Err(_) => return Ok(()),
            }
        }
    }

    /// Send `Terminate` and wait for the host to exit; kill it after `grace`.
    pub fn terminate(mut self, grace: Duration) -> Exit {
        let _ = self.conn.send(vec![MSG_TERMINATE]);
        let deadline = Instant::now() + grace;
        let mut exited_cleanly = false;
        while Instant::now() < deadline {
            if let Ok(Some(_)) = self.child.try_wait() {
                exited_cleanly = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        if !exited_cleanly {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = std::fs::remove_file(&self.socket_path);
        std::thread::sleep(Duration::from_millis(50));
        Exit {
            exited_cleanly,
            stdout: self.out.lock().unwrap().clone(),
            stderr: self.err.lock().unwrap().clone(),
        }
    }
}

/// What is left after [`Host::terminate`].
#[derive(Debug)]
pub struct Exit {
    pub exited_cleanly: bool,
    pub stdout: String,
    pub stderr: String,
}
