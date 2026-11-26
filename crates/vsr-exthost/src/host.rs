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
    /// Documents added for `workspace.openTextDocument` of a file the app has not opened, oldest
    /// first (ADR-0103). An editor opening the file takes one over; closing it, or the cap
    /// ([`MAX_ON_DEMAND`]), removes it.
    pub(crate) on_demand: Vec<PathBuf>,
    /// What a host without a session was asked to send (the unit tests read it).
    #[cfg(test)]
    sent: Vec<(&'static str, Vec<Value>)>,
}

/// Files bigger than this are not read for `workspace.openTextDocument` (upstream's large-file
/// guard, `files.maxMemoryForLargeFilesMB` territory).
pub const MAX_ON_DEMAND_BYTES: u64 = 50 * 1024 * 1024;

/// How many on-demand documents the host keeps; the oldest is removed beyond it.
pub const MAX_ON_DEMAND: usize = 256;

/// The language of a file nobody opened in an editor, from its extension.
fn on_demand_language(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()) {
        Some("json") => "json",
        Some("css") => "css",
        Some("scss") => "scss",
        Some("less") => "less",
        Some("html" | "htm") => "html",
        Some("md") => "markdown",
        _ => "plaintext",
    }
}

/// The text of a file for an on-demand document; `None` when it is not a regular file (a FIFO, a
/// device: their length is 0 and a read may block forever), cannot be read as UTF-8, or is bigger
/// than [`MAX_ON_DEMAND_BYTES`]. Symlinks are followed, so a link to a regular file still works.
fn read_on_demand(path: &Path) -> Option<String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    // Path check first, so a device node (e.g. a symlink to /dev/cu.*) is never opened: opening a
    // device can have side effects. Then open non-blocking (a path swapped for a FIFO after the check
    // must not block `open`) and check the type and size again on the handle that is actually read.
    if !std::fs::metadata(path).ok()?.is_file() {
        return None;
    }
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let meta = file.metadata().ok()?;
    if !meta.is_file() || meta.len() > MAX_ON_DEMAND_BYTES {
        return None;
    }
    // Bounded even if the file grows after the check: one byte past the cap is enough to refuse it.
    let mut text = String::new();
    file.take(MAX_ON_DEMAND_BYTES + 1).read_to_string(&mut text).ok()?;
    (text.len() as u64 <= MAX_ON_DEMAND_BYTES).then_some(text)
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
        main_thread.file_watchers.folders = p.workspace.iter().map(|(_, path)| path.clone()).collect();

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
            on_demand: Vec::new(),
            #[cfg(test)]
            sent: Vec::new(),
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
                    self.open_on_demand(&call);
                    responder.reply(self.main_thread.handle(&call));
                }
                Event::Closed => self.closed = true,
                Event::Cancelled(_) | Event::BadMessage(_) => {}
            }
            next = self.events.try_recv().ok();
        }
        self.flush_file_events();
        n
    }

    /// `MainThreadDocuments.$tryOpenDocument` for a file that is not open yet
    /// (`workspace.openTextDocument(uri)`, which the HTML client uses to read `html.customData`
    /// files): read it from disk, make it a document and tell the host before the reply goes out
    /// (ADR-0103). The delta and the reply travel on one connection, in that order.
    fn open_on_demand(&mut self, call: &vsr_rpc::rpc::Call) {
        if call.rpc_id != vsr_rpc::proxy_ids::main::MAIN_THREAD_DOCUMENTS
            || call.method != "$tryOpenDocument"
        {
            return;
        }
        let Some(Arg::Json(uri)) = call.args.first() else {
            return;
        };
        if self
            .main_thread
            .documents
            .open
            .contains_key(&crate::diagnostics::uri_key(uri))
        {
            return;
        }
        let Ok(path) = crate::file_system::file_path(uri) else {
            return;
        };
        let Some(text) = read_on_demand(&path) else {
            return;
        };
        let language = on_demand_language(&path);
        let delta = self.main_thread.documents.add_document(&path, &text, language);
        let _ = self.tell(
            ext_host::EXT_HOST_DOCUMENTS_AND_EDITORS,
            "$acceptDocumentsAndEditorsDelta",
            vec![delta],
        );
        self.on_demand.push(path);
        while self.on_demand.len() > MAX_ON_DEMAND {
            let oldest = self.on_demand.remove(0);
            if let Some(delta) = self.main_thread.documents.remove_document(&oldest) {
                let _ = self.tell(
                    ext_host::EXT_HOST_DOCUMENTS_AND_EDITORS,
                    "$acceptDocumentsAndEditorsDelta",
                    vec![delta],
                );
            }
        }
    }

    /// A request to the host that is not waited for (the reply is dropped).
    pub(crate) fn tell(
        &mut self,
        rpc_id: u8,
        method: &'static str,
        args: Vec<Value>,
    ) -> Result<(), HostError> {
        #[cfg(test)]
        if self.session.is_none() {
            self.sent.push((method, args));
            return Ok(());
        }
        let args: Vec<Arg> = args.into_iter().map(Arg::Json).collect();
        self.session()
            .request(rpc_id, method, &args)
            .map(|_| ())
            .map_err(|e| HostError::Codec(e.to_string()))
    }

    /// A request with JSON arguments, waited for.
    pub(crate) fn send_json(
        &mut self,
        rpc_id: u8,
        method: &'static str,
        args: Vec<Value>,
        timeout: Duration,
    ) -> Result<(), HostError> {
        #[cfg(test)]
        if self.session.is_none() {
            self.sent.push((method, args));
            return Ok(());
        }
        let args: Vec<Arg> = args.into_iter().map(Arg::Json).collect();
        self.call(rpc_id, method, &args, timeout).map(|_| ())
    }

    /// `ExtHostFileSystemEventService.$onFileEvent`: the changes the watchers were promised, sent
    /// without waiting for the reply (ADR-0090).
    fn flush_file_events(&mut self) {
        let Some(payload) = self.main_thread.file_watchers.take_payload() else {
            return;
        };
        if let Some(session) = self.session.as_ref() {
            let _ = session.request(
                ext_host::EXT_HOST_FILE_SYSTEM_EVENT_SERVICE,
                "$onFileEvent",
                &[Arg::Json(payload)],
            );
        }
    }

    /// A change made through the app (a save, a rename, a delete): the watchers hear of it
    /// (ADR-0090). Changes the host makes itself are not reported here.
    pub fn file_changed(&mut self, kind: crate::file_watcher::ChangeKind, path: &Path) {
        self.main_thread.file_watchers.accept(vec![crate::file_watcher::FileChange {
            kind,
            path: path.to_path_buf(),
        }]);
        self.flush_file_events();
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
        self.send_json(
            ext_host::EXT_HOST_DOCUMENTS_AND_EDITORS,
            "$acceptDocumentsAndEditorsDelta",
            vec![delta],
            timeout,
        )
    }

    /// Tell the host a document is open (the main thread's model gets it too).
    pub fn open_document(
        &mut self,
        path: &Path,
        text: &str,
        language: &str,
        timeout: Duration,
    ) -> Result<(), HostError> {
        // A document `workspace.openTextDocument` added is reused, not added twice (ADR-0103):
        // the host gets the edit that makes it the editor's text, or, for another language, the
        // document is removed and added again.
        if let Some(i) = self.on_demand.iter().position(|p| p == path) {
            self.on_demand.remove(i);
            if let Some(doc) = self.main_thread.documents.open.get(&crate::app_sync::resource_key(path)) {
                if doc.language == language {
                    return match crate::app_sync::replace_diff(&doc.text, text) {
                        Some(edit) => self.edit_document(path, &[edit], timeout),
                        None => Ok(()),
                    };
                }
                self.close_document(path, timeout)?;
            }
        }
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
        self.send_json(
            ext_host::EXT_HOST_DOCUMENTS,
            "$acceptModelChanged",
            vec![uri, event, json!(dirty)],
            timeout,
        )
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use vsr_rpc::handshake::file_uri;

    const T: Duration = Duration::from_secs(1);

    /// A host without a process: what it would send is recorded in `sent`.
    fn detached() -> ExtensionHost {
        let (_tx, events) = std::sync::mpsc::channel();
        ExtensionHost {
            session: None,
            events,
            main_thread: MainThread::new(),
            start: None,
            closed: false,
            extension_defaults: json!({}),
            on_demand: Vec::new(),
            sent: Vec::new(),
        }
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("rscode-host-{name}-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn try_open(host: &mut ExtensionHost, path: &Path) {
        let call = vsr_rpc::rpc::Call {
            req: 1,
            rpc_id: vsr_rpc::proxy_ids::main::MAIN_THREAD_DOCUMENTS,
            method: "$tryOpenDocument".into(),
            args: vec![Arg::Json(file_uri(path))],
            cancellable: false,
        };
        host.open_on_demand(&call);
    }

    fn added(host: &ExtensionHost) -> usize {
        host.sent.iter().filter(|(_, a)| a[0].get("addedDocuments").is_some()).count()
    }

    #[test]
    fn an_editor_opening_an_on_demand_document_reuses_it() {
        let dir = scratch("reuse");
        let file = dir.join("a.json");
        fs::write(&file, "{\"a\": 1}").unwrap();
        let mut host = detached();
        try_open(&mut host, &file);
        assert_eq!(added(&host), 1);
        // The same text: nothing is sent.
        host.open_document(&file, "{\"a\": 1}", "json", T).unwrap();
        assert_eq!(added(&host), 1, "no second addedDocuments");
        assert_eq!(host.sent.len(), 1);
        assert!(host.on_demand.is_empty(), "the editor took it over");
        // Closing it later removes it.
        host.close_document(&file, T).unwrap();
        assert!(host.main_thread.documents.open.is_empty());
        assert!(host.sent.last().unwrap().1[0].get("removedDocuments").is_some());
    }

    #[test]
    fn the_editor_text_that_differs_from_disk_bumps_the_version() {
        let dir = scratch("differs");
        let file = dir.join("a.json");
        fs::write(&file, "{\"a\": 1}").unwrap();
        let mut host = detached();
        try_open(&mut host, &file);
        host.open_document(&file, "{\"a\": 2}", "json", T).unwrap();
        assert_eq!(added(&host), 1);
        assert_eq!(host.sent.last().unwrap().0, "$acceptModelChanged");
        let doc = &host.main_thread.documents.open[&crate::app_sync::resource_key(&file)];
        assert_eq!((doc.version, doc.text.as_str()), (2, "{\"a\": 2}"));
    }

    #[test]
    fn a_closed_on_demand_document_is_removed_and_the_cap_drops_the_oldest() {
        let dir = scratch("cap");
        let mut host = detached();
        let files: Vec<PathBuf> = (0..MAX_ON_DEMAND + 2)
            .map(|i| {
                let f = dir.join(format!("f{i}.json"));
                fs::write(&f, "{}").unwrap();
                f
            })
            .collect();
        for f in &files {
            try_open(&mut host, f);
        }
        assert_eq!(host.main_thread.documents.open.len(), MAX_ON_DEMAND);
        assert!(!host.main_thread.documents.open.contains_key(&crate::app_sync::resource_key(&files[0])), "the oldest went first");
        host.close_document(&files[5], T).unwrap();
        assert!(!host.on_demand.contains(&files[5]));
        assert_eq!(host.main_thread.documents.open.len(), MAX_ON_DEMAND - 1);
    }

    #[test]
    fn fifos_and_devices_are_refused_without_blocking() {
        let dir = scratch("fifo");
        let fifo = dir.join("pipe.json");
        let link = dir.join("pipe-link.json");
        let _ = fs::remove_file(&link);
        let _ = fs::remove_file(&fifo);
        assert!(std::process::Command::new("mkfifo").arg(&fifo).status().unwrap().success());
        std::os::unix::fs::symlink(&fifo, &link).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        // A regression would block the read forever, so the reads run on a thread the test can time out on.
        std::thread::spawn(move || {
            let got = [read_on_demand(&fifo), read_on_demand(&link), read_on_demand(Path::new("/dev/zero"))];
            let _ = tx.send(got);
        });
        let got = rx.recv_timeout(Duration::from_secs(5)).expect("read_on_demand blocked on a FIFO or device");
        assert!(got.iter().all(Option::is_none), "{got:?}");
    }

    #[test]
    fn a_symlink_to_a_regular_file_is_read() {
        let dir = scratch("symlink");
        let target = dir.join("target.json");
        fs::write(&target, "{}").unwrap();
        let link = dir.join("link.json");
        let _ = fs::remove_file(&link);
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert_eq!(read_on_demand(&link).as_deref(), Some("{}"));
    }

    #[test]
    fn files_over_the_size_cap_are_not_read() {
        let dir = scratch("big");
        let big = dir.join("big.json");
        let f = fs::File::create(&big).unwrap();
        f.set_len(MAX_ON_DEMAND_BYTES + 1).unwrap();
        let mut host = detached();
        try_open(&mut host, &big);
        assert!(host.main_thread.documents.open.is_empty() && host.sent.is_empty());
        let small = dir.join("small.json");
        fs::write(&small, "{}").unwrap();
        try_open(&mut host, &small);
        assert_eq!(added(&host), 1);
    }
}
