//! The Quick Pick component: a centred-top overlay with an input box and a virtualised list of
//! picks. Quick Open (Cmd+P) uses it; the Command Palette will too.
//!
//! upstream: src/vs/platform/quickinput/browser/quickInput.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/quickInputController.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/quickInputList.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/quickInputBox.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/media/quickInput.css @ 50e1350fc78d
//!
//! The model (rows, active row, navigation, filtering) is `vsr-quickinput`; this is the view.
//! The owner supplies a [`PickSource`] (rows for the typed text) and listens to
//! [`QuickPickEvent`]: it closes the picker by dropping the entity. Escape, a click outside and
//! (when `closeOnFocusLost`) losing focus all emit `Dismiss`.
//!
//! Differences from VS Code: separator rows are as tall as items (the list is uniform), no icons,
//! buttons, details, checkboxes, title bar or progress bar, and no drag to move.

use std::ops::Range;

use gpui_kit::component::input::{Enter, Escape, Input, InputEvent, InputState, MoveDown, MovePageDown, MovePageUp, MoveUp};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use vsr_quickinput::fuzzy::{Match, match_byte_ranges};
use vsr_quickinput::pick::{PickItem, PickList, PickRow};

use crate::theme::{Theme, metrics as m};
use crate::ui::to_gpui;

/// `.quick-input-widget` width.
const WIDTH: f32 = 600.0;
/// Height of a row (`QuickInputItemDelegate`; separators are 30 upstream).
const ROW_HEIGHT: f32 = 22.0;
/// `.quick-input-list .monaco-list { max-height: calc(20 * 22px) }`.
const MAX_ROWS: usize = 20;

/// The rows for a typed text.
pub enum PickResult {
    /// Available now.
    Ready(Vec<PickRow>),
    /// Computed in the background; shown when done unless the text changed meanwhile.
    Pending(Task<Vec<PickRow>>),
}

/// Where the picker's rows come from (`IQuickPick.items` + the provider's filtering).
pub trait PickSource: 'static {
    /// Called for the initial text and after every change of the input text.
    fn rows(&mut self, query: &str, cx: &mut App) -> PickResult;
}

impl<F: FnMut(&str, &mut App) -> PickResult + 'static> PickSource for F {
    fn rows(&mut self, query: &str, cx: &mut App) -> PickResult {
        self(query, cx)
    }
}

/// What the picker asks its owner to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QuickPickEvent {
    /// An item was accepted (Enter, Cmd+Enter or a click). `to_side` is Cmd+Enter (open to the side).
    Accept { item: PickItem, query: String, to_side: bool },
    /// The picker wants to close (Escape, click outside, focus lost).
    Dismiss,
    /// The input text changed (also emitted for the rows of the initial text, which the owner
    /// misses when it subscribes after construction). Owners that fill rows themselves (Go to
    /// Line) and the ones that switch provider by prefix react to it.
    Change { query: String },
}

impl EventEmitter<QuickPickEvent> for QuickPick {}

pub struct QuickPick {
    theme: Theme,
    input: Entity<InputState>,
    list: PickList,
    source: Box<dyn PickSource>,
    scroll: UniformListScrollHandle,
    /// Bumped per refresh; a pending result applies only if it is still the latest.
    generation: u64,
    pending: Option<Task<()>>,
    closed: bool,
    _subs: Vec<Subscription>,
}

impl QuickPick {
    /// A picker showing the rows of `source` for `initial`. `select_from` is the byte offset
    /// from which the initial text is selected (so typing replaces it): `Some(0)` for all of it,
    /// the length of a Quick Access prefix to keep the prefix, `None` for no selection.
    /// `close_on_focus_lost` is `workbench.quickOpen.closeOnFocusLost`.
    pub fn new(
        placeholder: &str,
        initial: &str,
        select_from: Option<usize>,
        close_on_focus_lost: bool,
        source: impl PickSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = placeholder.to_string();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        let mut subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _w, cx| {
            if matches!(ev, InputEvent::Change) {
                this.refresh(cx);
            }
        })];
        let focus = input.read(cx).focus_handle(cx);
        if close_on_focus_lost {
            subs.push(cx.on_focus_out(&focus, window, |this, _ev, _w, cx| this.dismiss(cx)));
        }
        input.update(cx, |s, cx| {
            if !initial.is_empty() {
                s.set_value(initial, window, cx);
                if let Some(from) = select_from.filter(|&f| f < initial.len()) {
                    s.set_selected_range(from..initial.len(), cx);
                }
            }
            s.focus(window, cx);
        });
        let mut this = Self {
            theme: crate::theme_service::ThemeService::theme(cx),
            input,
            list: PickList::default(),
            source: Box::new(source),
            scroll: UniformListScrollHandle::new(),
            generation: 0,
            pending: None,
            closed: false,
            _subs: subs,
        };
        this.refresh(cx);
        this
    }

    // ---- state (also read by tests) ----------------------------------------------------------

    pub fn query(&self, cx: &App) -> String {
        self.input.read(cx).value().to_string()
    }

    #[cfg(test)]
    pub fn list(&self) -> &PickList {
        &self.list
    }

    #[cfg(test)]
    pub fn active_item(&self) -> Option<&PickItem> {
        self.list.active_item()
    }

    /// Replace the rows (for sources that deliver results later); the first item becomes active.
    pub fn set_rows(&mut self, rows: Vec<PickRow>, cx: &mut Context<Self>) {
        self.list.set_rows(rows);
        self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        cx.notify();
    }

    /// Ask the source for the rows of the current text.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.generation += 1;
        let generation = self.generation;
        let query = self.query(cx);
        cx.emit(QuickPickEvent::Change { query: query.clone() });
        match self.source.rows(&query, cx) {
            PickResult::Ready(rows) => {
                self.pending = None;
                self.set_rows(rows, cx);
            }
            PickResult::Pending(task) => {
                self.pending = Some(cx.spawn(async move |this, cx| {
                    let rows = task.await;
                    this.update(cx, |this, cx| {
                        if this.generation == generation {
                            this.set_rows(rows, cx);
                        }
                    })
                    .ok();
                }));
            }
        }
    }

    // ---- actions -----------------------------------------------------------------------------

    /// Replace the placeholder (each Quick Access provider has its own).
    pub fn set_placeholder(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = text.to_string();
        self.input.update(cx, |s, cx| s.set_placeholder(text, window, cx));
    }

    /// Replace the text (switching provider by its prefix) and refresh the rows.
    pub fn set_value(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        self.refresh(cx);
    }

    /// Focus the input, optionally selecting its text.
    pub fn focus_input(&mut self, select_all: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.input.update(cx, |s, cx| {
            s.focus(window, cx);
            if select_all {
                s.select_all(window, cx);
            }
        });
    }

    fn scroll_to_active(&mut self) {
        if let Some(a) = self.list.active() {
            self.scroll.scroll_to_item(a, ScrollStrategy::Nearest);
        }
    }

    fn visible_rows(&self) -> usize {
        self.list.len().clamp(1, MAX_ROWS)
    }

    pub fn move_next(&mut self, cx: &mut Context<Self>) {
        self.list.next();
        self.scroll_to_active();
        cx.notify();
    }

    pub fn move_previous(&mut self, cx: &mut Context<Self>) {
        self.list.previous();
        self.scroll_to_active();
        cx.notify();
    }

    pub fn page_down(&mut self, cx: &mut Context<Self>) {
        self.list.next_page(self.visible_rows());
        self.scroll_to_active();
        cx.notify();
    }

    pub fn page_up(&mut self, cx: &mut Context<Self>) {
        self.list.previous_page(self.visible_rows());
        self.scroll_to_active();
        cx.notify();
    }

    /// Accept the active item (Enter / Cmd+Enter).
    pub fn accept(&mut self, to_side: bool, cx: &mut Context<Self>) {
        let Some(item) = self.list.active_item().cloned() else { return };
        let query = self.query(cx);
        cx.emit(QuickPickEvent::Accept { item, query, to_side });
    }

    fn accept_row(&mut self, row: usize, cx: &mut Context<Self>) {
        self.list.set_active(row);
        self.accept(false, cx);
    }

    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if !self.closed {
            self.closed = true;
            cx.emit(QuickPickEvent::Dismiss);
        }
    }

    // ---- rendering ---------------------------------------------------------------------------

    fn render_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let Some(row) = self.list.rows().get(ix) else { return div().into_any_element() };
        match row {
            PickRow::Separator(label) => {
                // `.quick-input-list-separator-as-item`: a full-width label above a border.
                div()
                    .id(("qp-sep", ix))
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(to_gpui(t.quick_input_fg))
                    .when(ix > 0, |d| d.border_t_1().border_color(to_gpui(t.picker_group_border)))
                    .child(label.clone())
                    .into_any_element()
            }
            PickRow::Item(item) => {
                let focused = self.list.active() == Some(ix);
                let (fg, hl) = if focused {
                    (t.quick_input_list_focus_fg, t.list_focus_highlight_fg)
                } else {
                    (t.quick_input_fg, t.list_highlight_fg)
                };
                let mut row = div()
                    .id(("qp-item", ix))
                    .debug_selector(|| format!("quick-pick-row:{ix}"))
                    .h(px(ROW_HEIGHT))
                    .w_full()
                    .px(px(6.))
                    .rounded(px(3.))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .cursor_pointer()
                    .text_size(px(13.))
                    .text_color(to_gpui(fg))
                    .when(focused, |d| d.bg(to_gpui(t.quick_input_list_focus_bg)))
                    .when(!focused, |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.accept_row(ix, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .whitespace_nowrap()
                            .child(highlighted(&item.label, &item.label_highlights, to_gpui(hl))),
                    );
                if let Some(d) = &item.description {
                    row = row.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .ml(px(6.))
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_size(px(11.7)) // 0.9em
                            .opacity(if focused { 1. } else { 0.7 })
                            .child(highlighted(d, &item.description_highlights, to_gpui(hl))),
                    );
                }
                if !item.keybinding.is_empty() {
                    row = row.child(keybinding_label(&item.keybinding, t, focused));
                }
                row.into_any_element()
            }
        }
    }
}

/// `KeybindingLabel`: every key of every chord in its own small box, chords 6px apart. On the
/// focused row the boxes lose their background and take the widget shadow as border
/// (`quickInput.css`).
fn keybinding_label(chords: &[Vec<String>], t: &Theme, focused: bool) -> impl IntoElement {
    let (fg, bg, border, bottom) = (
        to_gpui(t.keybinding_label_fg),
        to_gpui(t.keybinding_label_bg),
        to_gpui(if focused { t.widget_shadow } else { t.keybinding_label_border }),
        to_gpui(t.keybinding_label_bottom_border),
    );
    let key = move |label: &str| {
        div()
            .debug_selector(|| "keybinding-key".to_string())
            .flex()
            .items_center()
            .justify_center()
            .min_w(px(12.))
            .px(px(5.))
            .py(px(3.))
            .mx(px(1.))
            .border_1()
            .border_color(border)
            .rounded(px(3.))
            .text_size(px(11.))
            .when(!focused, |d| d.bg(bg).text_color(fg))
            .shadow(vec![BoxShadow {
                color: if focused { border } else { bottom },
                offset: point(px(0.), px(-1.)),
                blur_radius: px(0.),
                spread_radius: px(0.),
                inset: true,
            }])
            .child(label.to_string())
    };
    let mut label = div()
        .flex_none()
        .flex()
        .items_center()
        .ml_auto()
        .pl(px(6.))
        .mr(px(2.))
        .debug_selector(|| "keybinding-label".to_string());
    for (i, chord) in chords.iter().enumerate() {
        if i > 0 {
            label = label.child(div().w(px(6.)));
        }
        label = label.child(div().flex().items_center().children(chord.iter().map(|k| key(k))));
    }
    label
}

/// `text` with the matched characters bold and in the highlight color.
fn highlighted(text: &str, matches: &[Match], color: Hsla) -> StyledText {
    let style = HighlightStyle { color: Some(color), font_weight: Some(FontWeight::BOLD), ..HighlightStyle::default() };
    let ranges: Vec<(Range<usize>, HighlightStyle)> =
        match_byte_ranges(text, matches).into_iter().map(|r| (r, style)).collect();
    StyledText::new(text.to_string()).with_highlights(ranges)
}

impl Render for QuickPick {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.theme = crate::theme_service::ThemeService::theme(cx);
        let t = self.theme.clone();
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        let rows = self.list.len();
        let list_height = rows.min(MAX_ROWS) as f32 * ROW_HEIGHT;

        let list = uniform_list(
            "quick-pick-list",
            rows,
            cx.processor(|this: &mut Self, range: Range<usize>, _w: &mut Window, cx| {
                range.map(|ix| this.render_row(ix, cx)).collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .h(px(list_height))
        .w_full();

        let input_box = div()
            .flex()
            .items_center()
            .h(px(26.))
            .w_full()
            .px(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(to_gpui(if focused { t.focus_border } else { t.input_border }))
            .bg(to_gpui(t.settings_text_input_bg))
            .child(div().flex_1().min_w_0().child(Input::new(&self.input).appearance(false).bordered(false)));

        let widget = div()
            .id("quick-input-widget")
            .debug_selector(|| "quick-input-widget".to_string())
            .key_context(KeyContext::parse("QuickInput").unwrap_or_default())
            .mt(px(m::TITLE_BAR_HEIGHT))
            .w(px(WIDTH))
            .max_w_full()
            .flex()
            .flex_col()
            .rounded(px(12.))
            .border_1()
            .border_color(to_gpui(t.widget_border))
            .bg(to_gpui(t.quick_input_bg))
            .text_color(to_gpui(t.quick_input_fg))
            .shadow(vec![BoxShadow {
                color: to_gpui(t.widget_shadow),
                offset: point(px(0.), px(2.)),
                blur_radius: px(8.),
                spread_radius: px(0.),
                inset: false,
            }])
            .overflow_hidden()
            // The Input consumes Up/Down/PageUp/PageDown in single-line mode and only propagates
            // Escape, so these are taken in the capture phase, before the input sees them.
            .capture_action(cx.listener(|this, _: &MoveDown, _, cx| {
                cx.stop_propagation();
                this.move_next(cx);
            }))
            .capture_action(cx.listener(|this, _: &MoveUp, _, cx| {
                cx.stop_propagation();
                this.move_previous(cx);
            }))
            .capture_action(cx.listener(|this, _: &MovePageDown, _, cx| {
                cx.stop_propagation();
                this.page_down(cx);
            }))
            .capture_action(cx.listener(|this, _: &MovePageUp, _, cx| {
                cx.stop_propagation();
                this.page_up(cx);
            }))
            .capture_action(cx.listener(|this, a: &Enter, _, cx| {
                cx.stop_propagation();
                this.accept(a.secondary, cx);
            }))
            .capture_action(cx.listener(|this, _: &Escape, _, cx| {
                cx.stop_propagation();
                this.dismiss(cx);
            }))
            // A click inside must not reach the backdrop.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(div().p(px(6.)).child(input_box))
            .when(rows > 0, |d| d.child(div().px(px(6.)).pb(px(7.)).child(list)));

        // The backdrop catches clicks outside the widget.
        div()
            .id("quick-input-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .flex()
            .justify_center()
            .items_start()
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _, _, cx| this.dismiss(cx)))
            .child(widget)
    }
}
