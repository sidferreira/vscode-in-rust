//! The overview ruler (`editor.overviewRulerBorder`, `editor.hideCursorInOverviewRuler`, ADR-0055): marks
//! for the cursor, the selection and the find matches, drawn down the lane of the vertical scrollbar at
//! the height of their line in the whole document.
//!
//! upstream: src/vs/editor/browser/viewParts/overviewRuler/decorationsOverviewRuler.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/overviewRuler/overviewRuler.ts @ 50e1350fc78d
//!
//! `zones` is the math (`zones.rs`, a port of `overviewZoneManager.ts`); `zones_for` picks what
//! is marked. The marks use the document's lines, so with folding they sit at the unfolded height,
//! which upstream's view lines would place differently.

#[cfg(test)]
mod upstream_tests;
pub mod zones;

use vsr_editor::Selection;
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

/// The marks for `zones` on a lane `viewport` pixels tall, standing for a document `content` pixels tall
/// with lines `line_height` apart. `color` gives each zone colour name its colour.
pub fn marks(
    zones: Vec<OverviewRulerZone>,
    line_height: f32,
    viewport: f32,
    content: f32,
    color: impl Fn(&str) -> Rgba8,
) -> Vec<Mark> {
    let lh = f64::from(line_height);
    let mut manager = OverviewZoneManager::new(move |line: i64| (line - 1) as f64 * lh);
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
