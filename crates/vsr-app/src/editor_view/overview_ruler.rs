//! The overview ruler (`editor.overviewRulerBorder`, `editor.hideCursorInOverviewRuler`, ADR-0055): marks
//! for the cursor, the selection and the find matches, drawn down the lane of the vertical scrollbar at
//! the height of their line in the whole document.
//!
//! upstream: src/vs/editor/browser/viewParts/overviewRuler/decorationsOverviewRuler.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/overviewRuler/overviewRuler.ts @ 50e1350fc78d
//!
//! `zones` is the math (`zones.rs`, a port of `overviewZoneManager.ts`); `zones_for` picks what
//! is marked. The marks are placed by view line (`view_line_of`, the wrap and fold projection,
//! ADR-0135), so a fold or a wrapped line moves the marks below it as it moves the rows.

#[cfg(test)]
mod upstream_tests;
pub mod zones;

use vsr_editor::Selection;
use vsr_editor::wrap::ViewLines;
use vsr_text::Range as TextRange;

use self::zones::{OverviewRulerZone, OverviewZoneManager};
use crate::theme::Rgba8;

/// The zone colour names (what `OverviewRulerZone.color` holds).
pub const CURSOR: &str = "cursor";
pub const SELECTION: &str = "selection";
pub const MATCH: &str = "match";

/// One mark on the lane: where it starts and how tall it is, in pixels from the lane's top.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Mark {
    pub top: f32,
    pub height: f32,
    pub color: Rgba8,
    /// The zone it marks (`cursor`, `selection`, `match`), for tests to find it.
    pub name: &'static str,
}

/// What the lane draws: the marks, and the border along its left edge (`editor.overviewRulerBorder`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Lane {
    pub marks: Vec<Mark>,
    pub border: Option<Rgba8>,
}

/// The zones of the cursor (unless hidden), the selection (when not empty) and each find match. Lines are
/// 1-based, like `Position`.
pub fn zones_for(
    selection: Selection,
    matches: &[TextRange],
    hide_cursor: bool,
) -> Vec<OverviewRulerZone> {
    let mut zones = Vec::new();
    if !hide_cursor {
        let line = selection.active.line as i64;
        zones.push(OverviewRulerZone::new(line, line, 0, CURSOR));
    }
    if selection.anchor != selection.active {
        let (a, b) = (selection.anchor.line as i64, selection.active.line as i64);
        zones.push(OverviewRulerZone::new(a.min(b), a.max(b), 0, SELECTION));
    }
    for m in matches {
        zones.push(OverviewRulerZone::new(
            m.start.line as i64,
            m.end.line as i64,
            0,
            MATCH,
        ));
    }
    zones
}

/// The view line (0-based) that model line `line` (1-based) starts on, so a mark sits where its line is
/// drawn. Without a projection (no wrap, no fold) that is the document's own line. A hidden line stands for
/// the visible line above it, the header of its fold.
pub fn view_line_of(view: Option<&ViewLines>, line: i64) -> i64 {
    let Some(v) = view else { return line - 1 };
    let count = v.model_line_count();
    if count == 0 {
        return 0;
    }
    let mut l = (line.max(1) as usize).min(count);
    while l > 1 && v.is_hidden(l) {
        l -= 1;
    }
    v.first_view_line_of(l) as i64
}

/// The marks for `zones` on a lane `viewport` pixels tall, standing for a document `content` pixels tall
/// with rows `line_height` apart. `view_line` gives the view line (0-based) of a model line (1-based), see
/// `view_line_of`. `color` gives each zone colour name its colour.
pub fn marks(
    zones: Vec<OverviewRulerZone>,
    line_height: f32,
    viewport: f32,
    content: f32,
    view_line: impl Fn(i64) -> i64,
    color: impl Fn(&str) -> Rgba8,
) -> Vec<Mark> {
    let lh = f64::from(line_height);
    let mut manager = OverviewZoneManager::new(move |line: i64| view_line(line) as f64 * lh);
    manager.set_line_height(lh);
    manager.set_dom_height(f64::from(viewport));
    manager.set_outer_height(f64::from(content));
    manager.set_pixel_ratio(1.0);
    manager.set_zones(zones);
    let resolved = manager.resolve_color_zones();
    resolved
        .into_iter()
        .filter_map(|z| {
            let name = manager.color_name(z.color_id)?;
            let name = match name {
                CURSOR => CURSOR,
                SELECTION => SELECTION,
                _ => MATCH,
            };
            Some(Mark {
                top: z.from as f32,
                height: (z.to - z.from) as f32,
                color: color(name),
                name,
            })
        })
        .collect()
}
