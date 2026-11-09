//! The bundled grammars as a `GrammarSource`, and per-language `Grammar` instances.
//!
//! upstream: src/vs/workbench/services/textMate/common/TMGrammars.ts @ 50e1350fc78d

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use vsr_languages::bundled;
use vsr_textmate::{Grammar, GrammarSource, RawGrammar};

/// Loads and caches the bundled grammar files by scope name.
pub struct BundledGrammars {
    raw: Mutex<HashMap<String, Option<Arc<RawGrammar>>>>,
    inject_to: HashMap<String, Vec<String>>,
    token_types: HashMap<String, Vec<(String, String)>>,
    language_scopes: HashMap<String, String>,
}

impl Default for BundledGrammars {
    fn default() -> Self {
        Self::new()
    }
}

impl BundledGrammars {
    pub fn new() -> Self {
        let mut inject_to: HashMap<String, Vec<String>> = HashMap::new();
        let mut token_types = HashMap::new();
        let mut language_scopes = HashMap::new();
        for g in bundled::grammar_contributions() {
            for target in &g.inject_to {
                inject_to.entry(target.clone()).or_default().push(g.scope_name.clone());
            }
            if !g.token_types.is_empty() {
                let mut v: Vec<_> = g.token_types.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                v.sort();
                token_types.insert(g.scope_name.clone(), v);
            }
            if let Some(l) = &g.language {
                language_scopes.entry(l.clone()).or_insert_with(|| g.scope_name.clone());
            }
        }
        Self { raw: Mutex::new(HashMap::new()), inject_to, token_types, language_scopes }
    }

    /// The scope name of the grammar of a language id.
    pub fn scope_for_language(&self, language_id: &str) -> Option<&str> {
        self.language_scopes.get(language_id).map(String::as_str)
    }

    /// Builds the grammar of `scope_name` (rules are compiled, regexes are not yet).
    pub fn load(&self, scope_name: &str) -> Result<Grammar, String> {
        let types = self.token_types.get(scope_name).cloned().unwrap_or_default();
        Grammar::new(self, scope_name, &types)
    }
}

impl GrammarSource for BundledGrammars {
    fn grammar(&self, scope_name: &str) -> Option<Arc<RawGrammar>> {
        let mut raw = self.raw.lock().expect("grammar cache lock");
        if let Some(g) = raw.get(scope_name) {
            return g.clone();
        }
        let g = bundled::grammar_json(scope_name).and_then(|json| RawGrammar::from_json(json).ok()).map(Arc::new);
        raw.insert(scope_name.to_string(), g.clone());
        g
    }

    fn injections_for(&self, scope_name: &str) -> Vec<String> {
        self.inject_to.get(scope_name).cloned().unwrap_or_default()
    }
}
