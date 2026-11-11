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
use std::collections::{HashSet, VecDeque};
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
use vsr_exthost::language_features::dto::{CompletionItem, CompletionList, DocumentSymbol, Position};
use vsr_exthost::language_features::main_thread::Kind;
use vsr_exthost::language_features::outline::ProviderSymbols;
use vsr_exthost::language_features::query::{CompletionTrigger, Query};
use vsr_rpc::handshake::Launch;
use vsr_suggest::adapter::{char_to_utf16_column, line_text, list_to_chars};

/// How long one call into the host may take before the worker gives up on it.
const CALL: Duration = Duration::from_secs(30);
/// A provider's `resolveCompletionItem` is asked while the user waits at a row: it gets little time.
const RESOLVE_TIMEOUT: Duration = Duration::from_secs(2);

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
    /// The login shell's environment is resolved with this timeout (ADR-0100); `None` keeps the
    /// inherited one.
    pub shell_env: Option<Duration>,
    /// Extra environment, after the resolved one (it wins).
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
    /// Completions at a 1-based position of an open document (ADR-0082). `column` counts `char`s,
    /// as the editor does. A newer request replaces the one in flight.
    Complete { id: u64, path: PathBuf, line: u32, column: u32, trigger: CompletionTrigger },
    /// `resolveCompletionItem` for the focused item of the list (ADR-0084). The item's columns are
    /// UTF-16, as the host counts them.
    ResolveCompletion { id: u64, handle: u32, item: Box<CompletionItem> },
    /// The lists the UI is done with: `(provider handle, list cache id)` (`$releaseCompletionItems`).
    ReleaseCompletions(Vec<(u32, u64)>),
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
    /// The providers' completion lists for request `id`, one per provider that answered. Ranges
    /// are in `char`s (converted here, where the document's text is); the lists stay cached in the
    /// host until released (ADR-0082).
    Completions { id: u64, path: PathBuf, line: u32, column: u32, results: Vec<(u32, CompletionList)> },
    /// The provider's answer to `ResolveCompletion` `id` (the item as sent when it could not resolve).
    CompletionResolved { id: u64, item: Box<CompletionItem> },
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
    // Resolved here, on the worker thread: the shell can take seconds (ADR-0100, ADR-0066).
    let mut env = match config.shell_env {
        Some(timeout) => vsr_exthost::shell_env::for_host(&vsr_exthost::shell_env::default_shell(), timeout),
        None => Vec::new(),
    };
    env.extend(config.env.iter().cloned());
    let mut host = match ExtensionHost::boot(&BootParams {
        launch: config.launch.clone(),
        version,
        storage_root: config.storage_root.clone(),
        workspace: Some((name, config.workspace.clone())),
        extensions: found.extensions.clone(),
        default_settings: config.settings.0.clone(),
        user_settings: config.settings.1.clone(),
        env,
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
    let mut completion: Option<CompletionJob> = None;
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
                Ok(ToHost::Complete { id, path, line, column, trigger }) => {
                    if let Some(mut old) = completion.take() {
                        cancel_completion(&mut host, &mut old);
                    }
                    completion = start_completion(&mut host, id, path, line, column, trigger, &out);
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
        if let Some(job) = completion.as_mut()
            && let Some(q) = job.query.as_mut()
            && host.poll_query(q)
        {
            let job = completion.take().expect("the job is in flight");
            completion = completion_answered(&mut host, job, &out);
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

/// A completion request on its way (ADR-0082). The providers are asked group by group (upstream's
/// `provideSuggestionItems`): the next group is asked only when the one before offered nothing.
struct CompletionJob {
    id: u64,
    path: PathBuf,
    line: u32,
    column: u32,
    trigger: CompletionTrigger,
    /// The document's text when the request came in: the answer's columns are converted with it.
    text: String,
    groups: VecDeque<Vec<u32>>,
    query: Option<Query<CompletionList>>,
    /// The lists of the groups answered so far.
    collected: Vec<(u32, CompletionList)>,
}

/// Starts a completion request. `None` when it was answered at once (no provider applies, or the
/// call failed); the answer has been sent then.
fn start_completion(
    host: &mut ExtensionHost,
    id: u64,
    path: PathBuf,
    line: u32,
    column: u32,
    trigger: CompletionTrigger,
    out: &UnboundedSender<FromHost>,
) -> Option<CompletionJob> {
    let text = host
        .main_thread
        .documents
        .open
        .get(&resource_key(&path))
        .map(|d| d.text.clone())
        .unwrap_or_default();
    let groups = host.completion_groups(&path, trigger).unwrap_or_else(|e| {
        let _ = out.unbounded_send(FromHost::Problem(format!("completion: {e}")));
        Vec::new()
    });
    let mut job = CompletionJob {
        id,
        path,
        line,
        column,
        trigger,
        text,
        groups: groups.into(),
        query: None,
        collected: Vec::new(),
    };
    if start_next_group(host, &mut job, out) {
        return Some(job);
    }
    send_completions(job, out);
    None
}

/// Asks the next group of providers. False when no group is left or the call could not start.
fn start_next_group(host: &mut ExtensionHost, job: &mut CompletionJob, out: &UnboundedSender<FromHost>) -> bool {
    let Some(group) = job.groups.pop_front() else {
        return false;
    };
    let column = char_to_utf16_column(line_text(&job.text, job.line), job.column);
    match host.begin_completion_group(&job.path, group, Position::new(job.line, column), job.trigger) {
        Ok(q) => {
            job.query = Some(q);
            true
        }
        Err(e) => {
            let _ = out.unbounded_send(FromHost::Problem(format!("completion: {e}")));
            false
        }
    }
}

/// A group's answers arrived. Returns the job while it still waits for a group: when no list of
/// the groups so far offered items, the next group is asked.
fn completion_answered(host: &mut ExtensionHost, mut job: CompletionJob, out: &UnboundedSender<FromHost>) -> Option<CompletionJob> {
    let mut query = job.query.take().expect("a group is in flight");
    let got = query.take();
    for (handle, message) in &got.errors {
        let name = host.main_thread.language_features.registration(*handle).and_then(|r| r.meta_str("displayName").map(String::from));
        let _ = out.unbounded_send(FromHost::Problem(format!("completion ({}): {message}", name.unwrap_or_default())));
    }
    let answered = got.results.iter().any(|(_, list)| !list.items.is_empty());
    job.collected.extend(got.results);
    if !answered && start_next_group(host, &mut job, out) {
        return Some(job);
    }
    send_completions(job, out);
    None
}

/// Sends the lists collected, in `char` columns.
fn send_completions(job: CompletionJob, out: &UnboundedSender<FromHost>) {
    let CompletionJob { id, path, line, column, text, mut collected, .. } = job;
    for (_, list) in &mut collected {
        list_to_chars(list, &text);
    }
    let _ = out.unbounded_send(FromHost::Completions { id, path, line, column, results: collected });
}

/// A request replaced by a newer one: its calls are cancelled, and the lists it already got are
/// released in the host.
fn cancel_completion(host: &mut ExtensionHost, job: &mut CompletionJob) {
    if let Some(q) = job.query.as_mut() {
        job.collected.extend(q.take().results);
        host.cancel_query(q);
    }
    for (handle, list) in job.collected.drain(..) {
        if let Some(cache) = list.cache_id {
            host.release(Kind::Completion, handle, cache);
        }
    }
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
        ToHost::ResolveCompletion { id, handle, item } => {
            let resolved = match host.resolve_completion_item(handle, &item, RESOLVE_TIMEOUT) {
                Ok(r) => r,
                Err(e) => {
                    let _ = out.unbounded_send(FromHost::Problem(format!("resolve completion: {e}")));
                    *item
                }
            };
            let _ = out.unbounded_send(FromHost::CompletionResolved { id, item: Box::new(resolved) });
        }
        ToHost::ReleaseCompletions(lists) => {
            for (handle, cache) in lists {
                host.release(Kind::Completion, handle, cache);
            }
        }
        ToHost::Symbols { .. } | ToHost::Complete { .. } | ToHost::Shutdown => {}
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
