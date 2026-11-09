//! `tokenColors` from theme JSON, and the bundled Dark Modern chain.
//!
//! Until the theme service lands this is where the token colors come from; it exposes them
//! through `TokenColorsProvider`, the trait the theme service will implement instead.

use serde_json::Value;
use vsr_languages::bundled::THEME_FILES;
use vsr_textmate::{RawThemeRule, TokenColorsProvider};

/// `tokenColors` of the theme file `name` with the themes it `include`s first (upstream's theme
/// loading: the included theme's rules come before the including theme's own).
pub fn token_colors_of(files: &[(&str, &str)], name: &str) -> Vec<RawThemeRule> {
    let Some((_, json)) = files.iter().find(|(n, _)| *n == name) else { return Vec::new() };
    let Ok(doc) = serde_json::from_str::<Value>(json) else { return Vec::new() };
    let mut rules = Vec::new();
    if let Some(include) = doc.get("include").and_then(Value::as_str) {
        let base = include.rsplit('/').next().unwrap_or(include);
        rules.extend(token_colors_of(files, base));
    }
    if let Some(entries) = doc.get("tokenColors").and_then(Value::as_array) {
        rules.extend(entries.iter().filter_map(parse_rule));
    }
    rules
}

/// One `tokenColors` entry. Entries without `settings` are skipped, like upstream.
pub fn parse_rule(entry: &Value) -> Option<RawThemeRule> {
    let settings = entry.get("settings")?.as_object()?;
    let (scope, scope_is_string) = match entry.get("scope") {
        Some(Value::String(s)) => (vec![s.clone()], true),
        Some(Value::Array(a)) => (a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect(), false),
        _ => (Vec::new(), false),
    };
    let get = |k: &str| settings.get(k).and_then(Value::as_str).map(str::to_string);
    Some(RawThemeRule {
        scope,
        scope_is_string,
        font_style: get("fontStyle"),
        foreground: get("foreground"),
        background: get("background"),
    })
}

/// The bundled VS Code Dark Modern theme (dark_modern, dark_plus, dark_vs).
pub struct BundledTheme {
    file: &'static str,
}

impl BundledTheme {
    pub fn dark_modern() -> Self {
        Self { file: "dark_modern.json" }
    }

    pub fn dark_plus() -> Self {
        Self { file: "dark_plus.json" }
    }

    pub fn dark_vs() -> Self {
        Self { file: "dark_vs.json" }
    }
}

impl TokenColorsProvider for BundledTheme {
    fn token_colors(&self) -> Vec<RawThemeRule> {
        token_colors_of(THEME_FILES, self.file)
    }
}
