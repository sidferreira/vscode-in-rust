//! The projection of a whole buffer into view lines: counts, model <-> view positions, view line
//! content, and incremental updates after edits.
//!
//! upstream: src/vs/editor/common/viewModel/viewModelLines.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/viewModel/modelLineProjection.ts @ 50e1350fc78d
//!
//! Simplifications: no hidden areas (folding), no injected text, no line decorations in the
//! projection. Instead of listening to edit events (`onModelLinesDeleted/Inserted/Changed`) the
//! projection compares the buffer with what it last saw: lines are matched from both ends by a
//! content hash and only the lines in between are recomputed, which is the same set of lines an
//! edit event would name and works for every kind of change (typing, undo, external reload).
//! View lines are 0-based, view columns 1-based and include the continuation indent.

use vsr_text::{Position, TextBuffer};

use super::line_breaks::{BreakParams, LineBreaksComputer, WordBreak, WrappingIndent};
use super::projection_data::{ModelLineProjectionData, PositionAffinity};

/// Everything that decides how lines wrap. Two equal values give equal projections.
#[derive(Debug, Clone, PartialEq)]
pub struct WrapParams {
    pub breaks: BreakParams,
    pub break_before: String,
    pub break_after: String,
}

impl WrapParams {
    /// Unit-width monospace parameters with upstream's default break characters.
    pub fn monospace(tab_size: usize, wrapping_column: i64, wrapping_indent: WrappingIndent) -> Self {
        Self {
            breaks: BreakParams::monospace(tab_size, wrapping_column, wrapping_indent),
            break_before: super::line_breaks::DEFAULT_BREAK_BEFORE.to_string(),
            break_after: super::line_breaks::DEFAULT_BREAK_AFTER.to_string(),
        }
    }

    pub fn with_word_break(mut self, word_break: WordBreak) -> Self {
        self.breaks.word_break = word_break;
        self
    }

    /// Whether lines wrap at all.
    pub fn is_wrapping(&self) -> bool {
        self.breaks.wrapping_column > 0
    }
}

#[derive(Debug, Clone)]
struct Entry {
    hash: u64,
    /// Length of the model line in UTF-16 units.
    len: usize,
    data: Option<ModelLineProjectionData>,
}

/// One view line: which part of which model line it shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ViewLine {
    pub model_line: usize,
    /// Index of this view line among the view lines of its model line (0 = the first).
    pub index: usize,
    /// The model column of the view line's first character.
    pub start_column: usize,
    /// The model column just after its last character.
    pub end_column: usize,
    /// Columns of indent before the text (0 on the first view line).
    pub indent: usize,
    /// Whether this is the last view line of the model line.
    pub is_last: bool,
}

#[derive(Debug, Clone)]
pub struct ViewLines {
    params: Option<WrapParams>,
    computer: LineBreaksComputer,
    lines: Vec<Entry>,
    /// `starts[i]` is the index of the first view line of model line `i + 1`; the last entry is the total.
    starts: Vec<usize>,
}

impl Default for ViewLines {
    fn default() -> Self {
        Self::new()
    }
}

fn hash_line(s: &str) -> u64 {
    // FxHash-style: fast and good enough to tell an edited line from an unchanged one.
    const K: u64 = 0x517c_c1b7_2722_0a95;
    let mut h: u64 = s.len() as u64;
    let (chunks, rest) = s.as_bytes().as_chunks::<8>();
    for c in chunks {
        h = (h.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(K);
    }
    for &b in rest {
        h = (h.rotate_left(5) ^ b as u64).wrapping_mul(K);
    }
    h
}

/// The UTF-16 range `[from, to)` of `s` as a string slice.
pub fn slice_utf16(s: &str, from: usize, to: usize) -> &str {
    if s.is_ascii() {
        return &s[from.min(s.len())..to.min(s.len())];
    }
    let (mut units, mut start, mut end) = (0usize, s.len(), s.len());
    let mut found_start = false;
    for (i, ch) in s.char_indices() {
        if !found_start && units >= from {
            start = i;
            found_start = true;
        }
        if units >= to {
            end = i;
            break;
        }
        units += ch.len_utf16();
    }
    &s[start.min(end)..end]
}

impl ViewLines {
    pub fn new() -> Self {
        Self { params: None, computer: LineBreaksComputer::with_defaults(), lines: Vec::new(), starts: vec![0] }
    }

    /// A projection built from known wrappings, one `(line length in UTF-16 units, wrapping)` per
    /// model line (upstream's tests build `ModelLineProjection`s from break offsets like this).
    /// It is not tied to a buffer: `sync` replaces it.
    pub fn from_parts(lines: Vec<(usize, Option<ModelLineProjectionData>)>) -> Self {
        let mut v = Self::new();
        v.lines = lines.into_iter().map(|(len, data)| Entry { hash: u64::MAX, len, data }).collect();
        v.rebuild_starts();
        v
    }

    fn rebuild_starts(&mut self) {
        self.starts.clear();
        self.starts.reserve(self.lines.len() + 1);
        let mut total = 0;
        for e in &self.lines {
            self.starts.push(total);
            total += e.data.as_ref().map_or(1, |d| d.output_line_count());
        }
        self.starts.push(total);
    }

    pub fn params(&self) -> Option<&WrapParams> {
        self.params.as_ref()
    }

    /// Brings the projection up to date with `buf` under `params`. A change of `params` recomputes
    /// every line; otherwise only the lines that differ from the last sync.
    pub fn sync(&mut self, buf: &TextBuffer, params: &WrapParams) {
        if self.params.as_ref() != Some(params) {
            self.params = Some(params.clone());
            self.computer = LineBreaksComputer::new(&params.break_before, &params.break_after);
            self.lines.clear();
        }
        let n_new = buf.line_count();
        let n_old = self.lines.len();
        let common = n_old.min(n_new);
        let mut pre = 0;
        while pre < common && self.lines[pre].hash == hash_line(buf.line_content(pre + 1)) {
            pre += 1;
        }
        let mut suf = 0;
        while suf < common - pre && self.lines[n_old - 1 - suf].hash == hash_line(buf.line_content(n_new - suf)) {
            suf += 1;
        }
        if pre == n_old && pre == n_new {
            return;
        }
        let breaks = &params.breaks;
        let fresh: Vec<Entry> = (pre..n_new - suf)
            .map(|i| {
                let text = buf.line_content(i + 1);
                let units: Vec<u16> = text.encode_utf16().collect();
                Entry { hash: hash_line(text), len: units.len(), data: self.computer.compute_units(&units, breaks) }
            })
            .collect();
        self.lines.splice(pre..n_old - suf, fresh);
        self.rebuild_starts();
    }

    /// The number of view lines.
    pub fn view_line_count(&self) -> usize {
        self.starts.last().copied().unwrap_or(0)
    }

    /// The number of model lines the projection covers.
    pub fn model_line_count(&self) -> usize {
        self.lines.len()
    }

    /// The index of the first view line of model line `line` (1-based).
    pub fn first_view_line_of(&self, line: usize) -> usize {
        self.starts[(line - 1).min(self.lines.len())]
    }

    /// How many view lines model line `line` has.
    pub fn view_lines_of(&self, line: usize) -> usize {
        self.starts[line] - self.starts[line - 1]
    }

    /// The model line a view line belongs to.
    pub fn model_line_of(&self, view_line: usize) -> usize {
        let n = self.lines.len();
        self.starts[..n].partition_point(|&s| s <= view_line).max(1)
    }

    /// Which part of which model line view line `view_line` shows (clamped to the last one).
    pub fn view_line(&self, view_line: usize) -> ViewLine {
        let view_line = view_line.min(self.view_line_count().saturating_sub(1));
        let model_line = self.model_line_of(view_line);
        let index = view_line - self.starts[model_line - 1];
        let entry = &self.lines[model_line - 1];
        match &entry.data {
            None => ViewLine { model_line, index: 0, start_column: 1, end_column: entry.len + 1, indent: 0, is_last: true },
            Some(d) => ViewLine {
                model_line,
                index,
                start_column: if index == 0 { 1 } else { d.break_offsets[index - 1] + 1 },
                end_column: d.break_offsets[index] + 1,
                indent: d.min_output_offset(index),
                is_last: index + 1 == d.output_line_count(),
            },
        }
    }

    /// The text of a view line: the continuation indent as spaces, then the model text.
    pub fn view_line_content(&self, buf: &TextBuffer, view_line: usize) -> String {
        let v = self.view_line(view_line);
        let text = buf.line_content(v.model_line);
        let part = slice_utf16(text, v.start_column - 1, v.end_column - 1);
        let mut out = String::with_capacity(v.indent + part.len());
        out.extend(std::iter::repeat_n(' ', v.indent));
        out.push_str(part);
        out
    }

    /// The length of a view line in UTF-16 units, indent included.
    pub fn view_line_length(&self, view_line: usize) -> usize {
        let v = self.view_line(view_line);
        v.indent + (v.end_column - v.start_column)
    }

    /// The view line and view column (1-based, indent included) of model position `p`.
    /// On a wrap boundary `Left` picks the end of the earlier view line.
    pub fn view_position_of(&self, p: Position, affinity: PositionAffinity) -> (usize, usize) {
        let line = p.line.clamp(1, self.lines.len().max(1));
        let base = self.starts[line - 1];
        match &self.lines[line - 1].data {
            None => (base, p.column),
            Some(d) => {
                let out = d.translate_to_output_position(p.column.saturating_sub(1), affinity);
                (base + out.output_line_index, out.output_offset + 1)
            }
        }
    }

    /// The model position of a view position, and whether it is the end of a view line that
    /// continues on the next one (the position that `view_position_of` places at the start of the
    /// next view line unless asked for `Left`).
    pub fn model_position_of(&self, view_line: usize, view_column: usize) -> (Position, bool) {
        let v = self.view_line(view_line);
        let column = match &self.lines[v.model_line - 1].data {
            None => view_column.min(v.end_column),
            Some(d) => (d.translate_to_input_offset(v.index, view_column.saturating_sub(1)) + 1).clamp(v.start_column, v.end_column),
        };
        (Position::new(v.model_line, column), !v.is_last && column == v.end_column)
    }
}
