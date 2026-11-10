//! upstream: src/vs/workbench/api/common/extHostExtensionService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/extensionHostManager.ts @ 50e1350fc78d
//!
//! [`ExtensionHost`]: boot a host and keep it running. `boot` spawns Node, does the handshake,
//! starts the [`vsr_rpc::session::Session`] and sends the three initial calls the host waits for
//! (`$initializeWorkspace`, `$initializeConfiguration`, then `$startExtensionHost`, ADR-0062).
//! The app calls [`ExtensionHost::pump`] from its own loop: events from the host's reader thread
//! are applied to [`MainThread`] on the caller's thread, so models need no locking.

use crate::description::{ExtensionDescription, delta_adding, identifier, nest};
use crate::documents::TextEdit;
use crate::main_thread::MainThread;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::handshake::{
    Exit, HandshakeError, Host, InitParams, Launch, configuration_data, init_data, workspace_data,
};
use vsr_rpc::proxy_ids::ext_host;
use vsr_rpc::rpc::Completed;
use vsr_rpc::session::{Event, Pending, Session, WaitError};

#[derive(Debug)]
pub enum HostError {
    Boot(HandshakeError),
    Wait(WaitError),
    /// The host answered with an error (`None`: an empty error).
    Remote(Option<Value>),
    Codec(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::Boot(e) => write!(f, "boot: {e}"),
            HostError::Wait(e) => write!(f, "no reply: {e:?}"),
            HostError::Remote(v) => write!(f, "the host replied with an error: {v:?}"),
            HostError::Codec(e) => write!(f, "codec: {e}"),
        }
    }
}

impl std::error::Error for HostError {}

/// What a host needs to start.
#[derive(Debug, Clone)]
pub struct BootParams {
    pub launch: Launch,
    pub version: String,
    /// Where the host keeps global / workspace storage and logs (created if missing).
    pub storage_root: PathBuf,
    /// The workspace folder, when one is open.
    pub workspace: Option<(String, PathBuf)>,
    pub extensions: Vec<ExtensionDescription>,
    /// Nested settings contents for the default and user layers. The defaults an extension
    /// contributes (`contributes.configuration`) are added under `defaults` by `boot`.
    pub default_settings: Value,
    pub user_settings: Value,
    /// Extra environment for the Node process.
    pub env: Vec<(String, String)>,
}

static SOCKETS: AtomicU32 = AtomicU32::new(0);

pub struct ExtensionHost {
    session: Option<Session>,
    events: Receiver<Event>,
    pub main_thread: MainThread,
    start: Option<Pending>,
    closed: bool,
    /// The defaults extensions contribute (nested), re-sent with every configuration change.
    extension_defaults: Value,
}

impl ExtensionHost {
    pub fn boot(p: &BootParams) -> Result<ExtensionHost, HostError> {
        let io = |e: std::io::Error| HostError::Boot(HandshakeError::Io(e));
        std::fs::create_dir_all(&p.storage_root).map_err(io)?;
        let socket = std::env::temp_dir().join(format!(
            "rscode-exthost-{}-{}.sock",
            std::process::id(),
            SOCKETS.fetch_add(1, Ordering::SeqCst)
        ));
        let env: Vec<(&str, &str)> = p
            .env
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let mut host = Host::spawn(
            &p.launch.node,
            &p.launch.code_oss.join("out/bootstrap-fork.js"),
            &p.launch.code_oss,
            &env,
            &socket,
            Duration::from_secs(30),
        )
        .map_err(HostError::Boot)?;
        let init = init_data(&InitParams {
            version: p.version.clone(),
            app_root: p.launch.code_oss.clone(),
            storage_root: p.storage_root.clone(),
            workspace: p.workspace.clone(),
            // We send `$startExtensionHost` ourselves (ADR-0062).
            auto_start: false,
        });
        host.handshake(&init, Duration::from_secs(30))
            .map_err(HostError::Boot)?;
        let (session, events) = Session::from_host(host);

        let mut main_thread = MainThread::new();
        main_thread.extensions.known = p.extensions.iter().map(|e| e.json.clone()).collect();
        let mut defaults = vec![];
        for e in &p.extensions {
            defaults.extend(e.configuration_defaults());
        }
        let extension_defaults = nest(defaults);
        let defaults = merge(p.default_settings.clone(), extension_defaults.clone());
        main_thread.workspace = crate::workspace_search::WorkspaceService::new(
            p.workspace.iter().map(|(_, path)| path.clone()).collect(),
            &defaults,
            &p.user_settings,
        );

        let codec = |e: vsr_rpc::codec::CodecError| HostError::Codec(e.to_string());
        let ws = session
            .request(
                ext_host::EXT_HOST_WORKSPACE,
                "$initializeWorkspace",
                &[
                    Arg::Json(match &p.workspace {
                        Some((name, path)) => workspace_data(name, path),
                        None => Value::Null,
                    }),
                    Arg::Json(json!(true)),
                ],
            )
            .map_err(codec)?;
        let cfg = session
            .request(
                ext_host::EXT_HOST_CONFIGURATION,
                "$initializeConfiguration",
                &[Arg::Json(configuration_data(
                    defaults,
                    p.user_settings.clone(),
                ))],
            )
            .map_err(codec)?;
        let start = session
            .request(
                ext_host::EXT_HOST_EXTENSION_SERVICE,
                "$startExtensionHost",
                &[Arg::Json(delta_adding(&p.extensions))],
            )
            .map_err(codec)?;
        let mut me = ExtensionHost {
            session: Some(session),
            events,
            main_thread,
            start: Some(start),
            closed: false,
            extension_defaults,
        };
        me.wait(&ws, Duration::from_secs(30))?;
        me.wait(&cfg, Duration::from_secs(30))?;
        Ok(me)
    }

    fn session(&self) -> &Session {
        self.session.as_ref().expect("the host is shut down")
    }

    /// Whether the host's connection closed (it exited or crashed).
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Apply events from the host to the models: wait up to `wait` for the first, then drain what
    /// else is queued. Returns how many were handled.
    pub fn pump(&mut self, wait: Duration) -> usize {
        let mut n = 0;
        let mut next = match self.events.recv_timeout(wait) {
            Ok(e) => Some(e),
            Err(RecvTimeoutError::Timeout) => None,
            Err(RecvTimeoutError::Disconnected) => {
                self.closed = true;
                None
            }
        };
        while let Some(ev) = next {
            n += 1;
            match ev {
                Event::Request(call, responder) => {
                    responder.reply(self.main_thread.handle(&call));
                }
                Event::Closed => self.closed = true,
                Event::Cancelled(_) | Event::BadMessage(_) => {}
            }
            next = self.events.try_recv().ok();
        }
        n
    }

    /// Wait for the reply to one of our requests while serving the host's.
    pub fn wait(&mut self, pending: &Pending, timeout: Duration) -> Result<Completed, HostError> {
        let deadline = Instant::now() + timeout;
        loop {
            match pending.wait(Duration::ZERO) {
                Ok(done) => return Ok(done),
                Err(WaitError::Closed) => return Err(HostError::Wait(WaitError::Closed)),
                Err(WaitError::Timeout) => {}
            }
            if Instant::now() >= deadline {
                return Err(HostError::Wait(WaitError::Timeout));
            }
            self.pump(Duration::from_millis(10));
        }
    }

    /// Call a proxy on the host and wait for the reply (`Reply::Empty` for `undefined`).
    pub fn call(
        &mut self,
        rpc_id: u8,
        method: &str,
        args: &[Arg],
        timeout: Duration,
    ) -> Result<Reply, HostError> {
        let pending = self
            .session()
            .request(rpc_id, method, args)
            .map_err(|e| HostError::Codec(e.to_string()))?;
        match self.wait(&pending, timeout)?.result {
            Ok(reply) => Ok(reply),
            Err(e) => Err(HostError::Remote(e)),
        }
    }

    /// Send a request without waiting: the caller polls the [`Pending`] while it pumps the host
    /// ([`ExtensionHost::pump`]). With `cancellable` the host appends a `CancellationToken` to the
    /// arguments, which most `$provide*` methods read (it is only pushed for this message type).
    pub fn begin_call(
        &self,
        rpc_id: u8,
        method: &str,
        args: &[Arg],
        cancellable: bool,
    ) -> Result<Pending, HostError> {
        self.session()
            .request_with(rpc_id, method, args, cancellable)
            .map_err(|e| HostError::Codec(e.to_string()))
    }

    /// Cancel one of our requests (`Pending::req`): the host's token fires.
    pub fn cancel_request(&self, req: u32) {
        self.session().cancel(req);
    }

    /// Wait for `$startExtensionHost` to finish: the eager extensions are activated.
    pub fn wait_started(&mut self, timeout: Duration) -> Result<(), HostError> {
        let Some(start) = self.start.take() else {
            return Ok(());
        };
        match self.wait(&start, timeout)?.result {
            Ok(_) => Ok(()),
            Err(e) => Err(HostError::Remote(e)),
        }
    }

    /// `ExtHostDocumentsAndEditors.$acceptDocumentsAndEditorsDelta`.
    fn send_delta(&mut self, delta: Value, timeout: Duration) -> Result<(), HostError> {
        self.call(
            ext_host::EXT_HOST_DOCUMENTS_AND_EDITORS,
            "$acceptDocumentsAndEditorsDelta",
            &[Arg::Json(delta)],
            timeout,
        )
        .map(|_| ())
    }

    /// Tell the host a document is open (the main thread's model gets it too).
    pub fn open_document(
        &mut self,
        path: &Path,
        text: &str,
        language: &str,
        timeout: Duration,
    ) -> Result<(), HostError> {
        let delta = self
            .main_thread
            .documents
            .add_document(path, text, language);
        self.send_delta(delta, timeout)
    }

    /// Show an editor on an open document and make it active; returns the editor id.
    pub fn show_editor(&mut self, path: &Path, timeout: Duration) -> Result<String, HostError> {
        let (id, delta) = self.main_thread.documents.add_editor(path).ok_or_else(|| {
            HostError::Codec(format!("{} is not an open document", path.display()))
        })?;
        self.send_delta(delta, timeout)?;
        Ok(id)
    }

    pub fn close_editor(&mut self, id: &str, timeout: Duration) -> Result<(), HostError> {
        let delta = self.main_thread.documents.remove_editor(id);
        self.send_delta(delta, timeout)
    }

    /// Edit an open document: `ExtHostDocuments.$acceptModelChanged`, which fires
    /// `onDidChangeTextDocument` in extensions.
    pub fn edit_document(
        &mut self,
        path: &Path,
        edits: &[TextEdit],
        timeout: Duration,
    ) -> Result<(), HostError> {
        let (uri, event, dirty) =
            self.main_thread
                .documents
                .edit(path, edits)
                .ok_or_else(|| {
                    HostError::Codec(format!("{} is not an open document", path.display()))
                })?;
        self.call(
            ext_host::EXT_HOST_DOCUMENTS,
            "$acceptModelChanged",
            &[Arg::Json(uri), Arg::Json(event), Arg::Json(json!(dirty))],
            timeout,
        )
        .map(|_| ())
    }

    /// `ExtHostConfiguration.$acceptConfigurationChanged`: new layers (see
    /// [`crate::configuration::layers`]) and the keys that changed. Extensions see
    /// `onDidChangeConfiguration`.
    pub fn configuration_changed(
        &mut self,
        defaults: Value,
        user: Value,
        changed_keys: &[&str],
        timeout: Duration,
    ) -> Result<(), HostError> {
        self.main_thread.workspace.set_layers(
            &merge(defaults.clone(), self.extension_defaults.clone()),
            &user,
        );
        let data = configuration_data(merge(defaults, self.extension_defaults.clone()), user);
        self.call(
            ext_host::EXT_HOST_CONFIGURATION,
            "$acceptConfigurationChanged",
            &[
                Arg::Json(data),
                Arg::Json(json!({ "keys": changed_keys, "overrides": [] })),
            ],
            timeout,
        )
        .map(|_| ())
    }

    /// `ExtHostExtensionService.$activateByEvent(event, ActivationKind.Normal)`.
    pub fn activate_by_event(&mut self, event: &str, timeout: Duration) -> Result<(), HostError> {
        self.call(
            ext_host::EXT_HOST_EXTENSION_SERVICE,
            "$activateByEvent",
            &[Arg::Json(json!(event)), Arg::Json(json!(0))],
            timeout,
        )
        .map(|_| ())
    }

    /// `ExtHostExtensionService.$activate(id, reason)`; the host answers `true` also when
    /// `activate()` threw (see `main_thread.extensions.activation_errors`).
    pub fn activate(&mut self, id: &str, timeout: Duration) -> Result<Reply, HostError> {
        let reason =
            json!({ "startup": false, "extensionId": identifier(id), "activationEvent": "api" });
        self.call(
            ext_host::EXT_HOST_EXTENSION_SERVICE,
            "$activate",
            &[Arg::Json(identifier(id)), Arg::Json(reason)],
            timeout,
        )
    }

    /// `ExtHostCommands.$executeContributedCommand(id, ...args)`: run an extension's command.
    pub fn execute_contributed_command(
        &mut self,
        id: &str,
        args: Vec<Value>,
        timeout: Duration,
    ) -> Result<Reply, HostError> {
        let mut a = vec![Arg::Json(json!(id))];
        a.extend(args.into_iter().map(Arg::Json));
        self.call(
            ext_host::EXT_HOST_COMMANDS,
            "$executeContributedCommand",
            &a,
            timeout,
        )
    }

    /// The host's stderr so far.
    pub fn stderr(&self) -> String {
        self.session
            .as_ref()
            .map(|s| s.stderr())
            .unwrap_or_default()
    }

    /// `Terminate`, wait for the exit, kill after `grace`.
    pub fn shutdown(mut self, grace: Duration) -> Option<Exit> {
        let exit = self.session.take()?.shutdown(grace);
        while self.events.try_recv().is_ok() {}
        exit
    }
}

/// Deep-merge `b` over `a` (objects merge, anything else is replaced).
fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let merged = match a.remove(&k) {
                    Some(old) => merge(old, v),
                    None => v,
                };
                a.insert(k, merged);
            }
            Value::Object(a)
        }
        (_, b) => b,
    }
}
