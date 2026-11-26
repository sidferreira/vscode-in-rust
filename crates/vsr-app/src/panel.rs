//! The panel: the container under (or beside) the editor area that holds Problems, Output and the
//! Terminal (ADR-0111). Toggled with Cmd+J, resized by its sash, with a tab per view.
//!
//! upstream: src/vs/workbench/browser/parts/panel/panelPart.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/parts/panel/panelActions.ts @ 50e1350fc78d
//!
//! The layout (`Workbench::editor_area`) puts the editor group and the panel in one box: a column
//! for the `bottom` and `top` locations, a row for `left` and `right`. A maximized panel takes the
//! whole box and the editor group is not drawn.

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde::Deserialize;
use vsr_settings::Configuration;

use crate::commands::{Handler, Scope};
use crate::config::AppConfig;
use crate::config::settings::LayoutSettings;
use crate::icons;
use crate::ui::to_gpui;
use crate::Workbench;

actions!(
    rscode,
    [TogglePanel, ClosePanel, FocusPanel, ToggleMaximizedPanel, NextPanelView, PreviousPanelView, ShowProblems, ShowOutput]
);

/// Upstream's `minimumHeight` of the panel part.
pub const MIN_SIZE: f32 = 77.0;
/// Room that stays for the editor group when the panel is dragged larger.
const MIN_EDITOR_ROOM: f32 = 200.0;
/// The size of a panel nobody has resized (upstream opens at 300).
pub const DEFAULT_SIZE: f32 = 300.0;
/// Height of the panel's tab bar (upstream `35`).
pub const TITLE_HEIGHT: f32 = 35.0;

/// Binds the panel's keys (Cmd+J, and the terminal's Ctrl+` in `terminal_panel`).
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-j", TogglePanel, None),
        KeyBinding::new("cmd-shift-m", ShowProblems, None),
        KeyBinding::new("cmd-shift-u", ShowOutput, None),
    ]);
    crate::terminal_view::bind_keys(cx);
}

/// A view the panel hosts. The order is the order of the tabs (upstream: Problems, Output, Debug
/// Console, Terminal, Ports; the last two are not built yet).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelView {
    Problems,
    Output,
    Terminal,
}

impl PanelView {
    pub const ALL: [PanelView; 3] = [PanelView::Problems, PanelView::Output, PanelView::Terminal];

    pub fn title(self) -> &'static str {
        match self {
            PanelView::Problems => "Problems",
            PanelView::Output => "Output",
            PanelView::Terminal => "Terminal",
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            PanelView::Problems => "problems",
            PanelView::Output => "output",
            PanelView::Terminal => "terminal",
        }
    }

    fn icon(self) -> char {
        match self {
            PanelView::Problems => icons::WARNING,
            PanelView::Output => icons::OUTPUT,
            PanelView::Terminal => icons::TERMINAL,
        }
    }
}

/// `workbench.panel.defaultLocation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PanelLocation {
    Left,
    #[default]
    Bottom,
    Top,
    Right,
}

impl PanelLocation {
    pub fn is_horizontal(self) -> bool {
        matches!(self, PanelLocation::Bottom | PanelLocation::Top)
    }
}

/// `workbench.panel.opensMaximized`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpensMaximized {
    Always,
    Never,
    #[default]
    Preserve,
}

/// The `workbench.panel.*` settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PanelSettings {
    /// `showLabels`: the tabs show a label (true) or only an icon.
    pub show_labels: bool,
    pub location: PanelLocation,
    pub opens_maximized: OpensMaximized,
}

impl PanelSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            show_labels: c.get_bool("workbench.panel.showLabels").unwrap_or(true),
            location: c.get_enum("workbench.panel.defaultLocation").unwrap_or_default(),
            opens_maximized: c.get_enum("workbench.panel.opensMaximized").unwrap_or_default(),
        }
    }
}

/// A sash drag in progress: where the mouse went down (along the panel's axis) and the size then.
#[derive(Debug, Clone, Copy)]
struct Drag {
    start: f32,
    start_size: f32,
}

pub struct PanelState {
    pub visible: bool,
    pub maximized: bool,
    /// Height for `bottom` and `top`, width for `left` and `right`.
    pub size: f32,
    pub active: PanelView,
    drag: Option<Drag>,
    /// The terminals (ADR-0045) and the one shown.
    pub terminals: Vec<crate::terminal_view::TerminalInstance>,
    pub active_terminal: usize,
}

impl Default for PanelState {
    fn default() -> Self {
        Self { visible: false, maximized: false, size: DEFAULT_SIZE, active: PanelView::Problems, drag: None, terminals: Vec::new(), active_terminal: 0 }
    }
}

impl PanelState {
    /// Shows the panel, applying `workbench.panel.opensMaximized` when it was hidden.
    pub fn show(&mut self, opens_maximized: OpensMaximized) {
        if !self.visible {
            self.visible = true;
            match opens_maximized {
                OpensMaximized::Always => self.maximized = true,
                OpensMaximized::Never => self.maximized = false,
                OpensMaximized::Preserve => {}
            }
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.drag = None;
    }

    /// The size a drag from `start` (with `start_size`) to `now` gives, within `[MIN_SIZE, available - room]`.
    /// `grows_toward_start`: dragging toward the origin of the axis makes the panel larger (a panel
    /// at the bottom or the right).
    pub fn dragged_size(start_size: f32, start: f32, now: f32, grows_toward_start: bool, available: f32) -> f32 {
        let delta = now - start;
        let size = if grows_toward_start { start_size - delta } else { start_size + delta };
        size.clamp(MIN_SIZE, (available - MIN_EDITOR_ROOM).max(MIN_SIZE))
    }
}

/// The panel's commands (ids and titles of upstream's panelActions.ts and the markers and output contributions).
pub fn register_commands(r: &mut vsr_commands::CommandsRegistry<Handler>) {
    use vsr_commands::CommandDescriptor as D;
    let wb = |a: Box<dyn Action>| Handler::Action { action: a, scope: Scope::Workbench };
    r.register(D::new("workbench.action.togglePanel", "Toggle Panel Visibility").category("View").f1(), wb(Box::new(TogglePanel)));
    r.register(D::new("workbench.action.closePanel", "Hide Panel").category("View").f1(), wb(Box::new(ClosePanel)));
    r.register(D::new("workbench.action.focusPanel", "Focus into Panel").category("View").f1(), wb(Box::new(FocusPanel)));
    r.register(D::new("workbench.action.toggleMaximizedPanel", "Toggle Maximized Panel").category("View").f1(), wb(Box::new(ToggleMaximizedPanel)));
    r.register(D::new("workbench.action.nextPanelView", "Next Panel View").category("View").f1(), wb(Box::new(NextPanelView)));
    r.register(D::new("workbench.action.previousPanelView", "Previous Panel View").category("View").f1(), wb(Box::new(PreviousPanelView)));
    r.register(D::new("workbench.actions.view.problems", "Focus on Problems View").category("View").f1(), wb(Box::new(ShowProblems)));
    r.register(D::new("workbench.action.output.toggleOutput", "Toggle Output").category("View").f1(), wb(Box::new(ShowOutput)));
    crate::terminal_view::register_commands(r);
}

impl Workbench {
    /// Attach the panel's action handlers to the workbench root.
    pub(crate) fn bind_panel_actions(el: Div, cx: &mut Context<Self>) -> Div {
        el.on_action(cx.listener(|this, _: &TogglePanel, _, cx| this.toggle_panel(cx)))
            .on_action(cx.listener(|this, _: &ClosePanel, _, cx| {
                this.panel.hide();
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FocusPanel, window, cx| {
                let view = this.panel.active;
                this.show_panel_view(view, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleMaximizedPanel, _, cx| this.toggle_maximized_panel(cx)))
            .on_action(cx.listener(|this, _: &NextPanelView, _, cx| this.step_panel_view(1, cx)))
            .on_action(cx.listener(|this, _: &PreviousPanelView, _, cx| this.step_panel_view(-1, cx)))
            .on_action(cx.listener(|this, _: &ShowProblems, window, cx| this.show_panel_view(PanelView::Problems, window, cx)))
            .map(|el| crate::terminal_view::bind_actions(el, cx))
            .on_action(cx.listener(|this, _: &ShowOutput, window, cx| this.show_panel_view(PanelView::Output, window, cx)))
    }

    pub(crate) fn panel_settings(&self, cx: &App) -> PanelSettings {
        PanelSettings::from_configuration(AppConfig::get(cx).configuration())
    }

    /// Runs every frame: when the panel was hidden with the keyboard focus inside a terminal, the focus
    /// goes back to the workbench (a hidden view cannot keep it, and the workbench keys need it).
    pub(crate) fn panel_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.panel.visible
            && let Some(root) = self.focus.clone()
            && self.panel.terminals.iter().any(|t| t.view.read(cx).is_focused(window))
        {
            root.focus(window, cx);
        }
    }

    /// Cmd+J, `workbench.action.togglePanel`.
    pub(crate) fn toggle_panel(&mut self, cx: &mut Context<Self>) {
        if self.panel.visible {
            self.panel.hide();
        } else {
            let opens = self.panel_settings(cx).opens_maximized;
            self.panel.show(opens);
        }
        cx.notify();
    }

    /// Shows the panel with `view` active.
    pub(crate) fn show_panel_view(&mut self, view: PanelView, window: &mut Window, cx: &mut Context<Self>) {
        let opens = self.panel_settings(cx).opens_maximized;
        self.panel.show(opens);
        self.panel.active = view;
        if view == PanelView::Terminal {
            if self.panel.terminals.is_empty() {
                self.new_terminal(window, cx);
            } else if let Some(t) = self.panel.terminals.get(self.panel.active_terminal) {
                t.view.update(cx, |v, cx| v.focus(window, cx));
            }
        }
        cx.notify();
    }

    pub(crate) fn toggle_maximized_panel(&mut self, cx: &mut Context<Self>) {
        let opens = self.panel_settings(cx).opens_maximized;
        self.panel.show(opens);
        self.panel.maximized = !self.panel.maximized;
        cx.notify();
    }

    /// Next (`step` = 1) or previous (-1) tab, wrapping.
    pub(crate) fn step_panel_view(&mut self, step: isize, cx: &mut Context<Self>) {
        if !self.panel.visible {
            return;
        }
        let n = PanelView::ALL.len() as isize;
        let i = PanelView::ALL.iter().position(|v| *v == self.panel.active).unwrap_or(0) as isize;
        self.panel.active = PanelView::ALL[(i + step).rem_euclid(n) as usize];
        cx.notify();
    }

    /// Mouse move while the sash is dragged. Returns whether a drag is in progress.
    pub(crate) fn panel_drag_move(&mut self, ev: &MouseMoveEvent, window: &Window, cx: &mut Context<Self>) -> bool {
        let Some(drag) = self.panel.drag else { return false };
        if ev.pressed_button != Some(MouseButton::Left) {
            self.panel.drag = None;
            return false;
        }
        let location = self.panel_settings(cx).location;
        let viewport = window.viewport_size();
        let (now, available) = if location.is_horizontal() {
            (f32::from(ev.position.y), f32::from(viewport.height))
        } else {
            (f32::from(ev.position.x), f32::from(viewport.width))
        };
        let grows_toward_start = matches!(location, PanelLocation::Bottom | PanelLocation::Right);
        self.panel.size = PanelState::dragged_size(drag.start_size, drag.start, now, grows_toward_start, available);
        cx.notify();
        true
    }

    pub(crate) fn panel_drag_end(&mut self, cx: &mut Context<Self>) {
        if self.panel.drag.take().is_some() {
            cx.notify();
        }
    }

    /// The editor group and, when shown, the panel, in the box between the side bars.
    pub(crate) fn editor_area(&mut self, cx: &mut Context<Self>, layout: &LayoutSettings) -> impl IntoElement {
        let settings = self.panel_settings(cx);
        let visible = self.panel.visible;
        let maximized = visible && self.panel.maximized;
        let location = settings.location;
        let editor = (!maximized).then(|| self.editor_group(cx, layout.show_tabs).into_any_element());
        let panel = visible.then(|| self.panel_element(settings, layout.sash_size, cx).into_any_element());
        let area = div().flex_1().min_w_0().h_full().flex().when(location.is_horizontal(), |d| d.flex_col());
        match location {
            PanelLocation::Bottom | PanelLocation::Right => area.children(editor).children(panel),
            PanelLocation::Top | PanelLocation::Left => area.children(panel).children(editor),
        }
    }

    fn panel_element(&mut self, settings: PanelSettings, sash_size: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme.clone();
        let location = settings.location;
        let maximized = self.panel.maximized;
        let size = self.panel.size;
        let body = self.panel_body(cx);
        let view_actions = self.panel_view_actions(cx);
        let tabs = PanelView::ALL.iter().map(|view| {
            let view = *view;
            let active = view == self.panel.active;
            let fg = to_gpui(if active { t.panel_title_active_fg } else { t.panel_title_inactive_fg });
            let tab = div()
                .id(SharedString::from(format!("panel-tab-{}", view.id())))
                .debug_selector(move || format!("panel-tab:{}", view.id()))
                .h_full()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(10.))
                .cursor_pointer()
                .text_color(fg)
                .text_size(px(11.))
                .border_b_1()
                .border_color(if active { to_gpui(t.panel_title_active_border) } else { gpui_kit::transparent_black() })
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.show_panel_view(view, window, cx)));
            if settings.show_labels {
                tab.child(view.title().to_uppercase())
            } else {
                tab.child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(view.icon())))
                    .tooltip(move |w, cx| gpui_kit::component::tooltip::Tooltip::new(view.title()).build(w, cx))
            }
        }).collect::<Vec<_>>();
        let action = |id: &'static str, glyph: char, cx: &mut Context<Self>, f: fn(&mut Workbench, &mut Context<Workbench>)| {
            div()
                .id(id)
                .debug_selector(move || id.to_string())
                .flex()
                .items_center()
                .justify_center()
                .size(px(22.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .text_color(to_gpui(t.panel_title_active_fg))
                .child(icons::glyph(glyph))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| f(this, cx)))
        };
        let title = div()
            .flex_none()
            .h(px(TITLE_HEIGHT))
            .w_full()
            .flex()
            .items_center()
            .justify_between()
            .px(px(8.))
            .debug_selector(|| "panel-title".to_string())
            .child(div().h_full().flex().items_center().children(tabs))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.))
                    .children(view_actions)
                    .child(action(
                        "panel-maximize",
                        if maximized { icons::SCREEN_NORMAL } else { icons::SCREEN_FULL },
                        cx,
                        |wb, cx| wb.toggle_maximized_panel(cx),
                    ))
                    .child(action("panel-close", icons::CLOSE, cx, |wb, cx| wb.toggle_panel(cx))),
            );

        let sash = div()
            .id("panel-sash")
            .absolute()
            .debug_selector(|| "panel-sash".to_string())
            .map(|d| match location {
                PanelLocation::Bottom => d.top(px(-sash_size / 2.)).left_0().w_full().h(px(sash_size)).cursor_row_resize(),
                PanelLocation::Top => d.bottom(px(-sash_size / 2.)).left_0().w_full().h(px(sash_size)).cursor_row_resize(),
                PanelLocation::Left => d.right(px(-sash_size / 2.)).top_0().h_full().w(px(sash_size)).cursor_col_resize(),
                PanelLocation::Right => d.left(px(-sash_size / 2.)).top_0().h_full().w(px(sash_size)).cursor_col_resize(),
            })
            .hover(|s| s.bg(to_gpui(t.sash_hover_border)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                    let start = if location.is_horizontal() { f32::from(ev.position.y) } else { f32::from(ev.position.x) };
                    this.panel.drag = Some(Drag { start, start_size: this.panel.size });
                    cx.notify();
                }),
            );

        let root = div().relative().flex().flex_col().bg(to_gpui(t.panel_bg)).debug_selector(|| "panel".to_string());
        let root = if maximized {
            root.flex_1().min_w_0().min_h_0()
        } else if location.is_horizontal() {
            root.flex_none().w_full().h(px(size))
        } else {
            root.flex_none().h_full().w(px(size))
        };
        let border = to_gpui(t.panel_border);
        let root = match location {
            PanelLocation::Bottom => root.border_t_1(),
            PanelLocation::Top => root.border_b_1(),
            PanelLocation::Left => root.border_r_1(),
            PanelLocation::Right => root.border_l_1(),
        };
        root.border_color(border)
            .child(title)
            .child(div().flex_1().min_h_0().overflow_hidden().child(body))
            .when(!maximized, |d| d.child(sash))
    }

    /// Actions the active view adds to the title bar (the terminal's new and kill buttons).
    fn panel_view_actions(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        if self.panel.active != PanelView::Terminal {
            return Vec::new();
        }
        vec![
            self.panel_button("terminal-new", icons::ADD, cx, |wb, window, cx| wb.new_terminal(window, cx)).into_any_element(),
            self.panel_button("terminal-kill", icons::TRASH, cx, |wb, _, cx| wb.kill_terminal(cx)).into_any_element(),
        ]
    }

    /// An icon button of the panel's title bar.
    fn panel_button(
        &self,
        id: &'static str,
        glyph: char,
        cx: &mut Context<Self>,
        f: fn(&mut Workbench, &mut Window, &mut Context<Workbench>),
    ) -> Stateful<Div> {
        let t = &self.theme;
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex()
            .items_center()
            .justify_center()
            .size(px(22.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(to_gpui(t.panel_title_active_fg))
            .child(icons::glyph(glyph))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| f(this, window, cx)))
    }

    fn panel_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let message = |text: &'static str| {
            div()
                .size_full()
                .px(px(20.))
                .py(px(8.))
                .text_size(px(13.))
                .text_color(to_gpui(t.description_fg))
                .child(text)
                .into_any_element()
        };
        match self.panel.active {
            PanelView::Problems => message("No problems have been detected in the workspace."),
            PanelView::Output => message(""),
            PanelView::Terminal => self.terminal_body(cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MIN_EDITOR_ROOM, MIN_SIZE, OpensMaximized, PanelState};

    #[test]
    fn dragging_a_bottom_panel_up_makes_it_larger() {
        // Mouse went down at y=500 on a 300 tall panel and moves up to y=400: 400 tall.
        assert_eq!(PanelState::dragged_size(300., 500., 400., true, 1000.), 400.);
        assert_eq!(PanelState::dragged_size(300., 500., 600., true, 1000.), 200.);
    }

    #[test]
    fn dragging_a_top_panel_down_makes_it_larger() {
        assert_eq!(PanelState::dragged_size(300., 300., 350., false, 1000.), 350.);
    }

    #[test]
    fn the_size_is_clamped() {
        assert_eq!(PanelState::dragged_size(300., 500., 900., true, 1000.), MIN_SIZE);
        assert_eq!(PanelState::dragged_size(300., 500., 0., true, 1000.), 1000. - MIN_EDITOR_ROOM);
    }

    #[test]
    fn opens_maximized_applies_only_when_the_panel_was_hidden() {
        let mut p = PanelState::default();
        p.show(OpensMaximized::Always);
        assert!(p.visible && p.maximized);
        p.maximized = false;
        p.show(OpensMaximized::Always); // already visible: unchanged
        assert!(!p.maximized);
        p.hide();
        p.show(OpensMaximized::Preserve);
        assert!(!p.maximized);
        p.maximized = true;
        p.hide();
        p.show(OpensMaximized::Preserve);
        assert!(p.maximized);
        p.hide();
        p.show(OpensMaximized::Never);
        assert!(!p.maximized);
    }
}
