//! The `editor.suggest*` settings the suggest list reads (ADR-0095).
//!
//! Upstream source: src/vs/editor/common/config/editorOptions.ts (`EditorSuggest`, `QuickSuggestionsOptions`,
//! `acceptSuggestionOnEnter`, `snippetSuggestions`, `wordBasedSuggestions`), suggest.ts (the kind filters in
//! `provideSuggestionItems`).

use std::time::Duration;

use serde_json::Value;
use vsr_settings::Configuration;
use vsr_suggest::accept::InsertMode;
use vsr_suggest::item::kind;
use vsr_suggest::sort::SnippetSortOrder;
use vsr_suggest::trigger::{QuickSuggestions, QuickValue, SuggestConfig, WordBased};

/// `editor.acceptSuggestionOnEnter`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AcceptOnEnter {
    On,
    /// Only when accepting changes the text.
    Smart,
    Off,
}

/// `editor.suggest.show<Kind>` keys by completion kind (`CompletionItemKind` values).
const KIND_KEYS: &[(u32, &str)] = &[
    (0, "editor.suggest.showMethods"),
    (1, "editor.suggest.showFunctions"),
    (2, "editor.suggest.showConstructors"),
    (3, "editor.suggest.showFields"),
    (4, "editor.suggest.showVariables"),
    (5, "editor.suggest.showClasses"),
    (6, "editor.suggest.showStructs"),
    (7, "editor.suggest.showInterfaces"),
    (8, "editor.suggest.showModules"),
    (9, "editor.suggest.showProperties"),
    (10, "editor.suggest.showEvents"),
    (11, "editor.suggest.showOperators"),
    (12, "editor.suggest.showUnits"),
    (13, "editor.suggest.showValues"),
    (14, "editor.suggest.showConstants"),
    (15, "editor.suggest.showEnums"),
    (16, "editor.suggest.showEnumMembers"),
    (17, "editor.suggest.showKeywords"),
    (18, "editor.suggest.showWords"),
    (19, "editor.suggest.showColors"),
    (20, "editor.suggest.showFiles"),
    (21, "editor.suggest.showReferences"),
    (22, "editor.suggest.showCustomcolors"),
    (23, "editor.suggest.showFolders"),
    (24, "editor.suggest.showTypeParameters"),
    (25, "editor.suggest.showUsers"),
    (26, "editor.suggest.showIssues"),
    (kind::SNIPPET, "editor.suggest.showSnippets"),
];

#[derive(Clone, Debug, PartialEq)]
pub struct SuggestSettings {
    pub config: SuggestConfig,
    pub accept_on_enter: AcceptOnEnter,
    pub insert_mode: InsertMode,
    pub show_icons: bool,
    pub show_inline_details: bool,
    /// `editor.snippetSuggestions: none`.
    pub hide_snippets: bool,
    /// The kinds `editor.suggest.show*` hide.
    hidden_kinds: Vec<u32>,
}

fn quick_value(v: Option<&Value>, default: QuickValue) -> QuickValue {
    match v {
        Some(Value::Bool(true)) => QuickValue::On,
        Some(Value::Bool(false)) => QuickValue::Off,
        Some(Value::String(s)) => match s.as_str() {
            "on" => QuickValue::On,
            "inline" => QuickValue::Inline,
            "off" => QuickValue::Off,
            _ => default,
        },
        _ => default,
    }
}

/// `editor.quickSuggestions`: a boolean or string for all kinds, or an object per kind.
fn quick_suggestions(v: Option<&Value>) -> QuickSuggestions {
    let d = QuickSuggestions::default();
    match v {
        Some(Value::Object(o)) => QuickSuggestions {
            other: quick_value(o.get("other"), d.other),
            comments: quick_value(o.get("comments"), d.comments),
            strings: quick_value(o.get("strings"), d.strings),
        },
        Some(v @ (Value::Bool(_) | Value::String(_))) => {
            let all = quick_value(Some(v), QuickValue::On);
            QuickSuggestions { other: all, comments: all, strings: all }
        }
        _ => d,
    }
}

fn word_based(v: Option<&Value>) -> WordBased {
    match v {
        Some(Value::Bool(false)) => WordBased::Off,
        Some(Value::String(s)) => match s.as_str() {
            "off" => WordBased::Off,
            "currentDocument" => WordBased::CurrentDocument,
            "allDocuments" => WordBased::AllDocuments,
            _ => WordBased::MatchingDocuments,
        },
        _ => WordBased::MatchingDocuments,
    }
}

impl SuggestSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let mut config = SuggestConfig {
            quick: quick_suggestions(c.get_value("editor.quickSuggestions")),
            quick_delay: Duration::from_millis(c.get_f64("editor.quickSuggestionsDelay").unwrap_or(10.0).max(0.0) as u64),
            on_trigger_characters: c.get_bool("editor.suggestOnTriggerCharacters").unwrap_or(true),
            word_based: word_based(c.get_value("editor.wordBasedSuggestions")),
            ..SuggestConfig::default()
        };
        config.options.filter_graceful = c.get_bool("editor.suggest.filterGraceful").unwrap_or(true);
        let snippets = c.get_str("editor.snippetSuggestions").unwrap_or("inline");
        config.snippet_order = match snippets {
            "top" => SnippetSortOrder::Top,
            "bottom" => SnippetSortOrder::Bottom,
            _ => SnippetSortOrder::Inline,
        };
        Self {
            config,
            accept_on_enter: match c.get_value("editor.acceptSuggestionOnEnter") {
                Some(Value::Bool(false)) => AcceptOnEnter::Off,
                Some(Value::String(s)) if s == "off" => AcceptOnEnter::Off,
                Some(Value::String(s)) if s == "smart" => AcceptOnEnter::Smart,
                _ => AcceptOnEnter::On,
            },
            insert_mode: if c.get_str("editor.suggest.insertMode") == Some("replace") { InsertMode::Replace } else { InsertMode::Insert },
            show_icons: c.get_bool("editor.suggest.showIcons").unwrap_or(true),
            show_inline_details: c.get_bool("editor.suggest.showInlineDetails").unwrap_or(true),
            hide_snippets: snippets == "none",
            hidden_kinds: KIND_KEYS.iter().filter(|(_, key)| c.get_bool(key) == Some(false)).map(|(k, _)| *k).collect(),
        }
    }

    /// Whether items of this kind may be offered.
    pub fn shows(&self, kind: u32) -> bool {
        !self.hidden_kinds.contains(&kind) && !(self.hide_snippets && kind == kind::SNIPPET)
    }
}

impl Default for SuggestSettings {
    fn default() -> Self {
        Self {
            config: SuggestConfig::default(),
            accept_on_enter: AcceptOnEnter::On,
            insert_mode: InsertMode::Insert,
            show_icons: true,
            show_inline_details: true,
            hide_snippets: false,
            hidden_kinds: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_settings::{SettingsRegistry, UserSettings};

    fn read(user: &str) -> SuggestSettings {
        SuggestSettings::from_configuration(&Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap()))
    }

    #[test]
    fn defaults_are_upstreams() {
        let s = read("{}");
        assert_eq!(s.config.quick, QuickSuggestions::default());
        assert_eq!(s.config.quick_delay, Duration::from_millis(10));
        assert!(s.config.on_trigger_characters && s.config.options.filter_graceful);
        assert_eq!((s.accept_on_enter, s.insert_mode), (AcceptOnEnter::On, InsertMode::Insert));
        assert!(s.show_icons && s.show_inline_details && s.shows(kind::SNIPPET) && s.shows(0));
    }

    #[test]
    fn quick_suggestions_take_a_boolean_a_string_or_an_object() {
        let q = |v: &str| read(&format!(r#"{{ "editor.quickSuggestions": {v} }}"#)).config.quick;
        assert!(q("false").is_all_off() && q(r#""off""#).is_all_off());
        assert!(q("true").is_all_on());
        let o = q(r#"{ "strings": "inline", "comments": true }"#);
        assert_eq!((o.other, o.comments, o.strings), (QuickValue::On, QuickValue::On, QuickValue::Inline));
    }

    #[test]
    fn word_based_off_replace_and_smart() {
        assert_eq!(read(r#"{ "editor.wordBasedSuggestions": "off" }"#).config.word_based, WordBased::Off);
        assert_eq!(read(r#"{ "editor.suggest.insertMode": "replace" }"#).insert_mode, InsertMode::Replace);
        assert_eq!(read(r#"{ "editor.acceptSuggestionOnEnter": "smart" }"#).accept_on_enter, AcceptOnEnter::Smart);
    }
}
