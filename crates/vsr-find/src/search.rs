//! Searching a text buffer: compile the search string and collect matches (with capture
//! groups) inside optional scopes.
//!
//! upstream: src/vs/editor/common/model/textModelSearch.ts @ 50e1350fc78d
//!
//! Query semantics come from `vsr_search::query` (the Search view uses the same code), without
//! smart case: literal vs regex, match case, `\n` in a regex making the search multiline. Whole
//! word is not `\b` but upstream's `isValidMatch`: a match is kept when the characters around it
//! (and its own first and last character) are not word characters of `editor.wordSeparators`
//! ([`SearchParams::word_separators`]). A search that is not multiline runs line by line (so `\s` cannot span lines, as
//! upstream); a multiline one runs over the LF-joined text. `^` and `$` always match at line
//! boundaries. Rust `regex` syntax: no look-around or backreferences (an invalid pattern is
//! reported through [`CompileError`]).

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
    /// `editor.wordSeparators`, used when `whole_word` is on. `None` is upstream's
    /// `USUAL_WORD_SEPARATORS`.
    pub word_separators: Option<String>,
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
    /// Whole word: the separators a match must be bounded by.
    word_separators: Option<String>,
}

/// Upstream's `WordCharacterClass`: only `Regular` characters make up a word (a space and a tab
/// are `Whitespace`, the characters of `editor.wordSeparators` are `WordSeparator`).
fn is_regular(separators: &str, c: char) -> bool {
    c != ' ' && c != '\t' && !separators.contains(c)
}

fn is_line_break(c: char) -> bool {
    c == '\r' || c == '\n'
}

/// Upstream `isValidMatch` (`textModelSearch.ts`): the match `matched` is a whole word, given the
/// character just `before` it and just `after` it in the buffer (`None` at the start or end of a line).
fn is_valid_match(separators: &str, before: Option<char>, matched: &str, after: Option<char>) -> bool {
    let first = matched.chars().next();
    let last = matched.chars().next_back();
    let left = match before {
        None => true,
        Some(c) => !is_regular(separators, c) || is_line_break(c) || first.is_some_and(|f| !is_regular(separators, f)),
    };
    let right = match after {
        None => true,
        Some(c) => !is_regular(separators, c) || is_line_break(c) || last.is_some_and(|l| !is_regular(separators, l)),
    };
    left && right
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
        is_word_match: false,
        is_multiline: false,
    };
    let source = regex_source(&query);
    let regex = RegexBuilder::new(&source)
        .case_insensitive(!params.match_case)
        .multi_line(true)
        .build()
        .map_err(|e| CompileError(e.to_string()))?;
    let word_separators = params
        .whole_word
        .then(|| params.word_separators.clone().unwrap_or_else(|| vsr_editor::USUAL_WORD_SEPARATORS.to_string()));
    Ok(Some(Compiled { regex, multiline: query.effective_multiline(), word_separators }))
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

    /// Whole word: is `text[start..end]` bounded by separators (always true when it is off).
    fn valid(&self, text: &str, start: usize, end: usize) -> bool {
        self.word_separators.as_ref().is_none_or(|s| {
            is_valid_match(s, text[..start].chars().next_back(), &text[start..end], text[end..].chars().next())
        })
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
            let mut done = false;
            self.scan(text, capture, |start, end, caps| {
                if !self.valid(content, b0 + start, b0 + end) {
                    return true;
                }
                let c0 = base + utf16_len(&text[..start]) + 1;
                let c1 = c0 + utf16_len(&text[start..end]);
                self.push(out, Range::new(Position::new(line, c0), Position::new(line, c1)), caps);
                done = out.len() >= limit;
                !done
            });
            if done {
                return;
            }
        }
    }

    /// Walks the matches the way a JavaScript global `RegExp` does: an empty match right after a
    /// non-empty one is found too (`\d*` on `10.2` gives `10`, ``, `2`), where the `regex`
    /// crate's own iterators skip it. `f` gets the byte span and, with `capture`, the groups;
    /// returning `false` stops.
    fn scan<'t>(&self, text: &'t str, capture: bool, mut f: impl FnMut(usize, usize, Option<regex::Captures<'t>>) -> bool) {
        let mut pos = 0;
        while pos <= text.len() {
            let (start, end, caps) = if capture {
                let Some(c) = self.regex.captures_at(text, pos) else { return };
                let m = c.get(0).expect("group 0");
                (m.start(), m.end(), Some(c))
            } else {
                let Some(m) = self.regex.find_at(text, pos) else { return };
                (m.start(), m.end(), None)
            };
            if !f(start, end, caps) {
                return;
            }
            pos = if end >= text.len() {
                // Upstream's `Searcher.next` (textModelSearch.ts:518-521) returns null once the previous
                // match ends at the text length, so no empty match follows a match that reaches the end:
                // `b*` on "ab" gives "", "b" (the same as upstream's `_findMatchesInLine`, :255-288).
                return;
            } else if end > start {
                end
            } else {
                match text[end..].chars().next() {
                    Some(c) => end + c.len_utf8(),
                    None => return,
                }
            };
        }
    }

    fn find_multiline(&self, buffer: &TextBuffer, r: Range, capture: bool, limit: usize, out: &mut Vec<FindMatch>) {
        // The text of the range, LF joined, and the byte offset where each line starts in it.
        let mut text = String::new();
        let mut starts = Vec::new();
        // The characters just outside the selection, which whole-word validity looks at.
        let (mut outer_before, mut outer_after) = (None, None);
        for line in r.start.line..=r.end.line {
            let content = buffer.line_content(line);
            let from_col = if line == r.start.line { r.start.column } else { 1 };
            let to_col = if line == r.end.line { r.end.column } else { buffer.line_max_column(line) };
            let (b0, b1) = (byte_at_column(content, from_col), byte_at_column(content, to_col));
            if line == r.start.line {
                outer_before = content[..b0].chars().next_back();
            }
            if line == r.end.line {
                outer_after = content[b1.max(b0)..].chars().next();
            }
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
        self.scan(&text, capture, |start, end, caps| {
            let valid = self.word_separators.as_ref().is_none_or(|s| {
                let before = if start == 0 { outer_before } else { text[..start].chars().next_back() };
                let after = if end == text.len() { outer_after } else { text[end..].chars().next() };
                is_valid_match(s, before, &text[start..end], after)
            });
            if !valid {
                return true;
            }
            self.push(out, Range::new(position_of(start), position_of(end)), caps);
            out.len() < limit
        });
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
    fn multiline_whole_word_checks_the_buffer_around_the_selection() {
        // Selection starts at the second "oo" of "xfoo": the word continues before it, so no match.
        let mut p = params("oo\\n?");
        p.is_regex = true;
        p.whole_word = true;
        let c = compile(&p).unwrap().unwrap();
        let b = buf("xfoo bar\nfoo");
        let scope = [Range::new(Position::new(1, 3), Position::new(2, 4))];
        assert!(c.find_matches(&b, Some(&scope), false, usize::MAX).is_empty());
        // Selection starts after the space: "bar" is a whole word, so it is found.
        let mut p = params("bar\\nfoo");
        p.is_regex = true;
        p.whole_word = true;
        let c = compile(&p).unwrap().unwrap();
        let scope = [Range::new(Position::new(1, 5), Position::new(2, 4))];
        assert_eq!(c.find_matches(&b, Some(&scope), false, usize::MAX).len(), 1);
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
