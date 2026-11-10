//! Section headers: the `#region name` and `MARK: name` comments the minimap shows as titles.
//!
//! upstream: src/vs/editor/common/services/findSectionHeaders.ts @ 50e1350fc78d
//!
//! A direct port. Regular expressions are Rust `regex` ones (named groups `(?<label>..)` and
//! `(?<separator>..)` work the same). Columns are UTF-16 like everywhere else in the editor.

use regex::Regex;
use vsr_text::TextBuffer;

/// The lines of a document, for the finder (upstream `ISectionHeaderFinderTarget`).
pub trait SectionHeaderTarget {
    fn line_count(&self) -> usize;
    fn line_content(&self, line: usize) -> &str;
}

impl SectionHeaderTarget for TextBuffer {
    fn line_count(&self) -> usize {
        TextBuffer::line_count(self)
    }
    fn line_content(&self, line: usize) -> &str {
        TextBuffer::line_content(self, line)
    }
}

impl<S: AsRef<str>> SectionHeaderTarget for [S] {
    fn line_count(&self) -> usize {
        self.len()
    }
    fn line_content(&self, line: usize) -> &str {
        self[line - 1].as_ref()
    }
}

#[derive(Debug, Clone, Default)]
pub struct FindSectionHeaderOptions<'a> {
    /// The start marker of the language's folding rules (`#region`).
    pub region_start: Option<&'a Regex>,
    pub find_region_section_headers: bool,
    pub find_mark_section_headers: bool,
    pub mark_section_header_regex: &'a str,
    /// The tokens that open and close a block comment in the language. A mark header inside a
    /// block comment that closes at the end of the header does not include the closing token.
    pub block_comment_start_token: Option<&'a str>,
    pub block_comment_end_token: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SectionHeader {
    /// Where the header text is: start line and column, end line and column (1-based, UTF-16).
    pub start_line_number: usize,
    pub start_column: usize,
    pub end_line_number: usize,
    pub end_column: usize,
    /// The section header text.
    pub text: String,
    /// Whether the section header includes a separator line.
    pub has_separator_line: bool,
    /// This section should be omitted before rendering if it is not in a comment.
    pub should_be_in_comments: bool,
}

const CHUNK_SIZE: usize = 100;
const MAX_SECTION_LINES: usize = 5;

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Upstream `findSectionHeaders`.
pub fn find_section_headers<T: SectionHeaderTarget + ?Sized>(model: &T, options: &FindSectionHeaderOptions) -> Vec<SectionHeader> {
    let mut headers = Vec::new();
    if options.find_region_section_headers && options.region_start.is_some() {
        headers.extend(collect_region_headers(model, options));
    }
    if options.find_mark_section_headers {
        headers.extend(collect_mark_headers(model, options));
    }
    headers
}

fn collect_region_headers<T: SectionHeaderTarget + ?Sized>(model: &T, options: &FindSectionHeaderOptions) -> Vec<SectionHeader> {
    let Some(start) = options.region_start else { return Vec::new() };
    let mut out = Vec::new();
    for line in 1..=model.line_count() {
        let content = model.line_content(line);
        let Some(m) = start.find(content) else { continue };
        let start_column = utf16_len(m.as_str()) + 1;
        let end_column = utf16_len(content) + 1;
        if end_column > start_column {
            let (text, has_separator_line) = header_text(&content[m.as_str().len().min(content.len())..]);
            if !text.is_empty() || has_separator_line {
                out.push(SectionHeader {
                    start_line_number: line,
                    start_column,
                    end_line_number: line,
                    end_column,
                    text,
                    has_separator_line,
                    should_be_in_comments: false,
                });
            }
        }
    }
    out
}

/// Upstream `isMultilineRegexSource`.
fn is_multiline_regex_source(s: &str) -> bool {
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\n' {
            return true;
        }
        if c == '\\' {
            match chars.next() {
                Some('n' | 'r' | 'W') => return true,
                Some(_) => {}
                None => break,
            }
        }
    }
    false
}

/// Upstream `collectMarkHeaders`.
pub fn collect_mark_headers<T: SectionHeaderTarget + ?Sized>(model: &T, options: &FindSectionHeaderOptions) -> Vec<SectionHeader> {
    let mut headers: Vec<SectionHeader> = Vec::new();
    let source = options.mark_section_header_regex;
    if source.trim().is_empty() {
        return headers;
    }
    // `m`: `^` and `$` match at line breaks; `s`: `.` matches a line break when the regex spans lines.
    let flags = if is_multiline_regex_source(source) { "(?ms)" } else { "(?m)" };
    let Ok(regex) = Regex::new(&format!("{flags}{source}")) else { return headers };
    // A regex that matches the empty string would never advance.
    if regex.is_match("") {
        return headers;
    }
    let end_line_number = model.line_count();
    let mut start_line = 1;
    while start_line <= end_line_number {
        let end_line = (start_line + CHUNK_SIZE - 1).min(end_line_number);
        let text = (start_line..=end_line).map(|i| model.line_content(i)).collect::<Vec<_>>().join("\n");
        let mut last_index = 0;
        while last_index <= text.len() {
            let Some(caps) = regex.captures_at(&text, last_index) else { break };
            let m = caps.get(0).expect("group 0");
            let preceding = &text[..m.start()];
            let line_offset = preceding.matches('\n').count();
            let line_number = start_line + line_offset;
            let match_lines: Vec<&str> = m.as_str().split('\n').collect();
            let match_end_line = line_number + match_lines.len() - 1;
            let line_start_index = preceding.rfind('\n').map_or(0, |i| i + 1);
            let start_column = utf16_len(&text[line_start_index..m.start()]) + 1;
            let last_line = match_lines[match_lines.len() - 1];
            let end_column = if match_lines.len() == 1 { start_column + utf16_len(m.as_str()) } else { utf16_len(last_line) + 1 };

            let mut label = caps.name("label").map_or("", |g| g.as_str()).to_string();
            let mut separator = caps.name("separator").map_or("", |g| g.as_str()).to_string();
            if let (Some(start_token), Some(end_token)) = (options.block_comment_start_token, options.block_comment_end_token)
                && !start_token.is_empty()
                && !end_token.is_empty()
            {
                // The comment context is determined right before the captured text, because a
                // custom regex may consume the comment start token itself.
                let sep = caps.name("separator");
                let lab = caps.name("label");
                let captures_start = [sep.map(|g| g.start()), lab.map(|g| g.start()), Some(m.end())].into_iter().flatten().min().unwrap_or(m.end());
                let captures_line_start = if captures_start == 0 { 0 } else { text[..captures_start].rfind('\n').map_or(0, |i| i + 1) };
                if is_inside_block_comment(&text[captures_line_start..captures_start], start_token, end_token) {
                    let contiguous = matches!((sep, lab), (Some(s), Some(l)) if s.end() == l.start());
                    (label, separator) = strip_block_comment_end(&label, &separator, end_token, contiguous);
                }
            }
            let has_separator_line = !separator.is_empty();
            if !label.is_empty() || has_separator_line {
                // Only push if the previous one does not end on this same line.
                if headers.last().is_none_or(|h| h.end_line_number < line_number) {
                    headers.push(SectionHeader {
                        start_line_number: line_number,
                        start_column,
                        end_line_number: match_end_line,
                        end_column,
                        text: label,
                        has_separator_line,
                        should_be_in_comments: true,
                    });
                }
            }
            // Move past the current match to avoid an infinite loop.
            last_index = m.end();
            if m.end() == m.start() {
                break;
            }
        }
        start_line += CHUNK_SIZE - MAX_SECTION_LINES;
    }
    headers
}

/// Whether the text before the captured header leaves a block comment open (the tokens are
/// matched in order, so identical start and end tokens work).
fn is_inside_block_comment(prefix: &str, start_token: &str, end_token: &str) -> bool {
    let mut open = false;
    let mut offset = 0;
    while offset < prefix.len() {
        let token = if open { end_token } else { start_token };
        let Some(i) = prefix[offset..].find(token) else { break };
        open = !open;
        offset += i + token.len();
    }
    open
}

/// Removes the token that closes the enclosing block comment from the end of a mark header.
fn strip_block_comment_end(label: &str, separator: &str, end_token: &str, contiguous: bool) -> (String, String) {
    let trimmed = label.trim_end();
    if let Some(rest) = trimmed.strip_suffix(end_token) {
        return (rest.trim_end().to_string(), separator.to_string());
    }
    let combined = format!("{separator}{trimmed}");
    if contiguous && trimmed.chars().count() < end_token.chars().count() && combined.ends_with(end_token) {
        // The token was split across the separator and the label, e.g. `<!-- MARK: -->`.
        let remaining = &combined[..combined.len() - end_token.len()];
        return (String::new(), if remaining.trim().is_empty() { String::new() } else { remaining.to_string() });
    }
    (label.to_string(), separator.to_string())
}

fn header_text(text: &str) -> (String, bool) {
    let text = text.trim();
    let has_separator_line = text.starts_with('-');
    (text.trim_matches('-').to_string(), has_separator_line)
}
