//! The language registry: language ids, display names, mime types, extensions, and the file
//! associations behind them.
//!
//! upstream: src/vs/editor/common/services/languagesRegistry.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/languages/modesRegistry.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/language/common/languageService.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/services/languagesRegistry.test.ts @ 50e1350fc78d
//!
//! Not ported: the numeric language id codec (token metadata packs language ids; our tokens
//! carry scope ids instead), language icons, change events, and the configuration registry
//! "override identifiers" (`[language]` settings keys are the configuration service's job).

use std::collections::HashMap;
use std::sync::Arc;

use onig::Regex;
use serde::Deserialize;

use crate::associations::{Associations, LanguageAssociation, MIME_TEXT, PLAINTEXT_LANGUAGE_ID};

/// A `contributes.languages` entry (upstream `ILanguageExtensionPoint`).
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LanguageExtensionPoint {
    pub id: String,
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    #[serde(default)]
    pub filenames: Option<Vec<String>>,
    #[serde(default)]
    pub filename_patterns: Option<Vec<String>>,
    #[serde(default)]
    pub first_line: Option<String>,
    #[serde(default)]
    pub aliases: Option<Vec<String>>,
    #[serde(default)]
    pub mimetypes: Option<Vec<String>>,
    #[serde(default)]
    pub configuration: Option<String>,
}

impl LanguageExtensionPoint {
    pub fn new(id: &str) -> Self {
        Self { id: id.to_string(), ..Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageNameIdPair {
    pub language_name: String,
    pub language_id: String,
}

#[derive(Debug, Default)]
struct ResolvedLanguage {
    name: Option<String>,
    mimetypes: Vec<String>,
    aliases: Vec<String>,
    extensions: Vec<String>,
    filenames: Vec<String>,
    configuration_files: Vec<String>,
}

#[derive(Debug, Default)]
pub struct LanguagesRegistry {
    order: Vec<String>,
    languages: HashMap<String, ResolvedLanguage>,
    mime_types_map: HashMap<String, String>,
    name_map: Vec<(String, String)>,
    lowercase_name_map: HashMap<String, String>,
    associations: Associations,
}

impl LanguagesRegistry {
    /// An empty registry (upstream `new LanguagesRegistry(false)`).
    pub fn new() -> Self {
        Self::default()
    }

    /// A registry with the plain text language upstream's `ModesRegistry` always registers.
    pub fn with_plaintext() -> Self {
        let mut r = Self::new();
        r.register_languages(&[plaintext()]);
        r
    }

    pub fn associations(&self) -> &Associations {
        &self.associations
    }

    /// `registerLanguages`: merges into existing languages with the same id.
    pub fn register_languages(&mut self, descs: &[LanguageExtensionPoint]) {
        for d in descs {
            self.register_language(d);
        }
        // Rebuild the fast path maps.
        self.mime_types_map.clear();
        self.name_map.clear();
        self.lowercase_name_map.clear();
        for id in &self.order {
            let language = &self.languages[id];
            if let Some(name) = &language.name {
                match self.name_map.iter_mut().find(|(n, _)| n == name) {
                    Some(e) => e.1 = id.clone(),
                    None => self.name_map.push((name.clone(), id.clone())),
                }
            }
            for alias in &language.aliases {
                self.lowercase_name_map.insert(alias.to_lowercase(), id.clone());
            }
            for mime in &language.mimetypes {
                self.mime_types_map.insert(mime.clone(), id.clone());
            }
        }
    }

    fn register_language(&mut self, lang: &LanguageExtensionPoint) {
        let lang_id = &lang.id;
        if !self.languages.contains_key(lang_id) {
            self.order.push(lang_id.clone());
            self.languages.insert(lang_id.clone(), ResolvedLanguage::default());
        }
        let resolved = self.languages.get_mut(lang_id).expect("just inserted");

        let mut primary_mime = None;
        if let Some(m) = lang.mimetypes.as_ref().filter(|m| !m.is_empty()) {
            resolved.mimetypes.extend(m.iter().cloned());
            primary_mime = Some(m[0].clone());
        }
        let primary_mime = primary_mime.unwrap_or_else(|| {
            let m = format!("text/x-{lang_id}");
            resolved.mimetypes.push(m.clone());
            m
        });

        if let Some(exts) = &lang.extensions {
            if lang.configuration.is_some() {
                // Insert first as this appears to be the primary language definition.
                let mut v = exts.clone();
                v.append(&mut resolved.extensions);
                resolved.extensions = v;
            } else {
                resolved.extensions.extend(exts.iter().cloned());
            }
            for e in exts {
                self.associations.register_platform(LanguageAssociation {
                    id: lang_id.clone(),
                    mime: primary_mime.clone(),
                    extension: Some(e.clone()),
                    ..Default::default()
                });
            }
        }
        if let Some(names) = &lang.filenames {
            for f in names {
                self.associations.register_platform(LanguageAssociation {
                    id: lang_id.clone(),
                    mime: primary_mime.clone(),
                    filename: Some(f.clone()),
                    ..Default::default()
                });
                resolved.filenames.push(f.clone());
            }
        }
        if let Some(patterns) = &lang.filename_patterns {
            for p in patterns {
                self.associations.register_platform(LanguageAssociation {
                    id: lang_id.clone(),
                    mime: primary_mime.clone(),
                    filepattern: Some(p.clone()),
                    ..Default::default()
                });
            }
        }
        if let Some(first) = lang.first_line.as_ref().filter(|s| !s.is_empty()) {
            let source = if first.starts_with('^') { first.clone() } else { format!("^{first}") };
            // A bad regular expression is ignored, like upstream (it logs a warning).
            if let Ok(re) = Regex::new(&source)
                && !leads_to_endless_loop(&source, &re)
            {
                self.associations.register_platform(LanguageAssociation {
                    id: lang_id.clone(),
                    mime: primary_mime.clone(),
                    firstline: Some(Arc::new(re)),
                    ..Default::default()
                });
            }
        }

        resolved.aliases.push(lang_id.clone());

        // `None` means no `aliases` key; an empty array means "this language gets no name".
        let lang_aliases: Option<Vec<Option<String>>> = lang.aliases.as_ref().map(|a| {
            if a.is_empty() {
                vec![None]
            } else {
                a.iter().cloned().map(Some).collect()
            }
        });
        if let Some(aliases) = &lang_aliases {
            for alias in aliases.iter().flatten().filter(|a| !a.is_empty()) {
                resolved.aliases.push(alias.clone());
            }
        }
        let contains_aliases = lang_aliases.as_ref().is_some_and(|a| !a.is_empty());
        let no_name = contains_aliases && lang_aliases.as_ref().is_some_and(|a| a[0].is_none());
        if !no_name {
            let first_alias = lang_aliases.as_ref().and_then(|a| a.first().cloned().flatten());
            let best_name = first_alias.filter(|a| !a.is_empty()).unwrap_or_else(|| lang_id.clone());
            if contains_aliases || resolved.name.is_none() {
                resolved.name = Some(best_name);
            }
        }

        if let Some(c) = &lang.configuration {
            resolved.configuration_files.push(c.clone());
        }
    }

    pub fn is_registered_language_id(&self, id: &str) -> bool {
        self.languages.contains_key(id)
    }

    pub fn registered_language_ids(&self) -> &[String] {
        &self.order
    }

    /// Names (aliases[0], or the id) with their ids, sorted case-insensitively.
    pub fn sorted_registered_language_names(&self) -> Vec<LanguageNameIdPair> {
        let mut out: Vec<LanguageNameIdPair> = self
            .name_map
            .iter()
            .map(|(n, id)| LanguageNameIdPair { language_name: n.clone(), language_id: id.clone() })
            .collect();
        out.sort_by_key(|p| p.language_name.to_lowercase());
        out
    }

    pub fn language_name(&self, id: &str) -> Option<&str> {
        self.languages.get(id)?.name.as_deref()
    }

    pub fn mime_type(&self, id: &str) -> Option<&str> {
        self.languages.get(id)?.mimetypes.first().map(String::as_str)
    }

    pub fn extensions(&self, id: &str) -> &[String] {
        self.languages.get(id).map_or(&[], |l| &l.extensions)
    }

    pub fn filenames(&self, id: &str) -> &[String] {
        self.languages.get(id).map_or(&[], |l| &l.filenames)
    }

    pub fn configuration_files(&self, id: &str) -> &[String] {
        self.languages.get(id).map_or(&[], |l| &l.configuration_files)
    }

    pub fn language_id_by_language_name(&self, name: &str) -> Option<&str> {
        self.lowercase_name_map.get(&name.to_lowercase()).map(String::as_str)
    }

    pub fn language_id_by_mime_type(&self, mime: &str) -> Option<&str> {
        self.mime_types_map.get(mime).map(String::as_str)
    }

    /// `guessLanguageIdByFilepathOrFirstLine`: the guessed ids, best first, empty without input.
    pub fn guess_language_ids(&self, path: Option<&str>, first_line: Option<&str>) -> Vec<String> {
        if path.is_none_or(str::is_empty) && first_line.is_none_or(str::is_empty) {
            return Vec::new();
        }
        self.associations.language_ids(path, first_line)
    }

    /// The language of a file: the guess if it is registered, else plain text
    /// (`createByFilepathOrFirstLine`).
    pub fn language_for_file(&self, path: Option<&str>, first_line: Option<&str>) -> String {
        match self.guess_language_ids(path, first_line).into_iter().next() {
            Some(id) if self.is_registered_language_id(&id) => id,
            _ => PLAINTEXT_LANGUAGE_ID.to_string(),
        }
    }

    /// `files.associations`: replaces the user configured associations (upstream `updateMime`).
    /// Entries whose value is not a string are skipped by the caller.
    pub fn set_configured_associations(&mut self, associations: &[(String, String)]) {
        self.associations.clear_configured();
        for (pattern, lang_id) in associations {
            let mime = self
                .mime_type(lang_id)
                .map(str::to_string)
                .unwrap_or_else(|| format!("text/x-{lang_id}"));
            self.associations.register_configured(LanguageAssociation {
                id: lang_id.clone(),
                mime,
                filepattern: Some(pattern.clone()),
                ..Default::default()
            });
        }
    }
}

/// The plain text language (upstream `modesRegistry.ts`).
pub fn plaintext() -> LanguageExtensionPoint {
    LanguageExtensionPoint {
        id: PLAINTEXT_LANGUAGE_ID.into(),
        extensions: Some(vec![".txt".into()]),
        aliases: Some(vec!["Plain Text".into(), "text".into()]),
        mimetypes: Some(vec![MIME_TEXT.into()]),
        ..Default::default()
    }
}

/// Upstream `regExpLeadsToEndlessLoop`: a regex that matches the empty string would not advance.
fn leads_to_endless_loop(source: &str, re: &Regex) -> bool {
    if matches!(source, "^" | "^$" | "$" | "^\\s*$") {
        return false;
    }
    re.find("").is_some()
}
