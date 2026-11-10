//! Typing with the language's rules: auto closing, over typing, surrounding a selection,
//! auto indentation and electric characters.
//!
//! upstream: src/vs/editor/common/cursor/cursorTypeOperations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/cursor/cursorTypeEditOperations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/cursorCommon.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/cursor/cursor.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/commands/surroundSelectionCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/commands/shiftCommand.ts @ 50e1350fc78d
//!
//! `Editor::type_text` is upstream's `typeWithInterceptors`, one character at a time; the
//! language comes from `Editor::set_language_support` and the options from `set_typing_options`.
//! Tokens (to keep pairs out of strings and comments) come from a `TokenSource` the caller passes.
//!
//! Simplifications, deliberate:
//! - One cursor. Upstream's auto closed ranges are tracked decorations; here they are line
//!   ranges mapped through single-line edits and dropped by any other edit or by the caret leaving
//!   them (`AutoClosed`).
//! - A multi-character auto closing pair (`/**` closing `*/`, `begin` ...) is one edit and one undo
//!   step; upstream makes an undo stop between the typed text and the auto closed text.
//! - `getTokenTypeIfInsertingCharacter` is `TokenSource::token_type_if_inserting`; a source that
//!   cannot tell leaves the decision to the token before the caret.
//! - `hasUnmatchedClosingBracketAfter` counts brackets of the pair forward in the document (at most
//!   2000 lines), skipping string and comment tokens, instead of using the bracket pair model.
//! - Composition (IME) bypasses all of this: the app uses `type_raw`.

use std::sync::Arc;

use vsr_languages::auto_indent::{self, IndentConverter, IndentModel, TextRange};
use vsr_languages::character_pair::{AutoClosingPairs, StandardAutoClosingPair};
pub use vsr_languages::config::{AutoIndentStrategy, LineTokens, StandardTokenType};
use vsr_languages::config::{IndentAction, LanguageSupport};
use vsr_text::{Position, Range, TextBuffer};

use crate::Editor;
use crate::Selection;
use crate::cursor_columns::{first_non_whitespace_column, normalize_indentation, prev_tab_stop, utf16_len, visible_column};
use crate::undo::Kind;
use crate::word_ops::{Class, class_of};

/// `editor.autoClosingBrackets`, `autoClosingQuotes`, `autoClosingComments` (upstream `EditorAutoClosingStrategy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoClosing {
    Always,
    LanguageDefined,
    BeforeWhitespace,
    Never,
}

/// `editor.autoClosingDelete`, `autoClosingOvertype` (upstream `EditorAutoClosingEditStrategy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoClosingEdit {
    Always,
    Auto,
    Never,
}

/// `editor.autoSurround` (upstream `EditorAutoSurroundStrategy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoSurround {
    LanguageDefined,
    Quotes,
    Brackets,
    Never,
}

/// The `editor.*` options typing and Enter read. `Default` is upstream's defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TypingOptions {
    pub auto_closing_brackets: AutoClosing,
    pub auto_closing_quotes: AutoClosing,
    pub auto_closing_comments: AutoClosing,
    pub auto_closing_delete: AutoClosingEdit,
    pub auto_closing_overtype: AutoClosingEdit,
    pub auto_surround: AutoSurround,
    pub auto_indent: AutoIndentStrategy,
    /// `editor.useTabStops`.
    pub use_tab_stops: bool,
    /// `editor.trimWhitespaceOnDelete`.
    pub trim_whitespace_on_delete: bool,
    /// `editor.trimAutoWhitespace`.
    pub trim_auto_whitespace: bool,
    /// `editor.indentSize` (`None`: `editor.tabSize`).
    pub indent_size: Option<usize>,
}

impl Default for TypingOptions {
    fn default() -> Self {
        TypingOptions {
            auto_closing_brackets: AutoClosing::LanguageDefined,
            auto_closing_quotes: AutoClosing::LanguageDefined,
            auto_closing_comments: AutoClosing::LanguageDefined,
            auto_closing_delete: AutoClosingEdit::Auto,
            auto_closing_overtype: AutoClosingEdit::Auto,
            auto_surround: AutoSurround::LanguageDefined,
            auto_indent: AutoIndentStrategy::Full,
            use_tab_stops: true,
            trim_whitespace_on_delete: false,
            trim_auto_whitespace: true,
            indent_size: None,
        }
    }
}

/// Where the tokens of a line come from. `None` means the line is not tokenized yet (upstream's
/// `isCheapToTokenize` is false): auto closing and auto indentation then leave the line alone.
pub trait TokenSource {
    fn line_tokens(&self, buffer: &TextBuffer, line: usize) -> Option<LineTokens>;

    /// The type of the token a `ch` inserted at 1-based `column` of `line` would be in (upstream
    /// `getTokenTypeIfInsertingCharacter`), when the tokenizer can tell. A quote typed after
    /// the end of an interpolation, for example, closes the string it is in.
    fn token_type_if_inserting(&self, _buffer: &TextBuffer, _line: usize, _column: usize, _ch: char) -> Option<StandardTokenType> {
        None
    }
}

/// No tokenizer: every line is one `Other` token.
pub struct PlainTokens;

impl TokenSource for PlainTokens {
    fn line_tokens(&self, buffer: &TextBuffer, line: usize) -> Option<LineTokens> {
        Some(LineTokens::plain(utf16_len(buffer.line_content(line))))
    }
}

/// One auto closed pair that can still be over typed or deleted (upstream `AutoClosedAction`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AutoClosed {
    line: usize,
    /// The columns of the open and close text together: `start..end`, end exclusive.
    start: usize,
    end: usize,
    /// The column of the first auto closed character.
    close_start: usize,
}

pub(crate) fn is_quote(ch: char) -> bool {
    ch == '\'' || ch == '"' || ch == '`'
}

/// Lines and tokens of the buffer for the language rules.
struct BufferModel<'a> {
    buffer: &'a TextBuffer,
    tokens: &'a dyn TokenSource,
}

impl IndentModel for BufferModel<'_> {
    fn line_content(&self, line: usize) -> String {
        if line == 0 || line > self.buffer.line_count() { String::new() } else { self.buffer.line_content(line).to_string() }
    }

    fn line_tokens(&self, line: usize) -> LineTokens {
        let text = self.line_content(line);
        self.tokens.line_tokens(self.buffer, line).unwrap_or_else(|| LineTokens::plain(utf16_len(&text)))
    }
}

struct Converter {
    tab_size: usize,
    indent_size: usize,
    insert_spaces: bool,
}

impl IndentConverter for Converter {
    fn shift_indent(&self, indentation: &str) -> String {
        let visible = visible_column(indentation, utf16_len(indentation) + 1, self.tab_size);
        if self.insert_spaces {
            " ".repeat(visible + self.indent_size - visible % self.indent_size)
        } else {
            "\t".repeat((visible + self.tab_size - visible % self.tab_size) / self.tab_size)
        }
    }

    fn unshift_indent(&self, indentation: &str) -> String {
        let visible = visible_column(indentation, utf16_len(indentation) + 1, self.tab_size);
        if self.insert_spaces {
            " ".repeat(prev_tab_stop(visible, self.indent_size))
        } else {
            "\t".repeat(prev_tab_stop(visible, self.tab_size) / self.tab_size)
        }
    }

    fn normalize_indentation(&self, indentation: &str) -> String {
        normalize_indentation(indentation, self.tab_size, self.insert_spaces)
    }
}

/// The part of `s` before UTF-16 offset `units`.
fn prefix_utf16(s: &str, units: usize) -> &str {
    let mut n = 0;
    for (i, c) in s.char_indices() {
        if n >= units {
            return &s[..i];
        }
        n += c.len_utf16();
    }
    s
}

/// The part of `s` from UTF-16 offset `units`.
fn suffix_utf16(s: &str, units: usize) -> &str {
    &s[prefix_utf16(s, units).len()..]
}

impl Editor {
    /// Sets the options typing, Enter and Backspace read (`editor.autoClosing*`, `autoSurround`,
    /// `autoIndent`, `useTabStops`, `trimAutoWhitespace`, `indentSize` ...).
    pub fn set_typing_options(&mut self, options: TypingOptions) {
        self.typing = options;
        if !options.trim_auto_whitespace {
            self.trim_lines.clear();
        }
    }

    pub fn typing_options(&self) -> TypingOptions {
        self.typing
    }

    /// Sets the language rules (pairs, comments, on-enter and indentation rules). `None`: plain
    /// editing, as before a language is known.
    pub fn set_language_support(&mut self, support: Option<Arc<LanguageSupport>>) {
        self.language = support;
        self.auto_closed.clear();
    }

    pub fn language_support(&self) -> Option<&Arc<LanguageSupport>> {
        self.language.as_ref()
    }

    /// `editor.indentSize`, or the tab size.
    pub fn indent_size(&self) -> usize {
        self.typing.indent_size.unwrap_or(self.options.tab_size).max(1)
    }

    fn converter(&self) -> Converter {
        Converter { tab_size: self.options.tab_size, indent_size: self.indent_size(), insert_spaces: self.options.insert_spaces }
    }

    /// Indents `indentation` by one level (upstream `TypeOperations.shiftIndent`).
    pub fn shift_indent(&self, indentation: &str) -> String {
        self.converter().shift_indent(indentation)
    }

    /// Outdents `indentation` by one level.
    pub fn unshift_indent(&self, indentation: &str) -> String {
        self.converter().unshift_indent(indentation)
    }

    pub(crate) fn normalize_indent(&self, indentation: &str) -> String {
        normalize_indentation(indentation, self.options.tab_size, self.options.insert_spaces)
    }

    /// Types `text` the way the keyboard does: each character through the auto closing, over typing,
    /// surround, auto indent and electric character rules, then as plain text. Does not use tokens.
    pub fn type_text(&mut self, text: &str) {
        self.type_text_with(text, &PlainTokens);
    }

    /// Like `type_text`, with the tokens of the lines (to keep pairs out of strings and comments).
    pub fn type_text_with(&mut self, text: &str, tokens: &dyn TokenSource) {
        for ch in text.chars() {
            if ch == '\n' {
                self.enter_with(tokens);
            } else if ch != '\r' {
                self.type_char(ch, tokens);
            }
        }
    }

    /// Replaces the selection with `text` as it is, with no language rules (composition, API use).
    pub fn type_raw(&mut self, text: &str) {
        let range = self.selection.range();
        let kind = if range.is_empty() { Kind::Typing } else { Kind::Other };
        self.apply_edit(range, text, kind);
    }

    fn model<'a>(&'a self, tokens: &'a dyn TokenSource) -> BufferModel<'a> {
        BufferModel { buffer: &self.buffer, tokens }
    }

    fn type_char(&mut self, ch: char, tokens: &dyn TokenSource) {
        let sel = self.selection;
        let mut buf = [0u8; 4];
        let s: &str = ch.encode_utf8(&mut buf);
        if let Some(support) = self.language.clone() {
            if self.auto_indent_type(&support, ch, tokens) {
                return;
            }
            if self.is_auto_closing_overtype(&support, ch) {
                let p = sel.active;
                self.apply_edit_full(Range::new(p, Position::new(p.line, p.column + 1)), s, Kind::Typing, None, false);
                return;
            }
            if let Some((open, close)) = self.get_auto_closing_pair(&support, ch, tokens) {
                self.type_auto_closing(&open, &close, ch);
                return;
            }
            if self.surround_selection(&support, ch) {
                return;
            }
            if self.electric_char(&support, ch, tokens) {
                return;
            }
        }
        self.type_raw(s);
    }

    // ---- auto closing ------------------------------------------------------------------------

    fn auto_closing_strategy(&self, ch: char, pair_open: &str, block_comment_start: Option<&str>) -> (AutoClosing, Kind3) {
        if is_quote(ch) {
            (self.typing.auto_closing_quotes, Kind3::Quote)
        } else if block_comment_start.is_some_and(|b| pair_open.contains(b)) {
            (self.typing.auto_closing_comments, Kind3::Comment)
        } else {
            (self.typing.auto_closing_brackets, Kind3::Bracket)
        }
    }

    fn should_auto_close_before(&self, support: &LanguageSupport, strategy: AutoClosing, kind: Kind3, next: char) -> bool {
        match strategy {
            AutoClosing::BeforeWhitespace => next == ' ' || next == '\t',
            AutoClosing::LanguageDefined => support.character_pair.auto_close_before_set(kind == Kind3::Quote).contains(next),
            AutoClosing::Always => true,
            AutoClosing::Never => false,
        }
    }

    /// upstream `AutoClosingOpenCharTypeOperation.getAutoClosingPair`, for one caret and `ch` not typed yet.
    fn get_auto_closing_pair(&self, support: &LanguageSupport, ch: char, tokens: &dyn TokenSource) -> Option<(String, String)> {
        if !self.selection.is_empty() {
            return None;
        }
        let pos = self.selection.active;
        let lookup = support.character_pair.auto_closing_pair_lookup();
        let line_text = self.buffer.line_content(pos.line);
        let line_before = prefix_utf16(line_text, pos.column - 1);
        let line_after = suffix_utf16(line_text, pos.column - 1);
        let mut s = [0u8; 4];
        let ch_str: &str = ch.encode_utf8(&mut s);
        // The longest open text that ends with `ch` and matches the text before the caret.
        let mut pair: Option<&StandardAutoClosingPair> = None;
        for candidate in lookup.open_by_end(ch) {
            if pair.is_some_and(|p| candidate.open.len() <= p.open.len()) {
                continue;
            }
            let open_before = candidate.open.strip_suffix(ch_str).unwrap_or("");
            if !candidate.open.ends_with(ch_str) || !line_before.ends_with(open_before) {
                continue;
            }
            if candidate.before_text.is_some() && !candidate.should_auto_close_before(&format!("{line_before}{ch}")) {
                continue;
            }
            pair = Some(candidate);
        }
        let pair = pair?;
        let block_start = support.comments().and_then(|c| c.block_comment.as_ref()).map(|b| b.0.as_str());
        let (strategy, kind) = self.auto_closing_strategy(ch, &pair.open, block_start);
        if strategy == AutoClosing::Never {
            return None;
        }
        // Two pairs where one contains the other, such as `(` and `(*`: typing `*` gives `(*|*)`, not `(*|*))`.
        let contained = find_contained_pair(&lookup, pair);
        let contained_close = contained.map_or("", |c| c.close.as_str());
        let is_contained_present = line_after.starts_with(contained_close);
        // Only auto close when an allowed character follows or another auto closed closing brace does.
        if let Some(next) = line_after.chars().next()
            && !is_before_closing_brace(&lookup, line_after)
            && !self.should_auto_close_before(support, strategy, kind, next)
        {
            return None;
        }
        if kind == Kind3::Bracket && strategy != AutoClosing::Always && self.has_unmatched_closing_bracket_after(support, pos, pair, tokens) {
            return None;
        }
        // Do not auto close ' or " after a word character.
        if pair.open.chars().count() == 1 && (ch == '\'' || ch == '"') && strategy != AutoClosing::Always
            && let Some(before) = line_before.chars().last()
            && class_of(&self.word_separators, before) == Class::Word
        {
            return None;
        }
        let line_tokens = tokens.line_tokens(&self.buffer, pos.line)?;
        if !pair.should_auto_close(&line_tokens, pos.column) {
            return None;
        }
        // Typing a quote could end a string instead of starting one. Ask what a neutral character
        // at this position would be: in a string, the quote would most likely end it.
        if let Some(neutral) = pair.find_neutral_character()
            && let Some(ty) = tokens.token_type_if_inserting(&self.buffer, pos.line, pos.column, neutral)
            && !pair.is_ok(ty)
        {
            return None;
        }
        let close = if is_contained_present { &pair.close[..pair.close.len() - contained_close.len()] } else { pair.close.as_str() };
        Some((pair.open.clone(), close.to_string()))
    }

    /// Whether a closing bracket of `pair` follows the caret that no open bracket matches
    /// (upstream asks the bracket pair model). Counts the brackets of this pair, 2000 lines
    /// either way, skipping string, comment and regex tokens.
    fn has_unmatched_closing_bracket_after(&self, support: &LanguageSupport, pos: Position, pair: &StandardAutoClosingPair, tokens: &dyn TokenSource) -> bool {
        // The bracket pair of the language that has this open text.
        let Some((open, close)) = support.brackets().iter().find(|(o, _)| *o == pair.open) else {
            return false;
        };
        let mut depth = 0usize;
        let first = pos.line.saturating_sub(2000).max(1);
        let last = self.buffer.line_count().min(pos.line + 2000);
        for line in first..=last {
            let text = self.buffer.line_content(line);
            let line_tokens = tokens.line_tokens(&self.buffer, line).unwrap_or_else(|| LineTokens::plain(utf16_len(text)));
            let mut i = 0;
            while i < text.len() {
                let offset = utf16_len(&text[..i]);
                let rest = &text[i..];
                let ignored = line_tokens.type_at_offset(offset as isize) != StandardTokenType::Other;
                if !ignored {
                    if rest.starts_with(open.as_str()) {
                        depth += 1;
                        i += open.len().max(1);
                        continue;
                    }
                    if rest.starts_with(close.as_str()) {
                        if depth > 0 {
                            depth -= 1;
                        } else if (line, offset + 1) >= (pos.line, pos.column) {
                            return true;
                        }
                        i += close.len().max(1);
                        continue;
                    }
                }
                i += rest.chars().next().map_or(1, char::len_utf8);
            }
        }
        false
    }

    /// Types `open` (whose last character is `ch`, the only one not in the text yet) and `close`
    /// after it, with the caret between.
    fn type_auto_closing(&mut self, open: &str, close: &str, ch: char) {
        let pos = self.selection.active;
        let mut s = [0u8; 4];
        let ch_str: &str = ch.encode_utf8(&mut s);
        let text = format!("{ch_str}{close}");
        let _ = open;
        self.apply_edit_full(Range::at(pos), &text, Kind::Other, Some((utf16_len(ch_str), utf16_len(ch_str))), false);
        // The enclosing range is the whole open text and the auto closed text.
        let open_len = utf16_len(open);
        let close_len = utf16_len(close);
        let ch_len = utf16_len(ch_str);
        let end = pos.column + ch_len + close_len;
        let start = (pos.column + ch_len).saturating_sub(open_len);
        self.auto_closed.push(AutoClosed { line: pos.line, start, end, close_start: pos.column + ch_len });
    }

    /// upstream `isAutoClosingOvertype`.
    fn is_auto_closing_overtype(&self, support: &LanguageSupport, ch: char) -> bool {
        if self.typing.auto_closing_overtype == AutoClosingEdit::Never {
            return false;
        }
        let lookup = support.character_pair.auto_closing_pair_lookup();
        if !lookup.is_close_single_char(ch) || !self.selection.is_empty() {
            return false;
        }
        let pos = self.selection.active;
        let line = self.buffer.line_content(pos.line);
        let after = suffix_utf16(line, pos.column - 1).chars().next();
        if after != Some(ch) {
            return false;
        }
        // Do not over type quotes after a backslash.
        let before = prefix_utf16(line, pos.column - 1).chars().last();
        if before == Some('\\') && is_quote(ch) {
            return false;
        }
        // Must over type a closing character typed by the editor.
        if self.typing.auto_closing_overtype == AutoClosingEdit::Auto {
            return self.auto_closed.iter().any(|a| a.line == pos.line && a.close_start == pos.column);
        }
        true
    }

    /// upstream `DeleteOperations.isAutoClosingPairDelete`: backspace between `(` and `)` deletes both.
    pub(crate) fn is_auto_closing_pair_delete(&self) -> bool {
        let Some(support) = &self.language else { return false };
        let t = self.typing;
        if (t.auto_closing_brackets == AutoClosing::Never && t.auto_closing_quotes == AutoClosing::Never) || t.auto_closing_delete == AutoClosingEdit::Never {
            return false;
        }
        if !self.selection.is_empty() {
            return false;
        }
        let pos = self.selection.active;
        let line = self.buffer.line_content(pos.line);
        if pos.column < 2 || pos.column > utf16_len(line) {
            return false;
        }
        let Some(character) = prefix_utf16(line, pos.column - 1).chars().last() else { return false };
        let lookup = support.character_pair.auto_closing_pair_lookup();
        let candidates = lookup.open_by_end(character);
        if candidates.is_empty() {
            return false;
        }
        if is_quote(character) {
            if t.auto_closing_quotes == AutoClosing::Never {
                return false;
            }
        } else if t.auto_closing_brackets == AutoClosing::Never {
            return false;
        }
        let Some(after) = suffix_utf16(line, pos.column - 1).chars().next() else { return false };
        let mut a = [0u8; 4];
        let mut b = [0u8; 4];
        let (c, d) = (character.encode_utf8(&mut a) as &str, after.encode_utf8(&mut b) as &str);
        if !candidates.iter().any(|p| p.open == c && p.close == d) {
            return false;
        }
        if t.auto_closing_delete == AutoClosingEdit::Auto {
            return self.auto_closed.iter().any(|x| x.line == pos.line && x.close_start == pos.column);
        }
        true
    }

    // ---- surround --------------------------------------------------------------------------

    fn should_surround_char(&self, ch: char) -> bool {
        match self.typing.auto_surround {
            AutoSurround::Never => false,
            AutoSurround::LanguageDefined => true,
            AutoSurround::Quotes => is_quote(ch),
            AutoSurround::Brackets => !is_quote(ch),
        }
    }

    /// upstream `SurroundSelectionOperation`: typing a bracket or quote over a selection wraps it.
    fn surround_selection(&mut self, support: &LanguageSupport, ch: char) -> bool {
        let sel = self.selection;
        let mut s = [0u8; 4];
        let ch_str: &str = ch.encode_utf8(&mut s);
        if !self.should_surround_char(ch) {
            return false;
        }
        let Some((_, close)) = support.character_pair.surrounding_pairs().iter().find(|(o, _)| o == ch_str) else {
            return false;
        };
        if sel.is_empty() {
            return false;
        }
        let range = sel.range();
        let selected = self.buffer.value_in_range(range);
        if selected.chars().all(|c| c == ' ' || c == '\t' || c == '\n' || c == '\r') {
            return false;
        }
        // Typing a quote on top of another quote selected does not surround.
        if is_quote(ch) && range.start.line == range.end.line && selected.chars().count() == 1 && selected.chars().next().is_some_and(is_quote) {
            return false;
        }
        let close = close.clone();
        let text = format!("{ch_str}{selected}{close}");
        let inner = (utf16_len(ch_str), utf16_len(ch_str) + utf16_len(&selected));
        self.apply_edit_full(range, &text, Kind::Other, Some(inner), false);
        true
    }

    // ---- auto indent and electric characters --------------------------------------------------

    /// upstream `AutoIndentOperation`: at `autoIndent: full`, a character that makes the line match
    /// `decreaseIndentPattern` re-indents the line, e.g. the `}` that closes a block.
    fn auto_indent_type(&mut self, support: &LanguageSupport, ch: char, tokens: &dyn TokenSource) -> bool {
        if self.typing.auto_indent < AutoIndentStrategy::Full || support.indent_rules.is_none() {
            return false;
        }
        let sel = self.selection;
        let range = sel.range();
        let mut s = [0u8; 4];
        let ch_str: &str = ch.encode_utf8(&mut s);
        if tokens.line_tokens(&self.buffer, range.start.line).is_none() {
            return false;
        }
        let lookup = support.character_pair.auto_closing_pair_lookup();
        let opens_pair = !lookup.open_by_end(ch).is_empty();
        let model = self.model(tokens);
        let text_range = TextRange { start_line: range.start.line, start_column: range.start.column, end_line: range.end.line, end_column: range.end.column };
        let conv = self.converter();
        let Some(indentation) = auto_indent::get_indent_action_for_type(self.typing.auto_indent, &model, support, text_range, ch_str, &conv, opens_pair) else {
            return false;
        };
        let line = self.buffer.line_content(range.start.line);
        // `getIndentationAtPosition`: the leading whitespace, as far as the caret.
        let current_indentation = prefix_utf16(auto_indent::leading_whitespace(line), range.start.column - 1);
        if indentation == self.normalize_indent(current_indentation) {
            return false; // already right: leave the line alone
        }
        let first_non_ws = first_non_whitespace_column(line);
        let mut text = self.normalize_indent(&indentation);
        if first_non_ws != 0 {
            text.push_str(substr_utf16(line, first_non_ws - 1, range.start.column - 1));
        }
        text.push_str(ch_str);
        let edit_range = Range::new(Position::new(range.start.line, 1), range.end);
        self.apply_edit_full(edit_range, &text, Kind::Other, None, true);
        true
    }

    /// upstream `InterceptorElectricCharOperation`: typing a closing bracket first on its line puts the
    /// line at the indentation of the line with the matching open bracket.
    fn electric_char(&mut self, support: &LanguageSupport, ch: char, tokens: &dyn TokenSource) -> bool {
        if !self.selection.is_empty() || !support.electric_characters().contains(&ch) {
            return false;
        }
        let pos = self.selection.active;
        let line_text = self.buffer.line_content(pos.line).to_string();
        let Some(line_tokens) = tokens.line_tokens(&self.buffer, pos.line) else { return false };
        // Brackets in strings, comments and regular expressions are not electric.
        if line_tokens.type_at_offset(pos.column as isize - 1) != StandardTokenType::Other {
            return false;
        }
        let before = prefix_utf16(&line_text, pos.column - 1);
        if !before.chars().all(|c| c.is_whitespace()) {
            return false;
        }
        let mut s = [0u8; 4];
        let ch_str: &str = ch.encode_utf8(&mut s);
        let Some((open, close)) = support.brackets().iter().find(|(_, c)| c.ends_with(ch_str)) else { return false };
        let Some(match_line) = self.find_matching_open_bracket_line(open, close, pos, tokens) else { return false };
        if match_line == pos.line {
            return false;
        }
        let match_indent = auto_indent::leading_whitespace(self.buffer.line_content(match_line)).to_string();
        let new_indentation = self.normalize_indent(&match_indent);
        let first_non_blank = first_non_whitespace_column(&line_text);
        let first_non_blank = if first_non_blank == 0 { pos.column } else { first_non_blank };
        let prefix = substr_utf16(&line_text, first_non_blank - 1, pos.column - 1);
        let type_text = format!("{new_indentation}{prefix}{ch_str}");
        let range = Range::new(Position::new(pos.line, 1), pos);
        self.apply_edit_full(range, &type_text, Kind::Typing, None, false);
        true
    }

    /// The line of the open bracket matching a `close` typed at `pos` (upstream `findMatchingBracketUp`).
    fn find_matching_open_bracket_line(&self, open: &str, close: &str, pos: Position, tokens: &dyn TokenSource) -> Option<usize> {
        let mut depth = 1usize;
        let first = pos.line.saturating_sub(500).max(1);
        for line in (first..=pos.line).rev() {
            let text = self.buffer.line_content(line);
            let end = if line == pos.line { prefix_utf16(text, pos.column - 1).len() } else { text.len() };
            let text = &text[..end];
            let line_tokens = tokens.line_tokens(&self.buffer, line).unwrap_or_else(|| LineTokens::plain(utf16_len(text)));
            let mut i = text.len();
            while i > 0 {
                let rest = &text[..i];
                let offset = utf16_len(rest) - 1;
                let ignored = line_tokens.type_at_offset(offset as isize) != StandardTokenType::Other;
                if !ignored {
                    if rest.ends_with(close) {
                        depth += 1;
                        i -= close.len().max(1);
                        continue;
                    }
                    if rest.ends_with(open) {
                        depth -= 1;
                        if depth == 0 {
                            return Some(line);
                        }
                        i -= open.len().max(1);
                        continue;
                    }
                }
                i -= rest.chars().last().map_or(1, char::len_utf8);
            }
        }
        None
    }

    // ---- Enter ------------------------------------------------------------------------------

    /// Enter: a line break and the indentation the language wants (upstream `EnterOperation._enter`).
    pub fn enter(&mut self) {
        self.enter_with(&PlainTokens);
    }

    /// Enter with the tokens of the lines.
    pub fn enter_with(&mut self, tokens: &dyn TokenSource) {
        let range = self.selection.range();
        let eol = self.buffer.eol().as_str();
        let keep = |this: &Editor| {
            let line = this.buffer.line_content(range.start.line);
            let indent = auto_indent::leading_whitespace(prefix_utf16(line, range.start.column - 1));
            format!("{eol}{}", this.normalize_indent(indent))
        };
        let auto_indent = self.typing.auto_indent;
        if auto_indent == AutoIndentStrategy::None {
            self.apply_edit_full(range, eol, Kind::Other, None, true);
            return;
        }
        let support = self.language.clone();
        let cheap = tokens.line_tokens(&self.buffer, range.start.line).is_some();
        let Some(support) = support.filter(|_| cheap && auto_indent != AutoIndentStrategy::Keep) else {
            let text = keep(self);
            self.apply_edit_full(range, &text, Kind::Other, None, true);
            return;
        };
        let model = self.model(tokens);
        let text_range = TextRange { start_line: range.start.line, start_column: range.start.column, end_line: range.end.line, end_column: range.end.column };
        let conv = self.converter();
        if let Some(r) = auto_indent::get_enter_action(auto_indent, &model, &support, text_range) {
            let norm = |s: &str| normalize_indentation(s, self.options.tab_size, self.options.insert_spaces);
            match r.indent_action {
                IndentAction::None | IndentAction::Indent => {
                    let text = format!("{eol}{}", norm(&format!("{}{}", r.indentation, r.append_text)));
                    self.apply_edit_full(range, &text, Kind::Other, None, true);
                }
                IndentAction::IndentOutdent => {
                    // Ultra special: `{|}` becomes the open line, an indented empty line and the close line.
                    let normal = norm(&r.indentation);
                    let increased = norm(&format!("{}{}", r.indentation, r.append_text));
                    let text = format!("{eol}{increased}{eol}{normal}");
                    let caret = utf16_len(eol) + utf16_len(&increased);
                    self.apply_edit_full(range, &text, Kind::Other, Some((caret, caret)), true);
                }
                IndentAction::Outdent => {
                    let actual = conv.unshift_indent(&r.indentation);
                    let text = format!("{eol}{}", norm(&format!("{actual}{}", r.append_text)));
                    self.apply_edit_full(range, &text, Kind::Other, None, true);
                }
            }
            return;
        }
        let line = self.buffer.line_content(range.start.line).to_string();
        if auto_indent >= AutoIndentStrategy::Full
            && let Some((_, after_enter)) = auto_indent::get_indent_for_enter(auto_indent, &model, &support, text_range, &conv)
        {
            let mut old_end_view_column = visible_column(self.buffer.line_content(range.end.line), range.end.column, self.options.tab_size);
            let old_end_column = range.end.column;
            let end_line_content = self.buffer.line_content(range.end.line);
            let first_non_ws = first_non_whitespace_column(end_line_content); // 1-based, 0 when blank
            let mut end = range.end;
            if first_non_ws != 0 {
                end.column = end.column.max(first_non_ws);
            } else {
                end.column = self.buffer.line_max_column(range.end.line);
            }
            let edit_range = Range::new(range.start, end);
            let text = format!("{eol}{}", self.normalize_indent(&after_enter));
            let mut caret_delta: isize = 0;
            if first_non_ws == 0 || old_end_column <= first_non_ws {
                if !self.options.insert_spaces {
                    old_end_view_column = old_end_view_column.div_ceil(self.indent_size());
                }
                let normalized = self.normalize_indent(&after_enter);
                caret_delta = (old_end_view_column as isize + 1 - utf16_len(&normalized) as isize - 1).min(0);
            }
            let end_caret = (utf16_len(&text) as isize + caret_delta).max(0) as usize;
            self.apply_edit_full(edit_range, &text, Kind::Other, Some((end_caret, end_caret)), true);
            return;
        }
        let indent = auto_indent::leading_whitespace(prefix_utf16(&line, range.start.column - 1)).to_string();
        let text = format!("{eol}{}", self.normalize_indent(&indent));
        self.apply_edit_full(range, &text, Kind::Other, None, true);
    }

    // ---- auto closed ranges and auto whitespace ---------------------------------------------

    /// Moves the auto closed ranges through an edit of one line, or forgets them when the edit
    /// changes the lines.
    pub(crate) fn map_auto_closed(&mut self, range: Range, inserted: &str) {
        if self.auto_closed.is_empty() {
            return;
        }
        if range.start.line != range.end.line || inserted.contains('\n') {
            self.auto_closed.clear();
            return;
        }
        let (line, s, e) = (range.start.line, range.start.column, range.end.column);
        let new_len = utf16_len(inserted);
        let map = |p: usize, is_start: bool| -> usize {
            if p < s || (p == s && !is_start && s == e) {
                p
            } else if p >= e {
                p + new_len - (e - s)
            } else if is_start {
                s
            } else {
                s + new_len
            }
        };
        for a in &mut self.auto_closed {
            if a.line == line {
                a.start = map(a.start, true);
                a.end = map(a.end, false);
                a.close_start = map(a.close_start, true);
            }
        }
    }

    /// Drops the auto closed ranges the caret is no longer strictly inside (upstream `AutoClosedAction.isValid`).
    pub(crate) fn prune_auto_closed(&mut self) {
        if self.auto_closed.is_empty() {
            return;
        }
        let sel = self.selection.range();
        self.auto_closed.retain(|a| {
            a.line == sel.start.line && sel.end.line == a.line && a.start < sel.start.column && sel.end.column < a.end
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind3 {
    Quote,
    Comment,
    Bracket,
}

/// The text between UTF-16 offsets `from` and `to` of `s`.
fn substr_utf16(s: &str, from: usize, to: usize) -> &str {
    let a = prefix_utf16(s, from).len();
    let b = prefix_utf16(s, to.max(from)).len();
    &s[a..b]
}

/// upstream `_findContainedAutoClosingPair`.
fn find_contained_pair<'a>(lookup: &'a AutoClosingPairs, pair: &StandardAutoClosingPair) -> Option<&'a StandardAutoClosingPair> {
    if pair.open.chars().count() <= 1 {
        return None;
    }
    let last = pair.close.chars().last()?;
    let mut result: Option<&StandardAutoClosingPair> = None;
    for candidate in lookup.close_by_end(last) {
        if candidate.open != pair.open && pair.open.contains(&candidate.open) && pair.close.ends_with(&candidate.close) && result.is_none_or(|r| candidate.open.len() > r.open.len()) {
            result = Some(candidate);
        }
    }
    result
}

/// upstream `_isBeforeClosingBrace`.
fn is_before_closing_brace(lookup: &AutoClosingPairs, line_after: &str) -> bool {
    let Some(next) = line_after.chars().next() else { return false };
    let is_before_starting = lookup.open_by_start(next).iter().any(|p| line_after.starts_with(&p.open));
    let is_before_closing = lookup.close_by_start(next).iter().any(|p| line_after.starts_with(&p.close));
    !is_before_starting && is_before_closing
}

impl Editor {
    /// The selection as `(anchor, active)` offsets for tests of the typing rules.
    #[doc(hidden)]
    pub fn auto_closed_count(&self) -> usize {
        self.auto_closed.len()
    }

    #[allow(dead_code)]
    pub(crate) fn caret(&self) -> Selection {
        self.selection
    }
}
