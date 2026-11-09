//! The find model: matches of the current search in one editor, the current match ("N of M"),
//! next / previous with wrap-around, replace and replace all.
//!
//! upstream: src/vs/editor/contrib/find/browser/findModel.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findState.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findDecorations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/replaceAllCommand.ts @ 50e1350fc78d
//!
//! Differences from upstream, all deliberate:
//! * No decorations. The matches are a sorted `Vec<Range>` recomputed (see [`FindModel::sync`])
//!   whenever the editor's version changes, instead of tracked decorations plus a 100 ms
//!   scheduler. `matchBeforePosition` / `matchAfterPosition` / `getCurrentMatchesPosition` are
//!   ported over that list.
//! * One cursor, so no `selectAllMatches` and a single find scope when searching in the selection.
//!   A scope moves with the model's own replaces but not with edits made elsewhere.
//! * Highlighting stops at [`MATCHES_LIMIT`] matches (the count then reads `19999+`), but every
//!   find operation still works on all matches, like upstream's "only the first N are highlighted".

use vsr_editor::{Editor, Selection};
use vsr_search::query::escape_regexp_characters;
use vsr_text::{Position, Range, TextBuffer};

use crate::replace_pattern::{ReplacePattern, parse_replace_string};
use crate::search::{CompileError, Compiled, FindMatch, SearchParams, compile};

/// `MATCHES_LIMIT`: matches beyond this are not highlighted or counted.
pub const MATCHES_LIMIT: usize = 19999;

/// `FindReplaceState`: what the widget shows and the options of the search.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FindState {
    pub search_string: String,
    pub replace_string: String,
    pub is_regex: bool,
    pub match_case: bool,
    pub whole_word: bool,
    pub preserve_case: bool,
    /// "Find in Selection": only search inside these ranges.
    pub search_scope: Option<Vec<Range>>,
    /// `editor.find.loop`.
    pub loop_: bool,
}

impl Default for FindState {
    fn default() -> Self {
        Self {
            search_string: String::new(),
            replace_string: String::new(),
            is_regex: false,
            match_case: false,
            whole_word: false,
            preserve_case: false,
            search_scope: None,
            loop_: true,
        }
    }
}

impl FindState {
    fn params(&self) -> SearchParams {
        SearchParams {
            search_string: self.search_string.clone(),
            is_regex: self.is_regex,
            match_case: self.match_case,
            whole_word: self.whole_word,
        }
    }
}

/// "N of M" data (`changeMatchInfo`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MatchInfo {
    /// 1-based index of the current match; 0 when the selection is not on a match ("?").
    pub position: usize,
    /// Number of matches, at most [`MATCHES_LIMIT`].
    pub count: usize,
    /// There are more matches than [`MATCHES_LIMIT`].
    pub truncated: bool,
}

#[derive(Debug)]
pub struct FindModel {
    state: FindState,
    compiled: Result<Option<Compiled>, CompileError>,
    /// All matches inside the scope, in document order.
    matches: Vec<Range>,
    /// 1-based position of the current match, 0 when unknown.
    current: usize,
    /// Where "next" starts while typing: the caret when the search began or last moved by the user.
    start_position: Position,
    /// The editor version `matches` are for.
    version: Option<u64>,
    /// The selection after the model's last own move; a different one means the user moved.
    own_selection: Option<Selection>,
    reveal: Option<Range>,
    /// The match navigation last landed on (drawn in the "current match" color); cleared by a new search.
    highlighted: Option<Range>,
    /// `editor.find.cursorMoveOnType`.
    pub cursor_move_on_type: bool,
}

fn compare_start(a: &Range, b: &Range) -> std::cmp::Ordering {
    (a.start, a.end).cmp(&(b.start, b.end))
}

impl FindModel {
    pub fn new(state: FindState, ed: &Editor) -> Self {
        let compiled = compile(&state.params());
        let mut m = Self {
            state,
            compiled,
            matches: Vec::new(),
            current: 0,
            start_position: ed.selection().active,
            version: None,
            own_selection: Some(ed.selection()),
            reveal: None,
            highlighted: None,
            cursor_move_on_type: true,
        };
        m.research_inner(ed);
        m
    }

    pub fn state(&self) -> &FindState {
        &self.state
    }

    /// The regex error to show (red border) for the current search, if any.
    pub fn error(&self) -> Option<&str> {
        self.compiled.as_ref().err().map(|e| e.0.as_str())
    }

    pub fn matches(&self) -> &[Range] {
        &self.matches
    }

    /// The match the selection is on (see [`MatchInfo::position`]).
    pub fn current_match(&self) -> Option<Range> {
        self.current.checked_sub(1).and_then(|i| self.matches.get(i)).copied()
    }

    /// The match navigation last selected, if the text and search are unchanged since.
    pub fn highlighted(&self) -> Option<Range> {
        self.highlighted
    }

    pub fn match_info(&self) -> MatchInfo {
        let count = self.matches.len().min(MATCHES_LIMIT);
        MatchInfo {
            position: if self.current <= MATCHES_LIMIT { self.current } else { 0 },
            count,
            truncated: self.matches.len() >= MATCHES_LIMIT,
        }
    }

    /// "3 of 12", "? of 12" or "No results".
    pub fn label(&self) -> String {
        let i = self.match_info();
        if i.count == 0 {
            return "No results".to_string();
        }
        let pos = if i.position > 0 { i.position.to_string() } else { "?".to_string() };
        format!("{pos} of {}{}", i.count, if i.truncated { "+" } else { "" })
    }

    /// The range the view should scroll into view (set by every move); cleared by the call.
    pub fn take_reveal(&mut self) -> Option<Range> {
        self.reveal.take()
    }

    /// The replace string changed: no new search is needed.
    pub fn set_replace_string(&mut self, s: String) {
        self.state.replace_string = s;
    }

    /// `editor.find.loop`.
    pub fn set_loop(&mut self, loop_: bool) {
        self.state.loop_ = loop_;
    }

    pub fn start_position(&self) -> Position {
        self.start_position
    }

    pub fn set_start_position(&mut self, p: Position) {
        self.start_position = p;
    }

    // ---- state ---------------------------------------------------------------------------

    /// Changes the state (`FindReplaceState.change`). A new search string, option or scope
    /// searches again; `move_cursor` then selects the next match from the start position when
    /// `editor.find.cursorMoveOnType` is on (typing in the box).
    pub fn change(&mut self, ed: &mut Editor, move_cursor: bool, f: impl FnOnce(&mut FindState)) {
        self.sync(ed);
        let before = self.state.clone();
        f(&mut self.state);
        let research = self.state.search_string != before.search_string
            || self.state.is_regex != before.is_regex
            || self.state.whole_word != before.whole_word
            || self.state.match_case != before.match_case
            || self.state.search_scope != before.search_scope;
        if research {
            self.compiled = compile(&self.state.params());
            self.research(ed, move_cursor);
        }
    }

    /// Call when the editor may have changed (every frame is fine): re-searches if the text
    /// changed and keeps the start position in step with caret moves the user made.
    pub fn sync(&mut self, ed: &Editor) {
        let sel = ed.selection();
        if self.own_selection != Some(sel) {
            self.own_selection = Some(sel);
            self.start_position = sel.active;
            self.current = self.position_for(sel.range());
        }
        if self.version != Some(ed.version()) {
            self.start_position = sel.active;
            self.research_inner(ed);
        }
    }

    // ---- searching -----------------------------------------------------------------------

    /// The find scopes as searched: a scope over several lines covers whole lines, and ends at the
    /// end of the previous line when it ends at column 1.
    pub fn scopes(&self, buffer: &TextBuffer) -> Option<Vec<Range>> {
        let scopes = self.state.search_scope.as_ref()?;
        Some(
            scopes
                .iter()
                .map(|s| {
                    let s = Range::new(buffer.validate_position(s.start), buffer.validate_position(s.end));
                    if s.start.line != s.end.line {
                        let end_line = if s.end.column == 1 { s.end.line - 1 } else { s.end.line };
                        Range::new(Position::new(s.start.line, 1), Position::new(end_line, buffer.line_max_column(end_line)))
                    } else {
                        s
                    }
                })
                .collect(),
        )
    }

    fn research_inner(&mut self, ed: &Editor) {
        let scopes = self.scopes(ed.buffer());
        self.matches = match &self.compiled {
            Ok(Some(c)) => c
                .find_matches(ed.buffer(), scopes.as_deref(), false, usize::MAX)
                .into_iter()
                .map(|m| m.range)
                .collect(),
            _ => Vec::new(),
        };
        self.version = Some(ed.version());
        self.highlighted = None;
        self.current = self.position_for(ed.selection().range());
    }

    /// `research`: search again; optionally jump to the next match from the start position.
    pub fn research(&mut self, ed: &mut Editor, move_cursor: bool) {
        self.research_inner(ed);
        if move_cursor && self.cursor_move_on_type {
            self.move_to_next_from(ed, self.start_position);
        }
    }

    /// `getCurrentMatchesPosition` plus the "nearest before the selection" fallback of `research`.
    fn position_for(&self, sel: Range) -> usize {
        let idx = self.matches.partition_point(|m| m.end < sel.start);
        if self.matches.get(idx).is_some_and(|m| m.start <= sel.end) {
            return idx + 1;
        }
        let after = self.matches.partition_point(|m| compare_start(m, &sel).is_lt());
        if !self.matches.is_empty() && after > 0 { after } else { 0 }
    }

    // ---- navigation ----------------------------------------------------------------------

    fn can_navigate_in_loop(&self) -> bool {
        self.state.loop_ || self.matches.len() >= MATCHES_LIMIT
    }

    pub fn can_navigate_back(&self) -> bool {
        self.can_navigate_in_loop() || self.current != 1
    }

    pub fn can_navigate_forward(&self) -> bool {
        self.can_navigate_in_loop() || self.current < self.match_info().count
    }

    /// `matchAfterPosition`: the first match starting at or after `p`, else the first match.
    fn match_after(&self, p: Position) -> Option<Range> {
        let i = self.matches.partition_point(|m| m.start < p);
        self.matches.get(i).or_else(|| self.matches.first()).copied()
    }

    /// `matchBeforePosition`: the last match ending at or before `p`, else the last match.
    fn match_before(&self, p: Position) -> Option<Range> {
        let i = self.matches.partition_point(|m| m.end <= p);
        if i > 0 { self.matches.get(i - 1) } else { self.matches.last() }.copied()
    }

    fn uses_line_stops(&self) -> bool {
        self.state.is_regex && (self.state.search_string.contains('^') || self.state.search_string.contains('$'))
    }

    fn next_search_position(&self, after: Position, b: &TextBuffer) -> Position {
        let Position { mut line, mut column } = after;
        if self.uses_line_stops() || column == b.line_max_column(line) {
            line = if line == b.line_count() { 1 } else { line + 1 };
            column = 1;
        } else {
            column += 1;
        }
        Position::new(line, column)
    }

    fn prev_search_position(&self, before: Position, b: &TextBuffer) -> Position {
        let Position { mut line, mut column } = before;
        if self.uses_line_stops() || column == 1 {
            line = if line == 1 { b.line_count() } else { line - 1 };
            column = b.line_max_column(line);
        } else {
            column -= 1;
        }
        Position::new(line, column)
    }

    /// Selects `m` as the current match and asks the view to reveal it (`_setCurrentFindMatch`).
    fn set_current_match(&mut self, ed: &mut Editor, m: Range) {
        self.current = self.matches.iter().position(|x| *x == m).map_or(0, |i| i + 1);
        ed.set_selection(Selection { anchor: m.start, active: m.end });
        self.own_selection = Some(ed.selection());
        self.highlighted = Some(m);
        self.reveal = Some(m);
    }

    fn move_to_next_from(&mut self, ed: &mut Editor, mut after: Position) {
        if !self.can_navigate_forward() {
            // Beyond the last match: reselect it instead of doing nothing.
            if let Some(m) = self.match_before(after) {
                self.set_current_match(ed, m);
            }
            return;
        }
        let mut next = self.match_after(after);
        if let Some(n) = next
            && n.is_empty()
            && n.start == after
        {
            after = self.next_search_position(after, ed.buffer());
            next = self.match_after(after);
        }
        if let Some(n) = next {
            self.set_current_match(ed, n);
        }
    }

    fn move_to_prev_from(&mut self, ed: &mut Editor, mut before: Position) {
        if !self.can_navigate_back() {
            if let Some(m) = self.match_after(before) {
                self.set_current_match(ed, m);
            }
            return;
        }
        let mut prev = self.match_before(before);
        if let Some(p) = prev
            && p.is_empty()
            && p.start == before
        {
            before = self.prev_search_position(before, ed.buffer());
            prev = self.match_before(before);
        }
        if let Some(p) = prev {
            self.set_current_match(ed, p);
        }
    }

    /// Next match after the selection's end. Returns whether the selection changed.
    pub fn move_to_next_match(&mut self, ed: &mut Editor) -> bool {
        self.sync(ed);
        let before = ed.selection();
        self.move_to_next_from(ed, before.range().end);
        ed.selection() != before
    }

    /// Previous match before the selection's start. Returns whether the selection changed.
    pub fn move_to_prev_match(&mut self, ed: &mut Editor) -> bool {
        self.sync(ed);
        let before = ed.selection();
        self.move_to_prev_from(ed, before.range().start);
        ed.selection() != before
    }

    // ---- replace -------------------------------------------------------------------------

    fn replace_pattern(&self) -> ReplacePattern {
        if self.state.is_regex {
            parse_replace_string(&self.state.replace_string)
        } else {
            ReplacePattern::from_static_value(&self.state.replace_string)
        }
    }

    /// All matches with capture groups inside `lines` (whole lines), for the replace string.
    fn matches_with_captures(&self, buffer: &TextBuffer, ranges: Option<&[Range]>) -> Vec<FindMatch> {
        match &self.compiled {
            Ok(Some(c)) => c.find_matches(buffer, ranges, true, usize::MAX),
            _ => Vec::new(),
        }
    }

    /// `replace`: if the selection is on a match, replace it and move to the next one; otherwise
    /// select the next match.
    pub fn replace(&mut self, ed: &mut Editor) {
        self.sync(ed);
        if self.matches.is_empty() {
            return;
        }
        let pattern = self.replace_pattern();
        let selection = ed.selection();
        let sel = selection.range();
        let Some(next) = self.match_after(sel.start) else { return };
        if sel != next {
            self.start_position = ed.selection().active;
            self.set_current_match(ed, next);
            return;
        }
        // The selection sits on a match: replace it.
        let b = ed.buffer();
        let span = Range::new(Position::new(sel.start.line, 1), Position::new(sel.end.line, b.line_max_column(sel.end.line)));
        let found = self.matches_with_captures(b, Some(&[span])).into_iter().find(|m| m.range == sel);
        let replacement = pattern.build_replace_string(found.as_ref().and_then(|m| m.matches.as_deref()), self.state.preserve_case);
        let start_offset = b.offset_at(sel.start);
        let end_offset = b.offset_at(sel.end);
        let len_before = b.len_utf16();
        let scope = self.state.search_scope.clone();
        let (_, moved) = ed.replace_ranges_tracking(&[(sel, replacement)], scope.as_deref().unwrap_or(&[]));
        if scope.is_some() {
            self.state.search_scope = Some(moved);
        }
        let inserted = (ed.buffer().len_utf16() + (end_offset - start_offset)).saturating_sub(len_before);
        let caret = ed.buffer().position_at(start_offset + inserted);
        ed.set_selection(Selection::caret(caret));
        self.own_selection = Some(ed.selection());
        self.start_position = caret;
        self.research(ed, true);
    }

    /// `replaceAll`: every match (not just the highlighted ones), as one undo step.
    pub fn replace_all(&mut self, ed: &mut Editor) {
        self.sync(ed);
        if self.matches.is_empty() {
            return;
        }
        let pattern = self.replace_pattern();
        let capture = pattern.has_replacement_patterns() || self.state.preserve_case;
        let scopes = self.scopes(ed.buffer());
        let found = match &self.compiled {
            Ok(Some(c)) => c.find_matches(ed.buffer(), scopes.as_deref(), capture, usize::MAX),
            _ => return,
        };
        let edits: Vec<(Range, String)> = found
            .iter()
            .map(|m| (m.range, pattern.build_replace_string(m.matches.as_deref(), self.state.preserve_case)))
            .collect();
        let tracked = self.state.search_scope.clone().unwrap_or_default();
        let (_, moved) = ed.replace_ranges_tracking(&edits, &tracked);
        if self.state.search_scope.is_some() {
            self.state.search_scope = Some(moved);
        }
        self.own_selection = Some(ed.selection());
        self.research_inner(ed);
    }
}

// ---- starting a search ---------------------------------------------------------------------

/// `editor.find.seedSearchStringFromSelection`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SeedMode {
    Never,
    /// The selection, or the word at the caret.
    Always,
    /// Only a non-empty selection.
    Selection,
}

/// `getSelectionSearchString` for a single-line selection: the selected text, or (when
/// `non_empty_only` is false) the word at the caret. A selection over several lines seeds nothing.
pub fn selection_search_string(ed: &Editor, non_empty_only: bool) -> Option<String> {
    let sel = ed.selection();
    if sel.anchor.line != sel.active.line {
        return None;
    }
    if sel.is_empty() {
        if non_empty_only {
            return None;
        }
        let word = ed.word_at(sel.active)?;
        return Some(ed.buffer().value_in_range(word));
    }
    let text = ed.selected_text();
    (text.len() < 524_288).then_some(text)
}

/// The search string to start with: `seed` per the setting, escaped when `is_regex`.
pub fn seed_search_string(ed: &Editor, mode: SeedMode, is_regex: bool) -> Option<String> {
    let s = match mode {
        SeedMode::Never => return None,
        SeedMode::Always => selection_search_string(ed, false)?,
        SeedMode::Selection => selection_search_string(ed, true)?,
    };
    (!s.is_empty()).then(|| if is_regex { escape_regexp_characters(&s) } else { s })
}

/// `toggleSearchScope`: the scope for "Find in Selection": the non-empty selection, ending on the
/// previous line when it ends at column 1.
pub fn scope_from_selection(ed: &Editor) -> Option<Vec<Range>> {
    let mut r = ed.selection().range();
    if r.end.column == 1 && r.end.line > r.start.line {
        let line = r.end.line - 1;
        r = Range::new(r.start, Position::new(line, ed.buffer().line_max_column(line)));
    }
    (!r.is_empty()).then(|| vec![r])
}

#[cfg(test)]
mod tests {
    use vsr_editor::EditorOptions;
    use vsr_text::DefaultEol;

    use super::*;

    fn editor(text: &str) -> Editor {
        Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true })
    }

    fn at(l: usize, c: usize) -> Position {
        Position::new(l, c)
    }

    #[test]
    fn label_reads_n_of_m_question_mark_and_no_results() {
        let mut ed = editor("- foo bar foo\nfoo");
        let mut m = FindModel::new(FindState::default(), &ed);
        assert_eq!(m.label(), "No results", "empty search");
        m.change(&mut ed, false, |s| s.search_string = "foo".into());
        assert_eq!(m.label(), "? of 3", "caret before the first match, none is current");
        m.move_to_next_match(&mut ed);
        assert_eq!(m.label(), "1 of 3");
        m.move_to_prev_match(&mut ed);
        assert_eq!(m.label(), "3 of 3", "wraps");
        m.change(&mut ed, false, |s| s.search_string = "zzz".into());
        assert_eq!(m.label(), "No results");
    }

    #[test]
    fn matches_update_when_the_text_changes() {
        let mut ed = editor("a a");
        let mut m = FindModel::new(FindState { search_string: "a".into(), ..FindState::default() }, &ed);
        assert_eq!(m.matches().len(), 2);
        ed.set_cursor(at(1, 4), false);
        ed.type_text("a");
        m.sync(&ed);
        assert_eq!(m.matches().len(), 3, "typing in the editor adds a match");
        assert!(m.highlighted().is_none());
    }

    #[test]
    fn invalid_regex_has_an_error_and_no_matches() {
        let mut ed = editor("a(b");
        let mut m = FindModel::new(FindState::default(), &ed);
        m.change(&mut ed, false, |s| (s.search_string, s.is_regex) = ("(".into(), true));
        assert!(m.error().is_some());
        assert!(m.matches().is_empty());
        m.change(&mut ed, false, |s| s.is_regex = false);
        assert!(m.error().is_none());
        assert_eq!(m.matches().len(), 1);
    }

    #[test]
    fn cursor_move_on_type_off_keeps_the_caret() {
        let mut ed = editor("x foo");
        let mut m = FindModel::new(FindState::default(), &ed);
        m.cursor_move_on_type = false;
        m.change(&mut ed, true, |s| s.search_string = "foo".into());
        assert_eq!(ed.selection().active, at(1, 1));
        m.cursor_move_on_type = true;
        m.change(&mut ed, true, |s| s.search_string = "fo".into());
        assert_eq!(ed.selection().range(), Range::new(at(1, 3), at(1, 5)));
    }

    #[test]
    fn regex_replace_uses_capture_groups_and_case_ops() {
        let mut ed = editor("john smith\nada lovelace");
        let mut m = FindModel::new(
            FindState {
                search_string: r"(\w+) (\w+)".into(),
                replace_string: r"\u$2, \u$1".into(),
                is_regex: true,
                ..FindState::default()
            },
            &ed,
        );
        m.replace_all(&mut ed);
        assert_eq!(ed.buffer().value(), "Smith, John\nLovelace, Ada");
        assert!(ed.undo());
        assert_eq!(ed.buffer().value(), "john smith\nada lovelace");
    }

    #[test]
    fn replace_moves_the_scope_with_the_text() {
        let mut ed = editor("a a a\na a");
        let mut m = FindModel::new(
            FindState {
                search_string: "a".into(),
                replace_string: "bbb".into(),
                search_scope: Some(vec![Range::new(at(1, 1), at(1, 6))]),
                ..FindState::default()
            },
            &ed,
        );
        assert_eq!(m.matches().len(), 3, "only line 1");
        m.replace_all(&mut ed);
        assert_eq!(ed.buffer().value(), "bbb bbb bbb\na a");
        assert_eq!(m.state().search_scope, Some(vec![Range::new(at(1, 1), at(1, 12))]));
    }

    #[test]
    fn seeding_from_selection_or_word() {
        let mut ed = editor("foo.bar baz\nqux");
        ed.set_cursor(at(1, 6), false); // inside "bar"
        assert_eq!(seed_search_string(&ed, SeedMode::Always, false).as_deref(), Some("bar"));
        assert_eq!(seed_search_string(&ed, SeedMode::Selection, false), None, "no selection");
        assert_eq!(seed_search_string(&ed, SeedMode::Never, false), None);
        ed.set_cursor(at(1, 1), false);
        ed.set_cursor(at(1, 8), true);
        assert_eq!(seed_search_string(&ed, SeedMode::Selection, false).as_deref(), Some("foo.bar"));
        assert_eq!(seed_search_string(&ed, SeedMode::Selection, true).as_deref(), Some("foo\\.bar"), "escaped for regex");
        ed.set_cursor(at(2, 2), true); // spans two lines
        assert_eq!(seed_search_string(&ed, SeedMode::Always, false), None);
    }

    #[test]
    fn scope_from_selection_ends_on_the_previous_line_at_column_one() {
        let mut ed = editor("ab\ncd\nef");
        assert_eq!(scope_from_selection(&ed), None, "a caret is no scope");
        ed.set_cursor(at(1, 2), false);
        ed.set_cursor(at(3, 1), true);
        assert_eq!(scope_from_selection(&ed), Some(vec![Range::new(at(1, 2), at(2, 3))]));
    }
}
