//! Searching a text buffer: compile the search string and collect matches (with capture
//! groups) inside optional scopes.
//!
//! upstream: src/vs/editor/common/model/textModelSearch.ts @ 50e1350fc78d
//!
//! Query semantics come from `vsr_search::query` (the Search view uses the same code), without
//! smart case: literal vs regex, match case, whole word (`\b`), `\n` in a regex making the search
//! multiline. A search that is not multiline runs line by line (so `\s` cannot span lines, as
//! upstream); a multiline one runs over the LF-joined text. `^` and `$` always match at line
//! boundaries. Rust `regex` syntax: no look-around or backreferences (an invalid pattern is
//! reported through [`CompileError`]). Whole word uses `\b`, not upstream's `wordSeparators`.

use regex::{Regex, RegexBuilder};
use vsr_search::TextSearchQuery;
use vsr_search::query::regex_source;
use vsr_text::{Position, Range, TextBuffer};

/// What the user typed and toggled (upstream `SearchParams`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchParams {
    pub search_string: String,
    pub is_regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
}

/// One match: where it is and, when asked for, the full match followed by the groups.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindMatch {
    pub range: Range,
    pub matches: Option<Vec<String>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError(pub String);

#[derive(Debug, Clone)]
pub struct Compiled {
    regex: Regex,
    multiline: bool,
}

/// `Ok(None)` for an empty search string (matches nothing), `Err` for an invalid regex.
pub fn compile(params: &SearchParams) -> Result<Option<Compiled>, CompileError> {
    if params.search_string.is_empty() {
        return Ok(None);
    }
    let query = TextSearchQuery {
        pattern: params.search_string.clone(),
        is_regex: params.is_regex,
        is_case_sensitive: params.match_case,
        is_word_match: params.whole_word,
        is_multiline: false,
    };
    let source = regex_source(&query);
    let regex = RegexBuilder::new(&source)
        .case_insensitive(!params.match_case)
        .multi_line(true)
        .build()
        .map_err(|e| CompileError(e.to_string()))?;
    Ok(Some(Compiled { regex, multiline: query.effective_multiline() }))
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// Narrows `r` to the buffer, `None` for an inverted result.
fn clamp(buffer: &TextBuffer, r: Range) -> Range {
    Range::new(buffer.validate_position(r.start), buffer.validate_position(r.end))
}

/// The byte index in `line` of the UTF-16 `column` (1-based), snapping into the line.
fn byte_at_column(line: &str, column: usize) -> usize {
    let mut units = 1;
    for (i, ch) in line.char_indices() {
        if units >= column {
            return i;
        }
        units += ch.len_utf16();
    }
    line.len()
}

impl Compiled {
    pub fn is_multiline(&self) -> bool {
        self.multiline
    }

    /// All matches starting inside `ranges` (the whole buffer when `None`), in document order and
    /// without duplicates, at most `limit`. `capture` fills `FindMatch::matches`.
    pub fn find_matches(
        &self,
        buffer: &TextBuffer,
        ranges: Option<&[Range]>,
        capture: bool,
        limit: usize,
    ) -> Vec<FindMatch> {
        let mut search: Vec<Range> = match ranges {
            Some(r) => r.iter().map(|&r| clamp(buffer, r)).collect(),
            None => vec![buffer.full_range()],
        };
        search.sort_by_key(|r| (r.start, r.end));
        // Merge overlapping scopes so no match is reported twice.
        let mut merged: Vec<Range> = Vec::new();
        for r in search {
            match merged.last_mut() {
                Some(last) if r.start <= last.end => last.end = last.end.max(r.end),
                _ => merged.push(r),
            }
        }
        let mut out = Vec::new();
        for r in merged {
            if out.len() >= limit {
                break;
            }
            if self.multiline {
                self.find_multiline(buffer, r, capture, limit, &mut out);
            } else {
                self.find_line_by_line(buffer, r, capture, limit, &mut out);
            }
        }
        out.truncate(limit);
        out
    }

    fn push(&self, out: &mut Vec<FindMatch>, range: Range, caps: Option<regex::Captures<'_>>) {
        let matches = caps.map(|c| {
            c.iter().map(|g| g.map_or_else(String::new, |m| m.as_str().to_string())).collect()
        });
        out.push(FindMatch { range, matches });
    }

    fn find_line_by_line(&self, buffer: &TextBuffer, r: Range, capture: bool, limit: usize, out: &mut Vec<FindMatch>) {
        for line in r.start.line..=r.end.line {
            let content = buffer.line_content(line);
            let from_col = if line == r.start.line { r.start.column } else { 1 };
            let to_col = if line == r.end.line { r.end.column } else { buffer.line_max_column(line) };
            let (b0, b1) = (byte_at_column(content, from_col), byte_at_column(content, to_col));
            let text = &content[b0..b1.max(b0)];
            let base = from_col - 1; // UTF-16 offset of the slice in the line
            if capture {
                for caps in self.regex.captures_iter(text) {
                    let m = caps.get(0).expect("group 0");
                    let c0 = base + utf16_len(&text[..m.start()]) + 1;
                    let c1 = c0 + utf16_len(m.as_str());
                    self.push(out, Range::new(Position::new(line, c0), Position::new(line, c1)), Some(caps));
                    if out.len() >= limit {
                        return;
                    }
                }
            } else {
                for m in self.regex.find_iter(text) {
                    let c0 = base + utf16_len(&text[..m.start()]) + 1;
                    let c1 = c0 + utf16_len(m.as_str());
                    self.push(out, Range::new(Position::new(line, c0), Position::new(line, c1)), None);
                    if out.len() >= limit {
                        return;
                    }
                }
            }
        }
    }

    fn find_multiline(&self, buffer: &TextBuffer, r: Range, capture: bool, limit: usize, out: &mut Vec<FindMatch>) {
        // The text of the range, LF joined, and the byte offset where each line starts in it.
        let mut text = String::new();
        let mut starts = Vec::new();
        for line in r.start.line..=r.end.line {
            let content = buffer.line_content(line);
            let from_col = if line == r.start.line { r.start.column } else { 1 };
            let to_col = if line == r.end.line { r.end.column } else { buffer.line_max_column(line) };
            let (b0, b1) = (byte_at_column(content, from_col), byte_at_column(content, to_col));
            starts.push(text.len());
            text.push_str(&content[b0..b1.max(b0)]);
            if line != r.end.line {
                text.push('\n');
            }
        }
        let position_of = |byte: usize| -> Position {
            let idx = starts.partition_point(|&s| s <= byte) - 1;
            let line = r.start.line + idx;
            let line_text = &text[starts[idx]..byte];
            let base = if idx == 0 { r.start.column - 1 } else { 0 };
            Position::new(line, base + utf16_len(line_text) + 1)
        };
        for caps in self.regex.captures_iter(&text) {
            let m = caps.get(0).expect("group 0");
            let range = Range::new(position_of(m.start()), position_of(m.end()));
            self.push(out, range, capture.then_some(caps));
            if out.len() >= limit {
                return;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_text::DefaultEol;

    fn buf(s: &str) -> TextBuffer {
        TextBuffer::new(s, DefaultEol::Lf, true)
    }

    fn params(s: &str) -> SearchParams {
        SearchParams { search_string: s.into(), ..SearchParams::default() }
    }

    fn find(text: &str, p: &SearchParams) -> Vec<(usize, usize, usize, usize)> {
        let c = compile(p).unwrap().unwrap();
        c.find_matches(&buf(text), None, false, usize::MAX)
            .iter()
            .map(|m| (m.range.start.line, m.range.start.column, m.range.end.line, m.range.end.column))
            .collect()
    }

    #[test]
    fn literal_is_case_insensitive_by_default() {
        assert_eq!(find("Foo foo\nFOO", &params("foo")), [(1, 1, 1, 4), (1, 5, 1, 8), (2, 1, 2, 4)]);
        let mut p = params("foo");
        p.match_case = true;
        assert_eq!(find("Foo foo\nFOO", &p), [(1, 5, 1, 8)]);
    }

    #[test]
    fn regex_metacharacters_are_literal_unless_regex() {
        assert_eq!(find("a.c abc", &params("a.c")), [(1, 1, 1, 4)]);
        let mut p = params("a.c");
        p.is_regex = true;
        assert_eq!(find("a.c abc", &p), [(1, 1, 1, 4), (1, 5, 1, 8)]);
    }

    #[test]
    fn whole_word() {
        let mut p = params("foo");
        p.whole_word = true;
        assert_eq!(find("foo foobar barfoo foo_ (foo)", &p), [(1, 1, 1, 4), (1, 25, 1, 28)]);
    }

    #[test]
    fn columns_are_utf16_and_multiline_regex_spans_lines() {
        assert_eq!(find("é😀ab ab", &params("ab")), [(1, 4, 1, 6), (1, 7, 1, 9)]);
        let mut p = params("a\\nb");
        p.is_regex = true;
        assert_eq!(find("xa\nb\nab", &p), [(1, 2, 2, 2)]);
    }

    #[test]
    fn captures_and_scopes() {
        let mut p = params("(\\w)(\\d)");
        p.is_regex = true;
        let c = compile(&p).unwrap().unwrap();
        let b = buf("a1 b2\nc3");
        let all = c.find_matches(&b, None, true, usize::MAX);
        assert_eq!(all[1].matches.as_deref(), Some(&["b2".to_string(), "b".into(), "2".into()][..]));
        let scoped = c.find_matches(&b, Some(&[Range::new(Position::new(1, 4), Position::new(2, 3))]), false, usize::MAX);
        assert_eq!(scoped.len(), 2, "b2 and c3");
        assert_eq!(c.find_matches(&b, None, false, 2).len(), 2, "limit");
    }

    #[test]
    fn empty_and_invalid() {
        assert!(compile(&params("")).unwrap().is_none());
        let mut p = params("(");
        p.is_regex = true;
        assert!(compile(&p).is_err());
        // zero-width matches
        let mut p = params("^");
        p.is_regex = true;
        assert_eq!(find("a\nb", &p), [(1, 1, 1, 1), (2, 1, 2, 1)]);
    }
}
