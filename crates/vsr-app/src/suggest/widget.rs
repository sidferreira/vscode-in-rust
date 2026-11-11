//! upstream: src/vs/editor/contrib/suggest/browser/suggestWidget.ts @ 50e1350fc78d
//!
//! The suggest list at the cursor (ADR-0084): rows with a kind icon, the label with the matched
//! characters highlighted, the label's detail on the right, and the focused row.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggestWidget.ts, suggestWidgetRenderer.ts,
//! suggest.css. Not drawn: the documentation side panel, the status bar, the details expansion,
//! `editorSuggestWidget.*` colours (the widget uses the colours those keys default to).

use std::ops::Range;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Styled, div, px,
};

use crate::editor_view::EditorHost;
use crate::icons;
use crate::theme::Theme;
use crate::ui::to_gpui;

/// Height of one row (upstream's widget rows are 22px at the default font).
pub const ROW_HEIGHT: f32 = 22.0;
/// The widget's width (upstream's default).
pub const WIDTH: f32 = 430.0;
/// Room for the kind icon and its margin before the label.
pub const ICON_ROOM: f32 = 26.0;

/// One visible row.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// The index in the filtered list.
    pub index: usize,
    pub icon: char,
    pub label: String,
    /// Matched UTF-16 ranges of `label`.
    pub matches: Vec<Range<usize>>,
    /// Shown dimmed after the label (the label's detail, and the description).
    pub detail: String,
    pub focused: bool,
}

/// What the widget draws, built by the workbench each frame.
#[derive(Clone, Debug, PartialEq)]
pub struct WidgetData {
    pub rows: Vec<Row>,
    /// How many items the list has.
    pub total: usize,
    /// The item index at the top.
    pub top: usize,
    /// The 1-based column on the cursor line the list lines its labels up with.
    pub anchor_column: usize,
    /// The detail of the focused item, once known (the provider's `detail`).
    pub focused_detail: Option<String>,
    /// `editor.suggest.showIcons`.
    pub show_icons: bool,
}

/// Splits `text` into runs, each flagged as matched or not. `matches` are UTF-16 ranges.
pub fn highlight_runs(text: &str, matches: &[Range<usize>]) -> Vec<(String, bool)> {
    let mut runs: Vec<(String, bool)> = Vec::new();
    let mut unit = 0usize;
    for c in text.chars() {
        let hit = matches.iter().any(|m| m.start <= unit && unit < m.end);
        match runs.last_mut() {
            Some((s, h)) if *h == hit => s.push(c),
            _ => runs.push((c.to_string(), hit)),
        }
        unit += c.len_utf16();
    }
    runs
}

/// The widget at `(left, top)` within the editor surface.
pub fn element<T: EditorHost>(
    data: Rc<WidgetData>,
    theme: &Theme,
    font_family: &str,
    font_size: f32,
    left: f32,
    top: f32,
    cx: &mut Context<T>,
) -> AnyElement {
    let fg = to_gpui(theme.editor_fg);
    let dim = to_gpui(theme.disabled_fg);
    let rows = data.rows.iter().map(|row| {
        let index = row.index;
        let (text_color, highlight) = if row.focused {
            (to_gpui(theme.quick_input_list_focus_fg), to_gpui(theme.list_focus_highlight_fg))
        } else {
            (fg, to_gpui(theme.list_highlight_fg))
        };
        let label = highlight_runs(&row.label, &row.matches).into_iter().map(move |(text, hit)| {
            div().whitespace_nowrap().text_color(if hit { highlight } else { text_color }).child(text)
        });
        let mut detail = row.detail.clone();
        if row.focused
            && let Some(extra) = data.focused_detail.as_ref().filter(|d| !d.is_empty())
        {
            if !detail.is_empty() {
                detail.push(' ');
            }
            detail.push_str(extra);
        }
        div()
            .id(("suggest-row", index))
            .flex()
            .flex_row()
            .items_center()
            .h(px(ROW_HEIGHT))
            .px(px(4.))
            .gap(px(6.))
            .overflow_hidden()
            .text_color(text_color)
            .when_focused(row.focused, to_gpui(theme.quick_input_list_focus_bg))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut T, _: &MouseDownEvent, window, cx| {
                    this.suggest_accept_row(index, window, cx);
                    cx.stop_propagation();
                }),
            )
            .children(data.show_icons.then(|| {
                div().w(px(16.)).flex_none().font_family(icons::FAMILY).text_color(text_color).child(icons::glyph(row.icon))
            }))
            .child(div().flex().flex_row().flex_none().children(label))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().text_color(dim).child(detail))
    });
    div()
        .id("suggest-widget")
        .absolute()
        .left(px(left))
        .top(px(top))
        .w(px(WIDTH))
        .flex()
        .flex_col()
        .font_family(font_family.to_string())
        .text_size(px(font_size))
        .bg(to_gpui(theme.editor_widget_bg))
        .border_1()
        .border_color(to_gpui(theme.widget_border))
        .rounded(px(3.))
        .shadow_md()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .children(rows)
        .into_any_element()
}

/// Paints the focused row's background only when it is focused.
trait WhenFocused: Styled + Sized {
    fn when_focused(self, focused: bool, bg: gpui_kit::Hsla) -> Self {
        if focused { self.bg(bg) } else { self }
    }
}

impl<T: Styled + Sized> WhenFocused for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_split_the_label_into_runs() {
        assert_eq!(
            highlight_runs("fooBar", &[0..1, 3..4]),
            vec![("f".into(), true), ("oo".into(), false), ("B".into(), true), ("ar".into(), false)]
        );
        assert_eq!(highlight_runs("ab", &[]), vec![("ab".into(), false)]);
        assert!(highlight_runs("", &[0..1]).is_empty());
    }

    #[test]
    fn matches_count_utf16_units() {
        // "a😀b": the emoji is two units, `b` is at unit 3.
        let runs = highlight_runs("a\u{1F600}b", &[3..4]);
        assert_eq!(runs.last(), Some(&("b".into(), true)));
        assert_eq!(runs.first(), Some(&("a\u{1F600}".into(), false)));
    }
}
