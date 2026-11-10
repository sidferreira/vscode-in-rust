//! What Enter does between brackets and by regular expression rules.
//!
//! upstream: src/vs/editor/common/languages/supports/onEnter.ts @ 50e1350fc78d
//!
//! `onEnter` runs the `onEnterRules` first (at `autoIndent` advanced and above), then the bracket
//! rules (`(|)` splits into an indented line and the closing line; an open bracket before the
//! caret indents).

use crate::config::{AutoIndentStrategy, EnterAction, IndentAction, JsRegex, OnEnterRule};

#[derive(Debug, Clone)]
struct ProcessedBracketPair {
    open_regexp: JsRegex,
    close_regexp: JsRegex,
}

/// upstream `OnEnterSupport`.
#[derive(Debug, Clone)]
pub struct OnEnterSupport {
    brackets: Vec<ProcessedBracketPair>,
    reg_exp_rules: Vec<OnEnterRule>,
}

/// `strings.escapeRegExpCharacters`.
fn escape_reg_exp(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        if "\\{}*+?|^$.[]()-".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// JavaScript's `/\B/.test(c)` for a one character string: a word character has a boundary on
/// each side, anything else does not.
fn matches_non_boundary(c: char) -> bool {
    !(c.is_ascii_alphanumeric() || c == '_')
}

impl OnEnterSupport {
    pub fn new(brackets: Option<Vec<(String, String)>>, on_enter_rules: Option<Vec<OnEnterRule>>) -> Self {
        let brackets = brackets.unwrap_or_else(|| vec![("(".into(), ")".into()), ("{".into(), "}".into()), ("[".into(), "]".into())]);
        let mut processed = Vec::new();
        for (open, close) in &brackets {
            let mut o = escape_reg_exp(open);
            if !o.chars().next().is_some_and(matches_non_boundary) {
                o.insert_str(0, "\\b");
            }
            o.push_str("\\s*$");
            let mut c = escape_reg_exp(close);
            if !c.chars().last().is_some_and(matches_non_boundary) {
                c.push_str("\\b");
            }
            let c = format!("^\\s*{c}");
            if let (Some(open_regexp), Some(close_regexp)) = (JsRegex::new(&o, ""), JsRegex::new(&c, "")) {
                processed.push(ProcessedBracketPair { open_regexp, close_regexp });
            }
        }
        OnEnterSupport { brackets: processed, reg_exp_rules: on_enter_rules.unwrap_or_default() }
    }

    pub fn on_enter(&self, auto_indent: AutoIndentStrategy, previous_line_text: &str, before_enter_text: &str, after_enter_text: &str) -> Option<EnterAction> {
        // (1): `regExpRules`
        if auto_indent >= AutoIndentStrategy::Advanced {
            for rule in &self.reg_exp_rules {
                let ok = rule.before_text.is_match(before_enter_text)
                    && rule.after_text.as_ref().is_none_or(|r| r.is_match(after_enter_text))
                    && rule.previous_line_text.as_ref().is_none_or(|r| r.is_match(previous_line_text));
                if ok {
                    return Some(rule.action.clone());
                }
            }
        }
        // (2): Special indent-outdent
        if auto_indent >= AutoIndentStrategy::Brackets && !before_enter_text.is_empty() && !after_enter_text.is_empty() {
            for bracket in &self.brackets {
                if bracket.open_regexp.is_match(before_enter_text) && bracket.close_regexp.is_match(after_enter_text) {
                    return Some(EnterAction::new(IndentAction::IndentOutdent));
                }
            }
        }
        // (4): Open bracket based logic
        if auto_indent >= AutoIndentStrategy::Brackets && !before_enter_text.is_empty() {
            for bracket in &self.brackets {
                if bracket.open_regexp.is_match(before_enter_text) {
                    return Some(EnterAction::new(IndentAction::Indent));
                }
            }
        }
        None
    }
}
