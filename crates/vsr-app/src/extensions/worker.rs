//! The thread that owns the extension host (ADR-0066). Booting, activating and every call into
//! the host block, so none of it runs on the UI thread: the UI sends [`ToHost`] messages (open
//! this document, run this command, ...) and receives [`FromHost`] messages (a new [`Snapshot`],
//! a request to run an app command, ...) over channels.
//!
//! Upstream source: src/vs/workbench/services/extensions/common/extensionHostManager.ts

use super::snapshot::Snapshot;
use crate::config::glob::Glob;
use futures::channel::mpsc::UnboundedSender;
use serde_json::Value;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::thread::JoinHandle;
use std::time::Duration;
use vsr_exthost::activation::{EventIndex, command_event, language_event};
use vsr_exthost::app_sync::{replace_diff, resource_key};
use vsr_exthost::configuration::ConfigUpdate;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::discovery::{self, ContributedCommand};
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_exthost::language_features::dto::DocumentSymbol;
use vsr_exthost::language_features::main_thread::Kind;
use vsr_exthost::language_features::outline::ProviderSymbols;
use vsr_exthost::language_features::query::Query;
use vsr_rpc::handshake::Launch;

/// How long one call into the host may take before the worker gives up on it.
const CALL: Duration = Duration::from_secs(30);

/// What to start: a Code-OSS build and Node, the extension folders and where the host keeps its
/// files.
#[derive(Debug, Clone)]
pub struct Config {
    pub launch: Launch,
    /// Folders of extensions, or single extension folders (a later one overrides an earlier).
    pub roots: Vec<PathBuf>,
    pub storage_root: PathBuf,
    pub workspace: PathBuf,
    /// Nested `(defaults, user)` settings layers at startup.
    pub settings: (Value, Value),
    pub env: Vec<(String, String)>,
}

pub enum ToHost {
    OpenDoc { path: PathBuf, text: String, language: String },
    /// The document now has this text.
    Change { path: PathBuf, text: String },
    CloseDoc(PathBuf),
    /// The active editor (`None`: a tab that is not a text editor).
    Activate(Option<PathBuf>),
    Selection { path: PathBuf, anchor: (usize, usize), active: (usize, usize) },
    Config { defaults: Value, user: Value, keys: Vec<String> },
    Run { id: String, args: Vec<Value> },
    /// The document symbols of an open document at this version (ADR-0086). A newer request
    /// replaces the one in flight.
    Symbols { path: PathBuf, version: u64 },
    Shutdown,
}

pub enum FromHost {
    /// The host is up and these extensions are loaded.
    Ready { extensions: Vec<String>, commands: Vec<ContributedCommand> },
    Failed(String),
    Snapshot(Box<Snapshot>),
    /// `WorkspaceConfiguration.update` calls to write into the settings.
    ConfigUpdates(Vec<ConfigUpdate>),
    /// Resource keys of documents an extension asked to save.
    Save(Vec<String>),
    /// An extension runs an app command; the answer goes back through `reply`.
    AppCommand { id: String, args: Vec<Value>, reply: Sender<Result<Option<Value>, String>> },
    /// The providers' document symbols for `path` at `version`, one group per provider.
    Symbols { path: PathBuf, version: u64, groups: Vec<ProviderSymbols> },
    /// Something went wrong that is worth showing (a command that failed, a call that timed out).
    Problem(String),
    /// The host process is gone.
    Exited,
}

pub struct Handle {
    tx: Sender<ToHost>,
    thread: Option<JoinHandle<()>>,
}

impl Handle {
    pub fn send(&self, msg: ToHost) {
        let _ = self.tx.send(msg);
    }

    /// Ask the host to shut down and wait for the thread (it kills the process after its grace).
    pub fn shutdown(&mut self) {
        let _ = self.tx.send(ToHost::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Handle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

pub fn spawn(config: Config, out: UnboundedSender<FromHost>) -> Handle {
    let (tx, rx) = channel();
    let thread = std::thread::Builder::new()
        .name("rscode-exthost".into())
        .spawn(move || run(config, rx, out))
        .expect("spawn the extension host thread");
    Handle { tx, thread: Some(thread) }
}

/// The version of the Code-OSS build (its `package.json`), the host's `version`.
fn host_version(code_oss: &Path) -> String {
    std::fs::read_to_string(code_oss.join("package.json"))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.get("version").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| "1.147.0".to_string())
}

fn run(config: Config, rx: Receiver<ToHost>, out: UnboundedSender<FromHost>) {
    let version = host_version(&config.launch.code_oss);
    let found = discovery::scan(&config.roots, &version, discovery::current_platform());
    let commands: Vec<ContributedCommand> =
        found.extensions.iter().flat_map(discovery::contributed_commands).collect();
    let index = EventIndex::new(&found.extensions);
    let name = config.workspace.file_name().map_or_else(|| "workspace".into(), |n| n.to_string_lossy().into_owned());
    let ids: Vec<String> = found.extensions.iter().map(|e| e.id.clone()).collect();
    let mut host = match ExtensionHost::boot(&BootParams {
        launch: config.launch.clone(),
        version,
        storage_root: config.storage_root.clone(),
        workspace: Some((name, config.workspace.clone())),
        extensions: found.extensions.clone(),
        default_settings: config.settings.0.clone(),
        user_settings: config.settings.1.clone(),
        env: config.env.clone(),
    }) {
        Ok(h) => h,
        Err(e) => {
            let _ = out.unbounded_send(FromHost::Failed(e.to_string()));
            return;
        }
    };
    let app_out = out.clone();
    host.main_thread.commands.set_executor(Box::new(move |id, args| {
        let (reply, answer) = channel();
        app_out
            .unbounded_send(FromHost::AppCommand { id: id.to_string(), args: args.to_vec(), reply })
            .map_err(|_| "the app is gone".to_string())?;
        answer.recv_timeout(Duration::from_secs(10)).map_err(|_| format!("command '{id}' did not answer"))?
    }));
    let _ = out.unbounded_send(FromHost::Ready { extensions: ids, commands });
    let mut fired: HashSet<String> = HashSet::new();
    if let Err(e) = host.wait_started(Duration::from_secs(120)) {
        let _ = out.unbounded_send(FromHost::Problem(format!("extension host start: {e}")));
    }
    fire(&mut host, &index, &mut fired, "onStartupFinished".into(), &out);
    for (event, pattern) in index.workspace_contains() {
        if workspace_contains(&config.workspace, &pattern) {
            fire(&mut host, &index, &mut fired, event, &out);
        }
    }

    let mut last: Option<Snapshot> = None;
    let mut symbols: Option<SymbolJob> = None;
    'main: loop {
        let mut dirty = false;
        loop {
            match rx.try_recv() {
                Ok(ToHost::Shutdown) | Err(TryRecvError::Disconnected) => break 'main,
                Ok(ToHost::Symbols { path, version }) => {
                    if let Some(mut old) = symbols.take() {
                        host.cancel_query(&mut old.query);
                    }
                    symbols = start_symbols(&mut host, path, version);
                }
                Ok(msg) => {
                    handle(&mut host, &index, &mut fired, msg, &out);
                    dirty = true;
                }
                Err(TryRecvError::Empty) => break,
            }
        }
        let handled = host.pump(Duration::from_millis(10));
        if let Some(job) = symbols.as_mut()
            && host.poll_query(&mut job.query)
        {
            let job = symbols.take().expect("the job is in flight");
            finish_symbols(&mut host, job, &out);
        }
        let updates = std::mem::take(&mut host.main_thread.configuration.updates);
        if !updates.is_empty() {
            let _ = out.unbounded_send(FromHost::ConfigUpdates(updates));
        }
        let saved = std::mem::take(&mut host.main_thread.documents.saved);
        if !saved.is_empty() {
            let _ = out.unbounded_send(FromHost::Save(saved));
        }
        if handled > 0 || dirty || last.is_none() {
            let snap = Snapshot::of(&host.main_thread);
            if last.as_ref() != Some(&snap) {
                last = Some(snap.clone());
                let _ = out.unbounded_send(FromHost::Snapshot(Box::new(snap)));
            }
        }
        if host.is_closed() {
            let _ = out.unbounded_send(FromHost::Exited);
            return;
        }
    }
    host.shutdown(Duration::from_secs(2));
}

/// A document-symbol request on its way: the providers' answers, polled from the loop.
struct SymbolJob {
    path: PathBuf,
    version: u64,
    query: Query<Vec<DocumentSymbol>>,
}

/// Ask the providers for the symbols of `path`. `None` when the document is not open in the
/// host (the UI asks again when it is).
fn start_symbols(host: &mut ExtensionHost, path: PathBuf, version: u64) -> Option<SymbolJob> {
    let convert = Box::new(|v: &Value| Some(v.as_array()?.iter().filter_map(DocumentSymbol::from_json).collect()));
    let query = host.begin_provide(Kind::DocumentSymbol, &path, "$provideDocumentSymbols", &[], convert).ok()?;
    Some(SymbolJob { path, version, query })
}

/// Send what the providers answered, labelled by their display names (upstream's
/// `provider.displayName ?? 'Unknown Outline Provider'`), and report the ones that failed.
fn finish_symbols(host: &mut ExtensionHost, mut job: SymbolJob, out: &UnboundedSender<FromHost>) {
    let provided = job.query.take();
    for (handle, message) in &provided.errors {
        let name = host.main_thread.language_features.registration(*handle).and_then(|r| r.meta_str("displayName").map(String::from));
        let _ = out.unbounded_send(FromHost::Problem(format!("document symbols ({}): {message}", name.unwrap_or_default())));
    }
    let groups = provided
        .results
        .into_iter()
        .map(|(handle, symbols)| {
            let label = host.main_thread.language_features.registration(handle).and_then(|r| r.meta_str("displayName").map(String::from));
            ProviderSymbols::new(label.as_deref(), symbols)
        })
        .collect();
    let _ = out.unbounded_send(FromHost::Symbols { path: job.path, version: job.version, groups });
}

fn fire(host: &mut ExtensionHost, index: &EventIndex, fired: &mut HashSet<String>, event: String, out: &UnboundedSender<FromHost>) {
    if !index.is_declared(&event) || !fired.insert(event.clone()) {
        return;
    }
    if let Err(e) = host.activate_by_event(&event, CALL) {
        let _ = out.unbounded_send(FromHost::Problem(format!("{event}: {e}")));
    }
}

fn handle(host: &mut ExtensionHost, index: &EventIndex, fired: &mut HashSet<String>, msg: ToHost, out: &UnboundedSender<FromHost>) {
    let problem = |what: &str, r: Result<(), vsr_exthost::host::HostError>| {
        if let Err(e) = r {
            let _ = out.unbounded_send(FromHost::Problem(format!("{what}: {e}")));
        }
    };
    match msg {
        ToHost::OpenDoc { path, text, language } => {
            problem("open document", host.open_document(&path, &text, &language, CALL));
            problem("show editor", host.show_editor(&path, CALL).map(|_| ()));
            fire(host, index, fired, language_event(&language), out);
        }
        ToHost::Change { path, text } => {
            let old = host.main_thread.documents.open.get(&resource_key(&path)).map(|d| d.text.clone());
            if let Some(edit) = old.and_then(|o| replace_diff(&o, &text)) {
                problem("edit document", host.edit_document(&path, &[edit], CALL));
            }
        }
        ToHost::CloseDoc(path) => problem("close document", host.close_document(&path, CALL)),
        ToHost::Activate(path) => problem("activate editor", host.activate_editor(path.as_deref(), CALL)),
        ToHost::Selection { path, anchor, active } => {
            problem("selection", host.set_selection(&path, anchor, active, CALL));
        }
        ToHost::Config { defaults, user, keys } => {
            let keys: Vec<&str> = keys.iter().map(String::as_str).collect();
            problem("configuration", host.configuration_changed(defaults, user, &keys, CALL));
        }
        ToHost::Run { id, args } => {
            // Like the command service: wake the extension first, then run.
            if let Err(e) = host.activate_by_event(&command_event(&id), CALL) {
                let _ = out.unbounded_send(FromHost::Problem(format!("{id}: {e}")));
            }
            problem(&id, host.execute_contributed_command(&id, args, CALL).map(|_| ()));
        }
        ToHost::Symbols { .. } | ToHost::Shutdown => {}
    }
}

/// `workspaceContains:<pattern>` against the workspace folder: a plain path is looked up, a glob
/// is matched against the files below it (skipping `node_modules`, `.git` and hidden folders, at
/// most 20,000 entries).
pub fn workspace_contains(root: &Path, pattern: &str) -> bool {
    let plain = !pattern.contains(['*', '?', '[', '{']);
    if plain {
        return root.join(pattern).exists();
    }
    let glob = Glob::new(pattern);
    let mut budget = 20_000usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if budget == 0 {
                return false;
            }
            budget -= 1;
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = path.strip_prefix(root).unwrap_or(&path).to_string_lossy().into_owned();
            if glob.is_match(&rel) {
                return true;
            }
            if path.is_dir() && !name.starts_with('.') && name != "node_modules" {
                stack.push(path);
            }
        }
    }
    false
}

#[allow(dead_code)]
fn _assert_send() {
    fn is_send<T: Send>() {}
    is_send::<ExtensionHost>();
    is_send::<ExtensionDescription>();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_contains_finds_plain_paths_and_globs() {
        let dir = std::env::temp_dir().join(format!("rscode-wc-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("Cargo.toml"), "").unwrap();
        std::fs::write(dir.join("a/b/main.rs"), "").unwrap();
        std::fs::write(dir.join("node_modules/x/skip.lint"), "").unwrap();
        assert!(workspace_contains(&dir, "Cargo.toml"));
        assert!(!workspace_contains(&dir, "Gemfile"));
        assert!(workspace_contains(&dir, "**/*.rs"));
        assert!(workspace_contains(&dir, "**/b/*.{rs,go}"));
        assert!(!workspace_contains(&dir, "**/*.py"));
        assert!(!workspace_contains(&dir, "**/*.lint"), "node_modules is not searched");
    }
}
