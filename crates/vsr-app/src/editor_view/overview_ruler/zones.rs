//! The overview ruler's zone math: where each zone lands on the lane, in pixels.
//!
//! upstream: src/vs/editor/common/viewModel/overviewZoneManager.ts @ 50e1350fc78d
//!
//! A zone covers lines of the document; its place on the lane is the document's height scaled to
//! the lane's height, with a minimum height so that a single line is still visible. Not ported:
//! the DOM width (it changes nothing in the zones) and the cache of resolved zones (upstream avoids
//! recomputing them between frames; here the ruler is rebuilt with each render).

use std::cmp::Ordering;
use std::collections::HashMap;

/// The smallest zone height on the lane, in pixels (`Constants.MINIMUM_HEIGHT`).
const MINIMUM_HEIGHT: f64 = 4.0;

/// A zone on the lane: from and to are pixels from the top, `color_id` names its colour.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ColorZone {
    pub from: i64,
    pub to: i64,
    pub color_id: u32,
}

/// A zone in the document: from `start_line` to `end_line` (1-based), `height_in_lines` tall, or,
/// with 0, as tall as the lines it covers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OverviewRulerZone {
    pub start_line: i64,
    pub end_line: i64,
    pub height_in_lines: i64,
    pub color: String,
}

impl OverviewRulerZone {
    pub fn new(start_line: i64, end_line: i64, height_in_lines: i64, color: &str) -> Self {
        Self {
            start_line,
            end_line,
            height_in_lines,
            color: color.to_string(),
        }
    }
}

/// `OverviewRulerZone.compare`: by colour, then start, height and end.
fn compare_zones(a: &OverviewRulerZone, b: &OverviewRulerZone) -> Ordering {
    a.color
        .cmp(&b.color)
        .then(a.start_line.cmp(&b.start_line))
        .then(a.height_in_lines.cmp(&b.height_in_lines))
        .then(a.end_line.cmp(&b.end_line))
}

pub struct OverviewZoneManager<F: Fn(i64) -> f64> {
    /// The vertical offset of a line (1-based) in the document, in pixels.
    vertical_offset_for_line: F,
    zones: Vec<OverviewRulerZone>,
    line_height: f64,
    dom_height: f64,
    outer_height: f64,
    pixel_ratio: f64,
    /// Colour ids are handed out in the order colours are first seen (from 1).
    color_ids: HashMap<String, u32>,
    id_colors: Vec<String>,
}

impl<F: Fn(i64) -> f64> OverviewZoneManager<F> {
    pub fn new(vertical_offset_for_line: F) -> Self {
        Self {
            vertical_offset_for_line,
            zones: Vec::new(),
            line_height: 0.0,
            dom_height: 0.0,
            outer_height: 0.0,
            pixel_ratio: 1.0,
            color_ids: HashMap::new(),
            id_colors: Vec::new(),
        }
    }

    pub fn set_zones(&mut self, mut zones: Vec<OverviewRulerZone>) {
        zones.sort_by(compare_zones);
        self.zones = zones;
    }

    pub fn set_line_height(&mut self, line_height: f64) {
        self.line_height = line_height;
    }

    pub fn set_pixel_ratio(&mut self, pixel_ratio: f64) {
        self.pixel_ratio = pixel_ratio;
    }

    pub fn set_dom_height(&mut self, height: f64) {
        self.dom_height = height;
    }

    /// The height of the whole document, which the lane's height stands for.
    pub fn set_outer_height(&mut self, outer_height: f64) {
        self.outer_height = outer_height;
    }

    /// The colour name of an id returned by `resolve_color_zones`.
    pub fn color_name(&self, id: u32) -> Option<&str> {
        self.id_colors
            .get((id as usize).checked_sub(1)?)
            .map(String::as_str)
    }

    fn color_id(&mut self, color: &str) -> u32 {
        if let Some(&id) = self.color_ids.get(color) {
            return id;
        }
        self.id_colors.push(color.to_string());
        let id = self.id_colors.len() as u32;
        self.color_ids.insert(color.to_string(), id);
        id
    }

    /// `resolveColorZones`: every zone as a span on the lane, sorted by colour then position.
    pub fn resolve_color_zones(&mut self) -> Vec<ColorZone> {
        let line_height = self.line_height.floor();
        let total_height = (self.dom_height * self.pixel_ratio).floor();
        let outer_height = self.outer_height.floor();
        if outer_height <= 0.0 || total_height <= 0.0 {
            return Vec::new();
        }
        let height_ratio = total_height / outer_height;
        let half_minimum_height = (MINIMUM_HEIGHT * self.pixel_ratio / 2.0).floor();

        let zones = std::mem::take(&mut self.zones);
        let mut all = Vec::with_capacity(zones.len());
        for zone in &zones {
            let offset1 = (self.vertical_offset_for_line)(zone.start_line);
            let offset2 = if zone.height_in_lines == 0 {
                (self.vertical_offset_for_line)(zone.end_line) + line_height
            } else {
                offset1 + zone.height_in_lines as f64 * line_height
            };
            let y1 = (height_ratio * offset1).floor();
            let y2 = (height_ratio * offset2).floor();
            let mut ycenter = ((y1 + y2) / 2.0).floor();
            let mut half_height = y2 - ycenter;
            if half_height < half_minimum_height {
                half_height = half_minimum_height;
            }
            if ycenter - half_height < 0.0 {
                ycenter = half_height;
            }
            if ycenter + half_height > total_height {
                ycenter = total_height - half_height;
            }
            let color_id = self.color_id(&zone.color);
            all.push(ColorZone {
                from: (ycenter - half_height) as i64,
                to: (ycenter + half_height) as i64,
                color_id,
            });
        }
        self.zones = zones;
        all.sort_by(|a, b| {
            a.color_id
                .cmp(&b.color_id)
                .then(a.from.cmp(&b.from))
                .then(a.to.cmp(&b.to))
        });
        all
    }
}
