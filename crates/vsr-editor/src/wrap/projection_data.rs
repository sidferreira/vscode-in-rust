//! The wrapping of one model line: where it breaks, and the offset translations.
//!
//! upstream: src/vs/editor/common/modelLineProjectionData.ts @ 50e1350fc78d
//!
//! Simplification: no injected text (rscode has none), so the `injection*` fields and the
//! cursor-stop rules of upstream's `ModelLineProjectionData` are left out. Offsets are UTF-16
//! code units, like `vsr_text::Position` columns minus one.

/// Upstream `PositionAffinity` without the injected-text variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionAffinity {
    /// A position on a wrap boundary belongs to the following view line.
    None,
    /// ... to the preceding view line (the end of it).
    Left,
    /// ... to the following view line (same as `None` without injected text).
    Right,
}

/// A position inside a model line's view lines: view line index and offset in that view line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputPosition {
    pub output_line_index: usize,
    pub output_offset: usize,
}

impl std::fmt::Display for OutputPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.output_line_index, self.output_offset)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelLineProjectionData {
    /// The end offset of every view line, in UTF-16 units; the last one is the line length.
    pub break_offsets: Vec<usize>,
    /// The visible column at every break offset (tabs expanded).
    pub break_offsets_visible_column: Vec<f64>,
    /// Columns the continuation view lines are indented by.
    pub wrapped_text_indent_length: usize,
}

impl ModelLineProjectionData {
    pub fn new(break_offsets: Vec<usize>, break_offsets_visible_column: Vec<f64>, wrapped_text_indent_length: usize) -> Self {
        Self { break_offsets, break_offsets_visible_column, wrapped_text_indent_length }
    }

    pub fn output_line_count(&self) -> usize {
        self.break_offsets.len()
    }

    /// The first offset of a view line (the indent of continuation lines).
    pub fn min_output_offset(&self, output_line_index: usize) -> usize {
        if output_line_index > 0 { self.wrapped_text_indent_length } else { 0 }
    }

    /// The length of a view line, including the continuation indent.
    pub fn line_length(&self, output_line_index: usize) -> usize {
        let start = if output_line_index > 0 { self.break_offsets[output_line_index - 1] } else { 0 };
        let end = self.break_offsets[output_line_index];
        let len = end - start;
        if output_line_index > 0 { len + self.wrapped_text_indent_length } else { len }
    }

    pub fn max_output_offset(&self, output_line_index: usize) -> usize {
        self.line_length(output_line_index)
    }

    /// Upstream `translateToInputOffset`: the model offset of an offset in a view line. Offsets
    /// inside the continuation indent map to the start of the view line's text.
    pub fn translate_to_input_offset(&self, output_line_index: usize, output_offset: usize) -> usize {
        let output_offset = if output_line_index > 0 {
            output_offset.saturating_sub(self.wrapped_text_indent_length)
        } else {
            output_offset
        };
        if output_line_index == 0 { output_offset } else { self.break_offsets[output_line_index - 1] + output_offset }
    }

    /// Upstream `translateToOutputPosition`: which view line a model offset is on. A break offset
    /// belongs to the following view line unless `affinity` is `Left`.
    pub fn translate_to_output_position(&self, input_offset: usize, affinity: PositionAffinity) -> OutputPosition {
        let mut low: isize = 0;
        let mut high = self.break_offsets.len() as isize - 1;
        let mut mid: isize = 0;
        let mut mid_start = 0usize;
        while low <= high {
            mid = low + (high - low) / 2;
            let mid_stop = self.break_offsets[mid as usize];
            mid_start = if mid > 0 { self.break_offsets[mid as usize - 1] } else { 0 };
            if affinity == PositionAffinity::Left {
                if input_offset <= mid_start {
                    high = mid - 1;
                } else if input_offset > mid_stop {
                    low = mid + 1;
                } else {
                    break;
                }
            } else if input_offset < mid_start {
                high = mid - 1;
            } else if input_offset >= mid_stop {
                low = mid + 1;
            } else {
                break;
            }
        }
        let mut output_offset = input_offset.saturating_sub(mid_start);
        if mid > 0 {
            output_offset += self.wrapped_text_indent_length;
        }
        OutputPosition { output_line_index: mid as usize, output_offset }
    }

    /// Upstream `normalizeOutputPosition` without injected text: with `Left` a position at the
    /// start of a continuation line moves to the end of the previous one, with `Right` the
    /// other way round.
    pub fn normalize_output_position(&self, output_line_index: usize, output_offset: usize, affinity: PositionAffinity) -> OutputPosition {
        match affinity {
            PositionAffinity::Left => {
                if output_line_index > 0 && output_offset == self.min_output_offset(output_line_index) {
                    return OutputPosition {
                        output_line_index: output_line_index - 1,
                        output_offset: self.max_output_offset(output_line_index - 1),
                    };
                }
            }
            PositionAffinity::Right => {
                let max_index = self.output_line_count() - 1;
                if output_line_index < max_index && output_offset == self.max_output_offset(output_line_index) {
                    return OutputPosition {
                        output_line_index: output_line_index + 1,
                        output_offset: self.min_output_offset(output_line_index + 1),
                    };
                }
            }
            PositionAffinity::None => {}
        }
        OutputPosition { output_line_index, output_offset }
    }
}
