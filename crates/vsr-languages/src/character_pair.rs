//! Auto closing pairs, surrounding pairs and the `autoCloseBefore` sets of a language.
//!
//! upstream: src/vs/editor/common/languages/supports/characterPair.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/languages/languageConfiguration.ts @ 50e1350fc78d
//!
//! `StandardAutoClosingPair` is upstream's `StandardAutoClosingPairConditional`: a pair that may be
//! forbidden in strings, comments or regular expressions (`notIn`) or by a `beforeText` regex.
//! `AutoClosingPairs` is the lookup by either end of the open or close text.

use crate::config::{AutoClosingPairConfig, JsRegex, LanguageConfiguration, LineTokens, StandardTokenType};

/// upstream `StandardAutoClosingPairConditional`.
#[derive(Debug, Clone, PartialEq)]
pub struct StandardAutoClosingPair {
    pub open: String,
    pub close: String,
    pub before_text: Option<JsRegex>,
    in_string: bool,
    in_comment: bool,
    in_regex: bool,
}

impl StandardAutoClosingPair {
    pub fn new(source: &AutoClosingPairConfig) -> Self {
        let not = |kind: &str| source.not_in.iter().any(|k| k == kind);
        StandardAutoClosingPair {
            open: source.open.clone(),
            close: source.close.clone(),
            before_text: source.before_text.clone(),
            in_string: !not("string"),
            in_comment: !not("comment"),
            in_regex: !not("regex"),
        }
    }

    pub fn should_auto_close_before(&self, text: &str) -> bool {
        self.before_text.as_ref().is_none_or(|r| r.is_match(text))
    }

    pub fn is_ok(&self, token: StandardTokenType) -> bool {
        match token {
            StandardTokenType::Other => true,
            StandardTokenType::Comment => self.in_comment,
            StandardTokenType::String => self.in_string,
            StandardTokenType::RegEx => self.in_regex,
        }
    }

    /// Whether the pair may be auto closed at 1-based `column` of a line with these tokens: always on
    /// an empty line, else by the type of the token before the caret.
    pub fn should_auto_close(&self, tokens: &LineTokens, column: usize) -> bool {
        if tokens.token_count() == 0 {
            return true;
        }
        let index = tokens.find_token_index_at_offset(column as isize - 2);
        self.is_ok(tokens.standard_token_type(index))
    }

    fn find_neutral_in_range(&self, from: char, to: char) -> Option<char> {
        (from..=to).find(|c| !self.open.contains(*c) && !self.close.contains(*c))
    }

    /// A character that is in neither the open nor the close text: a digit, else a letter.
    pub fn find_neutral_character(&self) -> Option<char> {
        self.find_neutral_in_range('0', '9')
            .or_else(|| self.find_neutral_in_range('a', 'z'))
            .or_else(|| self.find_neutral_in_range('A', 'Z'))
    }
}

/// upstream `AutoClosingPairs`: lookups by either end of the open or close text.
#[derive(Debug, Clone, Default)]
pub struct AutoClosingPairs {
    pairs: Vec<StandardAutoClosingPair>,
}

impl AutoClosingPairs {
    pub fn new(pairs: Vec<StandardAutoClosingPair>) -> Self {
        AutoClosingPairs { pairs }
    }

    pub fn all(&self) -> &[StandardAutoClosingPair] {
        &self.pairs
    }

    /// Pairs whose open text starts with `ch`.
    pub fn open_by_start(&self, ch: char) -> Vec<&StandardAutoClosingPair> {
        self.pairs.iter().filter(|p| p.open.starts_with(ch)).collect()
    }

    /// Pairs whose open text ends with `ch`.
    pub fn open_by_end(&self, ch: char) -> Vec<&StandardAutoClosingPair> {
        self.pairs.iter().filter(|p| p.open.ends_with(ch)).collect()
    }

    /// Pairs whose close text starts with `ch`.
    pub fn close_by_start(&self, ch: char) -> Vec<&StandardAutoClosingPair> {
        self.pairs.iter().filter(|p| p.close.starts_with(ch)).collect()
    }

    /// Pairs whose close text ends with `ch`.
    pub fn close_by_end(&self, ch: char) -> Vec<&StandardAutoClosingPair> {
        self.pairs.iter().filter(|p| p.close.ends_with(ch)).collect()
    }

    /// Whether some pair has the one character open and close, and `ch` is its close.
    pub fn is_close_single_char(&self, ch: char) -> bool {
        self.pairs.iter().any(|p| p.open.chars().count() == 1 && p.close.chars().count() == 1 && p.close.starts_with(ch))
    }
}

pub const DEFAULT_AUTOCLOSE_BEFORE_LANGUAGE_DEFINED_QUOTES: &str = ";:.,=}])> \n\t";
pub const DEFAULT_AUTOCLOSE_BEFORE_LANGUAGE_DEFINED_BRACKETS: &str = "'\"`;:.,=}])> \n\t";
pub const DEFAULT_AUTOCLOSE_BEFORE_WHITESPACE: &str = " \n\t";

/// upstream `CharacterPairSupport`.
#[derive(Debug, Clone)]
pub struct CharacterPairSupport {
    auto_closing_pairs: Vec<StandardAutoClosingPair>,
    surrounding_pairs: Vec<(String, String)>,
    auto_close_before_for_quotes: String,
    auto_close_before_for_brackets: String,
}

impl CharacterPairSupport {
    pub fn new(config: &LanguageConfiguration) -> Self {
        let auto_closing_pairs: Vec<StandardAutoClosingPair> = if let Some(pairs) = &config.auto_closing_pairs {
            pairs.iter().map(StandardAutoClosingPair::new).collect()
        } else if let Some(brackets) = &config.brackets {
            brackets.iter().map(|(o, c)| StandardAutoClosingPair::new(&AutoClosingPairConfig::new(o, c))).collect()
        } else {
            Vec::new()
        };
        let before = config.auto_close_before.clone();
        let surrounding_pairs = config
            .surrounding_pairs
            .clone()
            .unwrap_or_else(|| auto_closing_pairs.iter().map(|p| (p.open.clone(), p.close.clone())).collect());
        CharacterPairSupport {
            auto_closing_pairs,
            surrounding_pairs,
            auto_close_before_for_quotes: before.clone().unwrap_or_else(|| DEFAULT_AUTOCLOSE_BEFORE_LANGUAGE_DEFINED_QUOTES.to_string()),
            auto_close_before_for_brackets: before.unwrap_or_else(|| DEFAULT_AUTOCLOSE_BEFORE_LANGUAGE_DEFINED_BRACKETS.to_string()),
        }
    }

    pub fn auto_closing_pairs(&self) -> &[StandardAutoClosingPair] {
        &self.auto_closing_pairs
    }

    pub fn auto_closing_pair_lookup(&self) -> AutoClosingPairs {
        AutoClosingPairs::new(self.auto_closing_pairs.clone())
    }

    pub fn auto_close_before_set(&self, for_quotes: bool) -> &str {
        if for_quotes { &self.auto_close_before_for_quotes } else { &self.auto_close_before_for_brackets }
    }

    pub fn surrounding_pairs(&self) -> &[(String, String)] {
        &self.surrounding_pairs
    }
}
