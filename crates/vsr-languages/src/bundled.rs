//! The bundled data (see `scripts/bundle-grammars`).
//!
//! upstream: extensions/rust/package.json @ 50e1350fc78d

use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Deserialize;

use crate::bundled_files::GRAMMAR_FILES;
use crate::registry::{LanguageExtensionPoint, LanguagesRegistry, plaintext};

const LANGUAGES_JSON: &str = include_str!("../data/languages.json");
const GRAMMARS_JSON: &str = include_str!("../data/grammars.json");

/// Dark Modern and the themes it includes, as `(file name, JSON)`.
pub static THEME_FILES: &[(&str, &str)] = &[
    ("dark_vs.json", include_str!("../data/themes/dark_vs.json")),
    ("dark_plus.json", include_str!("../data/themes/dark_plus.json")),
    ("dark_modern.json", include_str!("../data/themes/dark_modern.json")),
];

/// A `contributes.grammars` entry.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GrammarContribution {
    pub scope_name: String,
    #[serde(default)]
    pub language: Option<String>,
    #[serde(default)]
    pub embedded_languages: HashMap<String, String>,
    /// Selector to standard token type name (`"meta.tag string.quoted": "other"`).
    #[serde(default)]
    pub token_types: HashMap<String, String>,
    /// The grammars (by scope name) this grammar injects into.
    #[serde(default)]
    pub inject_to: Vec<String>,
}

/// The `contributes.languages` of every built-in extension, in registration order.
pub fn language_contributions() -> Vec<LanguageExtensionPoint> {
    serde_json::from_str(LANGUAGES_JSON).expect("bundled languages.json is valid")
}

pub fn grammar_contributions() -> Vec<GrammarContribution> {
    serde_json::from_str(GRAMMARS_JSON).expect("bundled grammars.json is valid")
}

/// The JSON text of a bundled grammar.
pub fn grammar_json(scope_name: &str) -> Option<&'static str> {
    GRAMMAR_FILES.iter().find(|(s, _)| *s == scope_name).map(|(_, j)| *j)
}

/// The scope name of the grammar that tokenizes `language_id`, if one is bundled.
pub fn scope_name_for_language(language_id: &str) -> Option<&'static str> {
    static MAP: OnceLock<HashMap<String, String>> = OnceLock::new();
    MAP.get_or_init(|| {
        grammar_contributions()
            .into_iter()
            .filter_map(|g| g.language.map(|l| (l, g.scope_name)))
            .collect()
    })
    .get(language_id)
    .map(String::as_str)
}

/// The `folding` section of a language configuration (upstream `FoldingRules`): whether the
/// language is indentation based (`offSide`: blank lines belong to the block above), and the
/// regular expressions (JavaScript syntax, as written in `language-configuration.json`) of its
/// region markers, such as `//#region` and `//#endregion`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldingRules {
    #[serde(default)]
    pub off_side: bool,
    #[serde(default)]
    pub markers: Option<FoldingMarkerPatterns>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FoldingMarkerPatterns {
    pub start: String,
    pub end: String,
}

fn folding_table() -> &'static HashMap<String, FoldingRules> {
    #[derive(Deserialize)]
    struct Entry {
        id: String,
        folding: Option<FoldingRules>,
    }
    static MAP: OnceLock<HashMap<String, FoldingRules>> = OnceLock::new();
    MAP.get_or_init(|| {
        let entries: Vec<Entry> = serde_json::from_str(LANGUAGES_JSON).expect("bundled languages.json is valid");
        entries.into_iter().filter_map(|e| e.folding.map(|f| (e.id, f))).collect()
    })
}

/// The folding rules of `language_id`, `None` when its configuration has none.
pub fn folding_rules(language_id: &str) -> Option<&'static FoldingRules> {
    folding_table().get(language_id)
}

/// The block comment tokens of `language_id` (`/*` and `*/`), from its language configuration.
pub fn block_comment(language_id: &str) -> Option<(&'static str, &'static str)> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Entry {
        id: String,
        block_comment: Option<(String, String)>,
    }
    static MAP: OnceLock<HashMap<String, (String, String)>> = OnceLock::new();
    MAP.get_or_init(|| {
        let entries: Vec<Entry> = serde_json::from_str(LANGUAGES_JSON).expect("bundled languages.json is valid");
        entries.into_iter().filter_map(|e| e.block_comment.map(|b| (e.id, b))).collect()
    })
    .get(language_id)
    .map(|(a, b)| (a.as_str(), b.as_str()))
}

/// Every language id that has folding rules.
pub fn folding_rule_language_ids() -> Vec<&'static str> {
    folding_table().keys().map(String::as_str).collect()
}

/// A registry holding plain text and every built-in language (upstream's registry after the
/// built-in extensions registered).
pub fn builtin_registry() -> LanguagesRegistry {
    let mut r = LanguagesRegistry::new();
    let mut all = vec![plaintext()];
    all.extend(language_contributions());
    r.register_languages(&all);
    r
}
