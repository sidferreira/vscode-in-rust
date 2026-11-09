//! The service the workbench talks to: which language a file has, which grammar tokenizes it,
//! and the style of a token under the current theme.
//!
//! upstream: src/vs/workbench/services/textMate/common/TMGrammarFactory.ts @ 50e1350fc78d

use std::collections::HashMap;
use std::sync::{Arc, Mutex, RwLock};

use vsr_languages::bundled::builtin_registry;
use vsr_languages::{LanguageNameIdPair, LanguagesRegistry};
use vsr_textmate::{Grammar, ScopeId, Theme, ThemeStyles, Token, TokenColorsProvider, TokenStyle};

use crate::source::BundledGrammars;
use crate::theme::BundledTheme;
use crate::tokens::BackgroundTokenizer;

/// `editor.maxTokenizationLineLength` default.
pub const DEFAULT_MAX_TOKENIZATION_LINE_LENGTH: usize = 20_000;

/// A run of text with one style: a byte range of the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleRun {
    pub start: usize,
    pub end: usize,
    pub style: TokenStyle,
}

pub struct SyntaxService {
    registry: RwLock<LanguagesRegistry>,
    grammars: BundledGrammars,
    loaded: Mutex<HashMap<String, Option<Arc<Grammar>>>>,
    styles: RwLock<Arc<ThemeStyles>>,
}

impl Default for SyntaxService {
    fn default() -> Self {
        Self::new()
    }
}

impl SyntaxService {
    /// With every built-in language and the bundled Dark Modern token colors.
    pub fn new() -> Self {
        Self {
            registry: RwLock::new(builtin_registry()),
            grammars: BundledGrammars::new(),
            loaded: Mutex::new(HashMap::new()),
            styles: RwLock::new(Arc::new(ThemeStyles::new(Theme::from_rules(&BundledTheme::dark_modern().token_colors())))),
        }
    }

    // --- languages ---

    /// `files.associations`: `(pattern, language id)` pairs that win over the built-in ones.
    pub fn set_configured_associations(&self, associations: &[(String, String)]) {
        self.registry.write().expect("registry lock").set_configured_associations(associations);
    }

    /// The language id of a file: configured associations, then file name and extension, then the
    /// first line; plain text when nothing matches.
    pub fn language_for_file(&self, path: &str, first_line: Option<&str>) -> String {
        self.registry.read().expect("registry lock").language_for_file(Some(path), first_line)
    }

    pub fn language_name(&self, id: &str) -> Option<String> {
        self.registry.read().expect("registry lock").language_name(id).map(str::to_string)
    }

    /// The display name for the status bar: the language's name, `Plain Text` when unknown.
    pub fn language_display_name(&self, id: &str) -> String {
        self.language_name(id).unwrap_or_else(|| id.to_string())
    }

    pub fn language_id_by_name(&self, name: &str) -> Option<String> {
        self.registry.read().expect("registry lock").language_id_by_language_name(name).map(str::to_string)
    }

    pub fn is_registered_language(&self, id: &str) -> bool {
        self.registry.read().expect("registry lock").is_registered_language_id(id)
    }

    pub fn sorted_language_names(&self) -> Vec<LanguageNameIdPair> {
        self.registry.read().expect("registry lock").sorted_registered_language_names()
    }

    /// The extensions of a language (for the Quick Pick description).
    pub fn language_extensions(&self, id: &str) -> Vec<String> {
        self.registry.read().expect("registry lock").extensions(id).to_vec()
    }

    // --- grammars ---

    /// The grammar of a language, built on first use. `None` for languages without a bundled one.
    pub fn grammar_for_language(&self, language_id: &str) -> Option<Arc<Grammar>> {
        let mut loaded = self.loaded.lock().expect("grammar cache lock");
        if let Some(g) = loaded.get(language_id) {
            return g.clone();
        }
        let g = self
            .grammars
            .scope_for_language(language_id)
            .and_then(|scope| self.grammars.load(scope).ok())
            .map(Arc::new);
        loaded.insert(language_id.to_string(), g.clone());
        g
    }

    /// A background tokenizer for a document of `language_id`, or `None` without a grammar.
    pub fn tokenizer_for(&self, language_id: &str, max_line_length: usize) -> Option<BackgroundTokenizer> {
        Some(BackgroundTokenizer::new(self.grammar_for_language(language_id)?, max_line_length))
    }

    // --- theme ---

    /// Replaces the token colors (a theme change). Tokens are kept: only styles change.
    pub fn set_token_colors(&self, provider: &dyn TokenColorsProvider) {
        *self.styles.write().expect("styles lock") = Arc::new(ThemeStyles::new(Theme::from_rules(&provider.token_colors())));
    }

    pub fn style_of(&self, grammar: &Grammar, scope: ScopeId) -> TokenStyle {
        let styles = self.styles.read().expect("styles lock").clone();
        grammar.with_scopes(|s| {
            // Style resolution only reads the interner; `names`/`parent` are cheap.
            let snapshot_names = |id: ScopeId| s.names(id);
            let snapshot_parent = |id: ScopeId| s.parent(id);
            styles.style_of(scope, &snapshot_names, &snapshot_parent)
        })
    }

    /// The runs of a line: adjacent tokens with the same style are merged.
    pub fn style_runs(&self, grammar: &Grammar, line_len: usize, tokens: &[Token]) -> Vec<StyleRun> {
        let styles = self.styles.read().expect("styles lock").clone();
        let mut runs: Vec<StyleRun> = Vec::with_capacity(tokens.len());
        grammar.with_scopes(|s| {
            let names = |id: ScopeId| s.names(id);
            let parent = |id: ScopeId| s.parent(id);
            for (i, t) in tokens.iter().enumerate() {
                let start = t.start as usize;
                let end = tokens.get(i + 1).map_or(line_len, |n| n.start as usize).min(line_len);
                if start >= end {
                    continue;
                }
                let style = styles.style_of(t.scope, &names, &parent);
                match runs.last_mut() {
                    Some(last) if last.style == style && last.end == start => last.end = end,
                    _ => runs.push(StyleRun { start, end, style }),
                }
            }
        });
        runs
    }
}
