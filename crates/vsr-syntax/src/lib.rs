//! Syntax highlighting service (ADR-0038): the bundled grammars, per-document incremental
//! tokenization (on a background thread for the bulk), and styles from a theme's `tokenColors`.
//!
//! upstream: src/vs/editor/common/model/tokens/tokenizationTextModelPart.ts @ 50e1350fc78d
//! - `SyntaxService`: language of a file (`files.associations` included), grammar per language,
//!   token styles under the current theme
//! - `DocumentTokens` / `BackgroundTokenizer`: per-line tokens with the rule stack per line;
//!   an edit re-tokenizes from the first changed line until the state converges
//! - token colors: `SyntaxService::set_token_colors` takes the active theme's rules (the app's
//!   theme service provides them, ADR-0037); `BundledTheme` is the Dark Modern rules for tests

mod service;
mod source;
mod theme;
mod tokens;

pub use service::{DEFAULT_MAX_TOKENIZATION_LINE_LENGTH, StyleRun, SyntaxService};
pub use source::BundledGrammars;
pub use theme::{BundledTheme, parse_rule, token_colors_of};
pub use tokens::{BackgroundTokenizer, DocumentTokens, LineSource};
