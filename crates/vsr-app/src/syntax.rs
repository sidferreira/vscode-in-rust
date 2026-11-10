//! The workbench side of syntax highlighting (ADR-0038): the per-editor tokenizer, and the
//! mapping from token runs to the text gpui draws.
//!
//! upstream: src/vs/editor/common/model/tokens/tokenizationTextModelPart.ts @ 50e1350fc78d
//!
//! The tokens and styles come from `vsr-syntax`. An editor syncs its tokenizer with the buffer when
//! the buffer's version changed (changed lines are tokenized right away within a small budget, the
//! rest on a worker thread); drawing reads whatever tokens a line has, so a line shows its old
//! colours for a moment after an edit instead of flashing plain.

use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{FontStyle, FontWeight, HighlightStyle, Hsla, StrikethroughStyle, UnderlineStyle, px, rgba};
use vsr_syntax::{BackgroundTokenizer, LineSource, StyleRun, SyntaxService};
use vsr_text::TextBuffer;

use crate::editor_view::wrap::Seg;

/// How long a sync may tokenize on the editing thread.
const SYNC_BUDGET: Duration = Duration::from_millis(2);

/// The text buffer as the tokenizer's lines.
pub struct BufferLines<'a>(pub &'a TextBuffer);

impl LineSource for BufferLines<'_> {
    fn line_count(&self) -> usize {
        self.0.line_count()
    }
    fn line(&self, index: usize) -> &str {
        self.0.line_content(index + 1)
    }
}

/// An editor's tokens: the service (styles), and the tokenizer of its language.
pub struct SyntaxDoc {
    pub service: Arc<SyntaxService>,
    pub tokenizer: BackgroundTokenizer,
}

impl SyntaxDoc {
    pub fn sync(&self, buffer: &TextBuffer) {
        self.tokenizer.update(&BufferLines(buffer), SYNC_BUDGET);
    }

    /// Whether upstream's `files.trimTrailingWhitespaceInRegexAndStrings: false` leaves the
    /// trailing whitespace of 1-based `line` alone: the tokens are not accurate yet, or the
    /// whitespace starts inside a string or regular expression. `offset` is a 0-based UTF-16
    /// offset in the line.
    pub fn keeps_trailing_whitespace(&self, line: usize, text: &str, offset: usize) -> bool {
        use vsr_textmate::TokenType;
        self.tokenizer.with(|d| {
            if !d.is_accurate(line - 1) {
                return true;
            }
            let byte = byte_of_utf16(text, offset);
            matches!(d.token_type_at(line - 1, byte), TokenType::String | TokenType::RegEx)
        })
    }

    /// The style runs of a 1-based model line, as byte ranges of its text. `None` while the line
    /// has no tokens yet.
    pub fn style_runs(&self, line: usize, text: &str) -> Option<Vec<StyleRun>> {
        let (grammar, tokens) = self.tokenizer.with(|d| (d.grammar().clone(), d.tokens(line - 1)));
        let tokens = tokens?;
        Some(self.service.style_runs(&grammar, text.len(), &tokens))
    }
}

/// The tokens of an editor for the typing rules (`vsr_editor::TokenSource`, ADR-0046): the type of
/// each token of a line (string, comment, regex), so a quote or bracket is not auto closed inside one.
/// No tokenizer (plain text) means one `Other` token per line; a line that is not tokenized yet
/// gives `None` (auto closing and auto indentation leave it alone, like upstream's
/// `isCheapToTokenize`).
pub struct SyntaxTokens<'a>(pub Option<&'a SyntaxDoc>);

impl vsr_editor::TokenSource for SyntaxTokens<'_> {
    fn line_tokens(&self, buffer: &TextBuffer, line: usize) -> Option<vsr_editor::LineTokens> {
        use vsr_editor::{LineTokens, StandardTokenType};
        let text = buffer.line_content(line);
        let units = |s: &str| s.encode_utf16().count();
        let Some(doc) = self.0 else {
            return Some(LineTokens::plain(units(text)));
        };
        doc.sync(buffer);
        doc.tokenizer.with(|d| {
            if !d.is_accurate(line - 1) {
                return None;
            }
            let tokens = d.tokens(line - 1)?;
            let mut out: Vec<(usize, StandardTokenType)> = Vec::with_capacity(tokens.len());
            for (i, t) in tokens.iter().enumerate() {
                let end_byte = tokens.get(i + 1).map_or(text.len(), |n| (n.start as usize).min(text.len()));
                let start_byte = (t.start as usize).min(text.len());
                if end_byte <= start_byte {
                    continue;
                }
                let ty = match d.token_type_at(line - 1, start_byte) {
                    vsr_textmate::TokenType::Other => StandardTokenType::Other,
                    vsr_textmate::TokenType::Comment => StandardTokenType::Comment,
                    vsr_textmate::TokenType::String => StandardTokenType::String,
                    vsr_textmate::TokenType::RegEx => StandardTokenType::RegEx,
                };
                out.push((units(&text[..end_byte]), ty));
            }
            Some(LineTokens::new(&out))
        })
    }

    fn token_type_if_inserting(&self, buffer: &TextBuffer, line: usize, column: usize, ch: char) -> Option<vsr_editor::StandardTokenType> {
        use vsr_editor::StandardTokenType;
        let doc = self.0?;
        doc.sync(buffer);
        let text = buffer.line_content(line);
        let byte = byte_of_utf16(text, column - 1);
        doc.tokenizer.with(|d| d.token_type_if_inserting(line - 1, byte, ch)).map(|t| match t {
            vsr_textmate::TokenType::Other => StandardTokenType::Other,
            vsr_textmate::TokenType::Comment => StandardTokenType::Comment,
            vsr_textmate::TokenType::String => StandardTokenType::String,
            vsr_textmate::TokenType::RegEx => StandardTokenType::RegEx,
        })
    }
}

fn hsla(c: vsr_textmate::Rgba) -> Hsla {
    let [r, g, b, a] = c.0;
    rgba(u32::from_be_bytes([r, g, b, a])).into()
}

/// The highlight of a style, `None` when it changes nothing.
fn highlight_of(style: &vsr_textmate::TokenStyle) -> Option<HighlightStyle> {
    if style.foreground.is_none() && style.font_style == 0 {
        return None;
    }
    let color = style.foreground.map(hsla);
    Some(HighlightStyle {
        color,
        font_weight: style.bold().then_some(FontWeight::BOLD),
        font_style: style.italic().then_some(FontStyle::Italic),
        underline: style.underline().then(|| UnderlineStyle { thickness: px(1.), color, wavy: false }),
        strikethrough: style.strikethrough().then(|| StrikethroughStyle { thickness: px(1.), color }),
        ..Default::default()
    })
}

/// The highlights for the text of one row: `runs` are byte ranges of the model line `text`; the
/// row shows the part `seg` (UTF-16 columns of the line) after `seg.indent` spaces, with tabs
/// expanded to `tab` stops (what `expand_tabs` makes of the row's display text). Ranges are bytes
/// of that expanded text, sorted, and clipped to the row.
pub fn highlights_for_row(text: &str, runs: &[StyleRun], seg: &Seg, tab: usize, whole_line: bool) -> Vec<(Range<usize>, HighlightStyle)> {
    if runs.is_empty() {
        return Vec::new();
    }
    let tab = tab.max(1);
    // Byte range of the line this row shows.
    let (from_byte, to_byte) = if whole_line {
        (0, text.len())
    } else {
        (byte_of_utf16(text, seg.start - 1), byte_of_utf16(text, seg.end - 1))
    };
    // Offset into the expanded row text for every char boundary of that range.
    let mut map = vec![0usize; to_byte - from_byte + 1];
    let (mut exp, mut visual) = (seg.indent, seg.indent);
    for (i, ch) in text[from_byte..to_byte].char_indices() {
        map[i] = exp;
        if ch == '\t' {
            let n = tab - visual % tab;
            exp += n;
            visual += n;
        } else {
            exp += ch.len_utf8();
            visual += 1;
        }
    }
    map[to_byte - from_byte] = exp;
    let mut out = Vec::new();
    for run in runs {
        let (s, e) = (run.start.max(from_byte), run.end.min(to_byte));
        if s >= e {
            continue;
        }
        let Some(h) = highlight_of(&run.style) else { continue };
        let (a, b) = (map[s - from_byte], map[e - from_byte]);
        if a < b {
            out.push((a..b, h));
        }
    }
    out
}

/// Byte offset of the UTF-16 index `utf16` of `text` (clamped to its end).
fn byte_of_utf16(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (i, ch) in text.char_indices() {
        if units >= utf16 {
            return i;
        }
        units += ch.len_utf16();
    }
    text.len()
}

/// The colour runs of a line for the minimap: `(UTF-16 index, color)`, a run for every style
/// change, `default` where the theme sets no foreground.
pub fn minimap_colors(text: &str, runs: &[StyleRun], default: [u8; 3]) -> Vec<(usize, [u8; 3])> {
    let mut out: Vec<(usize, [u8; 3])> = Vec::with_capacity(runs.len());
    let mut units = 0;
    let mut at = 0;
    for run in runs {
        units += text[at..run.start].encode_utf16().count();
        at = run.start;
        let color = run.style.foreground.map_or(default, |c| [c.0[0], c.0[1], c.0[2]]);
        if out.last().is_none_or(|l| l.1 != color) {
            out.push((units, color));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_textmate::{Rgba, TokenStyle};

    fn run(start: usize, end: usize, fg: Option<[u8; 3]>, font: u8) -> StyleRun {
        StyleRun { start, end, style: TokenStyle { foreground: fg.map(|[r, g, b]| Rgba([r, g, b, 255])), background: None, font_style: font } }
    }

    fn whole(line_len_cols: usize) -> Seg {
        Seg::whole(1, line_len_cols + 1)
    }

    #[test]
    fn runs_map_onto_expanded_tabs_and_unicode() {
        // "\téa b": the tab expands to 4 spaces, é is two bytes.
        let text = "\té a";
        let runs = [run(0, 1, None, 0), run(1, 3, Some([1, 2, 3]), 0), run(4, 5, Some([4, 5, 6]), 2)];
        let h = highlights_for_row(text, &runs, &whole(4), 4, true);
        // The tab run changes nothing (no style); `é` is bytes 1..3 of text = 4..6 of the expanded text.
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].0, 4..6);
        assert_eq!(h[1].0, 7..8);
        assert!(h[1].1.font_weight.is_some());
    }

    #[test]
    fn wrapped_rows_clip_the_runs() {
        let text = "abcdefgh";
        let runs = [run(0, 8, Some([9, 9, 9]), 0)];
        // The row shows columns 4..7 (UTF-16 indexes 3..6, "def") after a 2 space indent.
        let seg = Seg { line: 1, start: 4, end: 7, indent: 2, first: false, last: false };
        let h = highlights_for_row(text, &runs, &seg, 4, false);
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].0, 2..5);
    }
}
