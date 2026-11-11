//! The extension host in the workbench (ADR-0066, ADR-0067). The workbench owns an optional host
//! (started when a Code-OSS build is configured, otherwise extensions are simply off), runs it on
//! its own thread ([`worker`]), feeds it the open documents, the active editor, selections and
//! configuration changes, and shows what it produces: status bar entries, decorations and
//! squiggles in the editor, contributed commands in the Command Palette.
//!
//! Upstream source: src/vs/workbench/api/browser/mainThreadDocumentsAndEditors.ts
//! Upstream source: src/vs/workbench/services/extensions/common/extensionHostManager.ts

pub mod paint;
pub mod paint_ui;
pub mod snapshot;
pub mod status;
pub mod worker;

#[cfg(test)]
pub(crate) mod e2e;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use futures::StreamExt;
use gpui_kit::*;
use serde_json::Value;
use vsr_commands::CommandDescriptor;
use vsr_commands::registry::Registration;
use vsr_rpc::handshake::Launch;

use crate::commands::{Commands, Handler};
use crate::config::AppConfig;
use crate::theme_service::ThemeService;
use crate::ui::to_gpui;
use crate::Workbench;
use snapshot::{Snapshot, resource_key};
use worker::{Config, FromHost, ToHost};

type Cursor = (usize, usize);

/// What the host last got, so that only changes are sent.
#[derive(Default)]
struct Mirror {
    docs: HashMap<PathBuf, u64>,
    active: Option<PathBuf>,
    selection: HashMap<PathBuf, (Cursor, Cursor)>,
}

/// The paint model of the active document, and what it was built from.
struct PaintCache {
    key: (u64, u64, PathBuf, u64, u32),
}

#[derive(Default)]
pub struct Extensions {
    /// Start the host on the first frame (the app sets it; tests start the host themselves).
    pub autostart: bool,
    started: bool,
    handle: Option<worker::Handle>,
    listener: Option<Task<()>>,
    /// The host answered: its extensions are loaded.
    pub ready: bool,
    pub loaded: Vec<String>,
    pub snapshot: Snapshot,
    /// Counts snapshots, to invalidate what is derived from them.
    pub generation: u64,
    /// Everything that went wrong (a failed boot, a command that failed, an activation error).
    pub problems: Vec<String>,
    registrations: Vec<Registration>,
    mirror: Mirror,
    paint: Option<PaintCache>,
    shown_errors: usize,
}

impl Extensions {
    pub fn is_running(&self) -> bool {
        self.handle.is_some()
    }
}

/// The user data folder: `RSCODE_USER_DATA_DIR`, else the platform's.
fn user_data_dir() -> PathBuf {
    match std::env::var_os("RSCODE_USER_DATA_DIR").filter(|d| !d.is_empty()) {
        Some(d) => PathBuf::from(d),
        None => {
            let settings = vsr_settings::default_user_settings_path();
            settings.parent().and_then(Path::parent).unwrap_or(Path::new(".")).to_path_buf()
        }
    }
}

/// What to start from the environment, `None` (silently: extensions are off) without a Code-OSS
/// build or Node.
///
/// * `RSCODE_CODE_OSS`: the Code-OSS build (`out/bootstrap-fork.js`); `RSCODE_NODE`: Node.
/// * `RSCODE_EXTENSIONS_DIR`: the folder of installed extensions, default `~/.vscode/extensions`
///   (read only; empty turns the installed ones off).
/// * `RSCODE_EXTENSIONS_DEV`: more folders, `:` separated, each a folder of extensions or one
///   extension (a development checkout), which override installed copies.
pub fn config_for(launch: Result<Launch, String>, workspace: &Path, cx: &App) -> Option<Config> {
    let launch = launch.ok()?;
    let mut roots: Vec<PathBuf> = Vec::new();
    match std::env::var_os("RSCODE_EXTENSIONS_DIR") {
        Some(d) if d.is_empty() => {}
        Some(d) => roots.push(PathBuf::from(d)),
        None => {
            if let Some(home) = std::env::var_os("HOME") {
                roots.push(Path::new(&home).join(".vscode/extensions"));
            }
        }
    }
    if let Some(dev) = std::env::var_os("RSCODE_EXTENSIONS_DEV") {
        roots.extend(std::env::split_paths(&dev).filter(|p| !p.as_os_str().is_empty()));
    }
    Some(config(launch, roots, user_data_dir().join("ExtensionHost"), workspace, cx))
}

pub fn config(launch: Launch, roots: Vec<PathBuf>, storage_root: PathBuf, workspace: &Path, cx: &App) -> Config {
    let app = AppConfig::get(cx);
    Config {
        launch,
        roots,
        storage_root,
        workspace: workspace.to_path_buf(),
        settings: vsr_exthost::configuration::layers(&app.registry(), app.configuration()),
        env: Vec::new(),
    }
}

/// Document text as the host sees it: `\n` line breaks.
fn host_text(text: String) -> String {
    if text.contains('\r') { text.replace("\r\n", "\n") } else { text }
}

impl Workbench {
    /// The first frame: start the host when asked to (`autostart`) and a build is configured.
    pub(crate) fn ext_render_hook(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext.autostart && !self.ext.started {
            self.ext_autostart(Launch::from_env(), window, cx);
        }
        self.ext_sync();
        self.ext_prepare_paint(cx);
    }

    /// Start the host if there is a build to start it from; without one extensions are off, quietly.
    pub(crate) fn ext_autostart(&mut self, launch: Result<Launch, String>, window: &mut Window, cx: &mut Context<Self>) {
        self.ext.started = true;
        if let Some(config) = config_for(launch, &self.root, cx) {
            self.start_extensions(config, window, cx);
        }
    }

    /// Start the host on its thread and listen to what it sends.
    pub(crate) fn start_extensions(&mut self, config: Config, window: &mut Window, cx: &mut Context<Self>) {
        if self.ext.handle.is_some() {
            return;
        }
        self.ext.started = true;
        let (tx, mut rx) = futures::channel::mpsc::unbounded();
        self.ext.handle = Some(worker::spawn(config, tx));
        self.ext.mirror = Mirror::default();
        self.ext.listener = Some(cx.spawn_in(window, async move |this, cx| {
            while let Some(msg) = rx.next().await {
                if this.update_in(cx, |wb, window, cx| wb.on_extension_message(msg, window, cx)).is_err() {
                    break;
                }
            }
        }));
    }

    /// Stop the host (the quit): it gets a moment to exit before it is killed.
    pub(crate) fn ext_shutdown(&mut self) {
        self.ext.listener = None;
        if let Some(mut h) = self.ext.handle.take() {
            h.shutdown();
        }
    }

    pub(crate) fn ext_send(&self, msg: ToHost) {
        if let Some(h) = &self.ext.handle {
            h.send(msg);
        }
    }

    fn ext_problem(&mut self, text: String, cx: &mut Context<Self>) {
        eprintln!("rscode: extensions: {text}");
        self.error = Some(format!("Extensions: {text}"));
        self.ext.problems.push(text);
        cx.notify();
    }

    fn on_extension_message(&mut self, msg: FromHost, window: &mut Window, cx: &mut Context<Self>) {
        match msg {
            FromHost::Ready { extensions, commands } => {
                self.ext.ready = true;
                self.ext.loaded = extensions;
                let registry = cx.global_mut::<Commands>();
                for c in commands {
                    if registry.registry.contains(&c.id) {
                        continue;
                    }
                    let mut d = CommandDescriptor::new(&c.id, &c.title);
                    if let Some(cat) = &c.category {
                        d = d.category(cat);
                    }
                    if c.in_palette {
                        d = d.f1();
                    }
                    self.ext.registrations.push(registry.registry.register(d, Handler::Extension));
                }
                cx.notify();
            }
            FromHost::Failed(e) => {
                self.ext.handle = None;
                self.ext_problem(format!("the extension host did not start: {e}"), cx);
            }
            FromHost::Snapshot(s) => {
                self.ext.snapshot = *s;
                self.ext.generation += 1;
                let errors = self.ext.snapshot.errors.clone();
                for e in errors.into_iter().skip(self.ext.shown_errors) {
                    self.ext.shown_errors += 1;
                    self.ext_problem(format!("activation failed: {e}"), cx);
                }
                cx.notify();
            }
            FromHost::ConfigUpdates(updates) => {
                for u in updates {
                    // Workspace targets have no settings file of their own yet: they go to the user's.
                    if let Err(e) = AppConfig::set_value(cx, &u.key, u.value) {
                        self.ext_problem(format!("could not write {}: {e}", u.key), cx);
                    }
                }
            }
            FromHost::Save(keys) => self.ext_save(&keys, cx),
            FromHost::Completions { id, path, line, column, results } => {
                if self.suggest.deliver(crate::suggest::Answer { id, path, line, column, results }) {
                    self.suggest_on_answer(cx);
                    cx.notify();
                }
            }
            FromHost::Symbols { path, version, groups } => {
                if self.outline.deliver(&path, version, groups) {
                    cx.notify();
                }
            }
            FromHost::Locations { id, nav, links } => self.navigation_deliver(id, nav, links, window, cx),
            FromHost::AppCommand { id, args, reply } => {
                let known = cx.try_global::<Commands>().is_some_and(|c| c.execution(&id).is_some());
                if known {
                    self.execute_command_with(&id, args, window, cx);
                    let _ = reply.send(Ok(None));
                } else {
                    let _ = reply.send(Err(format!("command '{id}' not found")));
                }
            }
            FromHost::Problem(p) => self.ext_problem(p, cx),
            FromHost::Exited => {
                self.ext.handle = None;
                self.ext.ready = false;
                self.ext.snapshot = Snapshot::default();
                self.ext.generation += 1;
                let registry = &mut cx.global_mut::<Commands>().registry;
                for r in self.ext.registrations.drain(..) {
                    registry.unregister(r);
                }
                self.ext_problem("the extension host exited".into(), cx);
            }
        }
    }

    /// An extension saved a document: write the matching editor as Cmd+S does.
    fn ext_save(&mut self, keys: &[String], cx: &mut Context<Self>) {
        let transforms = self.editor_settings.save_transforms(false);
        for e in self.editors.open.iter_mut().filter(|e| keys.contains(&resource_key(&e.path))) {
            if let Err(err) = e.save_with(transforms) {
                self.error = Some(format!("{}: {err}", e.path.display()));
            }
        }
        cx.notify();
    }

    /// `commands.executeCommand(id, ...args)` for a command that may be an extension's: app
    /// commands take the registry, the rest goes to the host (which wakes the extension first).
    pub(crate) fn ext_run_command(&mut self, id: &str, args: Vec<Value>, cx: &mut Context<Self>) {
        if self.ext.handle.is_none() {
            self.ext_problem(format!("command '{id}' needs the extension host, which is not running"), cx);
            return;
        }
        self.ext_send(ToHost::Run { id: id.to_string(), args });
    }

    /// Whether `id` is a command an extension provides.
    pub(crate) fn ext_provides(&self, id: &str) -> bool {
        self.ext.snapshot.commands.contains(id)
    }

    /// The configuration changed: the host gets the new layers and the changed keys.
    pub(crate) fn ext_config_changed(&mut self, cx: &App) {
        if self.ext.handle.is_none() {
            return;
        }
        let app = AppConfig::get(cx);
        let (defaults, user) = vsr_exthost::configuration::layers(&app.registry(), app.configuration());
        let keys = app.last_change().affected_keys.iter().cloned().collect();
        self.ext_send(ToHost::Config { defaults, user, keys });
    }

    /// Keep the host's mirror of the documents, the active editor and the selection in step with
    /// the tabs. Only differences are sent (a version number per document).
    pub(crate) fn ext_sync(&mut self) {
        if self.ext.handle.is_none() {
            return;
        }
        let mut sends: Vec<ToHost> = Vec::new();
        let mut opened = false;
        let mut open_now: HashSet<&Path> = HashSet::new();
        for e in &self.editors.open {
            if !e.path.is_absolute() {
                continue;
            }
            open_now.insert(&e.path);
            let version = e.editor.version();
            match self.ext.mirror.docs.get(&e.path) {
                None => {
                    opened = true;
                    sends.push(ToHost::OpenDoc {
                        path: e.path.clone(),
                        text: host_text(e.buffer().value()),
                        language: e.language_id().to_string(),
                    });
                }
                Some(v) if *v != version => {
                    sends.push(ToHost::Change { path: e.path.clone(), text: host_text(e.buffer().value()) });
                }
                Some(_) => continue,
            }
            self.ext.mirror.docs.insert(e.path.clone(), version);
        }
        let closed: Vec<PathBuf> = self.ext.mirror.docs.keys().filter(|p| !open_now.contains(p.as_path())).cloned().collect();
        for p in closed {
            self.ext.mirror.docs.remove(&p);
            self.ext.mirror.selection.remove(&p);
            sends.push(ToHost::CloseDoc(p));
        }
        let active = self
            .editor_surface_active()
            .then(|| self.editors.active_editor())
            .flatten()
            .filter(|e| e.path.is_absolute())
            .map(|e| e.path.clone());
        if opened || active != self.ext.mirror.active || !sends.is_empty() {
            // Opening a document makes its editor the active one in the host; say which is.
            if opened || active != self.ext.mirror.active {
                sends.push(ToHost::Activate(active.clone()));
            }
            self.ext.mirror.active = active.clone();
        }
        if let (Some(path), Some(e)) = (active, self.editors.active_editor()) {
            let s = e.editor.selection();
            let sel = ((s.anchor.line, s.anchor.column), (s.active.line, s.active.column));
            if self.ext.mirror.selection.get(&e.path) != Some(&sel) {
                self.ext.mirror.selection.insert(e.path.clone(), sel);
                sends.push(ToHost::Selection { path, anchor: sel.0, active: sel.1 });
            }
        }
        for s in sends {
            self.ext_send(s);
        }
    }

    /// Build what the editor paints from the extensions' decorations and diagnostics, once per
    /// snapshot, theme, document version and font size.
    fn ext_prepare_paint(&mut self, cx: &App) {
        let editor = self.editors.active_editor().filter(|_| self.editor_surface_active());
        let (Some(e), true) = (editor, self.ext.handle.is_some()) else {
            self.editor_view.ext_paint = None;
            return;
        };
        let key = (self.ext.generation, ThemeService::generation(cx), e.path.clone(), e.editor.version(), self.editor_settings.font_size.to_bits());
        if self.ext.paint.as_ref().is_some_and(|c| c.key == key) {
            return;
        }
        let theme = ThemeService::color_theme(cx);
        let resolve = |k: &str| theme.and_then(|t| t.rgba8(k));
        let buf = e.buffer();
        let max_column = |line: usize| buf.line_max_column(line);
        let dark = theme.is_none_or(|t| matches!(t.scheme(), vsr_theme::ColorScheme::Dark | vsr_theme::ColorScheme::HighContrastDark));
        let paint = Rc::new(paint::build(
            &self.ext.snapshot,
            &resource_key(&e.path),
            &paint::PaintContext { dark, font_size: self.editor_settings.font_size, resolve: &resolve, line_count: buf.line_count(), max_column: &max_column },
        ));
        self.editor_view.ext_paint = (!paint.is_empty()).then_some(paint);
        self.ext.paint = Some(PaintCache { key });
    }

    /// The status bar entries of extensions, in display order.
    pub(crate) fn ext_status_items(&self, cx: &App) -> Vec<status::Item> {
        let theme = ThemeService::color_theme(cx);
        status::items(&self.ext.snapshot, &|k| theme.and_then(|t| t.rgba8(k)))
    }
}

impl Workbench {
    /// One entry of an extension in the status bar: icons and text, the tooltip, and the command
    /// a click runs.
    pub(crate) fn ext_status_item(&self, it: &status::Item, cx: &mut Context<Self>) -> AnyElement {
        let id = it.id.clone();
        let mut el = div()
            .id(SharedString::from(format!("ext-status-{id}")))
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(5.))
            .h_full()
            .debug_selector(move || format!("ext-status:{id}"));
        if let Some(c) = it.color {
            el = el.text_color(to_gpui(c));
        }
        if let Some(b) = it.background {
            el = el.bg(to_gpui(b));
        }
        for part in &it.parts {
            el = match part {
                status::Part::Icon(c) => el.child(div().font_family(crate::icons::FAMILY).text_size(px(14.)).child(crate::icons::glyph(*c))),
                status::Part::Text(t) => el.child(t.trim().to_string()),
            };
        }
        if let Some(t) = it.tooltip.clone() {
            el = el.tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(SharedString::from(t.clone())).build(w, cx));
        }
        if let Some((cmd, args)) = it.command.clone() {
            el = el.cursor_pointer().on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| this.execute_command_with(&cmd, args.clone(), window, cx)),
            );
        }
        el.into_any_element()
    }
}
