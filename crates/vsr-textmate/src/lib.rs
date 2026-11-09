//! A TextMate grammar engine with vscode-textmate's semantics, on Oniguruma (`onig`), plus theme
//! scope matching (ADR-0038).
//!
//! - `raw`: the `.tmLanguage.json` document
//! - `rules`: rule compilation and `include` resolution
//! - `regex`: regex sources (anchors, back references) and the scanner
//! - `grammar`: the tokenizer (rule stack state, captures, `while`, injections)
//! - `scope`: interned scope stacks, selector matchers, standard token types
//! - `theme`: `tokenColors` matching and per-scope style resolution
//!
//! Grammars are loaded through a `GrammarSource`; `vsr-syntax` implements it on the bundled
//! grammars and drives incremental, per-line tokenization of documents.

pub mod grammar;
pub mod raw;
pub mod regex;
pub mod rules;
pub mod scope;
pub mod theme;

pub use grammar::{Grammar, LineTokens, StateStack, Token};
pub use raw::{RawGrammar, RawRule};
pub use rules::GrammarSource;
pub use scope::{ScopeId, TokenType};
pub use theme::{RawThemeRule, Rgba, Theme, ThemeStyles, TokenColorsProvider, TokenStyle};
