//! Result previews: trimming long lines around the match and eliding the gaps.
//!
//! upstream: src/vs/workbench/services/search/common/search.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/strings.ts @ 50e1350fc78d
//!
//! Columns are UTF-16 code unit offsets, like upstream (JS strings), so previews and ranges
//! agree with the editor model (`vsr-text` is UTF-16 based too).

/// A range with line and UTF-16 column numbers (basis depends on the producer; see the field
/// docs of the types that carry it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct SearchRange {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl SearchRange {
    pub const fn new(start_line: usize, start_column: usize, end_line: usize, end_column: usize) -> Self {
        Self { start_line, start_column, end_line, end_column }
    }

    /// `OneLineRange`.
    pub const fn one_line(line: usize, start_column: usize, end_column: usize) -> Self {
        Self::new(line, start_column, line, end_column)
    }
}

/// `ITextSearchPreviewOptions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PreviewOptions {
    pub match_lines: usize,
    pub chars_per_line: usize,
}

/// `DEFAULT_TEXT_SEARCH_PREVIEW_OPTIONS`.
pub const DEFAULT_PREVIEW_OPTIONS: PreviewOptions = PreviewOptions { match_lines: 100, chars_per_line: 10000 };

/// What the Search view asks for (`searchView.ts`: `previewOptions: { matchLines: 1, charsPerLine: 1000 }`).
pub const VIEW_PREVIEW_OPTIONS: PreviewOptions = PreviewOptions { match_lines: 1, chars_per_line: 1000 };

const ELIDED_PREFIX: &str = "⟪ ";
const ELIDED_SUFFIX: &str = " characters skipped ⟫";

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// `SearchRangeSetPairing`: a range in the file and where it falls in the preview text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RangePair {
    pub source: SearchRange,
    pub preview: SearchRange,
}

/// `TextSearchMatch`: preview text for one or more ranges of one match chunk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSearchMatch {
    pub range_locations: Vec<RangePair>,
    pub preview_text: String,
}

/// `getNLines`.
fn get_n_lines(text: &str, n: usize) -> &str {
    if n == 0 {
        return "";
    }
    let mut idx: Option<usize> = None;
    let mut remaining = n;
    loop {
        let from = idx.map_or(0, |i| i + 1);
        idx = text[from..].find('\n').map(|p| p + from);
        remaining -= 1;
        if remaining == 0 || idx.is_none() {
            break;
        }
    }
    match idx {
        None => text,
        Some(i) => {
            let i = if i > 0 && text.as_bytes()[i - 1] == b'\r' { i - 1 } else { i };
            &text[..i]
        }
    }
}

fn is_single_line_range_list(ranges: &[SearchRange]) -> bool {
    let line = ranges[0].start_line;
    ranges.iter().all(|r| r.start_line == line && r.end_line == line)
}

impl TextSearchMatch {
    /// `new TextSearchMatch(text, ranges, previewOptions)`. `ranges` must not be empty.
    pub fn new(text: &str, ranges: &[SearchRange], preview: Option<PreviewOptions>) -> Self {
        assert!(!ranges.is_empty());
        match preview {
            Some(opts) if opts.match_lines == 1 && is_single_line_range_list(ranges) => {
                let text = get_n_lines(text, opts.match_lines);
                let units: Vec<u16> = text.encode_utf16().collect();
                let slice = |a: usize, b: usize| -> String {
                    let b = b.min(units.len());
                    let a = a.min(b);
                    String::from_utf16_lossy(&units[a..b])
                };
                let min_len = (utf16_len(ELIDED_PREFIX) + utf16_len(ELIDED_SUFFIX) + 5) * 2;
                let mut result = String::new();
                let mut shift: i64 = 0;
                let mut last_end = 0usize;
                let leading = opts.chars_per_line / 5;
                let mut range_locations = Vec::new();
                for r in ranges {
                    let preview_start = r.start_column.saturating_sub(leading);
                    let preview_end = r.start_column + opts.chars_per_line;
                    if preview_start > last_end + leading + min_len {
                        let elision =
                            format!("{ELIDED_PREFIX}{}{ELIDED_SUFFIX}", preview_start - last_end);
                        result.push_str(&elision);
                        result.push_str(&slice(preview_start, preview_end));
                        shift += preview_start as i64 - (last_end + utf16_len(&elision)) as i64;
                    } else {
                        result.push_str(&slice(last_end, preview_end));
                    }
                    last_end = preview_end;
                    range_locations.push(RangePair {
                        source: *r,
                        preview: SearchRange::one_line(
                            0,
                            (r.start_column as i64 - shift) as usize,
                            (r.end_column as i64 - shift) as usize,
                        ),
                    });
                }
                Self { range_locations, preview_text: result }
            }
            _ => {
                let first = ranges[0].start_line;
                let range_locations = ranges
                    .iter()
                    .map(|r| RangePair {
                        source: *r,
                        preview: SearchRange::new(
                            r.start_line - first,
                            r.start_column,
                            r.end_line - first,
                            r.end_column,
                        ),
                    })
                    .collect();
                Self { range_locations, preview_text: text.to_string() }
            }
        }
    }
}
