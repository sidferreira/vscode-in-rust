//! Toggle line comment (Cmd+/), add and remove line comment, and toggle block comment (Shift+Alt+A).
//!
//! upstream: src/vs/editor/contrib/comment/browser/lineCommentCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/comment/browser/blockCommentCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/comment/browser/comment.ts @ 50e1350fc78d
//!
//! The comment tokens come from the language configuration (`comments.lineComment`,
//! `comments.blockComment`). `editor.comments.insertSpace` and `editor.comments.ignoreEmptyLines`
//! are the options. The edits are one undo step. One cursor only: upstream's `ignoreFirstLine` (two
//! selections on the same line) does not exist here.

use vsr_text::{Position, Range};

use crate::cursor_columns::{first_non_whitespace_column, utf16_len};
use crate::{Editor, Selection};

/// upstream `Type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommentType {
    Toggle,
    ForceAdd,
    ForceRemove,
}

/// upstream `ILinePreflightData`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinePreflight {
    pub ignore: bool,
    pub comment_str: String,
    pub comment_str_offset: usize,
    pub comment_str_length: usize,
}

/// upstream `IPreflightData` for a language with line comments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    pub should_remove_comments: bool,
    pub lines: Vec<LinePreflight>,
}

/// upstream `ISimpleModel`: the text of a line, 1-based.
pub trait SimpleModel {
    fn line_content(&self, line: usize) -> String;
}

impl<F: Fn(usize) -> String> SimpleModel for F {
    fn line_content(&self, line: usize) -> String {
        self(line)
    }
}

/// `strings.firstNonWhitespaceIndex`: the UTF-16 index of the first character that is not a space or a tab.
fn first_non_whitespace_index(line: &str) -> Option<usize> {
    let c = first_non_whitespace_column(line);
    (c != 0).then(|| c - 1)
}

/// upstream `BlockCommentCommand._haystackHasNeedleAtOffset`: ASCII case-insensitive.
pub fn haystack_has_needle_at_offset(haystack: &str, needle: &str, offset: usize) -> bool {
    let h: Vec<u16> = haystack.encode_utf16().collect();
    let n: Vec<u16> = needle.encode_utf16().collect();
    if offset + n.len() > h.len() {
        return false;
    }
    n.iter().enumerate().all(|(i, &b)| {
        let a = h[offset + i];
        let lower = |c: u16| if (b'A' as u16..=b'Z' as u16).contains(&c) { c + 32 } else { c };
        a == b || lower(a) == lower(b)
    })
}

/// upstream `LineCommentCommand._analyzeLines`: decides which lines take part and whether the toggle
/// removes or adds comments, and fills in the offsets and lengths of the comment tokens.
#[allow(clippy::too_many_arguments)]
pub fn analyze_lines(
    kind: CommentType,
    insert_space: bool,
    model: &dyn SimpleModel,
    mut lines: Vec<LinePreflight>,
    start_line: usize,
    ignore_empty_lines: bool,
    ignore_first_line: bool,
    line_comment_no_indent: bool,
) -> Preflight {
    let mut only_whitespace_lines = true;
    let mut should_remove_comments = kind != CommentType::ForceAdd;
    for (i, line_data) in lines.iter_mut().enumerate() {
        let line_number = start_line + i;
        if line_number == start_line && ignore_first_line {
            line_data.ignore = true;
            continue;
        }
        let content = model.line_content(line_number);
        let Some(content_start) = first_non_whitespace_index(&content) else {
            // Empty or whitespace only line
            line_data.ignore = ignore_empty_lines;
            line_data.comment_str_offset = if line_comment_no_indent { 0 } else { utf16_len(&content) };
            continue;
        };
        only_whitespace_lines = false;
        let offset = if line_comment_no_indent { 0 } else { content_start };
        line_data.ignore = false;
        line_data.comment_str_offset = offset;
        if should_remove_comments && !haystack_has_needle_at_offset(&content, &line_data.comment_str, offset) {
            match kind {
                // Every line so far has been a line comment, but this one is not.
                CommentType::Toggle => should_remove_comments = false,
                CommentType::ForceAdd => {}
                CommentType::ForceRemove => line_data.ignore = true,
            }
        }
        if should_remove_comments && insert_space {
            // Remove a following space if present.
            let end = content_start + line_data.comment_str_length;
            let units: Vec<u16> = content.encode_utf16().collect();
            if end < units.len() && units[end] == b' ' as u16 {
                line_data.comment_str_length += 1;
            }
        }
    }
    if kind == CommentType::Toggle && only_whitespace_lines {
        // For only whitespace lines, comments are inserted, and no line is ignored.
        should_remove_comments = false;
        for l in &mut lines {
            l.ignore = false;
        }
    }
    Preflight { should_remove_comments, lines }
}

/// upstream `LineCommentCommand._normalizeInsertionPoint`: aligns the insertion points of the
/// lines that are commented, to the shallowest indentation.
pub fn normalize_insertion_point(model: &dyn SimpleModel, lines: &mut [(bool, usize)], start_line: usize, indent_size: usize) {
    let next = |visible: usize, is_tab: bool| if is_tab { visible + (indent_size - visible % indent_size) } else { visible + 1 };
    let mut min_visible = usize::MAX;
    for (i, &(ignore, offset)) in lines.iter().enumerate() {
        if ignore {
            continue;
        }
        let content: Vec<u16> = model.line_content(start_line + i).encode_utf16().collect();
        let mut current = 0;
        let mut j = 0;
        while current < min_visible && j < offset {
            current = next(current, content.get(j) == Some(&(b'\t' as u16)));
            j += 1;
        }
        if current < min_visible {
            min_visible = current;
        }
    }
    min_visible = min_visible / indent_size * indent_size;
    for (i, line) in lines.iter_mut().enumerate() {
        if line.0 {
            continue;
        }
        let content: Vec<u16> = model.line_content(start_line + i).encode_utf16().collect();
        let mut current = 0;
        let mut j = 0;
        while current < min_visible && j < line.1 {
            current = next(current, content.get(j) == Some(&(b'\t' as u16)));
            j += 1;
        }
        line.1 = if current > min_visible { j - 1 } else { j };
    }
}

/// The edits for removing line comments (upstream `_createRemoveLineCommentsOperations`).
fn remove_line_comments_edits(lines: &[LinePreflight], start_line: usize) -> Vec<(Range, String)> {
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.ignore)
        .map(|(i, l)| {
            let line = start_line + i;
            (Range::new(Position::new(line, l.comment_str_offset + 1), Position::new(line, l.comment_str_offset + l.comment_str_length + 1)), String::new())
        })
        .collect()
}

/// The edits for adding line comments (upstream `_createAddLineCommentsOperations`).
fn add_line_comments_edits(lines: &[LinePreflight], start_line: usize, insert_space: bool) -> Vec<(Range, String)> {
    let after = if insert_space { " " } else { "" };
    lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.ignore)
        .map(|(i, l)| (Range::at(Position::new(start_line + i, l.comment_str_offset + 1)), format!("{}{after}", l.comment_str)))
        .collect()
}

fn remove_block_comment_edits(r: Range, start_token: &str, end_token: &str) -> Vec<(Range, String)> {
    let (st, et) = (utf16_len(start_token), utf16_len(end_token));
    if !r.is_empty() {
        vec![
            (Range::new(Position::new(r.start.line, r.start.column - st), r.start), String::new()),
            (Range::new(r.end, Position::new(r.end.line, r.end.column + et)), String::new()),
        ]
    } else {
        vec![(Range::new(Position::new(r.start.line, r.start.column - st), Position::new(r.end.line, r.end.column + et)), String::new())]
    }
}

fn add_block_comment_edits(r: Range, start_token: &str, end_token: &str, insert_space: bool) -> Vec<(Range, String)> {
    let sp = if insert_space { " " } else { "" };
    if !r.is_empty() {
        vec![(Range::at(r.start), format!("{start_token}{sp}")), (Range::at(r.end), format!("{sp}{end_token}"))]
    } else {
        vec![(r, format!("{start_token}  {end_token}"))]
    }
}

/// `String.prototype.lastIndexOf(search, from)` in UTF-16 indices.
fn last_index_of(haystack: &[u16], needle: &[u16], from: isize) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    let max_start = (haystack.len() - needle.len()) as isize;
    let mut i = from.min(max_start);
    while i >= 0 {
        if &haystack[i as usize..i as usize + needle.len()] == needle {
            return Some(i as usize);
        }
        i -= 1;
    }
    None
}

/// `String.prototype.indexOf(search, from)` in UTF-16 indices.
fn index_of(haystack: &[u16], needle: &[u16], from: isize) -> Option<usize> {
    let from = from.max(0) as usize;
    if needle.len() > haystack.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

impl Editor {
    /// The edits applied as one undo step, and the selection set to `selection` afterwards (also the
    /// selection redo restores).
    fn apply_comment_edits(&mut self, edits: &[(Range, String)], selection: impl FnOnce(&Editor, &[Range]) -> Selection) -> bool {
        // The post-edit range of each inserted text: edits sorted by position, offsets shifted by the ones before.
        let mut plan: Vec<(usize, usize, usize)> = edits
            .iter()
            .map(|(r, t)| {
                let s = self.buffer.offset_at(self.buffer.validate_position(r.start));
                let e = self.buffer.offset_at(self.buffer.validate_position(r.end));
                (s, e.max(s), utf16_len(t))
            })
            .collect();
        let order: Vec<usize> = {
            let mut idx: Vec<usize> = (0..plan.len()).collect();
            idx.sort_by_key(|&i| plan[i].0);
            idx
        };
        let mut inverse_offsets = vec![(0usize, 0usize); plan.len()];
        let mut delta: isize = 0;
        for &i in &order {
            let (s, e, n) = plan[i];
            let start = (s as isize + delta) as usize;
            inverse_offsets[i] = (start, start + n);
            delta += n as isize - (e - s) as isize;
        }
        plan.clear();
        if self.replace_ranges(edits) == 0 {
            return false;
        }
        let inverse: Vec<Range> = inverse_offsets.iter().map(|&(a, b)| Range::new(self.buffer.position_at(a), self.buffer.position_at(b))).collect();
        let new_selection = selection(self, &inverse);
        self.selection = new_selection;
        self.amend_last_after(new_selection);
        true
    }

    /// Applies `edits` as one undo step and moves the selection through them like upstream's tracked
    /// selection: a point at an insertion moves after it, except the end of a non-empty selection,
    /// which does not grow; a point inside a removed range goes to its start.
    fn apply_tracking_selection(&mut self, edits: &[(Range, String)]) -> bool {
        let before = self.selection;
        let mut plan: Vec<(usize, usize, usize)> = edits
            .iter()
            .map(|(r, t)| {
                let s = self.buffer.offset_at(self.buffer.validate_position(r.start));
                let e = self.buffer.offset_at(self.buffer.validate_position(r.end));
                (s, e.max(s), utf16_len(t))
            })
            .collect();
        plan.sort_by_key(|p| p.0);
        let (lo, hi) = (before.range().start, before.range().end);
        let empty = lo == hi;
        let lo_off = self.buffer.offset_at(self.buffer.validate_position(lo));
        let hi_off = self.buffer.offset_at(self.buffer.validate_position(hi));
        let track = |p: usize, is_end: bool| -> usize {
            let mut delta: isize = 0;
            for &(s, e, n) in &plan {
                let d = n as isize - (e - s) as isize;
                if s == e && p == s {
                    // An insertion exactly at the point.
                    if is_end {
                        break;
                    }
                    delta += d;
                } else if e <= p {
                    delta += d;
                } else if s < p {
                    // Inside a removed range: the start of what replaced it.
                    return (s as isize + delta) as usize + n;
                } else {
                    break;
                }
            }
            (p as isize + delta).max(0) as usize
        };
        let (new_lo, new_hi) = (track(lo_off, false), track(hi_off, !empty));
        if self.replace_ranges(edits) == 0 {
            return false;
        }
        let (a, b) = if before.is_reversed() { (new_hi, new_lo) } else { (new_lo, new_hi) };
        let selection = Selection { anchor: self.buffer.position_at(a), active: self.buffer.position_at(b) };
        self.selection = selection;
        self.amend_last_after(selection);
        true
    }

    /// Toggle Line Comment (`CommentType::Toggle`, Cmd+/), Add Line Comment (`ForceAdd`) and Remove
    /// Line Comment (`ForceRemove`). Languages without a line comment use their block comment.
    /// Returns whether the text changed.
    pub fn line_comment(&mut self, kind: CommentType, insert_space: bool, ignore_empty_lines: bool) -> bool {
        let Some(support) = self.language.clone() else { return false };
        let sel = self.selection;
        let range = sel.range();
        let (start_line, mut end_line, mut end_column) = (range.start.line, range.end.line, range.end.column);
        if start_line < end_line && end_column == 1 {
            // A selection that ends at the start of a line does not include that line.
            end_line -= 1;
            end_column = self.buffer.line_max_column(end_line);
        }
        let Some(rule) = support.comments().cloned() else { return false };
        match rule.line_comment.filter(|c| !c.is_empty()) {
            Some(comment) => {
                let lines: Vec<LinePreflight> = (start_line..=end_line)
                    .map(|_| LinePreflight { ignore: false, comment_str: comment.clone(), comment_str_offset: 0, comment_str_length: utf16_len(&comment) })
                    .collect();
                let buffer = &self.buffer;
                let model = |l: usize| buffer.line_content(l).to_string();
                let mut data = analyze_lines(kind, insert_space, &model, lines, start_line, ignore_empty_lines, false, rule.line_comment_no_indent);
                let edits = if data.should_remove_comments {
                    remove_line_comments_edits(&data.lines, start_line)
                } else {
                    let mut points: Vec<(bool, usize)> = data.lines.iter().map(|l| (l.ignore, l.comment_str_offset)).collect();
                    normalize_insertion_point(&model, &mut points, start_line, self.indent_size());
                    for (l, p) in data.lines.iter_mut().zip(&points) {
                        l.comment_str_offset = p.1;
                    }
                    add_line_comments_edits(&data.lines, start_line, insert_space)
                };
                self.apply_tracking_selection(&edits)
            }
            None => self.line_command_block_comment(Range::new(Position::new(start_line, range.start.column), Position::new(end_line, end_column)), insert_space, &support),
        }
    }

    /// upstream `LineCommentCommand._executeBlockComment`: the fallback of a line comment command
    /// for a language that only has block comments.
    fn line_command_block_comment(&mut self, s: Range, insert_space: bool, support: &vsr_languages::config::LanguageSupport) -> bool {
        let Some((start_token, end_token)) = support.comments().and_then(|c| c.block_comment.clone()) else { return false };
        let sel = self.selection;
        let (mut edits, mut delta_column) = (None, 0usize);
        if let Some(ops) = self.attempt_remove_block_comment(s, &start_token, &end_token) {
            edits = Some(ops);
        }
        let edits = match edits {
            Some(e) => e,
            None => {
                let ops = if s.is_empty() {
                    let content = self.buffer.line_content(s.start.line);
                    let first = first_non_whitespace_column(content);
                    let first_index = if first == 0 { utf16_len(content) } else { first - 1 };
                    add_block_comment_edits(
                        Range::new(Position::new(s.start.line, first_index + 1), Position::new(s.start.line, utf16_len(content) + 1)),
                        &start_token,
                        &end_token,
                        insert_space,
                    )
                } else {
                    add_block_comment_edits(
                        Range::new(
                            Position::new(s.start.line, first_non_whitespace_column(self.buffer.line_content(s.start.line)).max(1)),
                            Position::new(s.end.line, self.buffer.line_max_column(s.end.line)),
                        ),
                        &start_token,
                        &end_token,
                        insert_space,
                    )
                };
                if ops.len() == 1 {
                    // Leave the cursor after the token and its space.
                    delta_column = utf16_len(&start_token) + 1;
                }
                ops
            }
        };
        if delta_column > 0 {
            let moved = |p: Position| Position::new(p.line, p.column + delta_column);
            let result = Selection { anchor: moved(sel.anchor), active: moved(sel.active) };
            return self.apply_comment_edits(&edits, |_, _| result);
        }
        self.apply_tracking_selection(&edits)
    }

    /// upstream `LineCommentCommand._attemptRemoveBlockComment`.
    fn attempt_remove_block_comment(&self, s: Range, start_token: &str, end_token: &str) -> Option<Vec<(Range, String)>> {
        let (mut start_line, mut end_line) = (s.start.line, s.end.line);
        let st = units(start_token);
        let et = units(end_token);
        let first_non_ws = first_non_whitespace_column(self.buffer.line_content(s.start.line));
        let allowed_before = et.len() + first_non_ws.max(s.start.column);
        let start_content = units(self.buffer.line_content(start_line));
        let end_content = units(self.buffer.line_content(end_line));
        let mut start_index = last_index_of(&start_content, &st, allowed_before as isize - 1);
        let mut end_index = index_of(&end_content, &et, s.end.column as isize - 1 - st.len() as isize);
        if start_index.is_some() && end_index.is_none() {
            end_index = index_of(&start_content, &et, (start_index.unwrap_or(0) + st.len()) as isize);
            end_line = start_line;
        }
        if start_index.is_none() && end_index.is_some() {
            let content = units(self.buffer.line_content(end_line));
            start_index = last_index_of(&content, &st, end_index.unwrap_or(0) as isize);
            start_line = end_line;
        }
        if s.is_empty() && (start_index.is_none() || end_index.is_none()) {
            let content = units(self.buffer.line_content(start_line));
            start_index = index_of(&content, &st, 0);
            if let Some(si) = start_index {
                end_index = index_of(&content, &et, (si + st.len()) as isize);
            }
        }
        let mut start_token = start_token.to_string();
        let mut end_token = end_token.to_string();
        // Possible inner white space: a space after the start token goes with it, one before the end token too.
        if let Some(si) = start_index
            && units(self.buffer.line_content(start_line)).get(si + st.len()) == Some(&(b' ' as u16))
        {
            start_token.push(' ');
        }
        if let Some(ei) = end_index
            && ei > 0
            && units(self.buffer.line_content(end_line)).get(ei - 1) == Some(&(b' ' as u16))
        {
            end_token.insert(0, ' ');
            end_index = Some(ei - 1);
        }
        match (start_index, end_index) {
            (Some(si), Some(ei)) => Some(remove_block_comment_edits(
                Range::new(Position::new(start_line, si + utf16_len(&start_token) + 1), Position::new(end_line, ei + 1)),
                &start_token,
                &end_token,
            )),
            _ => None,
        }
    }

    /// Toggle Block Comment (Shift+Alt+A): wraps the selection in the language's block comment
    /// tokens, or removes them when the selection is inside a block comment. Returns whether the text changed.
    pub fn block_comment(&mut self, insert_space: bool) -> bool {
        let Some(support) = self.language.clone() else { return false };
        let Some((start_token, end_token)) = support.comments().and_then(|c| c.block_comment.clone()) else { return false };
        let sel = self.selection.range();
        let (sl, sc, el, ec) = (sel.start.line, sel.start.column, sel.end.line, sel.end.column);
        let start_text = units(self.buffer.line_content(sl));
        let end_text = units(self.buffer.line_content(el));
        let (st, et) = (units(&start_token), units(&end_token));
        let mut start_index = last_index_of(&start_text, &st, sc as isize - 1 + st.len() as isize);
        let mut end_index = index_of(&end_text, &et, ec as isize - 1 - et.len() as isize);
        if let (Some(si), Some(ei)) = (start_index, end_index) {
            let contains_end = if sl == el {
                let between = &start_text[(si + st.len()).min(ei)..ei];
                index_of(between, &et, 0).is_some()
            } else {
                let after_start = &start_text[si + st.len()..];
                let before_end = &end_text[..ei];
                index_of(after_start, &et, 0).is_some() || index_of(before_end, &et, 0).is_some()
            };
            if contains_end {
                // Force adding a block comment.
                start_index = None;
                end_index = None;
            }
        }
        if let (Some(si), Some(ei)) = (start_index, end_index) {
            let (mut start_token, mut end_token, mut ei) = (start_token.clone(), end_token.clone(), ei);
            if insert_space && si + st.len() < start_text.len() && start_text[si + st.len()] == b' ' as u16 {
                start_token.push(' ');
            }
            if insert_space && ei > 0 && end_text[ei - 1] == b' ' as u16 {
                end_token.insert(0, ' ');
                ei -= 1;
            }
            let edits = remove_block_comment_edits(
                Range::new(Position::new(sl, si + utf16_len(&start_token) + 1), Position::new(el, ei + 1)),
                &start_token,
                &end_token,
            );
            return self.apply_comment_edits(&edits, move |_, inv| {
                // The content between the two removed tokens (a caret where they met).
                match inv {
                    [first, second] => Selection { anchor: first.end, active: second.start },
                    _ => Selection::caret(inv[0].end),
                }
            });
        }
        let edits = add_block_comment_edits(sel, &start_token, &end_token, insert_space);
        let used_end_token = (edits.len() == 1).then(|| utf16_len(&end_token));
        self.apply_comment_edits(&edits, move |_, inv| {
            if inv.len() == 2 {
                Selection { anchor: inv[0].end, active: inv[1].start }
            } else {
                // Between the two spaces: before the end token and one space.
                let src = inv[0];
                let back = used_end_token.map_or(0, |n| n + 1);
                let p = Position::new(src.end.line, src.end.column - back);
                Selection::caret(p)
            }
        })
    }
}
