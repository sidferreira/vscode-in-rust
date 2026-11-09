//! The Search view (find in files): query box with toggles, files to include / exclude, a
//! streaming, virtualised result list and the view title actions.
//!
//! upstream: src/vs/workbench/contrib/search/browser/searchView.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchWidget.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchResultsView.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/patternInputWidget.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/media/searchview.css @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchActionsTopBar.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/search/browser/searchActionsNav.ts @ 50e1350fc78d
//!
//! The engine is `vsr-search` (ADR-0023). The pure result state is in `search/model.rs`.
//! A search runs on gpui's background executor and streams files over a channel into the view;
//! starting a new search (or Escape) sets the old search's cancel flag and drops its tasks.
//!
//! Not implemented: replace (the chevron is shown disabled), the search editor ("Open in
//! editor"), tree layout, open-editors / changed-files scopes, notebook and AI search.

#[path = "search/model.rs"]
pub mod model;

#[cfg(test)]
#[path = "search/e2e_tests.rs"]
mod e2e_tests;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use futures::StreamExt as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use vsr_search::{
    CollapseResults, FileMatch, SearchComplete, SearchOptions, SearchSettings, TextSearchQuery, search,
    split_pattern_input,
};

use crate::config::AppConfig;
use crate::config::settings::search_settings;
use crate::icons;
use crate::theme::Theme;
use crate::ui::to_gpui;
use model::{LIMIT_HIT_MESSAGE, Results, Row, no_results_message, result_count_message};

actions!(
    rscode,
    [FocusSearch, ToggleCaseSensitive, ToggleWholeWord, ToggleRegex, CancelSearch, ToggleSearchDetails]
);

/// Key context of the view; its bindings only apply while focus is inside it.
const KEY_CONTEXT: &str = "SearchView";
const ROW_HEIGHT: f32 = 22.0;
/// How often streamed results are handed to the UI while a search runs.
const FLUSH_INTERVAL: Duration = Duration::from_millis(100);

/// Register the Search key bindings (call once at startup).
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-shift-f", FocusSearch, None),
        KeyBinding::new("alt-cmd-c", ToggleCaseSensitive, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-cmd-w", ToggleWholeWord, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-cmd-r", ToggleRegex, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", CancelSearch, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-j", ToggleSearchDetails, Some(KEY_CONTEXT)),
    ]);
}

/// What the view asks the workbench to do.
pub enum SearchEvent {
    /// Open `path` and select `range` (1-based line, UTF-16 column).
    Open { path: PathBuf, range: vsr_search::SearchRange },
    OpenSettings,
}

impl EventEmitter<SearchEvent> for SearchView {}

/// The search settings from the live configuration (`search.*`, `files.exclude`). Without a
/// configuration (tests that install none) the upstream defaults apply.
pub fn current_settings(cx: &App) -> SearchSettings {
    cx.try_global::<AppConfig>().map_or_else(SearchSettings::default, |c| search_settings(c.configuration()))
}

/// Whether a change of settings changes what a search finds (the folders and files searched, the
/// ignore rules, symlinks, smart case, the result limit). Other settings only affect the next
/// search or how results are shown.
fn searches_differ(a: &SearchSettings, b: &SearchSettings) -> bool {
    a.exclude_globs(false) != b.exclude_globs(false)
        || a.use_ignore_files != b.use_ignore_files
        || a.use_global_ignore_files != b.use_global_ignore_files
        || a.use_parent_ignore_files != b.use_parent_ignore_files
        || a.follow_symlinks != b.follow_symlinks
        || a.smart_case != b.smart_case
        || a.max_results != b.max_results
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Searching,
    Done { limit_hit: bool, files_searched: usize },
    Cancelled,
    Failed(String),
}

enum Msg {
    File(FileMatch),
    Done(Result<SearchComplete, String>),
}

pub struct SearchView {
    theme: Theme,
    root: PathBuf,
    settings: SearchSettings,
    query: Entity<InputState>,
    include: Entity<InputState>,
    exclude: Entity<InputState>,
    match_case: bool,
    whole_word: bool,
    use_regex: bool,
    details_open: bool,
    /// "Use Exclude Settings and Ignore Files".
    use_excludes: bool,
    results: Results,
    rows: Vec<Row>,
    scroll: UniformListScrollHandle,
    status: Status,
    selected: Option<Row>,
    /// The include/exclude text the shown results were searched with (for the message).
    searched_include: String,
    searched_exclude: String,
    cancel: Arc<AtomicBool>,
    debounce: Option<Task<()>>,
    running: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl SearchView {
    pub fn new(root: PathBuf, settings: SearchSettings, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let query = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let include = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. *.ts, src/**/include"));
        let exclude = cx.new(|cx| InputState::new(window, cx).placeholder("e.g. *.ts, src/**/exclude"));
        let mut subs = Vec::new();
        for state in [&query, &include, &exclude] {
            subs.push(cx.subscribe_in(state, window, |this, _st, ev: &InputEvent, _w, cx| match ev {
                InputEvent::Change => this.schedule(cx),
                InputEvent::PressEnter { .. } => this.start_search(cx),
                _ => {}
            }));
        }
        subs.push(cx.observe_global::<AppConfig>(|this, cx| this.on_config_changed(cx)));
        Self {
            theme: Theme::dark_modern(),
            root,
            settings,
            query,
            include,
            exclude,
            match_case: false,
            whole_word: false,
            use_regex: false,
            details_open: false,
            use_excludes: true,
            results: Results::default(),
            rows: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            status: Status::Idle,
            selected: None,
            searched_include: String::new(),
            searched_exclude: String::new(),
            cancel: Arc::new(AtomicBool::new(false)),
            debounce: None,
            running: None,
            _subs: subs,
        }
    }

    // ---- accessors (also used by the headless E2E tests) -------------------------------------

    #[cfg(test)]
    pub fn results(&self) -> &Results {
        &self.results
    }

    #[cfg(test)]
    pub fn status(&self) -> &Status {
        &self.status
    }

    /// The "N results in M files" text, if there are results.
    pub fn summary(&self) -> Option<String> {
        (self.results.file_count() > 0)
            .then(|| result_count_message(self.results.count(), self.results.file_count()))
    }

    fn text(&self, input: &Entity<InputState>, cx: &App) -> String {
        input.read(cx).value().to_string()
    }

    pub fn query_text(&self, cx: &App) -> String {
        self.text(&self.query, cx)
    }

    /// Focus the query box (Shift+Cmd+F).
    pub fn focus_query(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.update(cx, |s, cx| s.focus(window, cx));
    }

    // ---- searching -------------------------------------------------------------------------

    /// The configuration changed. Takes the new `search.*` / `files.exclude` values; when one of
    /// them changes what a search finds and a query is set, the query runs again.
    fn on_config_changed(&mut self, cx: &mut Context<Self>) {
        let change = AppConfig::get(cx).last_change().clone();
        if !(change.affects("search") || change.affects("files")) {
            return;
        }
        let fresh = current_settings(cx);
        if fresh == self.settings {
            return;
        }
        let rerun = searches_differ(&self.settings, &fresh);
        self.settings = fresh;
        if rerun && !self.query_text(cx).is_empty() {
            self.start_search(cx);
        } else {
            cx.notify();
        }
    }

    /// Search on type (`search.searchOnType`), after `searchOnTypeDebouncePeriod`.
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if !self.settings.search_on_type {
            return;
        }
        let delay = Duration::from_millis(self.settings.search_on_type_debounce_ms);
        self.debounce = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |this, cx| this.start_search(cx)).ok();
        }));
    }

    fn build_query(&self, cx: &App) -> TextSearchQuery {
        TextSearchQuery {
            pattern: self.query_text(cx),
            is_regex: self.use_regex,
            is_case_sensitive: self.match_case,
            is_word_match: self.whole_word,
            is_multiline: false,
        }
    }

    fn build_options(&self, cx: &App) -> SearchOptions {
        let mut o = SearchOptions::from_settings(&self.root, &self.settings);
        o.includes = split_pattern_input(&self.text(&self.include, cx));
        o.excludes = split_pattern_input(&self.text(&self.exclude, cx));
        if !self.use_excludes {
            o.use_ignore_files = false;
            o.exclude_globs_from_settings.clear();
        }
        o
    }

    fn cancel_running(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        self.running = None;
        self.debounce = None;
    }

    fn rebuild_rows(&mut self) {
        self.rows = self.results.rows();
        if self.selected.is_some_and(|s| !self.rows.contains(&s)) {
            self.selected = None;
        }
    }

    /// Run the search now (Enter, a toggle, Refresh), cancelling a running one.
    pub fn start_search(&mut self, cx: &mut Context<Self>) {
        self.cancel_running();
        self.results.clear();
        self.selected = None;
        let query = self.build_query(cx);
        if query.pattern.is_empty() {
            self.status = Status::Idle;
            self.rebuild_rows();
            cx.notify();
            return;
        }
        let options = self.build_options(cx);
        self.searched_include = options.includes.join(", ");
        self.searched_exclude = options.excludes.join(", ");
        self.status = Status::Searching;
        self.rebuild_rows();

        let cancel = Arc::new(AtomicBool::new(false));
        self.cancel = Arc::clone(&cancel);
        let (tx, mut rx) = futures::channel::mpsc::unbounded::<Msg>();
        let search_cancel = Arc::clone(&cancel);
        let work = cx.background_executor().spawn(async move {
            // Results are forwarded in batches (at most every FLUSH_INTERVAL) so a flood of
            // files does not wake the UI per file. Whatever is left is flushed when the walk ends.
            let pending = std::sync::Mutex::new((Vec::<FileMatch>::new(), std::time::Instant::now()));
            let file_tx = tx.clone();
            let flush = |p: &mut (Vec<FileMatch>, std::time::Instant)| {
                for f in p.0.drain(..) {
                    let _ = file_tx.unbounded_send(Msg::File(f));
                }
                p.1 = std::time::Instant::now();
            };
            let res = search(&query, &options, &search_cancel, |f| {
                let mut p = pending.lock().unwrap_or_else(|e| e.into_inner());
                p.0.push(f);
                if p.1.elapsed() >= FLUSH_INTERVAL {
                    flush(&mut p);
                }
            });
            flush(&mut pending.lock().unwrap_or_else(|e| e.into_inner()));
            let _ = tx.unbounded_send(Msg::Done(res.map_err(|e| e.to_string())));
        });
        self.running = Some(cx.spawn(async move |this, cx| {
            let _work = work; // dropped (and cancelled) together with this task
            while let Some(first) = rx.next().await {
                let mut batch = vec![first];
                while let Ok(m) = rx.try_recv() {
                    batch.push(m);
                }
                if this.update(cx, |this, cx| this.apply(&cancel, batch, cx)).is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn apply(&mut self, token: &Arc<AtomicBool>, batch: Vec<Msg>, cx: &mut Context<Self>) {
        if !Arc::ptr_eq(token, &self.cancel) {
            return; // a newer search replaced this one
        }
        for msg in batch {
            match msg {
                Msg::File(fm) => {
                    let expanded = match self.settings.collapse_results {
                        CollapseResults::AlwaysExpand => true,
                        CollapseResults::AlwaysCollapse => false,
                        CollapseResults::Auto => fm.result_count() < 10,
                    };
                    self.results.add_file(&self.root, &fm, expanded);
                }
                Msg::Done(Ok(done)) => {
                    self.status = if done.cancelled {
                        Status::Cancelled
                    } else {
                        Status::Done { limit_hit: done.limit_hit, files_searched: done.files_searched }
                    }
                }
                Msg::Done(Err(e)) => self.status = Status::Failed(e),
            }
        }
        self.rebuild_rows();
        cx.notify();
    }

    // ---- actions ---------------------------------------------------------------------------

    pub fn toggle_case(&mut self, cx: &mut Context<Self>) {
        self.match_case = !self.match_case;
        self.start_search(cx);
    }

    pub fn toggle_word(&mut self, cx: &mut Context<Self>) {
        self.whole_word = !self.whole_word;
        self.start_search(cx);
    }

    pub fn toggle_regex(&mut self, cx: &mut Context<Self>) {
        self.use_regex = !self.use_regex;
        self.start_search(cx);
    }

    pub fn toggle_details(&mut self, cx: &mut Context<Self>) {
        self.details_open = !self.details_open;
        cx.notify();
    }

    pub fn toggle_excludes(&mut self, cx: &mut Context<Self>) {
        self.use_excludes = !self.use_excludes;
        self.start_search(cx);
    }

    fn cancel_search(&mut self, cx: &mut Context<Self>) {
        if self.status == Status::Searching {
            self.cancel_running();
            self.status = Status::Cancelled;
            cx.notify();
        }
    }

    /// "Clear Search Results": empty the query and filters and the list.
    fn clear_results(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.cancel_running();
        for s in [&self.query, &self.include, &self.exclude] {
            s.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.results.clear();
        self.status = Status::Idle;
        self.rebuild_rows();
        self.focus_query(window, cx);
        cx.notify();
    }

    /// "Collapse All" / "Expand All".
    pub fn toggle_collapse_all(&mut self, cx: &mut Context<Self>) {
        let expand = !self.results.any_expanded();
        self.results.set_all_expanded(expand);
        self.rebuild_rows();
        cx.notify();
    }

    fn search_in_all_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for s in [&self.include, &self.exclude] {
            s.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.start_search(cx);
    }

    /// Click or Enter on a row: files fold, matches open in the editor.
    pub fn activate(&mut self, row: Row, cx: &mut Context<Self>) {
        self.selected = Some(row);
        match row {
            Row::File(i) => {
                if let Some(f) = self.results.files.get_mut(i) {
                    f.expanded = !f.expanded;
                }
                self.rebuild_rows();
            }
            Row::Match(i, j) => {
                if let Some((f, m)) = self.results.files.get(i).and_then(|f| f.matches.get(j).map(|m| (f, m))) {
                    cx.emit(SearchEvent::Open { path: f.path.clone(), range: m.range });
                }
            }
        }
        cx.notify();
    }

    fn dismiss(&mut self, row: Row, cx: &mut Context<Self>) {
        match row {
            Row::File(i) => self.results.remove_file(i),
            Row::Match(i, j) => self.results.remove_match(i, j),
        }
        self.rebuild_rows();
        cx.notify();
    }

    // ---- rendering -------------------------------------------------------------------------

    fn icon_button(
        &self,
        id: impl Into<ElementId>,
        glyph: char,
        tip: &'static str,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let t = &self.theme;
        div()
            .id(id)
            .flex_none()
            .size(px(22.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(to_gpui(t.sidebar_fg))
            .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
            .child(icons::glyph(glyph))
            .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| on(this, window, cx)))
    }

    /// A toggle inside an input (Match Case, ...): `monaco-custom-toggle`.
    fn toggle(
        &self,
        id: &'static str,
        glyph: char,
        tip: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        let t = &self.theme;
        div()
            .id(id)
            .flex_none()
            .size(px(20.))
            .ml(px(2.))
            .rounded(px(3.))
            .border_1()
            .border_color(if active { to_gpui(t.input_option_active_border) } else { gpui_kit::transparent_black() })
            .when(active, |d| d.bg(to_gpui(t.input_option_active_bg)))
            .when(!active, |d| d.hover(|s| s.bg(to_gpui(t.input_option_hover_bg))))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(to_gpui(if active { t.input_option_active_fg } else { t.sidebar_fg }))
            .child(icons::glyph(glyph))
            .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| on(this, cx)))
    }

    fn input_box(&self, state: &Entity<InputState>, toggles: Vec<Stateful<Div>>, window: &Window, cx: &App) -> Div {
        let t = &self.theme;
        let focused = state.read(cx).focus_handle(cx).is_focused(window);
        div()
            .flex()
            .items_center()
            .h(px(26.))
            .w_full()
            .px(px(3.))
            .rounded(px(4.))
            .border_1()
            .border_color(to_gpui(if focused { t.focus_border } else { t.input_border }))
            .bg(to_gpui(t.settings_text_input_bg))
            .child(div().flex_1().min_w_0().child(Input::new(state).appearance(false).bordered(false)))
            .children(toggles)
    }

    fn title_bar(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let searching = self.status == Status::Searching;
        let has_results = self.results.file_count() > 0;
        let any_expanded = self.results.any_expanded();
        div()
            .flex_none()
            .h(px(35.))
            .pl(px(20.))
            .pr(px(8.))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(11.))
            .child("SEARCH")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .text_color(to_gpui(t.sidebar_fg))
                    .child(if searching {
                        self.icon_button("search-cancel", icons::STOP_CIRCLE, "Cancel Search", cx, |this, _, cx| {
                            this.cancel_search(cx)
                        })
                    } else {
                        self.icon_button("search-refresh", icons::REFRESH, "Refresh", cx, |this, _, cx| {
                            this.start_search(cx)
                        })
                    })
                    .child(self.icon_button("search-clear", icons::CLEAR_ALL, "Clear Search Results", cx, |this, w, cx| {
                        this.clear_results(w, cx)
                    }))
                    .child(
                        self.icon_button(
                            "search-collapse",
                            icons::COLLAPSE_ALL,
                            if any_expanded || !has_results { "Collapse All" } else { "Expand All" },
                            cx,
                            |this, _, cx| this.toggle_collapse_all(cx),
                        )
                        .when(!has_results, |d| d.opacity(0.4)),
                    ),
            )
    }

    fn widgets(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let toggles = vec![
            self.toggle("toggle-case", icons::CASE_SENSITIVE, "Match Case (⌥⌘C)", self.match_case, cx, |this, cx| {
                this.toggle_case(cx)
            }),
            self.toggle("toggle-word", icons::WHOLE_WORD, "Match Whole Word (⌥⌘W)", self.whole_word, cx, |this, cx| {
                this.toggle_word(cx)
            }),
            self.toggle("toggle-regex", icons::REGEX, "Use Regular Expression (⌥⌘R)", self.use_regex, cx, |this, cx| {
                this.toggle_regex(cx)
            }),
        ];
        // Replace is not implemented yet: the chevron is shown disabled, like a collapsed replace.
        let replace_toggle = div()
            .id("toggle-replace")
            .absolute()
            .top_0()
            .left_0()
            .w(px(16.))
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .opacity(0.4)
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .child(icons::glyph(icons::CHEVRON_RIGHT))
            .tooltip(|w, cx| Tooltip::new("Toggle Replace (not implemented yet)").build(w, cx));
        let search_row = div()
            .relative()
            .w_full()
            .child(replace_toggle)
            .child(div().ml(px(18.)).child(self.input_box(&self.query, toggles, window, cx)));

        let details_toggle = div()
            .id("toggle-details")
            .absolute()
            .right(px(-2.))
            .top_0()
            .w(px(25.))
            .h(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .rounded(px(3.))
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .when(self.details_open, |d| d.bg(to_gpui(t.input_option_active_bg)))
            .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
            .child(icons::glyph(icons::ELLIPSIS))
            .tooltip(|w, cx| Tooltip::new("Toggle Search Details (⇧⌘J)").build(w, cx))
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.toggle_details(cx)));

        let label = |text: &'static str| div().pt(px(4.)).text_size(px(11.)).child(text);
        let mut details = div().relative().ml(px(18.)).min_h(px(16.)).child(details_toggle);
        if self.details_open {
            let exclude_toggle = self.toggle(
                "toggle-excludes",
                icons::EXCLUDE,
                "Use Exclude Settings and Ignore Files",
                self.use_excludes,
                cx,
                |this, cx| this.toggle_excludes(cx),
            );
            details = details
                .pt(px(2.))
                .child(label("files to include"))
                .child(div().mt(px(2.)).child(self.input_box(&self.include, vec![], window, cx)))
                .child(label("files to exclude"))
                .child(div().mt(px(2.)).pb(px(4.)).child(self.input_box(&self.exclude, vec![exclude_toggle], window, cx)));
        }
        div()
            .flex_none()
            .mt(px(0.))
            .mr(px(12.))
            .ml(px(2.))
            .py(px(6.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(search_row)
            .child(details)
    }

    fn link(&self, id: &'static str, text: &'static str, cx: &mut Context<Self>, on: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static) -> Stateful<Div> {
        let t = &self.theme;
        div()
            .id(id)
            .cursor_pointer()
            .text_color(to_gpui(t.text_link_fg))
            .hover(|s| s.text_color(to_gpui(t.text_link_active_fg)).underline())
            .child(text)
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, w, cx| on(this, w, cx)))
    }

    /// The message block under the widgets (result count, no results, errors, limit warning).
    fn messages(&self, cx: &mut Context<Self>) -> Div {
        let t = &self.theme;
        let mut m = div()
            .flex_none()
            .px(px(22.))
            .pb(px(8.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(13.))
            .text_color(to_gpui(t.search_results_info_fg));
        let line = || div().flex().flex_wrap().items_center();
        match &self.status {
            Status::Idle => return div(),
            Status::Failed(e) => m = m.child(div().text_color(to_gpui(t.setting_error_fg)).child(e.clone())),
            Status::Cancelled if self.results.file_count() == 0 => {
                m = m.child(
                    line()
                        .child("Search was canceled before any results could be found - ")
                        .child(self.link("search-again", "Search again", cx, |this, _, cx| this.start_search(cx))),
                );
            }
            Status::Done { .. } if self.results.file_count() == 0 => {
                let has_filters = !self.searched_include.is_empty() || !self.searched_exclude.is_empty();
                let mut l = line()
                    .id("no-results")
                    .child(no_results_message(&self.searched_include, &self.searched_exclude));
                l = if has_filters {
                    l.child(self.link("search-all", "Search again in all files", cx, |this, w, cx| {
                        this.search_in_all_files(w, cx)
                    }))
                } else {
                    l.child(self.link("open-settings", "Open Settings", cx, |_, _, cx| cx.emit(SearchEvent::OpenSettings)))
                };
                m = m.child(l);
            }
            _ => {
                if let Some(s) = self.summary() {
                    let mut l = line().id("result-summary").child(s);
                    if !self.use_excludes {
                        l = l
                            .child(" - exclude settings and ignore files are disabled (")
                            .child(self.link("enable-excludes", "enable", cx, |this, _, cx| this.toggle_excludes(cx)))
                            .child(")");
                    }
                    m = m.child(l);
                }
                if matches!(self.status, Status::Done { limit_hit: true, .. }) {
                    m = m.child(LIMIT_HIT_MESSAGE);
                }
            }
        }
        m
    }

    fn render_row(&self, ix: usize, row: Row, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let selected = self.selected == Some(row);
        let base = |id: (&'static str, usize)| {
            div()
                .id(id)
                .debug_selector(|| format!("search-row:{}", id.1))
                .group(format!("{}-{}", id.0, id.1))
                .flex_none()
                .h(px(ROW_HEIGHT))
                .w_full()
                .flex()
                .items_center()
                .overflow_hidden()
                .cursor_pointer()
                .text_size(px(13.))
                .when(selected, |d| d.bg(to_gpui(t.list_active_selection_bg)).text_color(to_gpui(t.list_active_selection_fg)))
                .when(!selected, |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
                .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, _, cx| this.activate(row, cx)))
        };
        let glyph = |c: char| {
            div().flex_none().w(px(16.)).font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(c))
        };
        let dismiss = |gid: String, tip: &'static str, cx: &mut Context<Self>| {
            div()
                .id(("dismiss", ix))
                .flex_none()
                .size(px(20.))
                .mr(px(8.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .opacity(0.)
                .group_hover(gid, |s| s.opacity(1.))
                .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .child(icons::glyph(icons::CLOSE))
                .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.dismiss(row, cx);
                    }),
                )
        };
        match row {
            Row::File(i) => {
                let f = &self.results.files[i];
                let gid = format!("sr-file-{ix}");
                base(("sr-file", ix))
                    .pl(px(8.))
                    .child(glyph(if f.expanded { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT }))
                    .child(glyph(icons::FILE).mr(px(4.)))
                    .child(div().flex_none().whitespace_nowrap().child(f.name.clone()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ml(px(6.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(12.))
                            .opacity(0.7)
                            .child(f.dir.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .ml(px(6.))
                            .mr(px(4.))
                            .min_w(px(18.))
                            .h(px(18.))
                            .px(px(6.))
                            .rounded(px(11.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(11.))
                            .bg(to_gpui(t.badge_bg))
                            .text_color(to_gpui(t.badge_fg))
                            .child(f.matches.len().to_string()),
                    )
                    .child(dismiss(gid, "Dismiss", cx))
                    .into_any_element()
            }
            Row::Match(i, j) => {
                let m = &self.results.files[i].matches[j];
                let p = m.parts();
                let gid = format!("sr-match-{ix}");
                base(("sr-match", ix))
                    .pl(px(32.))
                    .when(self.settings.show_line_numbers, |d| {
                        d.child(
                            div()
                                .flex_none()
                                .mr(px(8.))
                                .text_size(px(12.))
                                .opacity(0.7)
                                .child(m.range.start_line.to_string()),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .child(div().flex_none().child(p.before))
                            .child(div().flex_none().bg(to_gpui(t.find_match_highlight_bg)).child(p.inside))
                            .child(div().flex_none().child(p.after)),
                    )
                    .child(dismiss(gid, "Dismiss", cx))
                    .into_any_element()
            }
        }
    }
}

impl Render for SearchView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme.clone();
        let list = uniform_list(
            "search-results",
            self.rows.len(),
            cx.processor(|this: &mut Self, range: std::ops::Range<usize>, _w: &mut Window, cx| {
                range
                    .filter_map(|ix| this.rows.get(ix).copied().map(|row| this.render_row(ix, row, cx)))
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .flex_1()
        .min_h_0();

        div()
            .id("search-view")
            .key_context(KeyContext::parse(KEY_CONTEXT).unwrap_or_default())
            .size_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.chrome_bg))
            .text_color(to_gpui(t.sidebar_fg))
            .on_action(cx.listener(|this, _: &ToggleCaseSensitive, _, cx| this.toggle_case(cx)))
            .on_action(cx.listener(|this, _: &ToggleWholeWord, _, cx| this.toggle_word(cx)))
            .on_action(cx.listener(|this, _: &ToggleRegex, _, cx| this.toggle_regex(cx)))
            .on_action(cx.listener(|this, _: &ToggleSearchDetails, _, cx| this.toggle_details(cx)))
            .on_action(cx.listener(|this, _: &CancelSearch, _, cx| this.cancel_search(cx)))
            .child(self.title_bar(cx))
            .child(self.widgets(window, cx))
            .child(self.messages(cx))
            .child(div().flex_1().min_h_0().flex().flex_col().child(list))
    }
}
