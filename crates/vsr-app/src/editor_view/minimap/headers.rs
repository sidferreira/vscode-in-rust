//! Section headers in the minimap: the `#region name` and `MARK: name` comments shown as titles
//! over the scaled-down lines (ADR-0042).
//!
//! upstream: src/vs/editor/browser/viewParts/minimap/minimap.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/services/findSectionHeaders.ts @ 50e1350fc78d
//!
//! `find` runs the ported finder (`vsr_editor::section_headers`) on the document with the
//! language's region markers and block comment tokens; `place` puts the headers on the minimap
//! canvas. Differences: the title is a text label over the line (upstream paints it into the
//! canvas with a letter spacing, which gpui does not have, so `sectionHeaderLetterSpacing` is not
//! read), and mark headers must sit in a comment according to the tokenizer (as upstream).

use regex::Regex;
use vsr_editor::section_headers::{FindSectionHeaderOptions, SectionHeader, find_section_headers};
use vsr_textmate::TokenType;

use super::Frame;
use crate::config::folding::FoldingSettings;
use crate::model::OpenEditor;

/// A header on the minimap canvas.
#[derive(Debug, Clone, PartialEq)]
pub struct Placed {
    pub line: usize,
    /// CSS pixels from the top of the canvas.
    pub y: f32,
    pub text: String,
    pub separator: bool,
}

/// The section headers of the document, mark headers only where the tokenizer says comment.
pub fn find(e: &OpenEditor, f: &FoldingSettings) -> Vec<SectionHeader> {
    if !f.minimap_region_headers && !f.minimap_mark_headers {
        return Vec::new();
    }
    let language = e.language_id();
    let rules = vsr_languages::bundled::folding_rules(language);
    let region_start = rules.and_then(|r| r.markers.as_ref()).and_then(|m| Regex::new(&m.start).ok());
    let block = vsr_languages::bundled::block_comment(language);
    let options = FindSectionHeaderOptions {
        region_start: region_start.as_ref(),
        find_region_section_headers: f.minimap_region_headers,
        find_mark_section_headers: f.minimap_mark_headers,
        mark_section_header_regex: &f.minimap_mark_regex,
        block_comment_start_token: block.map(|b| b.0),
        block_comment_end_token: block.map(|b| b.1),
    };
    let mut headers = find_section_headers(e.buffer(), &options);
    headers.retain(|h| !h.should_be_in_comments || in_comment(e, h));
    headers
}

/// Whether the header starts in a comment (upstream asks the tokenizer; lines it has not
/// tokenized accurately yet pass, so headers do not flicker while it works).
fn in_comment(e: &OpenEditor, h: &SectionHeader) -> bool {
    let Some(doc) = e.syntax() else { return false };
    let text = e.buffer().line_content(h.start_line_number);
    // The label, not the whole match, must be in the comment: use the match start like upstream.
    let byte = byte_of_utf16(text, h.start_column - 1);
    doc.tokenizer.with(|d| !d.is_accurate(h.start_line_number - 1) || d.token_type_at(h.start_line_number - 1, byte) == TokenType::Comment)
}

fn byte_of_utf16(s: &str, units: usize) -> usize {
    let mut n = 0;
    for (i, c) in s.char_indices() {
        if n >= units {
            return i;
        }
        n += c.len_utf16();
    }
    s.len()
}

/// Puts `headers` on the minimap canvas. Headers of hidden lines, and the ones off the drawn part,
/// are left out.
pub fn place(headers: &[SectionHeader], e: &OpenEditor, frame: &Frame, sampling_ratio: Option<f64>) -> Vec<Placed> {
    let pr = frame.pixel_ratio.max(0.1);
    let line_h = (frame.info.line_height / pr) as f32;
    let view = e.editor.view_lines();
    headers
        .iter()
        .filter_map(|h| {
            let line = h.start_line_number;
            let view_line = match view.as_ref() {
                Some(v) if v.is_hidden(line) => return None,
                Some(v) => v.first_view_line_of(line) + 1,
                None => line,
            };
            let m = sampling_ratio.map_or(view_line as f64, |r| (view_line as f64 / r).round().max(1.0));
            if m < frame.layout.start_line || m > frame.layout.end_line {
                return None;
            }
            let y = ((m - frame.layout.start_line + frame.layout.top_padding_line_count) as f32) * line_h;
            Some(Placed { line, y, text: h.text.clone(), separator: h.has_separator_line })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn byte_offsets_of_utf16_columns() {
        assert_eq!(byte_of_utf16("a\u{e9}b", 2), 3);
        assert_eq!(byte_of_utf16("abc", 9), 3);
    }
}
