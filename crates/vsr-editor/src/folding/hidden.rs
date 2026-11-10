//! The lines the collapsed regions hide, as sorted, disjoint line ranges.
//!
//! upstream: src/vs/editor/contrib/folding/browser/hiddenRangeModel.ts @ 50e1350fc78d
//!
//! Upstream's `HiddenRangeModel` listens to the folding model and fires an event with the new
//! ranges; here `update` is called after the regions changed and says whether the ranges did.
//! The ranges are whole lines: `start..=end`, 1-based, the first line of a region stays visible.

use vsr_text::{Position, TextBuffer};

use super::ranges::FoldingRegions;

/// A range of whole lines, both ends included.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineRange {
    pub start_line_number: usize,
    pub end_line_number: usize,
}

#[derive(Debug, Clone, Default)]
pub struct HiddenRangeModel {
    hidden: Vec<LineRange>,
}

impl HiddenRangeModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hidden_ranges(&self) -> &[LineRange] {
        &self.hidden
    }

    pub fn has_ranges(&self) -> bool {
        !self.hidden.is_empty()
    }

    /// Recomputes the ranges from `regions`. `force` reports a change even when the ranges are the
    /// same (upstream: an edit that changed the line count). Returns whether the ranges changed.
    pub fn update(&mut self, regions: &FoldingRegions, force: bool) -> bool {
        let mut new_hidden: Vec<LineRange> = Vec::new();
        let (mut last_start, mut last_end) = (usize::MAX, 0usize);
        for i in 0..regions.len() {
            if !regions.is_collapsed(i) {
                continue;
            }
            let start = regions.start_line_number(i) as usize + 1; // the first line is not hidden
            let end = regions.end_line_number(i) as usize;
            if last_start <= start && end <= last_end {
                continue; // contained in a collapsed region
            }
            new_hidden.push(LineRange { start_line_number: start, end_line_number: end });
            last_start = start;
            last_end = end;
        }
        let changed = force || new_hidden != self.hidden;
        self.hidden = new_hidden;
        changed
    }

    pub fn is_hidden(&self, line: usize) -> bool {
        self.find_range(line).is_some()
    }

    /// The hidden range that contains `line`.
    pub fn find_range(&self, line: usize) -> Option<LineRange> {
        let i = self.hidden.partition_point(|r| r.start_line_number <= line).checked_sub(1)?;
        (self.hidden[i].end_line_number >= line).then(|| self.hidden[i])
    }

    /// Moves the ends of a selection that are in hidden lines to the end of the line above the
    /// range. `selections` are `(anchor, active)`. Returns whether any changed.
    pub fn adjust_selections(&self, selections: &mut [(Position, Position)], buf: &TextBuffer) -> bool {
        let mut changed = false;
        let mut adjust = |p: &mut Position| {
            if let Some(r) = self.find_range(p.line) {
                let line = r.start_line_number - 1;
                if line > 0 {
                    *p = Position::new(line, buf.line_max_column(line));
                    changed = true;
                }
            }
        };
        for (a, b) in selections.iter_mut() {
            adjust(a);
            adjust(b);
        }
        changed
    }
}
