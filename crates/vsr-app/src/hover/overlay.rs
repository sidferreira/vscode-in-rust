//! The hover widget on screen (ADR-0109): the lines of the answer, above or below the word.
//!
//! Upstream source: src/vs/editor/contrib/hover/browser/contentHoverWidget.ts

use gpui_kit::{AnyElement, Context, InteractiveElement, IntoElement, MouseButton, ParentElement, StatefulInteractiveElement, Styled, div, px};

use super::HoverState;
use crate::editor_view::EditorHost;
use crate::ui::to_gpui;
use crate::theme::Theme;

/// The widget over the editor, or `None` when nothing is shown. `word` is the word's box
/// `(x, top, bottom)` in the editor surface, where the widget is anchored. Its pointer events keep
/// it open (`editor.hover.sticky`); a mouse down on it does not reach the editor (no dismissal).
pub fn overlay<T: EditorHost>(state: &HoverState, word: (f32, f32, f32), line_height: f32, theme: &Theme, cx: &mut Context<T>) -> Option<AnyElement> {
    let sh = state.shown.as_ref()?;
    let lh = line_height;
    let (x, top) = super::origin(word, sh.above, sh.lines.len(), lh);
    let mut body = div()
        .id("hover-widget")
        .absolute()
        .left(px(x))
        .top(px(top))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .max_w(px(480.0))
        .px_2()
        .py_1()
        .rounded_sm()
        .border_1()
        .bg(to_gpui(theme.editor_bg))
        .text_color(to_gpui(theme.editor_fg))
        .text_size(px(lh * 0.8))
        .on_hover(cx.listener(|this: &mut T, hovered: &bool, _, cx| {
            let view = this.editor_host().1;
            view.hover.over_widget(std::time::Instant::now(), *hovered);
            cx.notify();
        }));
    for line in &sh.lines {
        body = body.child(div().child(line.clone()));
    }
    Some(body.into_any_element())
}
