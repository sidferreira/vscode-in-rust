//! `indentationRules`: which lines increase or decrease the indentation of the next.
//!
//! upstream: src/vs/editor/common/languages/supports/indentRules.ts @ 50e1350fc78d

use crate::config::IndentationRule;

/// upstream `IndentConsts`.
pub const INCREASE_MASK: u32 = 0b0001;
pub const DECREASE_MASK: u32 = 0b0010;
pub const INDENT_NEXTLINE_MASK: u32 = 0b0100;
pub const UNINDENT_MASK: u32 = 0b1000;

/// upstream `IndentRulesSupport`.
#[derive(Debug, Clone)]
pub struct IndentRulesSupport {
    rules: IndentationRule,
}

impl IndentRulesSupport {
    pub fn new(rules: IndentationRule) -> Self {
        IndentRulesSupport { rules }
    }

    pub fn should_increase(&self, text: &str) -> bool {
        self.rules.increase_indent_pattern.is_match(text)
    }

    pub fn should_decrease(&self, text: &str) -> bool {
        self.rules.decrease_indent_pattern.is_match(text)
    }

    pub fn should_indent_next_line(&self, text: &str) -> bool {
        self.rules.indent_next_line_pattern.as_ref().is_some_and(|r| r.is_match(text))
    }

    /// The text matches `unIndentedLinePattern`.
    pub fn should_ignore(&self, text: &str) -> bool {
        self.rules.un_indented_line_pattern.as_ref().is_some_and(|r| r.is_match(text))
    }

    pub fn get_indent_metadata(&self, text: &str) -> u32 {
        let mut ret = 0;
        if self.should_increase(text) {
            ret += INCREASE_MASK;
        }
        if self.should_decrease(text) {
            ret += DECREASE_MASK;
        }
        if self.should_indent_next_line(text) {
            ret += INDENT_NEXTLINE_MASK;
        }
        if self.should_ignore(text) {
            ret += UNINDENT_MASK;
        }
        ret
    }
}
