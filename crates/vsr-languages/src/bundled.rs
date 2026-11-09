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

/// A registry holding plain text and every built-in language (upstream's registry after the
/// built-in extensions registered).
pub fn builtin_registry() -> LanguagesRegistry {
    let mut r = LanguagesRegistry::new();
    let mut all = vec![plaintext()];
    all.extend(language_contributions());
    r.register_languages(&all);
    r
}
