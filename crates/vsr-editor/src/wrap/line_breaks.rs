//! The monospace line breaks computer: where a model line wraps for a given column limit.
//!
//! upstream: src/vs/editor/common/viewModel/monospaceLineBreaksComputer.ts @ 50e1350fc78d
//!
//! A port of `createLineBreaks` and its helpers (`WrappingCharacterClassifier`, `canBreak`,
//! `computeWrappedTextIndentLength`, escaped line feeds, two-cell full-width characters).
//! Simplifications: no injected text (rscode has none, so the fixed-width ranges are gone) and
//! no `createLineBreaksFromPreviousLineBreaks` (upstream's resize shortcut: its result equals
//! the fresh computation, which upstream's own test checks, so a resize recomputes the lines).
//! Offsets are UTF-16 units like the buffer columns; widths are `f64` in the unit of
//! `BreakParams::half_width` (pixels upstream, any unit here).

use std::collections::HashMap;

/// `editor.wrappingIndent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WrappingIndent {
    None,
    #[default]
    Same,
    Indent,
    DeepIndent,
}

/// `editor.wordBreak`: `KeepAll` does not break inside CJK words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordBreak {
    #[default]
    Normal,
    KeepAll,
}

/// Upstream's defaults for `wordWrapBreakBeforeCharacters` and `wordWrapBreakAfterCharacters`.
pub const DEFAULT_BREAK_BEFORE: &str = "([{\u{2018}\u{201c}\u{3008}\u{300a}\u{300c}\u{300e}\u{3010}\u{3014}\u{ff08}\u{ff3b}\u{ff5b}\u{ff62}\u{a3}\u{a5}\u{ff04}\u{ffe1}\u{ffe5}+\u{ff0b}";
pub const DEFAULT_BREAK_AFTER: &str = " \t})]?|/&.,;\u{a2}\u{b0}\u{2032}\u{2033}\u{2030}\u{2103}\u{3001}\u{3002}\u{ff61}\u{ff64}\u{ffe0}\u{ff0c}\u{ff0e}\u{ff1a}\u{ff1b}\u{ff1f}\u{ff01}\u{ff05}\u{30fb}\u{ff65}\u{309d}\u{309e}\u{30fd}\u{30fe}\u{30fc}\u{30a1}\u{30a3}\u{30a5}\u{30a7}\u{30a9}\u{30c3}\u{30e3}\u{30e5}\u{30e7}\u{30ee}\u{30f5}\u{30f6}\u{3041}\u{3043}\u{3045}\u{3047}\u{3049}\u{3063}\u{3083}\u{3085}\u{3087}\u{308e}\u{3095}\u{3096}\u{31f0}\u{31f1}\u{31f2}\u{31f3}\u{31f4}\u{31f5}\u{31f6}\u{31f7}\u{31f8}\u{31f9}\u{31fa}\u{31fb}\u{31fc}\u{31fd}\u{31fe}\u{31ff}\u{3005}\u{303b}\u{ff67}\u{ff68}\u{ff69}\u{ff6a}\u{ff6b}\u{ff6c}\u{ff6d}\u{ff6e}\u{ff6f}\u{ff70}\u{201d}\u{3009}\u{300b}\u{300d}\u{300f}\u{3011}\u{3015}\u{ff09}\u{ff3d}\u{ff5d}\u{ff63}";

/// What the computer needs besides the line text.
#[derive(Debug, Clone, PartialEq)]
pub struct BreakParams {
    pub tab_size: usize,
    /// The column limit; `-1` disables wrapping.
    pub wrapping_column: i64,
    pub wrapping_indent: WrappingIndent,
    pub word_break: WordBreak,
    /// `wrapOnEscapedLineFeeds`: a literal `\n` inside a quoted string breaks the line.
    pub wrap_on_escaped_line_feeds: bool,
    /// Advance of a half-width character (`typicalHalfwidthCharacterWidth`).
    pub half_width: f64,
    /// Advance of a full-width character (`typicalFullwidthCharacterWidth`).
    pub full_width: f64,
    /// Count every full-width character as exactly two cells (`useTwoCellFullwidthCharacters`).
    pub use_two_cell_fullwidth: bool,
}

impl BreakParams {
    /// Unit-width monospace metrics: half-width 1, full-width 2, no escaped line feeds.
    pub fn monospace(tab_size: usize, wrapping_column: i64, wrapping_indent: WrappingIndent) -> Self {
        Self {
            tab_size,
            wrapping_column,
            wrapping_indent,
            word_break: WordBreak::Normal,
            wrap_on_escaped_line_feeds: false,
            half_width: 1.0,
            full_width: 2.0,
            use_two_cell_fullwidth: false,
        }
    }
}

use super::projection_data::ModelLineProjectionData;

const NONE: u8 = 0;
const BREAK_BEFORE: u8 = 1;
const BREAK_AFTER: u8 = 2;
const BREAK_IDEOGRAPHIC: u8 = 3; // Han and Kana.

/// Upstream `WrappingCharacterClassifier`.
#[derive(Debug, Clone)]
pub struct LineBreaksComputer {
    ascii: [u8; 256],
    other: HashMap<u16, u8>,
}

impl LineBreaksComputer {
    pub fn new(break_before: &str, break_after: &str) -> Self {
        let mut c = Self { ascii: [NONE; 256], other: HashMap::new() };
        for unit in break_before.encode_utf16() {
            c.set(unit, BREAK_BEFORE);
        }
        for unit in break_after.encode_utf16() {
            c.set(unit, BREAK_AFTER);
        }
        c
    }

    /// A computer with upstream's default break characters.
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_BREAK_BEFORE, DEFAULT_BREAK_AFTER)
    }

    fn set(&mut self, unit: u16, class: u8) {
        if unit < 256 {
            self.ascii[unit as usize] = class;
        } else {
            self.other.insert(unit, class);
        }
    }

    fn class(&self, unit: u16) -> u8 {
        if unit < 256 {
            return self.ascii[unit as usize];
        }
        // Hiragana and Katakana, CJK Extension A, CJK Unified Ideographs.
        if (0x3040..=0x30FF).contains(&unit) || (0x3400..=0x4DBF).contains(&unit) || (0x4E00..=0x9FFF).contains(&unit) {
            return BREAK_IDEOGRAPHIC;
        }
        self.other.get(&unit).copied().unwrap_or(NONE)
    }

    /// The wrapping of `line`, `None` when it does not wrap (it fits, or wrapping is off).
    pub fn compute(&self, line: &str, p: &BreakParams) -> Option<ModelLineProjectionData> {
        let text: Vec<u16> = line.encode_utf16().collect();
        self.compute_units(&text, p)
    }

    /// `compute` on UTF-16 units.
    pub fn compute_units(&self, text: &[u16], p: &BreakParams) -> Option<ModelLineProjectionData> {
        if p.wrapping_column == -1 {
            return None;
        }
        let len = text.len();
        if len <= 1 {
            return None;
        }
        let escaped = p.wrap_on_escaped_line_feeds && has_char(text, b'"') && has_escaped_n(text);
        self.create_line_breaks(text, p, escaped)
    }

    fn create_line_breaks(&self, text: &[u16], p: &BreakParams, wrap_on_escaped_line_feeds: bool) -> Option<ModelLineProjectionData> {
        let len = text.len();
        let first_column = p.wrapping_column as f64;
        let columns_for_full_width = p.full_width / p.half_width;
        let is_keep_all = p.word_break == WordBreak::KeepAll;
        let tab_size = p.tab_size.max(1);
        let full_cols = full_width_columns(columns_for_full_width, p.use_two_cell_fullwidth);
        let wrapped_indent = compute_wrapped_text_indent_length(text, tab_size, first_column, full_cols, p.wrapping_indent);

        // The wrapping decision is taken in widths of `half_width` units.
        let half = p.half_width;
        let wrapped_line_break_width = (first_column - wrapped_indent as f64) * half;

        let mut offsets: Vec<usize> = Vec::new();
        let mut columns: Vec<f64> = Vec::new();
        let mut break_offset = 0usize;
        let mut break_visible_column = 0f64;
        let mut break_width = 0f64;
        let mut breaking_width = first_column * half;

        let mut prev_code = text[0];
        let mut visible_column = char_width(prev_code, 0.0, tab_size, columns_for_full_width, p.use_two_cell_fullwidth);
        let mut current_width = char_pixel_width(prev_code, 0.0, tab_size, p);
        let mut start = 1;
        if is_high_surrogate(prev_code) {
            // A surrogate pair is one unit and is never broken.
            visible_column += 1.0;
            current_width += half;
            prev_code = text[1];
            start += 1;
        }
        let mut prev_class = self.class(prev_code);

        let mut i = start;
        while i < len {
            let char_start = i;
            let code = text[i];
            let class;
            let width;
            let pixel_width;
            if is_high_surrogate(code) {
                i += 1;
                class = NONE;
                width = 2.0;
                pixel_width = 2.0 * half;
            } else {
                class = self.class(code);
                width = char_width(code, visible_column, tab_size, columns_for_full_width, p.use_two_cell_fullwidth);
                pixel_width = char_pixel_width(code, visible_column, tab_size, p);
            }

            let mut wrap_escaped_line_feed = false;
            if wrap_on_escaped_line_feeds && is_escaped_line_break_at(text, char_start) {
                // A literal \n triggers a soft wrap.
                break_offset = char_start;
                break_visible_column = visible_column;
                break_width = current_width;
                wrap_escaped_line_feed = true;
            } else if can_break(prev_code, prev_class, code, class, is_keep_all) {
                break_offset = char_start;
                break_visible_column = visible_column;
                break_width = current_width;
            }

            visible_column += width;
            current_width += pixel_width;

            // Does adding the character at `i` go over the breaking width?
            if current_width > breaking_width || wrap_escaped_line_feed {
                if break_offset == 0 || current_width - break_width > wrapped_line_break_width {
                    // Cannot break at `break_offset`, must break before this character.
                    break_offset = char_start;
                    break_visible_column = visible_column - width;
                    break_width = current_width - pixel_width;
                }
                let current_line_start = offsets.last().copied().unwrap_or(0);
                if break_offset > current_line_start {
                    offsets.push(break_offset);
                    columns.push(break_visible_column);
                    breaking_width = break_width + wrapped_line_break_width;
                    break_offset = 0;
                }
            }

            prev_code = code;
            prev_class = class;
            i += 1;
        }

        if offsets.is_empty() {
            return None;
        }
        // The last segment.
        offsets.push(len);
        columns.push(visible_column);
        Some(ModelLineProjectionData::new(offsets, columns, wrapped_indent))
    }
}

fn has_char(text: &[u16], c: u8) -> bool {
    text.contains(&(c as u16))
}

/// Whether the text contains a backslash followed by `n`.
fn has_escaped_n(text: &[u16]) -> bool {
    text.windows(2).any(|w| w[0] == b'\\' as u16 && w[1] == b'n' as u16)
}

fn is_high_surrogate(unit: u16) -> bool {
    (0xD800..=0xDBFF).contains(&unit)
}

/// Upstream `strings.isFullWidthCharacter`.
fn is_full_width(unit: u16) -> bool {
    (0x2E80..=0xD7AF).contains(&unit)
        || (0xF900..=0xFAFF).contains(&unit)
        || (0xFF01..=0xFF5E).contains(&unit)
        || (0xFFE0..=0xFFE6).contains(&unit)
}

fn full_width_columns(columns_for_full_width: f64, use_two_cell: bool) -> f64 {
    if use_two_cell { 2.0 } else { columns_for_full_width }
}

fn tab_width(visible_column: f64, tab_size: usize) -> f64 {
    tab_size as f64 - visible_column % tab_size as f64
}

fn char_width(unit: u16, visible_column: f64, tab_size: usize, columns_for_full_width: f64, use_two_cell: bool) -> f64 {
    if unit == 9 {
        return tab_width(visible_column, tab_size);
    }
    if is_full_width(unit) {
        return full_width_columns(columns_for_full_width, use_two_cell);
    }
    if unit < 32 {
        // With `editor.renderControlCharacters` the substitutions are often wide.
        return columns_for_full_width;
    }
    1.0
}

fn char_pixel_width(unit: u16, visible_column: f64, tab_size: usize, p: &BreakParams) -> f64 {
    if unit == 9 {
        return tab_width(visible_column, tab_size) * p.half_width;
    }
    if is_full_width(unit) {
        return if p.use_two_cell_fullwidth { 2.0 * p.half_width } else { p.full_width };
    }
    if unit < 32 {
        return p.full_width;
    }
    p.half_width
}

/// Upstream `isEscapedLineBreakAtPosition`: the character before `i` is an `n` that follows an odd
/// number of backslashes.
fn is_escaped_line_break_at(text: &[u16], i: usize) -> bool {
    if i >= 2 && text[i - 1] == b'n' as u16 {
        let mut escapes = 0;
        for j in (0..=i - 2).rev() {
            if text[j] == b'\\' as u16 {
                escapes += 1;
            } else {
                return escapes % 2 == 1;
            }
        }
    }
    false
}

/// Upstream `canBreak` (Kinsoku Shori: no break after an opening character, none before a closing one).
fn can_break(prev_code: u16, prev_class: u8, code: u16, class: u8, is_keep_all: bool) -> bool {
    let _ = prev_code;
    code != 32
        && ((prev_class == BREAK_AFTER && class != BREAK_AFTER)
            || (prev_class != BREAK_BEFORE && class == BREAK_BEFORE)
            || (!is_keep_all && prev_class == BREAK_IDEOGRAPHIC && class != BREAK_AFTER)
            || (!is_keep_all && class == BREAK_IDEOGRAPHIC && prev_class != BREAK_BEFORE))
}

fn compute_wrapped_text_indent_length(
    text: &[u16],
    tab_size: usize,
    first_line_break_column: f64,
    columns_for_full_width_char: f64,
    wrapping_indent: WrappingIndent,
) -> usize {
    let mut indent = 0usize;
    if wrapping_indent != WrappingIndent::None {
        let first_non_ws = text.iter().position(|&u| u != 32 && u != 9);
        if let Some(first_non_ws) = first_non_ws {
            // Track the existing indent.
            for &u in &text[..first_non_ws] {
                indent += if u == 9 { tab_width(indent as f64, tab_size) as usize } else { 1 };
            }
            // Increase the indent of continuation lines, if desired.
            let extra_tabs = match wrapping_indent {
                WrappingIndent::DeepIndent => 2,
                WrappingIndent::Indent => 1,
                _ => 0,
            };
            for _ in 0..extra_tabs {
                indent += tab_width(indent as f64, tab_size) as usize;
            }
            // Stick to the beginning of the line if no character would fit besides the indentation.
            if indent as f64 + columns_for_full_width_char > first_line_break_column {
                indent = 0;
            }
        }
    }
    indent
}
