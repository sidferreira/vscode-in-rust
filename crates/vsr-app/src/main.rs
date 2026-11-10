//! rscode: the native workbench window (gpui via gpui-kit, ADR-0012).
//!
//! Layout mirrors the VS Code workbench parts: title bar, activity bar,
//! side bar (explorer), editor group (tabs + editor), status bar.
//!
//! upstream: src/vs/workbench/browser/layout.ts @ 50e1350fc78d

mod autosave; // files.autoSave triggers (ADR-0034)
mod breadcrumbs; // the path bar under the tab strip and its dropdown (ADR-0047)
mod commands; // command registry + Command Palette (ADR-0030)
mod config;
mod editor_view;
mod explorer; // file watcher, explorer file operations (ADR-0039)
mod extensions; // the extension host in the workbench (ADR-0066)
mod icons;
mod keymap;
mod language_mode; // Change Language Mode, syntax repaint poll (ADR-0038)
mod model;
mod quick_access; // Command Palette, Go to Line, help on the Quick Pick (ADR-0030)
mod quick_input; // Quick Pick component (ADR-0028)
mod quick_open; // Quick Open, Cmd+P
mod outline; // the document outline: symbols per document, debounced (ADR-0086)
mod suggest; // completion suggestions: requests to the worker, answers kept (ADR-0082)
mod search_view; // Search view (ADR-0023)
mod syntax; // syntax highlighting glue (ADR-0038)
mod session; // session restore, hot exit (ADR-0040)
mod settings_view; // Settings editor (ADR-0017)
mod text_layout;
mod theme;
mod theme_picker; // Preferences: Color Theme (ADR-0037)
mod theme_service;
mod untitled_editors; // File: New Untitled Text File, Save As of untitled texts (ADR-0058)
#[cfg(test)]
mod untitled_e2e; // headless tests of untitled texts (ADR-0058)
#[cfg(test)]
mod theme_e2e;
mod ui;
mod welcome; // the Welcome page tab (ADR-0040)
mod window_title; // window.title and window.titleSeparator (the title bar and the OS window title)

use std::collections::{BTreeSet, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use futures::StreamExt;
use explorer::trash::Trash;
use explorer::watcher::FileWatcher;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use config::files::ReadonlyRules;
use config::glob::ExcludeMatcher;
use config::settings::{
    ActivityBarLocation, EditorGroupSettings, EditorSettings, LayoutSettings, OpenSource, ShowTabs, SideBarLocation,
};
use config::AppConfig;
use editor_view::{EditorHost, EditorView};
use model::{DirEntry, Editors, OpenOptions, list_dir};
use theme::{Theme, metrics as m};
use theme_service::ThemeService;
use search_view::{FocusSearch, SearchEvent, SearchView};
use settings_view::{OpenSettings, SettingsEvent, SettingsView};
use ui::to_gpui;

actions!(rscode, [Quit]);


fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::current_dir().expect("cwd"));
    let root = root.canonicalize().unwrap_or(root);

    gpui_kit::application().run(move |cx| {
        gpui_kit::init(cx);
        icons::install(cx);
        install_app_lifecycle(cx);
        AppConfig::install(cx, Some(settings_path())); // defaults + settings.json, live reload
        cx.set_global(session::store::SessionStorage(Some(session::store::SessionStore::from_environment()))); // restore + hot exit
        ThemeService::install(cx); // active colour theme (workbench.colorTheme, colorCustomizations)
        settings_view::install(cx); // Cmd+, binding
        editor_view::init(cx); // editor key bindings
        search_view::init(cx); // Shift+Cmd+F and Search view bindings
        quick_open::init(cx); // Cmd+P
        commands::init(cx); // command registry; Shift+Cmd+P, F1, Cmd+B, Shift+Cmd+E, Ctrl+G
        breadcrumbs::init(cx); // Shift+Cmd+; and Shift+Cmd+. focus the breadcrumbs (ADR-0047)
        language_mode::init(cx); // Cmd+K M
        explorer::actions::init(cx); // Explorer tree keys (ADR-0041)

        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(1280.), px(800.)),
                cx,
            ))),
            titlebar: Some(TitlebarOptions {
                title: Some("rscode".into()),
                appears_transparent: true,
                // Vertically centred in the 35px title bar (traffic lights are ~14px).
                traffic_light_position: Some(point(px(13.), px(10.))),
            }),
            window_min_size: Some(size(px(640.), px(400.))),
            // Background mode (RSCODE_BACKGROUND=1, `scripts/rscode --background`): open
            // without taking focus or activating the app, so it stays behind whatever the
            // user is working in. Used for automated runs of the real app (ADR-0024).
            focus: !background_mode(),
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |window, cx| {
            let wb = cx.new(|cx| {
                Workbench::with_services(
                    root,
                    cx,
                    Box::new(explorer::watcher::NotifyWatcher::default()),
                    Box::new(explorer::trash::SystemTrash),
                )
            });
            // The red window button: keep or ask about unsaved text, then close (and so quit).
            wb.update(cx, |wb, _| wb.ext.autostart = true); // the extension host starts on the first frame
            let weak = wb.downgrade();
            window.on_window_should_close(cx, move |_, cx| weak.update(cx, |wb, cx| wb.request_quit(true, cx)).unwrap_or(true));
            wb
        })
        .expect("failed to open window");
        if !background_mode() {
            cx.activate(true);
        }
    });
}

/// True when launched with RSCODE_BACKGROUND=1 (no focus steal, no activation).
fn background_mode() -> bool {
    std::env::var_os("RSCODE_BACKGROUND").is_some_and(|v| v != "0" && !v.is_empty())
}

/// macOS app lifecycle (ADR-0019): Cmd+Q / "Quit rscode" menu, and quit when the
/// last window closes (unlike the macOS default of staying alive in the Dock).
fn install_app_lifecycle(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| session::runtime::quit_app(cx)); // hot exit, or the save prompt
    cx.bind_keys([KeyBinding::new("cmd-q", Quit, None)]);
    cx.set_menus([Menu::new("rscode").items([MenuItem::action("Quit rscode", Quit)])]);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum ActivityView {
    Explorer,
    Search,
    Scm,
    Extensions,
}

impl EditorHost for Workbench {
    fn editor_host(&mut self) -> (&mut Editors, &mut EditorView) {
        (&mut self.editors, &mut self.editor_view)
    }

    fn close_requested(&mut self, path: &Path, cx: &mut Context<Self>) {
        self.request_close(vec![path.to_path_buf()], cx);
    }

    fn save_untitled_requested(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.editors.active {
            self.ask_save_untitled(i, window, cx);
        }
    }

    fn editor_command(&mut self, cmd: keymap::Command, _window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.suggest_command(cmd, cx)
    }

    fn editor_event(&mut self, event: editor_view::EditorEvent, _window: &mut Window, cx: &mut Context<Self>) {
        self.suggest_editor_event(event, cx);
    }
}

struct Workbench {
    theme: Theme,
    /// Re-render when the active theme changes; the window appearance feeds `autoDetectColorScheme`.
    _theme_sub: Subscription,
    _appearance_sub: Option<Subscription>,
    root: PathBuf,
    view: ActivityView,
    sidebar_visible: bool,
    sidebar_width: f32,
    resizing_sidebar: bool,
    /// The sash grip is receded: `workbench.sash.autoHideGrippers` after the mouse was idle for the delay.
    sash_grippers_hidden: bool,
    /// Bumped with every mouse move, so a timer from an earlier move does not recede the grip.
    sash_idle_epoch: u64,
    expanded: HashSet<PathBuf>,
    /// The Explorer tree's focus, selection and scroll (ADR-0041).
    explorer: explorer::state::ExplorerState,
    /// Lazily loaded directory listings (loaded the first time a folder is expanded).
    children: HashMap<PathBuf, Vec<DirEntry>>,
    editors: Editors,
    editor_view: EditorView,
    /// Settings editor tab (one instance) and whether it is the active tab.
    settings: Option<Entity<SettingsView>>,
    settings_active: bool,
    /// The Welcome page tab (one instance, ADR-0040) and whether it is the active tab.
    welcome_open: bool,
    welcome_active: bool,
    /// Session restore, hot exit and the quit prompt (ADR-0040).
    session: session::Runtime,
    /// The editor settings in effect (fonts resolved), rebuilt when the configuration changes.
    editor_settings: EditorSettings,
    /// `files.exclude`.
    exclude: ExcludeMatcher,
    /// `files.watcherExclude`: the changes the watcher ignores (ADR-0039).
    watcher_exclude: ExcludeMatcher,
    /// Keeps the file watcher running (dropping it stops the events).
    _watcher: Box<dyn FileWatcher>,
    /// Where `files.enableTrash` deletes go (ADR-0039).
    trash: Box<dyn Trash>,
    /// Installed font families, read once when first needed.
    installed_fonts: Option<HashSet<String>>,
    _config_sub: Subscription,
    /// The window title and dirty flag last given to the window (set only when they change).
    applied_window_title: Option<(String, bool)>,
    /// The Search view (created the first time Search is shown).
    search: Option<Entity<SearchView>>,
    /// Quick Open (Cmd+P) state.
    quick: quick_open::QuickOpenState,
    crumbs: breadcrumbs::Crumbs,
    /// Change Language Mode picker and the syntax repaint poll (ADR-0038).
    lang: language_mode::LanguageUi,
    /// Root focus handle so workbench-level actions (Cmd+,) dispatch even before anything is focused.
    focus: Option<FocusHandle>,
    /// `files.autoSave` timers and triggers (ADR-0034).
    auto_save: autosave::AutoSaveState,
    /// The extension host and what it shows (ADR-0066); off without a Code-OSS build.
    ext: extensions::Extensions,
    /// The document outline the breadcrumbs and Go to Symbol read (ADR-0086).
    outline: outline::OutlineState,
    suggest: suggest::SuggestState,
    branch: Option<String>,
    error: Option<String>,
}

impl Workbench {
    /// A workbench without a file watcher or Trash (tests and embedders). The app uses
    /// `with_services`.
    #[cfg_attr(not(test), allow(dead_code))]
    fn new(root: PathBuf, cx: &mut Context<Self>) -> Self {
        Self::with_services(root, cx, Box::new(explorer::watcher::NoWatcher), Box::new(explorer::trash::NoTrash))
    }

    /// A workbench that reports changes on disk under `root` from `watcher`, and deletes to `trash`
    /// (ADR-0039).
    fn with_services(root: PathBuf, cx: &mut Context<Self>, mut watcher: Box<dyn FileWatcher>, trash: Box<dyn Trash>) -> Self {
        if !cx.has_global::<AppConfig>() {
            AppConfig::install(cx, None); // in memory: tests and embedders that set no file
        }
        commands::ensure_installed(cx);
        ThemeService::ensure_installed(cx);
        let configuration = AppConfig::get(cx).configuration();
        let exclude = ExcludeMatcher::from_value(configuration.get_value("files.exclude").unwrap_or(&serde_json::Value::Null));
        let watcher_exclude = ExcludeMatcher::from_value(&explorer::watcher::watcher_exclude_value(
            configuration.get_value("files.watcherExclude"),
        ));
        let editor_settings = EditorSettings::from_configuration(configuration);
        let editors = Editors {
            options: editor_settings.editor_options(),
            typing: editor_settings.typing,
            detect_indentation: editor_settings.detect_indentation,
            settings: EditorGroupSettings::from_configuration(configuration),
            default_eol: editor_settings.eol,
            read_only: ReadonlyRules::from_configuration(configuration, &root),
            ..Editors::default()
        };
        theme_service::sync_token_colors(&editors.syntax, cx);
        let mut wb = Self {
            theme: ThemeService::theme(cx),
            _theme_sub: cx.observe_global::<ThemeService>(|this, cx| {
                this.theme = ThemeService::theme(cx);
                theme_service::sync_token_colors(&this.editors.syntax, cx);
                cx.notify();
            }),
            _appearance_sub: None,
            root: root.clone(),
            view: ActivityView::Explorer,
            sidebar_visible: true,
            sidebar_width: m::SIDEBAR_WIDTH,
            resizing_sidebar: false,
            sash_grippers_hidden: false,
            sash_idle_epoch: 0,
            expanded: HashSet::new(),
            explorer: explorer::state::ExplorerState::new(cx),
            children: HashMap::new(),
            editors,
            editor_view: EditorView::default(),
            settings: None,
            settings_active: false,
            welcome_open: false,
            welcome_active: false,
            session: Default::default(),
            editor_settings,
            exclude,
            watcher_exclude,
            _watcher: Box::new(explorer::watcher::NoWatcher),
            trash,
            installed_fonts: None,
            _config_sub: cx.observe_global::<AppConfig>(|this, cx| this.on_config_changed(cx)),
            applied_window_title: None,
            search: None,
            quick: Default::default(),
            crumbs: Default::default(),
            lang: Default::default(),
            focus: None,
            auto_save: Default::default(),
            ext: Default::default(),
            outline: Default::default(),
            suggest: Default::default(),
            branch: git_branch(&root),
            error: None,
        };
        wb.resolve_editor_font(cx);
        wb.apply_language_settings(cx);
        wb.children.insert(root.clone(), wb.list(&root));
        match watcher.watch(&root) {
            Ok(mut changes) => {
                wb._watcher = watcher;
                cx.spawn(async move |this, cx| {
                    while let Some(paths) = changes.next().await {
                        if this.update(cx, |wb, cx| wb.on_disk_changes(paths, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
            }
            Err(err) => eprintln!("rscode: file watcher: {err}"),
        }
        // Session restore and hot exit (ADR-0040), then `workbench.startupEditor` when nothing came back.
        wb.session_init(cx);
        wb.open_startup_editor(cx);
        wb
    }

    // ---- configuration --------------------------------------------------------

    fn layout(&self, cx: &App) -> LayoutSettings {
        LayoutSettings::from_configuration(AppConfig::get(cx).configuration())
    }

    /// The mouse moved: the grip comes back, and recedes again after the idle delay
    /// (`SashGrippersController`'s `showGrippers`). Nothing happens when auto-hide is off.
    fn sash_mouse_activity(&mut self, cx: &mut Context<Self>) {
        let layout = self.layout(cx);
        if !layout.sash_auto_hide_grippers {
            if self.sash_grippers_hidden {
                self.sash_grippers_hidden = false;
                cx.notify();
            }
            return;
        }
        if self.sash_grippers_hidden {
            self.sash_grippers_hidden = false;
            cx.notify();
        }
        self.sash_idle_epoch = self.sash_idle_epoch.wrapping_add(1);
        let epoch = self.sash_idle_epoch;
        let delay = std::time::Duration::from_millis(layout.sash_auto_hide_grippers_delay);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |this, cx| {
                if this.sash_idle_epoch == epoch && !this.sash_grippers_hidden {
                    this.sash_grippers_hidden = true;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn list(&self, dir: &Path) -> Vec<DirEntry> {
        let mut entries = list_dir(dir, &self.root, &self.exclude).unwrap_or_default();
        if self.explorer.settings.exclude_git_ignore {
            explorer::gitignore::retain_visible(&mut entries, &self.root, dir);
        }
        let s = &self.explorer.settings;
        explorer::sort::sort_entries(&mut entries, s.sort_order, s.lexicographic, s.sort_reverse);
        entries
    }

    /// Picks the first installed family of `editor.fontFamily`.
    fn resolve_editor_font(&mut self, cx: &mut Context<Self>) {
        let fonts = self
            .installed_fonts
            .get_or_insert_with(|| cx.text_system().all_font_names().into_iter().collect());
        self.editor_settings.resolve_font(|name| fonts.contains(name));
    }

    /// The configuration changed (Settings editor or an edit of settings.json): re-read what the
    /// workbench uses and apply it to what is already open.
    fn on_config_changed(&mut self, cx: &mut Context<Self>) {
        let app = AppConfig::get(cx);
        let configuration = app.configuration();
        let affects_exclude = app.last_change().affects("files.exclude");
        let affects_git_ignore = app.last_change().affects("explorer.excludeGitIgnore");
        self.session_config_changed(cx);
        let new_exclude = affects_exclude
            .then(|| ExcludeMatcher::from_value(configuration.get_value("files.exclude").unwrap_or(&serde_json::Value::Null)));
        if app.last_change().affects("files.watcherExclude") {
            self.watcher_exclude = ExcludeMatcher::from_value(&explorer::watcher::watcher_exclude_value(
                configuration.get_value("files.watcherExclude"),
            ));
        }
        let new_settings = EditorSettings::from_configuration(configuration);
        self.editors.apply_settings(EditorGroupSettings::from_configuration(configuration));
        self.editors.default_eol = new_settings.eol;
        self.editors.apply_read_only(ReadonlyRules::from_configuration(configuration, &self.root));
        self.apply_language_settings(cx);
        self.ext_config_changed(cx);

        if new_settings.font_families != self.editor_settings.font_families {
            self.editor_settings = new_settings;
            self.resolve_editor_font(cx);
        } else {
            let family = self.editor_settings.font_family.clone();
            self.editor_settings = EditorSettings { font_family: family, ..new_settings };
        }
        self.editors.apply_editor_settings(&self.editor_settings);
        self.apply_explorer_settings(cx);
        // The listed folders are read again when their exclusions change (files.exclude or
        // explorer.excludeGitIgnore; `list` applies both).
        if affects_exclude || affects_git_ignore {
            if let Some(exclude) = new_exclude {
                self.exclude = exclude;
            }
            let dirs: Vec<PathBuf> = self.children.keys().cloned().collect();
            for dir in dirs {
                let entries = self.list(&dir);
                self.children.insert(dir, entries);
            }
        }
        cx.notify();
    }

    /// Open `path` as a pinned editor (the default for opens without a source).
    fn open(&mut self, path: &Path) {
        self.open_with(path, OpenOptions::PINNED);
    }

    /// Open `path` from `source`: as the preview editor or pinned, as the settings say.
    fn open_from(&mut self, path: &Path, source: OpenSource) {
        let options = if self.editors.settings.opens_pinned(source) { OpenOptions::PINNED } else { OpenOptions::PREVIEW };
        self.open_with(path, options);
    }

    fn open_with(&mut self, path: &Path, options: OpenOptions) {
        self.settings_active = false;
        self.welcome_active = false;
        let was_open = self.editors.index_of(path).is_some();
        if let Err(e) = self.editors.open_with(path, options) {
            self.error = Some(format!("{}: {e}", path.display()));
        } else if !was_open {
            self.apply_remembered_view_state(path); // workbench.editor.restoreViewState
        }
        self.reveal_in_tree(path); // explorer.autoReveal (ADR-0041)
    }

    /// `workbench.action.files.toggleActiveEditorReadonlyInSession`: flips the active editor's
    /// read-only state for this session, whatever the settings say.
    fn toggle_read_only_in_session(&mut self, cx: &mut Context<Self>) {
        if let Some(i) = self.editors.active {
            let now = self.editors.open[i].is_read_only();
            self.editors.set_session_read_only(i, Some(!now));
        }
        cx.notify();
    }

    // ---- zoom (window.zoomLevel) -------------------------------------------------------------

    /// `workbench.action.zoomIn` / `zoomOut` (`Some(±1)`) change `window.zoomLevel`; reset (`None`)
    /// removes the user's value. The setting is the state, as upstream, so it is saved and applied
    /// through the configuration.
    fn zoom(&mut self, step: Option<f64>, cx: &mut Context<Self>) {
        let value = step.map(|step| {
            let level = AppConfig::get(cx).configuration().get_f64("window.zoomLevel").unwrap_or(0.0);
            let (min, max) = config::settings::ZOOM_LEVEL_RANGE;
            serde_json::json!((level + step).clamp(min, max))
        });
        if let Err(e) = AppConfig::set_value(cx, "window.zoomLevel", value) {
            self.error = Some(e);
        }
    }

    // ---- Search view (ADR-0023) ------------------------------------------------------------

    fn ensure_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<SearchView> {
        if let Some(v) = &self.search {
            return v.clone();
        }
        let root = self.root.clone();
        let settings = search_view::current_settings(cx);
        let view = cx.new(|cx| SearchView::new(root, settings, window, cx));
        cx.subscribe_in(&view, window, |this, _, ev: &SearchEvent, window, cx| match ev {
            SearchEvent::Open { path, range } => this.open_at(path, *range, OpenSource::Search, window, cx),
            SearchEvent::OpenSettings => this.open_settings(window, cx),
            SearchEvent::Replace(req) => this.apply_search_replace(req, cx),
        })
        .detach();
        self.search = Some(view.clone());
        view
    }

    /// Shift+Cmd+F: show the Search view and focus its query box.
    fn focus_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.view = ActivityView::Search;
        self.sidebar_visible = true;
        let seed = self.search_seed(cx);
        let view = self.ensure_search(window, cx);
        view.update(cx, |v, cx| {
            v.seed_query(seed, window, cx);
            v.focus_query(window, cx)
        });
        cx.notify();
    }

    /// The query's starting text when the Search view is focused (`search.seedOnFocus`,
    /// `search.seedWithNearestWord`), from the active editor.
    fn search_seed(&self, cx: &App) -> Option<String> {
        let editor = &self.editors.active_editor()?.editor;
        search_view::model::seed_text(editor, &search_view::current_settings(cx))
    }

    /// Open `path` with `range` selected and scrolled into view (a Search result or a
    /// `:line:column` in Quick Open was activated).
    fn open_at(
        &mut self,
        path: &Path,
        range: vsr_search::SearchRange,
        source: OpenSource,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        use vsr_editor::Position;
        self.open_from(path, source);
        if let Some(e) = self.editors.active.and_then(|i| self.editors.open.get_mut(i)).filter(|e| e.path == path) {
            e.editor.set_cursor(Position::new(range.start_line, range.start_column), false);
            e.editor.set_cursor(Position::new(range.end_line, range.end_column), true);
            self.editor_view.reveal(e);
        }
        cx.notify();
    }

    // ---- Settings editor tab (grouped: open / close / tab / surface) ---------

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            let view = cx.new(|cx| SettingsView::new(None, window, cx));
            cx.subscribe(&view, |this, _, ev: &SettingsEvent, cx| match ev {
                SettingsEvent::OpenSettingsJson(path) => {
                    this.open(path);
                    cx.notify();
                }
            })
            .detach();
            self.settings = Some(view);
        }
        self.settings_active = true;
        self.welcome_active = false;
        if let Some(view) = &self.settings {
            view.update(cx, |v, cx| v.focus_search(window, cx));
        }
        cx.notify();
    }

    /// A text editor is the tab being shown (neither the Settings editor nor the Welcome page).
    fn editor_surface_active(&self) -> bool {
        !self.settings_active && !self.welcome_active
    }

    fn close_settings(&mut self) {
        self.settings = None;
        self.settings_active = false;
    }

    fn settings_tab(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        self.settings.as_ref()?;
        let t = &self.theme;
        let active = self.settings_active;
        let fg = to_gpui(if active { t.tab_active_fg } else { t.tab_inactive_fg });
        let line = |d: Div, c| d.absolute().left_0().w_full().h(px(1.)).bg(to_gpui(c));
        let close = div()
            .id("settings-tab-close")
            .flex_none()
            .w(px(20.))
            .h(px(20.))
            .rounded(px(5.))
            .flex()
            .items_center()
            .justify_center()
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(if active { fg } else { gpui_kit::transparent_black() })
            .group_hover("settings-tab", |s| s.text_color(fg))
            .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
            .child(icons::glyph(icons::CLOSE))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.close_settings();
                    cx.notify();
                }),
            );
        Some(
            div()
                .id("settings-tab")
                .group("settings-tab")
                .relative()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(12.))
                .pr(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(fg)
                .bg(to_gpui(if active { t.tab_active_bg } else { t.tab_inactive_bg }))
                .child(if active { line(div().top_0(), t.tab_active_border_top) } else { line(div().bottom_0(), t.tabs_border) })
                .child(div().absolute().right_0().top_0().h_full().w(px(1.)).bg(to_gpui(t.tab_border)))
                .child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(icons::SETTINGS_GEAR)))
                .child("Settings")
                .child(close)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.settings_active = true;
                        this.welcome_active = false;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(|this, _, _, cx| {
                        this.close_settings();
                        cx.notify();
                    }),
                ),
        )
    }

    /// The file system changed under the workspace (ADR-0039): re-list the folders the tree shows,
    /// forget the listings of folders that are gone, reload clean editors whose file changed, and
    /// close the editors of files that are gone (`closeOnFileDelete`).
    fn on_disk_changes(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let paths = explorer::watcher::filter_changes(paths, &self.root, &self.watcher_exclude);
        if paths.is_empty() {
            return;
        }
        let parents: BTreeSet<PathBuf> = paths.iter().filter_map(|p| p.parent().map(Path::to_path_buf)).collect();
        for dir in parents {
            self.refresh_dir(&dir);
        }
        for p in paths.iter().filter(|p| !p.exists()) {
            self.forget_tree(p);
        }
        self.reload_clean_editors(&paths);
        self.close_deleted_files(cx);
        cx.notify();
    }

    /// An editor with no unsaved changes whose file changed on disk shows the new text. A dirty
    /// editor keeps its text; its save asks what to do (`files.saveConflictResolution`).
    fn reload_clean_editors(&mut self, paths: &[PathBuf]) {
        let default_eol = self.editors.default_eol;
        for e in &mut self.editors.open {
            if e.is_dirty() || !paths.contains(&e.path) || !e.disk_changed() {
                continue;
            }
            if let Ok(bytes) = fs::read(&e.path) {
                e.reload(&String::from_utf8_lossy(&bytes), default_eol);
            }
        }
    }

    /// The conflict bar's actions (ADR-0039): Overwrite saves the buffer over the file on disk;
    /// Revert replaces the buffer with the file on disk.
    fn resolve_save_conflict(&mut self, overwrite: bool, cx: &mut Context<Self>) {
        let Some(path) = self.editor_view.save_conflict.take() else { return };
        if let Some(index) = self.editors.index_of(&path) {
            if overwrite {
                let transforms = self.editor_settings.save_transforms(false);
                if let Err(err) = self.editors.open[index].save_with(transforms) {
                    eprintln!("rscode: save {}: {err}", path.display());
                }
            } else if let Ok(bytes) = fs::read(&path) {
                let default_eol = self.editors.default_eol;
                self.editors.open[index].reload(&String::from_utf8_lossy(&bytes), default_eol);
            }
        }
        cx.notify();
    }

    /// `workbench.editor.closeOnFileDelete`: close the editors whose file is gone (a dirty one
    /// stays open, as upstream does).
    fn close_deleted_files(&mut self, cx: &mut Context<Self>) {
        if !self.editors.settings.close_on_file_delete {
            return;
        }
        let closed = self.editors.close_missing(|p| p.exists());
        if !closed.is_empty() {
            for path in &closed {
                self.editor_view.forget(path);
            }
            cx.notify();
        }
    }

    fn close_tab(&mut self, index: usize) {
        if let Some(e) = self.editors.open.get(index) {
            let (path, dirty) = (e.path.clone(), e.is_dirty());
            self.editor_view.remember(e);
            self.editors.close(index);
            self.editor_view.forget(&path);
            self.session_editor_closed(&path, dirty);
        }
    }

    // ---- parts --------------------------------------------------------------

    /// The text of `window.title` for the current state (upstream `WindowTitle.getWindowTitle`).
    fn window_title(&self, cx: &App) -> String {
        let config = AppConfig::get(cx).configuration();
        let template = config.get_value("window.title").and_then(|v| v.as_str());
        let separator = config.get_value("window.titleSeparator").and_then(|v| v.as_str());
        self.window_title_variables().expand(template, separator)
    }

    fn window_title_variables(&self) -> window_title::Variables {
        let root = &self.root;
        let mut vars = window_title::Variables {
            root_name: folder_name(root),
            root_path: root.display().to_string(),
            folder_name: folder_name(root),
            folder_path: root.display().to_string(),
            app_name: "rscode".into(),
            ..Default::default()
        };
        if self.welcome_showing() && !self.settings_active {
            vars.active_editor_short = "Welcome".into();
            vars.active_editor_medium = "Welcome".into();
            vars.active_editor_long = "Welcome".into();
        } else if self.settings_active {
            vars.active_editor_short = "Settings".into();
            vars.active_editor_medium = "Settings".into();
            vars.active_editor_long = "Settings".into();
        } else if let Some(e) = self.editors.active_editor() {
            let relative = |p: &Path| p.strip_prefix(root).map_or_else(|_| p.display().to_string(), |r| r.display().to_string());
            let folder = e.path.parent().unwrap_or(root);
            vars.active_editor_short = self.editors.title_of(e);
            vars.active_editor_medium = relative(&e.path);
            vars.active_editor_long = e.path.display().to_string();
            if let Some(name) = e.untitled_name() {
                // An untitled text has no path: its name stands for it (ADR-0058).
                vars.active_editor_medium = name.to_string();
                vars.active_editor_long = name.to_string();
            }
            vars.active_editor_language_id = e.language_id().to_string();
            vars.active_folder_short = folder.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            vars.active_folder_medium = relative(folder);
            vars.active_folder_long = folder.display().to_string();
            vars.dirty = e.is_dirty();
        }
        vars
    }

    fn title_bar(&self, title: &str, command_center: bool, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let bar = div()
            .flex_none()
            .h(px(m::TITLE_BAR_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(to_gpui(t.title_bar_bg))
            .border_b_1()
            .border_color(to_gpui(t.title_bar_border))
            .text_size(px(13.))
            .text_color(to_gpui(t.title_bar_fg))
            .window_control_area(WindowControlArea::Drag);
        if !command_center {
            return bar.child(title.to_string());
        }
        // `window.commandCenter` (upstream commandCenterControl): a search box that shows the
        // title; a click opens Quick Open.
        bar.child(
            div()
                .id("command-center")
                .debug_selector(|| "command-center".to_string())
                .flex()
                .items_center()
                .gap(px(6.))
                .h(px(22.))
                .w(px(m::COMMAND_CENTER_WIDTH))
                .px(px(8.))
                .rounded(px(5.))
                .border_1()
                .border_color(to_gpui(t.input_border))
                .bg(to_gpui(t.editor_bg))
                .cursor_pointer()
                .child(
                    div()
                        .font_family(icons::FAMILY)
                        .text_size(px(13.))
                        .text_color(to_gpui(t.description_fg))
                        .child(icons::glyph(icons::SEARCH)),
                )
                .child(div().min_w_0().overflow_hidden().whitespace_nowrap().child(title.to_string()))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| this.show_quick_open(window, cx)),
                ),
        )
    }

    fn status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let fg = to_gpui(t.status_bar_fg);
        let icon = |c: char| {
            div()
                .font_family(icons::FAMILY)
                .text_size(px(14.))
                .child(icons::glyph(c))
        };
        let item = || div().flex().items_center().gap(px(4.)).px(px(5.)).h_full();

        let mut left = div().flex().items_center().h_full();
        if let Some(b) = &self.branch {
            left = left.child(item().child(icon(icons::GIT_BRANCH)).child(b.clone()));
        }
        // The problem counters: what the extensions' diagnostics add up to.
        let (errors, warnings) = self.ext.snapshot.counts;
        left = left.child(
            item()
                .debug_selector(|| "status-problems".to_string())
                .child(icon(icons::ERROR))
                .child(errors.to_string())
                .child(div().w(px(6.)))
                .child(icon(icons::WARNING))
                .child(warnings.to_string()),
        );
        let ext_items = self.ext_status_items(cx);
        for it in ext_items.iter().filter(|i| i.align_left) {
            left = left.child(self.ext_status_item(it, cx));
        }
        if let Some(err) = &self.error {
            left = left.child(item().text_color(to_gpui(t.error_fg)).child(err.clone()));
        }

        let mut right = div().flex().items_center().h_full();
        for it in ext_items.iter().filter(|i| !i.align_left) {
            right = right.child(self.ext_status_item(it, cx));
        }
        if let Some(e) = self.editors.active_editor() {
            right = right
                .child(item().id("status-position").child(editor_view::cursor_status(e)))
                .child(item().child(editor_view::indent_status(e)))
                .child(item().child(format!("{} lines", e.buffer().line_count())))
                .child(item().child("UTF-8"))
                .child(item().child(e.eol_label()))
                .child(item().id("status-language").child(self.editors.language_name(e)));
        }
        right = right.child(
            item()
                .id("status-bell")
                .child(icon(icons::BELL))
                .tooltip(|w, cx| Tooltip::new("No Notifications").build(w, cx)),
        );

        div()
            .flex_none()
            .debug_selector(|| "status-bar".to_string())
            .h(px(m::STATUS_BAR_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(5.))
            .bg(to_gpui(t.status_bar_bg))
            .border_t_1()
            .border_color(to_gpui(t.status_bar_border))
            .text_size(px(12.))
            .text_color(fg)
            .child(left)
            .child(right)
    }

    /// The activity bar: a vertical bar next to the side bar (`default`; on the side bar's outer
    /// edge, so right of it when the side bar is on the right, like layout.ts) or, for `top` and
    /// `bottom`, a horizontal strip in the side bar column.
    fn activity_bar(&self, cx: &mut Context<Self>, side: SideBarLocation, location: ActivityBarLocation) -> impl IntoElement {
        let t = &self.theme;
        let horizontal = location != ActivityBarLocation::Default;
        let compact = !horizontal && self.layout(cx).activity_bar_compact;
        let (cell_w, cell_h, icon_size) = if horizontal {
            (40., 35., 24.)
        } else if compact {
            (m::ACTIVITY_BAR_COMPACT_WIDTH, m::ACTIVITY_BAR_COMPACT_ITEM_HEIGHT, m::ACTIVITY_BAR_COMPACT_ICON_SIZE)
        } else {
            (m::ACTIVITY_BAR_WIDTH, m::ACTIVITY_BAR_WIDTH, 24.)
        };
        let cell = |id: &'static str, ix: usize, glyph: char, tip: &'static str, active: bool| {
            let fg = if active { t.activity_fg } else { t.activity_inactive_fg };
            div()
                .id((id, ix))
                .relative()
                .flex_none()
                .w(px(cell_w))
                .h(px(cell_h))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .font_family(icons::FAMILY)
                .text_size(px(icon_size))
                .debug_selector(move || format!("{id}:{ix}"))
                .text_color(to_gpui(fg))
                .hover(|s| s.text_color(to_gpui(t.activity_fg)))
                .when(active, |d| {
                    let bar = div().absolute().bg(to_gpui(t.activity_active_border));
                    d.child(match (horizontal, side) {
                        (true, _) => bar.left_0().bottom_0().w_full().h(px(2.)),
                        (false, SideBarLocation::Left) => bar.left_0().top_0().w(px(2.)).h_full(),
                        (false, SideBarLocation::Right) => bar.right_0().top_0().w(px(2.)).h_full(),
                    })
                })
                .child(icons::glyph(glyph))
                .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
        };

        let mut top = div().flex();
        if !horizontal {
            top = top.flex_col();
        }
        for (i, (view, glyph, tip)) in [
            (ActivityView::Explorer, icons::FILES, "Explorer (⇧⌘E)"),
            (ActivityView::Search, icons::SEARCH, "Search (⇧⌘F)"),
            (ActivityView::Scm, icons::SOURCE_CONTROL, "Source Control (⌃⇧G)"),
            (ActivityView::Extensions, icons::EXTENSIONS, "Extensions (⇧⌘X)"),
        ]
        .into_iter()
        .enumerate()
        {
            let active = self.sidebar_visible && self.view == view;
            top = top.child(cell("activity", i, glyph, tip, active).on_click(cx.listener(
                move |this, _, window, cx| {
                    let click_focus = this.layout(cx).activity_bar_click_focus;
                    // Clicking the active view toggles the side bar, like VS Code (`toggle`); with
                    // `focus` the view stays shown.
                    if this.sidebar_visible && this.view == view {
                        if !click_focus {
                            this.sidebar_visible = false;
                        }
                    } else {
                        this.view = view;
                        this.sidebar_visible = true;
                        if view == ActivityView::Search {
                            this.ensure_search(window, cx);
                        }
                    }
                    cx.notify();
                },
            )));
        }

        // Global activities pinned to the bottom.
        let mut bottom = div().flex();
        if !horizontal {
            bottom = bottom.flex_col();
        }
        let bottom = bottom
            .child(cell("global", 0, icons::ACCOUNT, "Accounts", false))
            .child(
                cell("global", 1, icons::SETTINGS_GEAR, "Manage (Settings)", false).on_click(cx.listener(
                    |this, _, window, cx| this.open_settings(window, cx),
                )),
            );

        let bar = div().flex_none().flex().justify_between().bg(to_gpui(t.activity_bar_bg)).border_color(to_gpui(t.activity_bar_border));
        match location {
            ActivityBarLocation::Default => {
                let bar = bar.w(px(cell_w)).h_full().flex_col();
                match side {
                    SideBarLocation::Left => bar.border_r_1(),
                    SideBarLocation::Right => bar.border_l_1(),
                }
            }
            ActivityBarLocation::Bottom => bar.w_full().h(px(cell_h)).border_t_1(),
            _ => bar.w_full().h(px(cell_h)).border_b_1(),
        }
        .debug_selector(|| "activity-bar".to_string())
        .child(top)
        .child(bottom)
    }

    fn side_bar(&self, cx: &mut Context<Self>, activity: ActivityBarLocation) -> AnyElement {
        let t = &self.theme;
        let heading = match self.view {
            ActivityView::Explorer => "EXPLORER",
            ActivityView::Search => "SEARCH",
            ActivityView::Scm => "SOURCE CONTROL",
            ActivityView::Extensions => "EXTENSIONS",
        };
        if let (ActivityView::Search, Some(search)) = (self.view, &self.search) {
            // The Search view draws its own title row (it carries the view actions).
            return div()
                .flex_none()
                .w(px(self.sidebar_width))
                .h_full()
                .bg(to_gpui(t.side_bar_bg))
                .text_color(to_gpui(t.side_bar_fg))
                .child(search.clone())
                .into_any_element();
        }
        let body = match self.view {
            ActivityView::Explorer => self.explorer(cx).into_any_element(),
            _ => div()
                .px(px(20.))
                .text_size(px(12.))
                .text_color(to_gpui(t.tab_inactive_fg))
                .child("Not implemented yet.")
                .into_any_element(),
        };
        // `top` / `bottom` activity bar: a strip above or below the view.
        let strip = (activity != ActivityBarLocation::Default && activity != ActivityBarLocation::Hidden)
            .then(|| self.activity_bar(cx, SideBarLocation::Left, activity).into_any_element());
        let (strip_top, strip_bottom) =
            if activity == ActivityBarLocation::Top { (strip, None) } else { (None, strip) };
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.side_bar_bg))
            .text_color(to_gpui(t.side_bar_fg))
            .debug_selector(|| "side-bar".to_string())
            .children(strip_top)
            .child(
                div()
                    .flex_none()
                    .h(px(35.))
                    .px(px(20.))
                    .flex()
                    .items_center()
                    .text_size(px(11.))
                    .child(heading),
            )
            .child(body)
            .children(strip_bottom)
            .into_any_element()
    }

    fn tab_strip(&self, cx: &mut Context<Self>, show_tabs: ShowTabs) -> impl IntoElement {
        let t = &self.theme;
        let tab_height = if self.layout(cx).compact_tabs { m::COMPACT_TAB_HEIGHT } else { m::TAB_HEIGHT };
        let mut strip = div()
            .relative()
            .debug_selector(|| "tab-strip".to_string())
            .flex_none()
            .h(px(tab_height))
            .w_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .bg(to_gpui(t.tab_strip_bg))
            // `workbench.editor.scrollToSwitchTabs`: a wheel step over the strip activates the
            // neighbouring tab (scrolling down goes right, as upstream).
            .on_scroll(cx.listener(|this, ev: &ScrollEvent, _, cx| {
                if this.editors.settings.scroll_to_switch_tabs {
                    let dy = f32::from(ev.delta.pixel_delta(px(m::TAB_HEIGHT)).y);
                    if dy != 0.0 {
                        this.editors.step_active(if dy < 0.0 { 1 } else { -1 });
                        this.settings_active = false;
                        this.welcome_active = false;
                        cx.notify();
                    }
                }
            }))
            // Bottom border across the whole strip; the active tab paints over it.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w_full()
                    .h(px(1.))
                    .bg(to_gpui(t.tabs_border)),
            );
        let single = show_tabs == ShowTabs::Single;
        if !single || self.welcome_showing() {
            strip = strip.children(self.welcome_tab(cx));
        }
        let highlight = self.editors.settings.highlight_modified_tabs;
        let labels = self.editors.tab_labels(&self.root, self.editors.settings.label_format);
        for (i, e) in self.editors.open.iter().enumerate() {
            // `workbench.editor.showTabs: single` shows only the active editor's tab.
            if single && (self.editors.active != Some(i) || !self.editor_surface_active()) {
                continue;
            }
            let active = self.editors.active == Some(i) && self.editor_surface_active();
            let fg = to_gpui(if active { t.tab_active_fg } else { t.tab_inactive_fg });
            let line = |d: Div, c| d.absolute().left_0().w_full().h(px(1.)).bg(to_gpui(c));
            // `workbench.editor.highlightModifiedTabs` marks a modified tab with a top border in
            // place of the dirty dot (upstream hides the dot in that case).
            let modified = e.is_dirty();
            let highlighted = modified && highlight;
            let dirty = modified && !highlight;
            let top_line = if highlighted {
                Some(if active { t.tab_active_modified_border } else { t.tab_inactive_modified_border })
            } else if active {
                Some(t.tab_active_border_top)
            } else {
                None
            };
            // Dirty tabs show a filled circle in place of the close button until the button
            // itself is hovered (upstream `.tab.dirty ... .action-label:not(:hover)`).
            let layer = |id: &'static str, c: char| {
                div()
                    .id((id, i))
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icons::glyph(c))
            };
            let close = div()
                .id(("tab-close", i))
                .debug_selector(|| format!("tab-close:{i}"))
                .group(format!("tab-close-{i}"))
                .relative()
                .flex_none()
                .w(px(20.))
                .h(px(20.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .text_color(if active || dirty { fg } else { gpui_kit::transparent_black() })
                .group_hover(format!("tab-{i}"), |s| s.text_color(fg))
                .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
                .when(!dirty, |d| d.child(icons::glyph(icons::CLOSE)))
                .when(dirty, |d| {
                    d.child(
                        layer("tab-dot", icons::CIRCLE_FILLED)
                            .group_hover(format!("tab-close-{i}"), |s| s.opacity(0.)),
                    )
                    .child(
                        layer("tab-x", icons::CLOSE)
                            .opacity(0.)
                            .group_hover(format!("tab-close-{i}"), |s| s.opacity(1.)),
                    )
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.request_close_tab(i, cx);
                        cx.notify();
                    }),
                );
            let tab = div()
                .id(("tab", i))
                .debug_selector(|| format!("tab:{i}"))
                .group(format!("tab-{i}"))
                .when(self.editors.is_preview(i), |d| d.italic())
                .relative()
                .flex_none()
                .h_full()
                .flex()
                .items_center()
                .gap(px(6.))
                .pl(px(12.))
                .pr(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .text_color(fg)
                .bg(to_gpui(if active { t.tab_active_bg } else { t.tab_inactive_bg }))
                .child(match top_line {
                    Some(c) => line(div().top_0(), c),
                    None => line(div().bottom_0(), t.tabs_border),
                })
                .child(
                    div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .h_full()
                        .w(px(1.))
                        .bg(to_gpui(t.tab_border)),
                )
                .when(self.editors.open[i].is_read_only(), |d| {
                    // `files.readonly*` (ADR-0034): a lock in front of the name, as upstream.
                    d.child(div().font_family(icons::FAMILY).text_size(px(12.)).child(icons::glyph(icons::LOCK)))
                })
                .child(labels[i].title.clone())
                .children(labels[i].description.clone().map(|d| div().text_color(to_gpui(t.description_fg)).child(d)))
                .child(close)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        this.editors.activate(i);
                        this.settings_active = false;
                        this.welcome_active = false;
                        // A double click pins a preview (upstream: double click keeps the editor).
                        if let Some(path) = this.editors.open.get(i).map(|e| e.path.clone()).filter(|_| ev.click_count >= 2) {
                            this.editors.pin(&path);
                        }
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, _, _, cx| {
                        this.request_close_tab(i, cx);
                        cx.notify();
                    }),
                );
            strip = strip.child(tab);
        }
        if !single || self.settings_active {
            strip = strip.children(self.settings_tab(cx));
        }
        strip
    }

    fn editor_group(&mut self, cx: &mut Context<Self>, show_tabs: ShowTabs) -> impl IntoElement {
        let t = self.theme.clone();
        let crumbs = self.breadcrumb_bar(cx);
        let settings = self.editor_settings.clone();
        let settings_surface = self.settings.clone().filter(|_| self.settings_active);
        let conflict = self
            .editor_view
            .save_conflict
            .clone()
            .filter(|p| self.editors.active_editor().is_some_and(|e| &e.path == p));
        let surface = match self.editors.active.and_then(|i| self.editors.open.get(i)) {
            _ if settings_surface.is_some() => settings_surface.unwrap().into_any_element(),
            _ if self.welcome_showing() => self.welcome_surface(cx),
            Some(editor) => self.editor_view.element(editor, &t, &settings, cx).into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(16.))
                .text_color(to_gpui(t.tab_inactive_fg))
                // `workbench.editor.empty.hint: hidden` shows nothing.
                .children(self.editors.settings.empty_hint.then_some("rscode — open a file from the Explorer"))
                .into_any_element(),
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.editor_bg))
            .debug_selector(|| "editor-group".to_string())
            .when(show_tabs != ShowTabs::None, |d| d.child(self.tab_strip(cx, show_tabs)))
            .when_some(crumbs, |d, bar| d.child(bar))
            .when_some(conflict, |d, path| d.child(self.conflict_bar(&path, cx)))
            .child(div().flex_1().min_h_0().overflow_hidden().child(surface))
    }

    /// The save conflict bar (upstream `textFileSaveErrorHandler`, `staleSaveError`): the file
    /// changed on disk while the editor had unsaved changes. Compare is not available yet (it needs
    /// the diff editor).
    fn conflict_bar(&self, path: &Path, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let button = |id: &'static str, label: &'static str, overwrite: bool| {
            div()
                .id(id)
                .flex_none()
                .px(px(8.))
                .py(px(2.))
                .rounded(px(2.))
                .text_size(px(12.))
                .bg(to_gpui(t.list_active_bg))
                .cursor_pointer()
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_save_conflict(overwrite, cx)))
        };
        div()
            .flex_none()
            .px(px(8.))
            .py(px(4.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(to_gpui(t.editor_bg))
            .text_size(px(12.))
            .text_color(to_gpui(t.tab_inactive_fg))
            .child(div().flex_1().min_w_0().child(format!(
                "Failed to save '{name}': The content of the file is newer. Please compare your version with the file contents or overwrite the content of the file with your changes."
            )))
            .child(button("save-conflict-overwrite", "Overwrite", true))
            .child(button("save-conflict-revert", "Revert", false))
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A preview that was edited (it is dirty now) is pinned. Editing re-renders the tab strip,
        // so this runs on the frame that shows the dirty state.
        self.editors.pin_dirty_preview();
        self.auto_save_tick(window, cx);
        self.poll_syntax(cx);
        self.session_render_hook(window, cx);
        self.ext_render_hook(window, cx);
        self.outline_sync();
        self.suggest_sync(cx);
        // The OS window title and the dirty dot (macOS shows it in the close button).
        let title = self.window_title(cx);
        let dirty = self.editors.active_editor().is_some_and(|e| e.is_dirty()) && self.editor_surface_active();
        if self.applied_window_title.as_ref() != Some(&(title.clone(), dirty)) {
            window.set_window_title(&title);
            window.set_window_edited(dirty);
            self.applied_window_title = Some((title.clone(), dirty));
        }
        let focus = match &self.focus {
            Some(f) => f.clone(),
            None => {
                let f = cx.focus_handle();
                f.focus(window, cx);
                self.focus = Some(f.clone());
                f
            }
        };
        if self._appearance_sub.is_none() {
            self._appearance_sub = Some(theme_service::watch_window_appearance(window, cx));
        }
        self.theme = ThemeService::theme(cx);
        let t = self.theme.clone();
        let layout = self.layout(cx);
        let right = layout.side_bar == SideBarLocation::Right;
        let vertical_activity = layout.activity_bar == ActivityBarLocation::Default;

        // Left to right: [activity bar] [side bar + sash] editor group [sash + side bar] [activity
        // bar]; the activity bar stays on the side bar's outer edge (layout.ts).
        let mut middle = div().flex_1().min_h_0().w_full().flex().flex_row();
        if vertical_activity && !right {
            middle = middle.child(self.activity_bar(cx, layout.side_bar, layout.activity_bar));
        }
        let side = self.sidebar_visible.then(|| {
            let sash = div()
                .id("sidebar-resize")
                .flex_none()
                .w(px(1.))
                .h_full()
                .bg(to_gpui(t.side_bar_border))
                .relative()
                .debug_selector(|| "sidebar-resize".to_string())
                .child(
                    div()
                        .absolute()
                        .top_0()
                        // `workbench.sash.size` is the drag area, centred on the 1px border.
                        .left(px(-layout.sash_size / 2.))
                        .w(px(layout.sash_size))
                        .h_full()
                        .debug_selector(|| "sidebar-resize-hit".to_string())
                        .cursor_col_resize()
                        .hover(|s| s.bg(to_gpui(t.sash_hover_border)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.resizing_sidebar = true;
                                cx.notify();
                            }),
                        ),
                )
                // The grip: drawn in the middle of the sash, receded while the mouse is idle (ADR-0056).
                .children((!self.sash_grippers_hidden).then(|| {
                    div()
                        .absolute()
                        .top_1_2()
                        .mt(px(-12.))
                        .left(px(-1.5))
                        .w(px(3.))
                        .h(px(24.))
                        .rounded(px(1.5))
                        .debug_selector(|| "sidebar-resize-gripper".to_string())
                        .bg(to_gpui(t.sash_hover_border))
                }));
            (self.side_bar(cx, layout.activity_bar).into_any_element(), sash.into_any_element())
        });
        let mut right_group = Vec::new();
        match side {
            Some((side_bar, sash)) if !right => {
                middle = middle.child(side_bar).child(sash);
            }
            Some((side_bar, sash)) => {
                right_group.push(sash.into_any_element());
                right_group.push(side_bar.into_any_element());
            }
            None => {}
        }
        middle = middle.child(self.editor_group(cx, layout.show_tabs)).children(right_group);
        if vertical_activity && right {
            middle = middle.child(self.activity_bar(cx, layout.side_bar, layout.activity_bar));
        }

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.editor_bg))
            .text_color(to_gpui(t.foreground))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .relative()
            .key_context("Workbench")
            .track_focus(&focus)
            .on_action(cx.listener(|this, _: &quick_open::GoToFile, window, cx| this.show_quick_open(window, cx)))
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(window, cx)))
            .on_action(cx.listener(|this, _: &FocusSearch, window, cx| this.focus_search(window, cx)))
            .on_action(cx.listener(|this, _: &commands::ShowCommands, window, cx| this.show_quick_access(">", window, cx)))
            .on_action(cx.listener(|this, _: &commands::GoToLine, window, cx| this.show_quick_access(":", window, cx)))
            .on_action(cx.listener(|this, _: &commands::GoToSymbol, window, cx| this.show_quick_access("@", window, cx)))
            .on_action(cx.listener(|this, _: &commands::ToggleSidebar, _, cx| this.toggle_sidebar(cx)))
            .on_action(cx.listener(|this, _: &theme_picker::SelectTheme, window, cx| this.show_theme_picker(window, cx)))
            .on_action(cx.listener(|this, _: &breadcrumbs::BreadcrumbsFocus, window, cx| this.focus_crumbs(false, window, cx)))
            .on_action(cx.listener(|this, _: &breadcrumbs::BreadcrumbsFocusAndSelect, window, cx| this.focus_crumbs(true, window, cx)))
            .on_action(cx.listener(|this, _: &breadcrumbs::BreadcrumbsToggle, _, cx| this.toggle_breadcrumbs(cx)))
            .on_action(cx.listener(|this, _: &commands::ZoomIn, _, cx| this.zoom(Some(1.0), cx)))
            .on_action(cx.listener(|this, _: &commands::ZoomOut, _, cx| this.zoom(Some(-1.0), cx)))
            .on_action(cx.listener(|this, _: &commands::ZoomReset, _, cx| this.zoom(None, cx)))
            .on_action(cx.listener(|this, _: &commands::ShowExplorer, _, cx| this.show_explorer(cx)))
            .on_action(cx.listener(|this, _: &commands::NewUntitledFile, window, cx| this.new_untitled(window, cx)))
            .on_action(cx.listener(|this, _: &welcome::ShowWelcome, _, cx| this.open_welcome(cx)))
            .on_action(cx.listener(|this, _: &commands::ClearCommandHistory, _, cx| this.clear_command_history(cx)))
            .on_action(cx.listener(|this, _: &commands::ToggleReadonlyInSession, _, cx| this.toggle_read_only_in_session(cx)))
            .on_action(cx.listener(|this, _: &language_mode::ChangeLanguageMode, window, cx| this.show_language_picker(window, cx)))
            .map(|root| Workbench::bind_explorer_actions(root, cx))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, window, cx| {
                this.sash_mouse_activity(cx);
                if this.resizing_sidebar {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        let layout = this.layout(cx);
                        let activity = if layout.activity_bar == ActivityBarLocation::Default { activity_bar_width(&layout) } else { 0. };
                        let x = match layout.side_bar {
                            SideBarLocation::Left => f32::from(ev.position.x) - activity,
                            SideBarLocation::Right => f32::from(window.viewport_size().width) - f32::from(ev.position.x) - activity,
                        };
                        this.sidebar_width = x.clamp(170., 600.);
                    } else {
                        this.resizing_sidebar = false;
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.resizing_sidebar {
                        this.resizing_sidebar = false;
                        cx.notify();
                    }
                }),
            )
            .child(self.title_bar(&title, layout.command_center, cx))
            .child(middle)
            .children(self.quit_prompt_bar(cx))
            .children(self.close_prompt_bar(cx))
            .when(layout.status_bar_visible, |d| d.child(self.status_bar(cx)))
            .children(self.quick.view())
            .children(self.lang.view())
    }
}

/// Current branch of the folder's git repo, if any (placeholder until an SCM service exists).
fn git_branch(root: &Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    let name = String::from_utf8(out.stdout).ok()?.trim().to_string();
    (out.status.success() && !name.is_empty()).then_some(name)
}

/// The settings.json the app reads and writes. `RSCODE_USER_DATA_DIR=<dir>` puts it at
/// `<dir>/User/settings.json` instead of the default user location (for sandboxed runs and tests).
fn settings_path() -> PathBuf {
    match std::env::var_os("RSCODE_USER_DATA_DIR").filter(|d| !d.is_empty()) {
        Some(dir) => PathBuf::from(dir).join("User/settings.json"),
        None => vsr_settings::default_user_settings_path(),
    }
}

fn folder_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// The width of the activity bar when it sits on the side bar's outer edge (location `default`):
/// 48px, or 36px with `workbench.activityBar.compact`.
fn activity_bar_width(layout: &LayoutSettings) -> f32 {
    if layout.activity_bar_compact { m::ACTIVITY_BAR_COMPACT_WIDTH } else { m::ACTIVITY_BAR_WIDTH }
}
