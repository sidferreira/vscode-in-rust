//! Quick Open (Cmd+P, "Go to File"): the workbench side. Builds the file list of the workspace in
//! the background, feeds the Quick Pick, and opens what is accepted. Also owns the picker's life
//! cycle for every Quick Access provider (`>` commands, `:` go to line, `?` help live in
//! `quick_access.rs`, ADR-0030): the text typed decides which provider fills the rows.
//!
//! upstream: src/vs/workbench/contrib/search/browser/anythingQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/quickAccess.ts @ 50e1350fc78d
//!
//! The pure parts (scoring, history and file picks, the `:line:column` suffix) are in
//! `vsr-quickinput`. The file list comes from the `ignore` walker with `vsr-search`'s file filter,
//! honouring `files.exclude`, `search.exclude` and the ignore files (`search.useIgnoreFiles`...).
//! The list is cached and walked again in the background each time Quick Open opens when it is
//! older than `INDEX_MAX_AGE` or the settings changed (no file watcher yet).
//!
//! Not implemented: symbol picks (`@`, `#`), open to the side (Cmd+Enter just opens), the
//! remove-from-history button, absolute path results, links, persisted history across sessions.

#[cfg(test)]
#[path = "quick_open/e2e_tests.rs"]
mod e2e_tests;

use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::config::settings::OpenSource;
use gpui_kit::*;
use serde_json::Value;
use vsr_commands::ContextKeys;
use vsr_quickinput::open::{FileEntry, QuickOpenOptions, Range, extract_range_from_filter, quick_open_picks};
use vsr_settings::Configuration;

use crate::config::AppConfig;
use crate::quick_input::{PickResult, QuickPick, QuickPickEvent};
use crate::Workbench;

actions!(rscode, [GoToFile]);

/// The file list is built again when it is older than this.
const INDEX_MAX_AGE: Duration = Duration::from_secs(5);
/// Above this many files, picks are computed on the background executor.
const BACKGROUND_THRESHOLD: usize = 20_000;

/// Register the Quick Open key binding (call once at startup).
pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-p", GoToFile, None)]);
}

/// The settings Quick Open reads.
#[derive(Clone, Debug, PartialEq)]
pub struct QuickOpenSettings {
    /// `workbench.quickOpen.closeOnFocusLost`.
    pub close_on_focus_lost: bool,
    /// `workbench.quickOpen.preserveInput`.
    pub preserve_input: bool,
    /// `search.quickOpen.includeHistory`.
    pub include_history: bool,
    /// `search.quickOpen.history.filterSortOrder` is `recency`.
    pub history_by_recency: bool,
    /// What the file list leaves out: `files.exclude` + `search.exclude`, `search.useIgnoreFiles`...
    pub index: IndexSettings,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexSettings {
    pub exclude_globs: Vec<String>,
    pub use_ignore_files: bool,
    pub use_parent_ignore_files: bool,
    pub use_global_ignore_files: bool,
    pub follow_symlinks: bool,
}

impl QuickOpenSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            close_on_focus_lost: c.get_bool("workbench.quickOpen.closeOnFocusLost").unwrap_or(true),
            preserve_input: c.get_bool("workbench.quickOpen.preserveInput").unwrap_or(false),
            include_history: c.get_bool("search.quickOpen.includeHistory").unwrap_or(true),
            history_by_recency: c.get_str("search.quickOpen.history.filterSortOrder") == Some("recency"),
            index: IndexSettings {
                exclude_globs: exclude_globs(c),
                use_ignore_files: c.get_bool("search.useIgnoreFiles").unwrap_or(true),
                use_parent_ignore_files: c.get_bool("search.useParentIgnoreFiles").unwrap_or(false),
                use_global_ignore_files: c.get_bool("search.useGlobalIgnoreFiles").unwrap_or(false),
                follow_symlinks: c.get_bool("search.followSymlinks").unwrap_or(true),
            },
        }
    }

    fn options(&self) -> QuickOpenOptions {
        QuickOpenOptions { include_history: self.include_history, history_by_recency: self.history_by_recency }
    }
}

/// The enabled globs of `files.exclude` then `search.exclude` (a later entry with the same glob
/// overrides an earlier one, like `getExcludes`). `when` conditions are not evaluated: such an
/// entry is left out.
pub fn exclude_globs(c: &Configuration) -> Vec<String> {
    let mut merged: Vec<(String, bool)> = Vec::new();
    for key in ["files.exclude", "search.exclude"] {
        let Some(Value::Object(map)) = c.get_value(key) else { continue };
        for (glob, v) in map {
            let on = matches!(v, Value::Bool(true));
            match merged.iter_mut().find(|(g, _)| g == glob) {
                Some(slot) => slot.1 = on,
                None => merged.push((glob.clone(), on)),
            }
        }
    }
    merged.into_iter().filter(|(_, on)| *on).map(|(g, _)| g).collect()
}

/// Every file under `root` that the settings do not exclude, sorted by path.
pub fn list_files(root: &Path, settings: &IndexSettings) -> Vec<FileEntry> {
    use ignore::{WalkBuilder, WalkState};
    use vsr_search::patterns::FileFilter;

    let filter = Arc::new(FileFilter::new(&[], &[], &[], &settings.exclude_globs, false));
    let mut wb = WalkBuilder::new(root);
    let local = settings.use_ignore_files;
    wb.hidden(false)
        .require_git(false)
        .follow_links(settings.follow_symlinks)
        .git_ignore(local)
        .git_exclude(local)
        .ignore(local)
        .git_global(local && settings.use_global_ignore_files)
        .parents(local && settings.use_parent_ignore_files);
    if local {
        wb.add_custom_ignore_filename(".rgignore");
    }
    let (f, base) = (Arc::clone(&filter), root.to_path_buf());
    wb.filter_entry(move |e| {
        if e.depth() == 0 || !e.file_type().is_some_and(|t| t.is_dir()) {
            return true;
        }
        e.path().strip_prefix(&base).map_or(true, |rel| !f.prune_dir(&rel.to_string_lossy()))
    });

    let found = Mutex::new(Vec::<PathBuf>::new());
    let root_owned = root.to_path_buf();
    wb.build_parallel().run(|| {
        let (found, filter, root) = (&found, &filter, &root_owned);
        Box::new(move |entry| {
            let Ok(entry) = entry else { return WalkState::Continue };
            if entry.file_type().is_some_and(|t| t.is_file()) {
                let rel = entry.path().strip_prefix(root).unwrap_or(entry.path()).to_string_lossy().into_owned();
                if filter.accept_file(&rel) {
                    found.lock().unwrap_or_else(|e| e.into_inner()).push(entry.path().to_path_buf());
                }
            }
            WalkState::Continue
        })
    });
    let mut paths = found.into_inner().unwrap_or_else(|e| e.into_inner());
    paths.sort();
    let root_str = root.to_string_lossy();
    paths.iter().map(|p| FileEntry::new(&root_str, &p.to_string_lossy())).collect()
}

/// What the picker's source reads; shared with the workbench, which updates it as the file list
/// and the editor history change.
#[derive(Default)]
pub(crate) struct Data {
    files: Arc<Vec<FileEntry>>,
    history: Arc<Vec<FileEntry>>,
    options: QuickOpenOptions,
    /// The context keys when the picker opened (preconditions of the Command Palette).
    pub(crate) ctx: ContextKeys,
}

#[derive(Default)]
pub struct QuickOpenState {
    pub(crate) pick: Option<Entity<QuickPick>>,
    pub(crate) _sub: Option<Subscription>,
    /// Where focus was when Quick Open opened (restored on Escape).
    pub(crate) return_focus: Option<FocusHandle>,
    pub(crate) data: Rc<RefCell<Data>>,
    /// The file list was built with these settings, at this time.
    indexed: Option<(IndexSettings, Instant)>,
    index_task: Option<Task<()>>,
    /// The text of the last session per Quick Access provider prefix (`preserveInput` settings).
    pub(crate) last_inputs: HashMap<String, String>,
    /// Prefix of the provider whose rows and placeholder are showing.
    pub(crate) provider: String,
    /// The picker is the colour theme picker (`theme_picker.rs`): closing it drops the preview.
    pub(crate) theme_pick: bool,
}

impl QuickOpenState {
    #[cfg(test)]
    pub fn picker(&self) -> Option<&Entity<QuickPick>> {
        self.pick.as_ref()
    }

    /// The picker view, to be drawn above everything else.
    pub fn view(&self) -> Option<Entity<QuickPick>> {
        self.pick.clone()
    }
}

impl Workbench {
    /// Cmd+P: open Quick Open (or switch to it when another Quick Access provider is showing).
    pub(crate) fn show_quick_open(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_quick_access("", window, cx);
    }

    /// Open the Quick Pick with the provider of `prefix` (`""` files, `>` commands, `:` go to
    /// line, `?` help), or switch the one that is open (`quickAccess.show(prefix)`).
    pub(crate) fn show_quick_access(&mut self, prefix: &str, window: &mut Window, cx: &mut Context<Self>) {
        crate::commands::ensure_installed(cx);
        if self.quick.theme_pick || self.crumbs.picker_open() {
            self.hide_quick_open(false, window, cx);
        }
        self.quick.data.borrow_mut().ctx = self.context_keys();
        if let Some(pick) = self.quick.pick.clone() {
            let current = pick.read(cx).query(cx);
            if self.provider_prefix(&current, cx) != self.provider_prefix(prefix, cx) {
                pick.update(cx, |p, cx| p.set_value(prefix, window, cx));
            } else {
                // Selecting the whole text would also select a prefix such as `>`.
                pick.update(cx, |p, cx| p.focus_input(prefix.is_empty(), window, cx));
            }
            return;
        }
        let settings = QuickOpenSettings::from_configuration(AppConfig::get(cx).configuration());
        let palette = crate::quick_access::PaletteSettings::from_configuration(AppConfig::get(cx).configuration());
        cx.update_global::<crate::commands::Commands, _>(|c, _| c.history.set_limit(palette.history));
        let root = self.root.to_string_lossy().into_owned();
        {
            let mut d = self.quick.data.borrow_mut();
            d.history = Arc::new(self.editors.recent_files().iter().map(|p| FileEntry::new(&root, &p.to_string_lossy())).collect());
            d.options = settings.options();
        }
        self.refresh_file_index(&settings, cx);

        self.quick.return_focus = window.focused(cx);
        let provider = self.provider_prefix(prefix, cx);
        let preserve = match provider.as_str() {
            "" => settings.preserve_input,
            ">" => palette.preserve_input,
            _ => false,
        };
        let preserved = preserve.then(|| self.quick.last_inputs.get(&provider).cloned()).flatten().filter(|t| t.starts_with(prefix));
        let initial = preserved.clone().unwrap_or_else(|| prefix.to_string());
        // A restored text is selected after its prefix, so typing replaces it.
        let select_from = preserved.is_some().then_some(prefix.len());
        let data = Rc::clone(&self.quick.data);
        let source = move |query: &str, cx: &mut App| -> PickResult {
            let d = data.borrow();
            match crate::quick_access::provider_of(query, &d.ctx, cx).as_str() {
                ">" => PickResult::Ready(crate::quick_access::commands_rows(&d.ctx, query, cx)),
                "?" => PickResult::Ready(crate::quick_access::help_rows(&d.ctx, cx)),
                ":" | "@" | "@:" => PickResult::Ready(Vec::new()), // filled by the workbench (it owns the editors)
                _ => {
                    if d.files.len() <= BACKGROUND_THRESHOLD {
                        return PickResult::Ready(quick_open_picks(query, &d.history, &d.files, d.options).rows);
                    }
                    let (query, history, files, options) = (query.to_string(), Arc::clone(&d.history), Arc::clone(&d.files), d.options);
                    PickResult::Pending(cx.background_executor().spawn(async move { quick_open_picks(&query, &history, &files, options).rows }))
                }
            }
        };
        let close_on_focus_lost = settings.close_on_focus_lost;
        let placeholder = self.placeholder_of(&provider, cx);
        self.quick.provider = provider;
        let pick = cx.new(|cx| QuickPick::new(&placeholder, &initial, select_from, close_on_focus_lost, source, window, cx));
        self.quick._sub = Some(cx.subscribe_in(&pick, window, |this, _, ev: &QuickPickEvent, window, cx| match ev {
            QuickPickEvent::Accept { item, query, .. } => this.accept_quick_access(item, query, window, cx),
            QuickPickEvent::Dismiss => this.hide_quick_open(true, window, cx),
            QuickPickEvent::Change { query } => this.quick_access_changed(query, window, cx),
            QuickPickEvent::ActiveChange { .. } => {}
        }));
        self.quick.pick = Some(pick);
        // The rows of the initial text were produced before anyone listened.
        self.quick_access_changed(&initial, window, cx);
        cx.notify();
    }

    /// (Re)build the file list in the background if it is missing, stale or built with other settings.
    fn refresh_file_index(&mut self, settings: &QuickOpenSettings, cx: &mut Context<Self>) {
        let fresh = self.quick.indexed.as_ref().is_some_and(|(s, at)| *s == settings.index && at.elapsed() < INDEX_MAX_AGE);
        if fresh {
            return;
        }
        self.quick.indexed = Some((settings.index.clone(), Instant::now()));
        let (root, index_settings) = (self.root.clone(), settings.index.clone());
        self.quick.index_task = Some(cx.spawn(async move |this, cx| {
            let files = cx.background_executor().spawn(async move { list_files(&root, &index_settings) }).await;
            this.update(cx, |this, cx| {
                this.quick.data.borrow_mut().files = Arc::new(files);
                if let Some(pick) = this.quick.pick.clone() {
                    pick.update(cx, |p, cx| p.refresh(cx));
                }
            })
            .ok();
        }));
    }

    /// Close Quick Open. `restore_focus` gives focus back to where it was (Escape, click outside).
    pub(crate) fn hide_quick_open(&mut self, restore_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.end_theme_pick(cx);
        self.crumbs.end_picker();
        if let Some(pick) = self.quick.pick.take() {
            let text = pick.read(cx).query(cx);
            let provider = self.provider_prefix(&text, cx);
            self.quick.last_inputs.insert(provider, text);
        }
        self.quick._sub = None;
        let focus = self.quick.return_focus.take();
        if restore_focus
            && let Some(f) = focus.or_else(|| self.focus.clone()) { f.focus(window, cx) }
        cx.notify();
    }

    /// An item was accepted in the provider of the typed text.
    fn accept_quick_access(&mut self, item: &vsr_quickinput::pick::PickItem, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        match self.provider_prefix(query, cx).as_str() {
            ">" => self.accept_command(item.value.clone(), window, cx),
            ":" => self.accept_go_to_line(query, window, cx),
            "@" | "@:" => self.accept_go_to_symbol(item.value.clone(), window, cx),
            "?" => self.accept_help(item.value.clone(), window, cx),
            _ => self.accept_quick_open(item.path.clone(), query, window, cx),
        }
    }

    /// A file was accepted: open it (at the `:line:column` of the typed text) and focus the editor.
    fn accept_quick_open(&mut self, path: Option<String>, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = path.map(PathBuf::from) else { return };
        let range = extract_range_from_filter(query, &["@"]).map(|f| f.range);
        self.hide_quick_open(false, window, cx);
        if !path.is_file() {
            self.error = Some(format!("{}: file not found", path.display()));
            return;
        }
        match range {
            Some(r) => self.open_at(&path, search_range(r), OpenSource::QuickOpen, window, cx),
            None => {
                self.open_from(&path, OpenSource::QuickOpen);
                if let Some(e) = self.editors.active.and_then(|i| self.editors.open.get(i)).filter(|e| e.path == path) {
                    self.editor_view.reveal(e); // also hands focus to the editor on the next frame
                }
            }
        }
        cx.notify();
    }
}

/// The 1-based range of a `:line:column[-line:column]` suffix as the editor wants it.
fn search_range(r: Range) -> vsr_search::SearchRange {
    let (sl, sc) = (r.start_line.max(1) as usize, r.start_column.max(1) as usize);
    let (el, ec) = (r.end_line.max(1) as usize, r.end_column.max(1) as usize);
    vsr_search::SearchRange::new(sl, sc, el, ec)
}
