//! Language registry and the language/grammar/theme data bundled from VS Code's built-in
//! extensions (ADR-0038).
//!
//! `LanguagesRegistry` ports upstream's registry and file association rules. `bundled` holds the
//! data `scripts/bundle-grammars` copies from the upstream clone: every built-in extension's
//! `contributes.languages` (83 languages), the TextMate grammars of a first set of languages
//! (`contributes.grammars` and the grammar files), and the Dark Modern theme files. The grammars'
//! provenance and licences are in `data/NOTICES.md`.

mod associations;
mod bundled_files;
mod registry;

pub mod auto_indent;
pub mod character_pair;
pub mod config;
pub mod indent_rules;
pub mod on_enter;

pub mod bundled;

pub use associations::{
    Associations, IdAndMime, LanguageAssociation, MIME_TEXT, MIME_UNKNOWN, PLAINTEXT_LANGUAGE_ID,
};
pub use registry::{LanguageExtensionPoint, LanguageNameIdPair, LanguagesRegistry, plaintext};
