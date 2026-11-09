//! Editor surface: a read-only, virtualised view over a `vsr-text` buffer.
//!
//! This module owns everything about drawing a document (gutter, rows, scroll
//! state). It reads the document through `&OpenEditor`, so when `vsr-editor`
//! (cursor/selection/edits) lands, this is the one place that grows an editing
//! model; the workbench only asks for `EditorView::element`.

use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;

use gpui_kit::{
    AnyElement, Context, IntoElement, ListHorizontalSizingBehavior, ParentElement, Styled,
    UniformListScrollHandle, Window, div, px, uniform_list,
};

use crate::model::OpenEditor;
use crate::theme::{Theme, metrics as m};
use crate::ui::to_gpui;

/// Approximate advance of one Menlo digit at 12px, used for gutter sizing.
const CHAR_WIDTH: f32 = 7.224;
const TAB_SIZE: usize = 4;

#[derive(Default)]
pub struct EditorView {
    /// Per-document scroll state, so switching tabs keeps your place.
    scrolls: HashMap<PathBuf, UniformListScrollHandle>,
    /// Per-document (line_count, index of widest line), for horizontal scrolling.
    widest: HashMap<PathBuf, (usize, usize)>,
}

impl EditorView {
    pub fn forget(&mut self, path: &PathBuf) {
        self.scrolls.remove(path);
        self.widest.remove(path);
    }

    /// Build the editor element for `editor`. `active` re-resolves the document
    /// inside the row callback (rows are rendered lazily, after this returns).
    pub fn element<T: 'static>(
        &mut self,
        editor: &OpenEditor,
        theme: &Theme,
        cx: &mut Context<T>,
        active: fn(&T) -> Option<&OpenEditor>,
    ) -> impl IntoElement {
        let lines = editor.buffer.line_count();
        let scroll = self.scrolls.entry(editor.path.clone()).or_default().clone();
        let widest = match self.widest.get(&editor.path) {
            Some(&(n, i)) if n == lines => i,
            _ => {
                let i = widest_line(editor);
                self.widest.insert(editor.path.clone(), (lines, i));
                i
            }
        };
        let gutter = gutter_width(lines);
        let theme = theme.clone();
        let id: gpui_kit::SharedString = editor.path.display().to_string().into();
        uniform_list(
            id,
            lines,
            cx.processor(move |this: &mut T, range: Range<usize>, _w: &mut Window, _cx| {
                match active(this) {
                    Some(e) => rows(e, &theme, gutter, range),
                    None => Vec::new(),
                }
            }),
        )
        .track_scroll(&scroll)
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(widest))
        .size_full()
    }
}

fn gutter_width(lines: usize) -> f32 {
    let digits = lines.to_string().len().max(3);
    // VS Code: lineNumbersMinChars=5 worth of room, number right-aligned.
    digits as f32 * CHAR_WIDTH + 34.0
}

fn expand_tabs(s: &str) -> String {
    s.replace('\t', &" ".repeat(TAB_SIZE))
}

fn widest_line(e: &OpenEditor) -> usize {
    (1..=e.buffer.line_count())
        .max_by_key(|&l| expand_tabs(e.buffer.line_content(l)).chars().count())
        .map_or(0, |l| l - 1)
}

fn rows(e: &OpenEditor, t: &Theme, gutter: f32, range: Range<usize>) -> Vec<AnyElement> {
    range
        .map(|ix| {
            let line = ix + 1;
            div()
                .flex()
                .flex_row()
                .h(px(m::EDITOR_LINE_HEIGHT))
                .line_height(px(m::EDITOR_LINE_HEIGHT))
                .font_family("Menlo")
                .text_size(px(m::EDITOR_FONT_SIZE))
                .child(
                    div()
                        .flex_none()
                        .w(px(gutter))
                        .pr(px(26.))
                        .text_right()
                        .text_color(to_gpui(if line == 1 {
                            t.line_number_active_fg
                        } else {
                            t.line_number_fg
                        }))
                        .child(line.to_string()),
                )
                .child(
                    div()
                        .flex_none()
                        .whitespace_nowrap()
                        .text_color(to_gpui(t.editor_fg))
                        .child(expand_tabs(e.buffer.line_content(line))),
                )
                .into_any_element()
        })
        .collect()
}
