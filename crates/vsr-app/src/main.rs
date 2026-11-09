//! rscode: the native workbench window (gpui via gpui-kit, ADR-0012).
//!
//! Layout mirrors the VS Code workbench parts: title bar, activity bar,
//! side bar (explorer), editor group (tabs + editor), status bar.
//!
//! upstream: src/vs/workbench/browser/layout.ts @ 50e1350fc78d

mod editor_view;
mod icons;
mod model;
mod settings_view; // Settings editor (ADR-0017)
mod theme;
mod ui;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use editor_view::EditorView;
use model::{DirEntry, Editors, list_dir};
use theme::{Theme, metrics as m};
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
        settings_view::install(cx); // Cmd+, binding + component theme

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
            ..Default::default()
        };
        gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| Workbench::new(root)))
            .expect("failed to open window");
        cx.activate(true);
    });
}

/// macOS app lifecycle (ADR-0019): Cmd+Q / "Quit rscode" menu, and quit when the
/// last window closes (unlike the macOS default of staying alive in the Dock).
fn install_app_lifecycle(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
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

struct Workbench {
    theme: Theme,
    root: PathBuf,
    view: ActivityView,
    sidebar_visible: bool,
    sidebar_width: f32,
    resizing_sidebar: bool,
    expanded: HashSet<PathBuf>,
    /// Lazily loaded directory listings (loaded the first time a folder is expanded).
    children: HashMap<PathBuf, Vec<DirEntry>>,
    editors: Editors,
    editor_view: EditorView,
    /// Settings editor tab (one instance) and whether it is the active tab.
    settings: Option<Entity<SettingsView>>,
    settings_active: bool,
    /// settings.json the Settings editor edits (overridable so tests never touch the real one).
    settings_path: PathBuf,
    /// Root focus handle so workbench-level actions (Cmd+,) dispatch even before anything is focused.
    focus: Option<FocusHandle>,
    branch: Option<String>,
    error: Option<String>,
}

impl Workbench {
    fn new(root: PathBuf) -> Self {
        let mut wb = Self {
            theme: Theme::dark_modern(),
            root: root.clone(),
            view: ActivityView::Explorer,
            sidebar_visible: true,
            sidebar_width: m::SIDEBAR_WIDTH,
            resizing_sidebar: false,
            expanded: HashSet::new(),
            children: HashMap::new(),
            editors: Editors::default(),
            editor_view: EditorView::default(),
            settings: None,
            settings_active: false,
            settings_path: settings_path(),
            focus: None,
            branch: git_branch(&root),
            error: None,
        };
        wb.children.insert(root.clone(), list_dir(&root).unwrap_or_default());
        // Open a README if there is one, so the first launch shows real text.
        for name in ["README.md", "Cargo.toml"] {
            let p = root.join(name);
            if p.is_file() {
                wb.open(&p);
                break;
            }
        }
        wb
    }

    fn open(&mut self, path: &Path) {
        self.settings_active = false;
        if let Err(e) = self.editors.open_file(path) {
            self.error = Some(format!("{}: {e}", path.display()));
        }
    }

    // ---- Settings editor tab (grouped: open / close / tab / surface) ---------

    fn open_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.is_none() {
            let this_path = self.settings_path.clone();
            let view = cx.new(|cx| SettingsView::new(this_path, window, cx));
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
        if let Some(view) = &self.settings {
            view.update(cx, |v, cx| v.focus_search(window, cx));
        }
        cx.notify();
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
                .bg(to_gpui(if active { t.tab_active_bg } else { t.chrome_bg }))
                .child(if active { line(div().top_0(), t.accent) } else { line(div().bottom_0(), t.border) })
                .child(div().absolute().right_0().top_0().h_full().w(px(1.)).bg(to_gpui(t.border)))
                .child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(icons::SETTINGS_GEAR)))
                .child("Settings")
                .child(close)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _, cx| {
                        this.settings_active = true;
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

    fn close_tab(&mut self, index: usize) {
        if let Some(e) = self.editors.open.get(index) {
            let path = e.path.clone();
            self.editors.close(index);
            self.editor_view.forget(&path);
        }
    }

    fn toggle_dir(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
            self.children
                .entry(path.to_path_buf())
                .or_insert_with(|| list_dir(path).unwrap_or_default());
        }
    }

    /// Visible tree rows, depth-first, honouring `expanded`.
    fn flatten(&self, dir: &Path, depth: usize, out: &mut Vec<(DirEntry, usize)>) {
        let Some(entries) = self.children.get(dir) else { return };
        for e in entries {
            out.push((e.clone(), depth));
            if e.is_dir && self.expanded.contains(&e.path) {
                self.flatten(&e.path, depth + 1, out);
            }
        }
    }

    // ---- parts --------------------------------------------------------------

    fn title_bar(&self) -> impl IntoElement {
        let t = &self.theme;
        let title = match self.editors.active_editor() {
            _ if self.settings_active => format!("Settings — {}", folder_name(&self.root)),
            Some(e) => format!("{} — {}", e.title, folder_name(&self.root)),
            None => folder_name(&self.root),
        };
        div()
            .flex_none()
            .h(px(m::TITLE_BAR_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(to_gpui(t.chrome_bg))
            .border_b_1()
            .border_color(to_gpui(t.border))
            .text_size(px(13.))
            .text_color(to_gpui(t.sidebar_fg))
            .window_control_area(WindowControlArea::Drag)
            .child(title)
    }

    fn status_bar(&self) -> impl IntoElement {
        let t = &self.theme;
        let fg = to_gpui(t.sidebar_fg);
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
        left = left.child(
            item()
                .child(icon(icons::ERROR))
                .child("0")
                .child(div().w(px(6.)))
                .child(icon(icons::WARNING))
                .child("0"),
        );
        if let Some(err) = &self.error {
            left = left.child(item().text_color(to_gpui(t.error_fg)).child(err.clone()));
        }

        let mut right = div().flex().items_center().h_full();
        if let Some(e) = self.editors.active_editor() {
            right = right
                .child(item().child(format!("{} lines", e.buffer.line_count())))
                .child(item().child("UTF-8"))
                .child(item().child(e.eol_label()))
                .child(item().child(language_of(&e.path)));
        }
        right = right.child(
            item()
                .id("status-bell")
                .child(icon(icons::BELL))
                .tooltip(|w, cx| Tooltip::new("No Notifications").build(w, cx)),
        );

        div()
            .flex_none()
            .h(px(m::STATUS_BAR_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(5.))
            .bg(to_gpui(t.chrome_bg))
            .border_t_1()
            .border_color(to_gpui(t.border))
            .text_size(px(12.))
            .text_color(fg)
            .child(left)
            .child(right)
    }

    fn activity_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let cell = |id: &'static str, ix: usize, glyph: char, tip: &'static str, active: bool| {
            let fg = if active { t.activity_fg } else { t.activity_inactive_fg };
            div()
                .id((id, ix))
                .relative()
                .flex_none()
                .w(px(m::ACTIVITY_BAR_WIDTH))
                .h(px(m::ACTIVITY_BAR_WIDTH))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .font_family(icons::FAMILY)
                .text_size(px(24.))
                .text_color(to_gpui(fg))
                .hover(|s| s.text_color(to_gpui(t.activity_fg)))
                .when(active, |d| {
                    d.child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .w(px(2.))
                            .h_full()
                            .bg(to_gpui(t.accent)),
                    )
                })
                .child(icons::glyph(glyph))
                .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
        };

        let mut top = div().flex().flex_col();
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
                move |this, _, _, cx| {
                    // Clicking the active view toggles the side bar, like VS Code.
                    if this.sidebar_visible && this.view == view {
                        this.sidebar_visible = false;
                    } else {
                        this.view = view;
                        this.sidebar_visible = true;
                    }
                    cx.notify();
                },
            )));
        }

        // Global activities pinned to the bottom.
        let bottom = div()
            .flex()
            .flex_col()
            .child(cell("global", 0, icons::ACCOUNT, "Accounts", false))
            .child(
                cell("global", 1, icons::SETTINGS_GEAR, "Manage (Settings)", false).on_click(cx.listener(
                    |this, _, window, cx| this.open_settings(window, cx),
                )),
            );

        div()
            .flex_none()
            .w(px(m::ACTIVITY_BAR_WIDTH))
            .h_full()
            .flex()
            .flex_col()
            .justify_between()
            .bg(to_gpui(t.chrome_bg))
            .border_r_1()
            .border_color(to_gpui(t.border))
            .child(top)
            .child(bottom)
    }

    fn side_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let heading = match self.view {
            ActivityView::Explorer => "EXPLORER",
            ActivityView::Search => "SEARCH",
            ActivityView::Scm => "SOURCE CONTROL",
            ActivityView::Extensions => "EXTENSIONS",
        };
        let body = match self.view {
            ActivityView::Explorer => self.explorer(cx).into_any_element(),
            _ => div()
                .px(px(20.))
                .text_size(px(12.))
                .text_color(to_gpui(t.tab_inactive_fg))
                .child("Not implemented yet.")
                .into_any_element(),
        };
        div()
            .flex_none()
            .w(px(self.sidebar_width))
            .h_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.chrome_bg))
            .text_color(to_gpui(t.sidebar_fg))
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
    }

    fn explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let mut rows = Vec::new();
        self.flatten(&self.root, 0, &mut rows);
        let active = self.editors.active_editor().map(|e| e.path.clone());

        let chevron = |c: char| {
            div()
                .flex_none()
                .w(px(16.))
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .child(icons::glyph(c))
        };

        let header = div()
            .flex_none()
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .child(chevron(icons::CHEVRON_DOWN))
            .child(folder_name(&self.root).to_uppercase());

        let list = rows.into_iter().enumerate().map(|(i, (entry, depth))| {
            let is_active = active.as_deref() == Some(entry.path.as_path());
            let open = self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            div()
                .id(("tree", i))
                .flex_none()
                .h(px(22.))
                .w_full()
                .flex()
                .items_center()
                .pl(px(8. + depth as f32 * 8.))
                .text_size(px(13.))
                .cursor_pointer()
                .when(is_active, |d| d.bg(to_gpui(t.list_active_bg)))
                .when(!is_active, |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
                .child(match (is_dir, open) {
                    (true, true) => chevron(icons::CHEVRON_DOWN),
                    (true, false) => chevron(icons::CHEVRON_RIGHT),
                    _ => div().flex_none().w(px(16.)),
                })
                .child(div().whitespace_nowrap().overflow_hidden().child(entry.name))
                .on_click(cx.listener(move |this, _, _, cx| {
                    if is_dir {
                        this.toggle_dir(&path);
                    } else {
                        this.open(&path);
                    }
                    cx.notify();
                }))
        });

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("tree-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(list),
            )
    }

    fn tab_strip(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let mut strip = div()
            .relative()
            .flex_none()
            .h(px(m::TAB_HEIGHT))
            .w_full()
            .flex()
            .flex_row()
            .overflow_hidden()
            .bg(to_gpui(t.chrome_bg))
            // Bottom border across the whole strip; the active tab paints over it.
            .child(
                div()
                    .absolute()
                    .left_0()
                    .bottom_0()
                    .w_full()
                    .h(px(1.))
                    .bg(to_gpui(t.border)),
            );
        for (i, e) in self.editors.open.iter().enumerate() {
            let active = self.editors.active == Some(i) && !self.settings_active;
            let fg = to_gpui(if active { t.tab_active_fg } else { t.tab_inactive_fg });
            let line = |d: Div, c| d.absolute().left_0().w_full().h(px(1.)).bg(to_gpui(c));
            let close = div()
                .id(("tab-close", i))
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
                .group_hover(format!("tab-{i}"), |s| s.text_color(fg))
                .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
                .child(icons::glyph(icons::CLOSE))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(i);
                        cx.notify();
                    }),
                );
            let tab = div()
                .id(("tab", i))
                .group(format!("tab-{i}"))
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
                .bg(to_gpui(if active { t.tab_active_bg } else { t.chrome_bg }))
                .child(if active {
                    line(div().top_0(), t.accent)
                } else {
                    line(div().bottom_0(), t.border)
                })
                .child(
                    div()
                        .absolute()
                        .right_0()
                        .top_0()
                        .h_full()
                        .w(px(1.))
                        .bg(to_gpui(t.border)),
                )
                .child(e.title.clone())
                .child(close)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        this.editors.active = Some(i);
                        this.settings_active = false;
                        cx.notify();
                    }),
                )
                .on_mouse_down(
                    MouseButton::Middle,
                    cx.listener(move |this, _, _, cx| {
                        this.close_tab(i);
                        cx.notify();
                    }),
                );
            strip = strip.child(tab);
        }
        strip = strip.children(self.settings_tab(cx));
        strip
    }

    fn editor_group(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme.clone();
        let settings_surface = self.settings.clone().filter(|_| self.settings_active);
        let surface = match self.editors.active.and_then(|i| self.editors.open.get(i)) {
            _ if settings_surface.is_some() => settings_surface.unwrap().into_any_element(),
            Some(editor) => self
                .editor_view
                .element(editor, &t, cx, |wb: &Workbench| wb.editors.active_editor())
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(16.))
                .text_color(to_gpui(t.tab_inactive_fg))
                .child("rscode — open a file from the Explorer")
                .into_any_element(),
        };
        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.editor_bg))
            .child(self.tab_strip(cx))
            .child(div().flex_1().min_h_0().overflow_hidden().child(surface))
    }
}

impl Render for Workbench {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = match &self.focus {
            Some(f) => f.clone(),
            None => {
                let f = cx.focus_handle();
                f.focus(window, cx);
                self.focus = Some(f.clone());
                f
            }
        };
        let t = self.theme.clone();
        let sidebar = self.sidebar_visible;

        let mut middle = div()
            .flex_1()
            .min_h_0()
            .w_full()
            .flex()
            .flex_row()
            .child(self.activity_bar(cx));
        if sidebar {
            middle = middle
                .child(self.side_bar(cx))
                // Drag handle on the side bar's right edge (also its border).
                .child(
                    div()
                        .id("sidebar-resize")
                        .flex_none()
                        .w(px(1.))
                        .h_full()
                        .bg(to_gpui(t.border))
                        .relative()
                        .child(
                            div()
                                .absolute()
                                .top_0()
                                .left(px(-2.))
                                .w(px(5.))
                                .h_full()
                                .cursor_col_resize()
                                .hover(|s| s.bg(to_gpui(t.accent)))
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(|this, _, _, cx| {
                                        this.resizing_sidebar = true;
                                        cx.notify();
                                    }),
                                ),
                        ),
                );
        }
        middle = middle.child(self.editor_group(cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(to_gpui(t.editor_bg))
            .text_color(to_gpui(t.sidebar_fg))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .key_context("Workbench")
            .track_focus(&focus)
            .on_action(cx.listener(|this, _: &OpenSettings, window, cx| this.open_settings(window, cx)))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if this.resizing_sidebar {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        let x = f32::from(ev.position.x) - m::ACTIVITY_BAR_WIDTH;
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
            .child(self.title_bar())
            .child(middle)
            .child(self.status_bar())
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

fn settings_path() -> PathBuf {
    vsr_settings::default_user_settings_path()
}

fn folder_name(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

fn language_of(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "rs" => "Rust",
        "md" => "Markdown",
        "toml" => "TOML",
        "ts" => "TypeScript",
        "js" => "JavaScript",
        "json" => "JSON",
        "py" => "Python",
        _ => "Plain Text",
    }
}
