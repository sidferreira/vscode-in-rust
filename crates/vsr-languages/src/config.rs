//! Language configuration: the editing rules of a language (ADR-0046).
//!
//! upstream: src/vs/editor/common/languages/languageConfiguration.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/languages/languageConfigurationRegistry.ts @ 50e1350fc78d
//!
//! A `LanguageConfiguration` is the parsed form of a `language-configuration.json`: comments,
//! brackets, auto closing and surrounding pairs, indentation rules, on-enter rules and the word
//! pattern. `scripts/bundle-grammars` copies those keys into `data/language-configurations.json`
//! for every built-in language; `language_support(id)` resolves one into the `LanguageSupport`
//! the editor asks (upstream's `ResolvedLanguageConfiguration`).
//!
//! Regular expressions are JavaScript's in the JSON (`{"pattern", "flags"}` or a plain string).
//! They run on Oniguruma's Perl syntax (`JsRegex`), which handles the lookaheads the bundled rules use.
//! Not ported: configurations merged from several extensions for one language, `colorizedBracketPairs`
//! and the legacy `__electricCharacterSupport.docComment`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use onig::{Regex, RegexOptions, Syntax};
use serde::Deserialize;

use crate::character_pair::CharacterPairSupport;
use crate::indent_rules::IndentRulesSupport;
use crate::on_enter::OnEnterSupport;

/// A JavaScript regular expression, run with Oniguruma. `lastIndex` and the `g` and `m` flags
/// do not matter here: every use is a single `test` on one line of text.
#[derive(Clone)]
pub struct JsRegex {
    regex: Arc<Regex>,
    source: Arc<str>,
}

impl std::fmt::Debug for JsRegex {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JsRegex({})", self.source)
    }
}

impl PartialEq for JsRegex {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
    }
}

impl JsRegex {
    /// `None` when the pattern does not compile.
    pub fn new(pattern: &str, flags: &str) -> Option<JsRegex> {
        let options = if flags.contains('i') { RegexOptions::REGEX_OPTION_IGNORECASE } else { RegexOptions::REGEX_OPTION_NONE };
        let translated = translate_js_classes(pattern);
        let regex = Regex::with_options(&translated, options, Syntax::perl_ng())
            .or_else(|_| Regex::with_options(&translated, options, Syntax::ruby()))
            .ok()?;
        Some(JsRegex { regex: Arc::new(regex), source: pattern.into() })
    }

    /// `RegExp.prototype.test`.
    pub fn is_match(&self, text: &str) -> bool {
        // Not `Regex::is_match`: that one requires the match to span the whole text.
        self.regex.search_with_options(text, 0, text.len(), onig::SearchOptions::SEARCH_OPTION_NONE, None).is_some()
    }

    pub fn source(&self) -> &str {
        &self.source
    }
}

/// JavaScript's two special character classes: `[]` matches nothing and `[^]` matches anything,
/// where Oniguruma reads both as the start of a class that contains `]`.
fn translate_js_classes(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    let mut out = String::with_capacity(pattern.len());
    let (mut i, mut in_class) = (0, false);
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' {
            out.push(c);
            if let Some(&n) = chars.get(i + 1) {
                out.push(n);
            }
            i += 2;
            continue;
        }
        if in_class {
            if c == ']' {
                in_class = false;
            }
        } else if c == '[' {
            if chars.get(i + 1) == Some(&']') {
                out.push_str("[^\\s\\S]");
                i += 2;
                continue;
            }
            if chars.get(i + 1) == Some(&'^') && chars.get(i + 2) == Some(&']') {
                out.push_str("[\\s\\S]");
                i += 3;
                continue;
            }
            in_class = true;
        }
        out.push(c);
        i += 1;
    }
    out
}

/// upstream `StandardTokenType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StandardTokenType {
    #[default]
    Other,
    Comment,
    String,
    RegEx,
}

/// The tokens of one line: the type of each token and where it ends (UTF-16 offsets), upstream's
/// `ScopedLineTokens` for the single-language case.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LineTokens {
    ends: Vec<usize>,
    types: Vec<StandardTokenType>,
}

impl LineTokens {
    /// From `(end offset, type)` pairs in order.
    pub fn new(tokens: &[(usize, StandardTokenType)]) -> Self {
        LineTokens { ends: tokens.iter().map(|t| t.0).collect(), types: tokens.iter().map(|t| t.1).collect() }
    }

    /// The tokens of a line without a tokenizer: one `Other` token (none for an empty line).
    pub fn plain(len: usize) -> Self {
        if len == 0 { LineTokens::default() } else { LineTokens::new(&[(len, StandardTokenType::Other)]) }
    }

    pub fn token_count(&self) -> usize {
        self.ends.len()
    }

    /// The index of the token that contains `offset`: the first one that ends after it (the first
    /// token for a negative offset, the last for an offset past the end).
    pub fn find_token_index_at_offset(&self, offset: isize) -> usize {
        let last = self.ends.len().saturating_sub(1);
        if offset < 0 {
            return 0;
        }
        self.ends.iter().position(|&e| e as isize > offset).unwrap_or(last)
    }

    pub fn standard_token_type(&self, index: usize) -> StandardTokenType {
        self.types.get(index).copied().unwrap_or_default()
    }

    /// The type at `offset`; `Other` for an empty line.
    pub fn type_at_offset(&self, offset: isize) -> StandardTokenType {
        if self.ends.is_empty() { StandardTokenType::Other } else { self.standard_token_type(self.find_token_index_at_offset(offset)) }
    }

    /// `(start, end, type)` of each token.
    pub fn spans(&self) -> impl Iterator<Item = (usize, usize, StandardTokenType)> + '_ {
        let mut start = 0;
        self.ends.iter().zip(&self.types).map(move |(&end, &ty)| {
            let s = start;
            start = end;
            (s, end, ty)
        })
    }
}

/// `IndentAction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndentAction {
    None,
    Indent,
    IndentOutdent,
    Outdent,
}

/// `EnterAction`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterAction {
    pub indent_action: IndentAction,
    pub append_text: Option<String>,
    pub remove_text: Option<usize>,
}

impl EnterAction {
    pub fn new(indent_action: IndentAction) -> Self {
        EnterAction { indent_action, append_text: None, remove_text: None }
    }
}

/// `OnEnterRule`.
#[derive(Debug, Clone)]
pub struct OnEnterRule {
    pub before_text: JsRegex,
    pub after_text: Option<JsRegex>,
    pub previous_line_text: Option<JsRegex>,
    pub action: EnterAction,
}

/// `IndentationRule`.
#[derive(Debug, Clone)]
pub struct IndentationRule {
    pub decrease_indent_pattern: JsRegex,
    pub increase_indent_pattern: JsRegex,
    pub indent_next_line_pattern: Option<JsRegex>,
    pub un_indented_line_pattern: Option<JsRegex>,
}

/// `IAutoClosingPairConditional` (`IAutoClosingPair` when `not_in` is empty and there is no `before_text`).
#[derive(Debug, Clone, PartialEq)]
pub struct AutoClosingPairConfig {
    pub open: String,
    pub close: String,
    pub not_in: Vec<String>,
    pub before_text: Option<JsRegex>,
}

impl AutoClosingPairConfig {
    pub fn new(open: &str, close: &str) -> Self {
        AutoClosingPairConfig { open: open.into(), close: close.into(), not_in: Vec::new(), before_text: None }
    }

    pub fn not_in(mut self, kinds: &[&str]) -> Self {
        self.not_in = kinds.iter().map(|s| s.to_string()).collect();
        self
    }
}

/// `CommentRule`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommentRule {
    pub line_comment: Option<String>,
    /// A line comment that is written at column 1 (`lineComment: { comment, noIndent }`).
    pub line_comment_no_indent: bool,
    pub block_comment: Option<(String, String)>,
}

/// `LanguageConfiguration`.
#[derive(Debug, Clone, Default)]
pub struct LanguageConfiguration {
    pub comments: Option<CommentRule>,
    pub brackets: Option<Vec<(String, String)>>,
    pub word_pattern: Option<JsRegex>,
    pub indentation_rules: Option<IndentationRule>,
    pub on_enter_rules: Option<Vec<OnEnterRule>>,
    pub auto_closing_pairs: Option<Vec<AutoClosingPairConfig>>,
    pub surrounding_pairs: Option<Vec<(String, String)>>,
    pub auto_close_before: Option<String>,
}

// ---- JSON ---------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(untagged)]
enum RawRegex {
    Plain(String),
    Object { pattern: String, #[serde(default)] flags: String },
}

impl RawRegex {
    fn compile(&self) -> Option<JsRegex> {
        match self {
            RawRegex::Plain(p) => JsRegex::new(p, ""),
            RawRegex::Object { pattern, flags } => JsRegex::new(pattern, flags),
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawPair {
    Pair((String, String)),
    Object { open: String, close: String, #[serde(default, rename = "notIn")] not_in: Vec<String>, #[serde(default, rename = "beforeText")] before_text: Option<RawRegex> },
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawLineComment {
    Plain(String),
    Object { comment: String, #[serde(default, rename = "noIndent")] no_indent: bool },
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawComments {
    line_comment: Option<RawLineComment>,
    block_comment: Option<(String, String)>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawIndentationRules {
    decrease_indent_pattern: RawRegex,
    increase_indent_pattern: RawRegex,
    indent_next_line_pattern: Option<RawRegex>,
    un_indented_line_pattern: Option<RawRegex>,
}

#[derive(Deserialize)]
struct RawAction {
    indent: String,
    #[serde(default, rename = "appendText")]
    append_text: Option<String>,
    #[serde(default, rename = "removeText")]
    remove_text: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOnEnterRule {
    before_text: RawRegex,
    after_text: Option<RawRegex>,
    previous_line_text: Option<RawRegex>,
    action: RawAction,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct RawConfiguration {
    comments: Option<RawComments>,
    brackets: Option<Vec<(String, String)>>,
    word_pattern: Option<RawRegex>,
    indentation_rules: Option<RawIndentationRules>,
    on_enter_rules: Option<Vec<RawOnEnterRule>>,
    auto_closing_pairs: Option<Vec<RawPair>>,
    surrounding_pairs: Option<Vec<RawPair>>,
    auto_close_before: Option<String>,
}

fn parse_indent_action(s: &str) -> IndentAction {
    match s {
        "indent" => IndentAction::Indent,
        "indentOutdent" => IndentAction::IndentOutdent,
        "outdent" => IndentAction::Outdent,
        _ => IndentAction::None,
    }
}

impl LanguageConfiguration {
    /// Parses the JSON of a language configuration (the keys `scripts/bundle-grammars` copies).
    /// A rule whose regular expression does not compile is dropped, like upstream's
    /// `onUnexpectedError` on a bad pattern.
    pub fn from_json(json: &str) -> Result<LanguageConfiguration, serde_json::Error> {
        let raw: RawConfiguration = serde_json::from_str(json)?;
        Ok(Self::from_raw(raw))
    }

    fn from_raw(raw: RawConfiguration) -> LanguageConfiguration {
        let comments = raw.comments.map(|c| {
            let (line_comment, line_comment_no_indent) = match c.line_comment {
                Some(RawLineComment::Plain(s)) => (Some(s), false),
                Some(RawLineComment::Object { comment, no_indent }) => (Some(comment), no_indent),
                None => (None, false),
            };
            CommentRule { line_comment, line_comment_no_indent, block_comment: c.block_comment }
        });
        let indentation_rules = raw.indentation_rules.and_then(|r| {
            Some(IndentationRule {
                decrease_indent_pattern: r.decrease_indent_pattern.compile()?,
                increase_indent_pattern: r.increase_indent_pattern.compile()?,
                indent_next_line_pattern: r.indent_next_line_pattern.and_then(|x| x.compile()),
                un_indented_line_pattern: r.un_indented_line_pattern.and_then(|x| x.compile()),
            })
        });
        let on_enter_rules = raw.on_enter_rules.map(|rules| {
            rules
                .into_iter()
                .filter_map(|r| {
                    Some(OnEnterRule {
                        before_text: r.before_text.compile()?,
                        after_text: r.after_text.and_then(|x| x.compile()),
                        previous_line_text: r.previous_line_text.and_then(|x| x.compile()),
                        action: EnterAction {
                            indent_action: parse_indent_action(&r.action.indent),
                            append_text: r.action.append_text,
                            remove_text: r.action.remove_text,
                        },
                    })
                })
                .collect()
        });
        let pair = |p: RawPair| match p {
            RawPair::Pair((open, close)) => AutoClosingPairConfig { open, close, not_in: Vec::new(), before_text: None },
            RawPair::Object { open, close, not_in, before_text } => {
                AutoClosingPairConfig { open, close, not_in, before_text: before_text.and_then(|r| r.compile()) }
            }
        };
        LanguageConfiguration {
            comments,
            brackets: raw.brackets,
            word_pattern: raw.word_pattern.and_then(|r| r.compile()),
            indentation_rules,
            on_enter_rules,
            auto_closing_pairs: raw.auto_closing_pairs.map(|v| v.into_iter().map(pair).collect()),
            surrounding_pairs: raw.surrounding_pairs.map(|v| {
                v.into_iter()
                    .map(|p| {
                        let c = pair(p);
                        (c.open, c.close)
                    })
                    .collect()
            }),
            auto_close_before: raw.auto_close_before,
        }
    }
}

// ---- Resolved support ---------------------------------------------------------------------------

/// upstream `ResolvedLanguageConfiguration`: the configuration and the supports built from it.
#[derive(Debug)]
pub struct LanguageSupport {
    pub config: LanguageConfiguration,
    pub character_pair: CharacterPairSupport,
    pub on_enter: Option<OnEnterSupport>,
    pub indent_rules: Option<IndentRulesSupport>,
}

impl LanguageSupport {
    pub fn new(config: LanguageConfiguration) -> LanguageSupport {
        let on_enter = (config.brackets.is_some() || config.indentation_rules.is_some() || config.on_enter_rules.is_some())
            .then(|| OnEnterSupport::new(config.brackets.clone(), config.on_enter_rules.clone()));
        let indent_rules = config.indentation_rules.clone().map(IndentRulesSupport::new);
        let character_pair = CharacterPairSupport::new(&config);
        LanguageSupport { config, character_pair, on_enter, indent_rules }
    }

    /// `onEnter` of the resolved configuration: `None` without brackets, indentation rules or on-enter rules.
    pub fn on_enter(&self, auto_indent: AutoIndentStrategy, previous_line: &str, before: &str, after: &str) -> Option<EnterAction> {
        self.on_enter.as_ref()?.on_enter(auto_indent, previous_line, before, after)
    }

    /// The bracket pairs of the language (`brackets`), empty when it has none.
    pub fn brackets(&self) -> &[(String, String)] {
        self.config.brackets.as_deref().unwrap_or(&[])
    }

    /// The comment tokens, if the language has any.
    pub fn comments(&self) -> Option<&CommentRule> {
        self.config.comments.as_ref()
    }

    /// upstream `BracketElectricCharacterSupport.getElectricCharacters`: the last character of
    /// every closing bracket.
    pub fn electric_characters(&self) -> Vec<char> {
        let mut out: Vec<char> = Vec::new();
        for (_, close) in self.brackets() {
            if let Some(c) = close.chars().last()
                && !out.contains(&c)
            {
                out.push(c);
            }
        }
        out
    }
}

/// upstream `EditorAutoIndentStrategy`; ordered, so `>=` compares levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AutoIndentStrategy {
    None,
    Keep,
    Brackets,
    Advanced,
    Full,
}

const CONFIGURATIONS_JSON: &str = include_str!("../data/language-configurations.json");

fn configuration_table() -> &'static HashMap<String, serde_json::Value> {
    static MAP: OnceLock<HashMap<String, serde_json::Value>> = OnceLock::new();
    MAP.get_or_init(|| serde_json::from_str(CONFIGURATIONS_JSON).expect("bundled language-configurations.json is valid"))
}

/// The configuration path (`rust/language-configuration.json`) of every bundled language.
fn configuration_path(language_id: &str) -> Option<String> {
    crate::bundled::language_contributions().into_iter().find(|l| l.id == language_id).and_then(|l| l.configuration)
}

/// The configuration upstream registers for plain text: brackets and surrounding pairs.
fn plaintext_configuration() -> LanguageConfiguration {
    let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
    LanguageConfiguration {
        brackets: Some(vec![pair("(", ")"), pair("[", "]"), pair("{", "}")]),
        surrounding_pairs: Some(vec![pair("{", "}"), pair("[", "]"), pair("(", ")"), pair("<", ">"), pair("\"", "\""), pair("'", "'"), pair("`", "`")]),
        ..Default::default()
    }
}

/// The resolved configuration of a language: bundled data for the built-in languages, upstream's
/// own for plain text, and an empty one (no pairs, no rules) for anything else. Cached.
pub fn language_support(language_id: &str) -> Arc<LanguageSupport> {
    static CACHE: OnceLock<Mutex<HashMap<String, Arc<LanguageSupport>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(s) = cache.get(language_id) {
        return s.clone();
    }
    let config = if language_id == crate::PLAINTEXT_LANGUAGE_ID {
        plaintext_configuration()
    } else {
        configuration_path(language_id)
            .and_then(|path| configuration_table().get(&path).cloned())
            .and_then(|v| LanguageConfiguration::from_json(&v.to_string()).ok())
            .unwrap_or_default()
    };
    let support = Arc::new(LanguageSupport::new(config));
    cache.insert(language_id.to_string(), support.clone());
    support
}

/// Every bundled configuration path (for the test that compiles all their regular expressions).
pub fn bundled_configurations() -> Vec<(String, String)> {
    configuration_table().iter().map(|(k, v)| (k.clone(), v.to_string())).collect()
}
