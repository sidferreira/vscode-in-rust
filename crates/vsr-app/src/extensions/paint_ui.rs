//! The gpui side of [`super::paint`]: boxes, text after a range and squiggles as elements of an
//! editor row (ADR-0067). Each element has a `debug_selector` (`ext-box:<line>:<from>`,
//! `ext-after:<line>`, `ext-squiggle:<line>:<from>`) so headless tests find it.

use super::paint::{LinePaint, Severity, TO_LINE_END};
use crate::ui::to_gpui;
use gpui_kit::*;

fn border_width(div: Div, w: f32) -> Div {
    if w <= 1.0 {
        div.border_1()
    } else if w <= 2.0 {
        div.border_2()
    } else if w <= 4.0 {
        div.border_4()
    } else {
        div.border_8()
    }
}

/// `isWholeLine` boxes: behind the whole row, from the gutter's edge to the right.
pub fn whole_line(mut row: Div, l: &LinePaint, line: usize, gutter: f32, lh: f32) -> Div {
    for b in l.boxes.iter().filter(|b| b.whole_line) {
        let mut d = div().absolute().top_0().h(px(lh)).left(px(gutter)).right_0();
        if let Some(bg) = b.background {
            d = d.bg(to_gpui(bg));
        }
        if let Some((w, c)) = b.border {
            d = border_width(d, w).border_color(to_gpui(c));
        }
        row = row.child(d.debug_selector(|| format!("ext-box:{line}:{}", b.from)));
    }
    row
}

/// Range boxes, text after ranges and squiggles inside the text box of a row. `clip` maps a
/// column range of the line to the part this row shows; `x_at` is the x of a column.
#[allow(clippy::too_many_arguments)]
pub fn in_text(
    mut text_box: Div,
    l: &LinePaint,
    line: usize,
    lh: f32,
    line_max_column: usize,
    last_row: bool,
    clip: &dyn Fn(usize, usize) -> Option<(usize, usize)>,
    x_at: &dyn Fn(usize) -> f32,
) -> Div {
    let end = |to: usize| if to == TO_LINE_END { line_max_column } else { to.min(line_max_column.max(1)) };
    for b in l.boxes.iter().filter(|b| !b.whole_line) {
        let Some((from, to)) = clip(b.from, end(b.to)) else { continue };
        let (x0, x1) = (x_at(from), x_at(to));
        if x1 <= x0 {
            continue;
        }
        let mut d = div().absolute().top_0().left(px(x0)).w(px(x1 - x0)).h(px(lh));
        if let Some(bg) = b.background {
            d = d.bg(to_gpui(bg));
        }
        if let Some((w, c)) = b.border {
            d = border_width(d, w).border_color(to_gpui(c));
        }
        text_box = text_box.child(d.debug_selector(|| format!("ext-box:{line}:{}", b.from)));
    }
    for s in &l.squiggles {
        let Some((from, to)) = clip(s.from, end(s.to)) else { continue };
        let (x0, x1) = (x_at(from), x_at(to));
        if x1 <= x0 {
            continue;
        }
        let color = to_gpui(s.color);
        let kind = match s.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        let wave = canvas(
            |_, _, _| (),
            move |bounds, _, window, _| {
                let style = UnderlineStyle { thickness: px(1.), color: Some(color), wavy: true };
                window.paint_underline(point(bounds.origin.x, bounds.origin.y), bounds.size.width, &style);
            },
        )
        .size_full();
        text_box = text_box.child(
            div()
                .absolute()
                .top(px(lh - 4.))
                .left(px(x0))
                .w(px(x1 - x0))
                .h(px(4.))
                .debug_selector(|| format!("ext-squiggle:{line}:{}:{kind}", s.from))
                .child(wave),
        );
    }
    if last_row {
        for a in &l.after {
            let x = x_at(a.column.min(line_max_column.max(1))) + a.margin_left;
            let mut d = div().absolute().top_0().left(px(x)).h(px(lh)).whitespace_nowrap().pr(px(a.margin_right));
            if let Some(c) = a.color {
                d = d.text_color(to_gpui(c));
            }
            if let Some(bg) = a.background {
                d = d.bg(to_gpui(bg));
            }
            if a.italic {
                d = d.italic();
            }
            text_box = text_box.child(d.debug_selector(|| format!("ext-after:{line}")).child(a.text.clone()));
        }
    }
    text_box
}
