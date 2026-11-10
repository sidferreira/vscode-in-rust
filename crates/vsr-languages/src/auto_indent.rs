//! Auto indentation: the indentation of a new line from the lines above it, and of a line after
//! typing a character.
//!
//! upstream: src/vs/editor/common/languages/autoIndent.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/languages/enterAction.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/languages/supports/indentationLineProcessor.ts @ 50e1350fc78d
//!
//! A language has one configuration here (no embedded languages), so `getLanguageIdAtPosition`
//! checks and `isLanguageDifferentFromLineStart` are always "same language". Lines are given by
//! an `IndentModel`: their text and their token types. The indentation rules look at *processed*
//! lines: the language's brackets are removed from string, comment and regular expression tokens,
//! so `"{"` does not increase the indentation.

use crate::config::{AutoIndentStrategy, IndentAction, LanguageSupport, LineTokens, StandardTokenType};
use crate::indent_rules::{DECREASE_MASK, INCREASE_MASK, INDENT_NEXTLINE_MASK};

/// upstream `IVirtualModel`: the lines of a document, 1-based.
pub trait IndentModel {
    fn line_content(&self, line: usize) -> String;
    fn line_tokens(&self, line: usize) -> LineTokens;
}

/// upstream `IIndentConverter`.
pub trait IndentConverter {
    fn shift_indent(&self, indentation: &str) -> String;
    fn unshift_indent(&self, indentation: &str) -> String;
    fn normalize_indentation(&self, indentation: &str) -> String {
        indentation.to_string()
    }
}

/// The leading spaces and tabs of `text`.
pub fn leading_whitespace(text: &str) -> &str {
    let end = text.find(|c: char| c != ' ' && c != '\t').unwrap_or(text.len());
    &text[..end]
}

/// The part of `text` between UTF-16 offsets `from` and `to`.
fn slice_utf16(text: &str, from: usize, to: usize) -> &str {
    let byte = |units: usize| {
        let mut n = 0;
        for (i, c) in text.char_indices() {
            if n >= units {
                return i;
            }
            n += c.len_utf16();
        }
        text.len()
    };
    &text[byte(from)..byte(to.max(from))]
}

/// Removes the brackets of the language from the string, comment and regex tokens of `text`
/// (upstream `IndentationLineProcessor.getProcessedTokens`), for the part between UTF-16 offsets
/// `from` and `to`.
fn process_range(support: &LanguageSupport, text: &str, tokens: &LineTokens, from: usize, to: usize) -> String {
    let mut brackets: Vec<&str> = Vec::new();
    for (open, close) in support.brackets() {
        brackets.push(open);
        brackets.push(close);
    }
    brackets.sort_by_key(|b| std::cmp::Reverse(b.len()));
    brackets.dedup();
    let mut out = String::new();
    let spans: Vec<(usize, usize, StandardTokenType)> = if tokens.token_count() == 0 {
        vec![(0, usize::MAX, StandardTokenType::Other)]
    } else {
        tokens.spans().collect()
    };
    for (start, end, ty) in spans {
        let (s, e) = (start.max(from), end.min(to));
        if s >= e {
            continue;
        }
        let piece = slice_utf16(text, s, e);
        if matches!(ty, StandardTokenType::String | StandardTokenType::RegEx | StandardTokenType::Comment) {
            let mut piece = piece.to_string();
            for b in &brackets {
                piece = piece.replace(b, "");
            }
            out.push_str(&piece);
        } else {
            out.push_str(piece);
        }
    }
    out
}

fn utf16_len(s: &str) -> usize {
    s.encode_utf16().count()
}

/// upstream `ProcessedIndentRulesSupport` over one model.
struct Processed<'a> {
    model: &'a dyn IndentModel,
    support: &'a LanguageSupport,
}

impl Processed<'_> {
    fn processed_line(&self, line: usize) -> String {
        let text = self.model.line_content(line);
        let tokens = self.model.line_tokens(line);
        process_range(self.support, &text, &tokens, 0, utf16_len(&text))
    }

    fn rules(&self) -> &crate::indent_rules::IndentRulesSupport {
        self.support.indent_rules.as_ref().expect("checked by the callers")
    }

    fn should_increase(&self, line: usize) -> bool {
        self.rules().should_increase(&self.processed_line(line))
    }
    fn should_decrease(&self, line: usize) -> bool {
        self.rules().should_decrease(&self.processed_line(line))
    }
    fn should_ignore(&self, line: usize) -> bool {
        self.rules().should_ignore(&self.processed_line(line))
    }
    fn should_indent_next_line(&self, line: usize) -> bool {
        self.rules().should_indent_next_line(&self.processed_line(line))
    }
}

fn is_blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// Nearest preceding line that is not blank and does not match `unIndentedLinePattern`; 0 when
/// every line above is invalid, -1 when there is no line above.
fn get_preceding_valid_line(model: &dyn IndentModel, line_number: usize, p: &Processed) -> isize {
    if line_number > 1 {
        for last in (1..line_number).rev() {
            let text = model.line_content(last);
            if p.should_ignore(last) || (!text.is_empty() && is_blank(&text)) || text.is_empty() {
                continue;
            }
            return last as isize;
        }
    }
    // Upstream returns -1 here too when every line above was blank or ignored.
    -1
}

/// upstream's return of `getInheritIndentForLine`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InheritIndent {
    pub indentation: String,
    pub action: Option<IndentAction>,
    pub line: Option<usize>,
}

/// Inherited indentation from the lines above (upstream `getInheritIndentForLine`). Needs
/// `autoIndent: full` and the language's `indentationRules`.
pub fn get_inherit_indent_for_line(
    auto_indent: AutoIndentStrategy,
    model: &dyn IndentModel,
    support: &LanguageSupport,
    line_number: usize,
    honor_intentional_indent: bool,
) -> Option<InheritIndent> {
    if auto_indent < AutoIndentStrategy::Full {
        return None;
    }
    support.indent_rules.as_ref()?;
    let p = Processed { model, support };
    if line_number <= 1 {
        return Some(InheritIndent { indentation: String::new(), action: None, line: None });
    }
    // Use no indent if this is the first non-blank line.
    for prior in (1..line_number).rev() {
        if !model.line_content(prior).is_empty() {
            break;
        }
        if prior == 1 {
            return Some(InheritIndent { indentation: String::new(), action: None, line: None });
        }
    }
    let preceding = get_preceding_valid_line(model, line_number, &p);
    if preceding < 0 {
        return None;
    } else if preceding < 1 {
        return Some(InheritIndent { indentation: String::new(), action: None, line: None });
    }
    let preceding = preceding as usize;
    let indent_of = |line: usize| leading_whitespace(&model.line_content(line)).to_string();
    if p.should_increase(preceding) || p.should_indent_next_line(preceding) {
        return Some(InheritIndent { indentation: indent_of(preceding), action: Some(IndentAction::Indent), line: Some(preceding) });
    } else if p.should_decrease(preceding) {
        return Some(InheritIndent { indentation: indent_of(preceding), action: None, line: Some(preceding) });
    }
    // `preceding` is not ignored, does not increase the indent of the following lines and does
    // not increase just the next line, so the line is not affected by it.
    if preceding == 1 {
        return Some(InheritIndent { indentation: indent_of(preceding), action: None, line: Some(preceding) });
    }
    let previous_line = preceding - 1;
    let metadata = support.indent_rules.as_ref()?.get_indent_metadata(&model.line_content(previous_line));
    if metadata & (INCREASE_MASK | DECREASE_MASK) == 0 && metadata & INDENT_NEXTLINE_MASK != 0 {
        let mut stop_line = 0;
        for i in (1..previous_line).rev() {
            if p.should_indent_next_line(i) {
                continue;
            }
            stop_line = i;
            break;
        }
        return Some(InheritIndent { indentation: indent_of(stop_line + 1), action: None, line: Some(stop_line + 1) });
    }
    if honor_intentional_indent {
        return Some(InheritIndent { indentation: indent_of(preceding), action: None, line: Some(preceding) });
    }
    // Search from `preceding` until a line whose indent is not temporary.
    for i in (1..=preceding).rev() {
        if p.should_increase(i) {
            return Some(InheritIndent { indentation: indent_of(i), action: Some(IndentAction::Indent), line: Some(i) });
        } else if p.should_indent_next_line(i) {
            let mut stop_line = 0;
            for j in (1..i).rev() {
                // upstream tests `i` again here, not `j`: the loop stops at once.
                if p.should_indent_next_line(i) {
                    continue;
                }
                stop_line = j;
                break;
            }
            return Some(InheritIndent { indentation: indent_of(stop_line + 1), action: None, line: Some(stop_line + 1) });
        } else if p.should_decrease(i) {
            return Some(InheritIndent { indentation: indent_of(i), action: None, line: Some(i) });
        }
    }
    Some(InheritIndent { indentation: indent_of(1), action: None, line: Some(1) })
}

/// The indentation `line_number` should have, from the lines above and what it contains
/// (upstream `getGoodIndentForLine`). Needs `autoIndent: full`.
pub fn get_good_indent_for_line(
    auto_indent: AutoIndentStrategy,
    model: &dyn IndentModel,
    support: &LanguageSupport,
    line_number: usize,
    converter: &dyn IndentConverter,
) -> Option<String> {
    if auto_indent < AutoIndentStrategy::Full {
        return None;
    }
    support.indent_rules.as_ref()?;
    let p = Processed { model, support };
    let indent = get_inherit_indent_for_line(auto_indent, model, support, line_number, true)?;
    if let Some(inherit_line) = indent.line {
        // Apply the enter action as long as there are only whitespace lines between the inherited line and this one.
        let mut should_apply = true;
        for between in inherit_line..line_number.saturating_sub(1) {
            if !is_blank(&model.line_content(between)) {
                should_apply = false;
                break;
            }
        }
        if should_apply
            && let Some(enter) = support.on_enter(auto_indent, "", &model.line_content(inherit_line), "")
        {
            let mut indentation = leading_whitespace(&model.line_content(inherit_line)).to_string();
            if let Some(remove) = enter.remove_text {
                let keep = indentation.len().saturating_sub(remove);
                indentation.truncate(keep);
            }
            match enter.indent_action {
                IndentAction::Indent | IndentAction::IndentOutdent => indentation = converter.shift_indent(&indentation),
                IndentAction::Outdent => indentation = converter.unshift_indent(&indentation),
                IndentAction::None => {}
            }
            if p.should_decrease(line_number) {
                indentation = converter.unshift_indent(&indentation);
            }
            if let Some(append) = &enter.append_text {
                indentation.push_str(append);
            }
            return Some(leading_whitespace(&indentation).to_string());
        }
    }
    if p.should_decrease(line_number) {
        if indent.action == Some(IndentAction::Indent) {
            Some(indent.indentation)
        } else {
            Some(converter.unshift_indent(&indent.indentation))
        }
    } else if indent.action == Some(IndentAction::Indent) {
        Some(converter.shift_indent(&indent.indentation))
    } else {
        Some(indent.indentation)
    }
}

/// A range inside one document: 1-based line and UTF-16 column of both ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextRange {
    pub start_line: usize,
    pub start_column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

/// The processed text before the range's start, after its end, and of the previous line
/// (upstream `IndentationContextProcessor.getProcessedTokenContextAroundRange`).
pub struct ProcessedContext {
    pub before_range: String,
    pub after_range: String,
    pub previous_line: String,
}

pub fn processed_context_around_range(model: &dyn IndentModel, support: &LanguageSupport, range: TextRange) -> ProcessedContext {
    let start_text = model.line_content(range.start_line);
    let start_tokens = model.line_tokens(range.start_line);
    let before_range = process_range(support, &start_text, &start_tokens, 0, range.start_column - 1);
    let (end_line, end_column) = if range.start_line == range.end_line && range.start_column == range.end_column {
        (range.start_line, range.start_column)
    } else {
        (range.end_line, range.end_column)
    };
    let end_text = model.line_content(end_line);
    let end_tokens = model.line_tokens(end_line);
    let after_range = process_range(support, &end_text, &end_tokens, end_column - 1, utf16_len(&end_text));
    let previous_line = if range.start_line <= 1 {
        String::new()
    } else {
        let prev = model.line_content(range.start_line - 1);
        let tokens = model.line_tokens(range.start_line - 1);
        process_range(support, &prev, &tokens, 0, utf16_len(&prev))
    };
    ProcessedContext { before_range, after_range, previous_line }
}

/// A model where one line has other text (upstream `createVirtualModelWithModifiedTokensAtLine`).
struct ModifiedLine<'a> {
    model: &'a dyn IndentModel,
    line: usize,
    text: String,
}

impl IndentModel for ModifiedLine<'_> {
    fn line_content(&self, line: usize) -> String {
        if line == self.line { self.text.clone() } else { self.model.line_content(line) }
    }
    fn line_tokens(&self, line: usize) -> LineTokens {
        if line == self.line {
            // The processed text has no brackets in strings and comments left; it is read as plain tokens.
            LineTokens::plain(utf16_len(&self.text))
        } else {
            self.model.line_tokens(line)
        }
    }
}

/// The indentation of the line before and after Enter (upstream `getIndentForEnter`). Needs
/// `autoIndent: full`.
pub fn get_indent_for_enter(
    auto_indent: AutoIndentStrategy,
    model: &dyn IndentModel,
    support: &LanguageSupport,
    range: TextRange,
    converter: &dyn IndentConverter,
) -> Option<(String, String)> {
    if auto_indent < AutoIndentStrategy::Full {
        return None;
    }
    let rules = support.indent_rules.as_ref()?;
    let ctx = processed_context_around_range(model, support, range);
    let before_enter_indent = leading_whitespace(&ctx.before_range).to_string();
    let virtual_model = ModifiedLine { model, line: range.start_line, text: ctx.before_range.clone() };
    let after_enter_action = get_inherit_indent_for_line(auto_indent, &virtual_model, support, range.start_line + 1, false);
    let Some(action) = after_enter_action else {
        return Some((before_enter_indent.clone(), before_enter_indent));
    };
    let mut after_enter_indent = action.indentation.clone();
    if action.action == Some(IndentAction::Indent) {
        after_enter_indent = converter.shift_indent(&after_enter_indent);
    }
    if rules.should_decrease(&ctx.after_range) {
        after_enter_indent = converter.unshift_indent(&after_enter_indent);
    }
    Some((before_enter_indent, after_enter_indent))
}

/// The indentation a line should get after `ch` is typed at `range`, or `None` to leave it
/// (upstream `getIndentActionForType`). `ch_opens_auto_closing_pair` is whether an auto closing pair
/// opens with `ch`. Needs `autoIndent: full`.
pub fn get_indent_action_for_type(
    auto_indent: AutoIndentStrategy,
    model: &dyn IndentModel,
    support: &LanguageSupport,
    range: TextRange,
    ch: &str,
    converter: &dyn IndentConverter,
    ch_opens_auto_closing_pair: bool,
) -> Option<String> {
    if auto_indent < AutoIndentStrategy::Full {
        return None;
    }
    let rules = support.indent_rules.as_ref()?;
    let ctx = processed_context_around_range(model, support, range);
    let text_around = format!("{}{}", ctx.before_range, ctx.after_range);
    let text_around_with_char = format!("{}{}{}", ctx.before_range, ch, ctx.after_range);
    // If the content already matches decreaseIndentPattern the indentation was adjusted before; the
    // user may have changed it on purpose.
    if !rules.should_decrease(&text_around) && rules.should_decrease(&text_around_with_char) {
        let r = get_inherit_indent_for_line(auto_indent, model, support, range.start_line, false)?;
        let mut indentation = r.indentation;
        if r.action != Some(IndentAction::Indent) {
            indentation = converter.unshift_indent(&indentation);
        }
        return Some(indentation);
    }
    let empty = range.start_line == range.end_line && range.start_column == range.end_column;
    if empty && range.start_line > 1 {
        let previous = model.line_content(range.start_line - 1);
        if rules.should_indent_next_line(&previous) && rules.should_increase(&text_around_with_char) {
            let inherited = get_inherit_indent_for_line(auto_indent, model, support, range.start_line, false).map(|i| i.indentation);
            if let Some(inherited) = inherited {
                let actual = leading_whitespace(&model.line_content(range.start_line)).to_string();
                let inferred = converter.shift_indent(&inherited);
                let only_whitespace = is_blank(&text_around);
                if inferred == actual && ch_opens_auto_closing_pair && only_whitespace {
                    return Some(inherited);
                }
            }
        }
    }
    None
}

/// upstream `CompleteEnterAction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteEnterAction {
    pub indent_action: IndentAction,
    pub append_text: String,
    pub remove_text: usize,
    pub indentation: String,
}

/// What Enter does at `range` (upstream `getEnterAction`): the `onEnter` result completed with the
/// current indentation. `None` when the language has no rule for it.
pub fn get_enter_action(auto_indent: AutoIndentStrategy, model: &dyn IndentModel, support: &LanguageSupport, range: TextRange) -> Option<CompleteEnterAction> {
    let ctx = processed_context_around_range(model, support, range);
    let result = support.on_enter(auto_indent, &ctx.previous_line, &ctx.before_range, &ctx.after_range)?;
    let indent_action = result.indent_action;
    let mut append_text = result.append_text.unwrap_or_default();
    let remove_text = result.remove_text.unwrap_or(0);
    // `\t` is added to appendText first because the enter action changes indentation through appendText and removeText.
    if append_text.is_empty() {
        if matches!(indent_action, IndentAction::Indent | IndentAction::IndentOutdent) {
            append_text = "\t".to_string();
        }
    } else if indent_action == IndentAction::Indent {
        append_text = format!("\t{append_text}");
    }
    let line = model.line_content(range.start_line);
    let before = slice_utf16(&line, 0, range.start_column - 1);
    let mut indentation = leading_whitespace(before).to_string();
    if remove_text > 0 {
        let keep = indentation.len().saturating_sub(remove_text);
        indentation.truncate(keep);
    }
    Some(CompleteEnterAction { indent_action, append_text, remove_text, indentation })
}
