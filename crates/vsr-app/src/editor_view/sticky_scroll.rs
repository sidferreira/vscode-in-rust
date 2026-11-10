//! Sticky scroll: the headers of the regions around the first visible line stay pinned at the top
//! of the editor (ADR-0042).
//!
//! upstream: src/vs/editor/contrib/stickyScroll/browser/stickyScrollController.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/stickyScroll/browser/stickyScrollWidget.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/stickyScroll/browser/stickyScrollModelProvider.ts @ 50e1350fc78d
//!
//! Which lines are pinned is `Editor::sticky_lines` (the folding regions: there is no outline
//! provider, so `editor.stickyScroll.defaultModel` reads as the folding model for all three values).
//! This module draws them: each pinned header is a normal editor row (same gutter, syntax colours
//! and fold control), stacked over the first rows of the list. A click on one moves the caret to it
//! and scrolls it to the top. With `scrollWithEditor` the pinned rows follow the horizontal scroll.

use std::ops::Range;

use gpui_kit::{
    AnyElement, Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, ScrollStrategy, Styled,
    UniformListScrollHandle, div, px, uniform_list,
};
use vsr_editor::Position;

use super::{EditorHost, folding};
use crate::config::settings::EditorSettings;
use crate::model::OpenEditor;
use crate::theme::Theme;
use crate::ui::to_gpui;

/// The first view line the viewport shows and its horizontal scroll, from the list's scroll state.
pub fn viewport_top(scroll: &UniformListScrollHandle, line_height: f32) -> (usize, f32) {
    let o = scroll.0.borrow().base_handle.offset();
    let top = (-f32::from(o.y) / line_height.max(1.0)).floor().max(0.0) as usize;
    (top, f32::from(o.x))
}

/// The pinned headers for this frame, as model lines, outermost first.
pub fn lines(editor: &OpenEditor, scroll: &UniformListScrollHandle, cfg: &EditorSettings) -> Vec<usize> {
    if !cfg.folding.sticky_enabled {
        return Vec::new();
    }
    let (top, _) = viewport_top(scroll, cfg.line_height);
    editor.editor.sticky_lines(top, cfg.folding.sticky_max_line_count)
}

/// The overlay with one row per pinned header, `None` when nothing is pinned.
pub fn overlay<T: EditorHost>(
    pinned: Vec<usize>,
    cfg: &EditorSettings,
    theme: &Theme,
    gutter: f32,
    scroll_x: f32,
    fold_paint: folding::Paint,
    cx: &mut Context<T>,
) -> Option<AnyElement> {
    if pinned.is_empty() {
        return None;
    }
    let lh = cfg.line_height;
    let count = pinned.len();
    let (cfg_rows, theme_rows, lines_rows) = (cfg.clone(), theme.clone(), pinned.clone());
    let list = uniform_list(
        "sticky-scroll-rows",
        count,
        cx.processor(move |this: &mut T, range: Range<usize>, window, _cx| {
            let Some(e) = this.editor_host().0.active_editor() else { return Vec::new() };
            let mut out = Vec::new();
            for line in range.filter_map(|i| lines_rows.get(i).copied()) {
                let row = e.editor.view_lines().map_or(line - 1, |v| v.first_view_line_of(line));
                for el in super::rows(e, &theme_rows, &cfg_rows, gutter, row..row + 1, window, false, false, None, None, None, fold_paint, 0.0, None) {
                    out.push(div().debug_selector(move || format!("sticky:{line}")).child(el).into_any_element());
                }
            }
            out
        }),
    )
    .h(px(count as f32 * lh));
    let left = if cfg.folding.sticky_scroll_with_editor { scroll_x } else { 0.0 };
    Some(
        div()
            .id("sticky-scroll")
            .absolute()
            .top_0()
            .left_0()
            .right_0()
            .h(px(count as f32 * lh))
            .overflow_hidden()
            .bg(to_gpui(theme.editor_bg))
            .child(div().relative().left(px(left)).h_full().child(list))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this: &mut T, ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    let (editors, view) = this.editor_host();
                    let Some(e) = editors.active.and_then(|i| editors.open.get_mut(i)) else { return };
                    let y = f32::from(ev.position.y - view.bounds.get().origin.y);
                    let Some(&line) = pinned.get((y / view.cfg.line_height.max(1.0)) as usize) else { return };
                    e.editor.set_cursor(Position::new(line, 1), false);
                    view.after_change(e, false, true, window); // a click on a sticky line
                    let row = e.editor.caret_view_position().map_or(line - 1, |(r, _)| r);
                    if let Some(s) = view.scrolls.get(&e.path) {
                        s.scroll_to_item(row, ScrollStrategy::Top);
                    }
                    cx.notify();
                }),
            )
            .into_any_element(),
    )
}
