//! upstream: src/vs/editor/common/languageSelector.ts @ 50e1350fc78d
//!
//! `LanguageSelector` and `score`: how well a document (uri, language) matches what a provider
//! registered for. `10` is an exact match, `5` a wildcard, `0` no match.
//!
//! Glob patterns use `globset` (`literal_separator`, as VS Code's glob does); a pattern without a
//! `/` is matched against the file's base name, as upstream's `glob.match` does.

use super::uri::Uri;
use globset::GlobBuilder;
use serde_json::Value;
use std::collections::BTreeSet;

/// `LanguageFilter.pattern`: a glob, or a glob relative to a base folder (`IRelativePattern`).
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Glob(String),
    Relative { base: String, pattern: String },
}

/// `LanguageFilter`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LanguageFilter {
    pub language: Option<String>,
    pub scheme: Option<String>,
    pub pattern: Option<Pattern>,
    pub notebook_type: Option<String>,
    /// The provider lives in the UI thread, so it sees models that are not synchronized.
    pub has_access_to_all_models: bool,
    pub exclusive: bool,
    /// The provider comes from a built-in extension.
    pub is_builtin: bool,
}

/// `LanguageSelector = string | LanguageFilter | ReadonlyArray<string | LanguageFilter>`.
#[derive(Debug, Clone, PartialEq)]
pub enum LanguageSelector {
    Language(String),
    Filter(LanguageFilter),
    Many(Vec<LanguageSelector>),
}

impl From<&str> for LanguageSelector {
    fn from(s: &str) -> Self {
        LanguageSelector::Language(s.into())
    }
}

impl From<LanguageFilter> for LanguageSelector {
    fn from(f: LanguageFilter) -> Self {
        LanguageSelector::Filter(f)
    }
}

impl LanguageFilter {
    pub fn language(id: &str) -> LanguageFilter {
        LanguageFilter {
            language: Some(id.into()),
            ..Default::default()
        }
    }

    /// An `IDocumentFilterDto` (what the host sends in `$register*Support`). A relative pattern
    /// carries `baseUri`, whose `fsPath` becomes `base` (upstream's `_reviveFilter`).
    pub fn from_dto(v: &Value) -> LanguageFilter {
        let s = |k: &str| {
            v.get(k)
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let b = |k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
        let pattern = match v.get("pattern") {
            Some(Value::String(p)) if !p.is_empty() => Some(Pattern::Glob(p.clone())),
            Some(o @ Value::Object(_)) => {
                let pattern = o.get("pattern").and_then(Value::as_str).unwrap_or("");
                let base = o
                    .get("baseUri")
                    .and_then(Uri::from_components)
                    .map(|u| u.fs_path())
                    .or_else(|| o.get("base").and_then(Value::as_str).map(str::to_string))
                    .unwrap_or_default();
                Some(Pattern::Relative {
                    base,
                    pattern: pattern.into(),
                })
            }
            _ => None,
        };
        LanguageFilter {
            language: s("language"),
            scheme: s("scheme"),
            pattern,
            notebook_type: s("notebookType"),
            has_access_to_all_models: b("hasAccessToAllModels"),
            exclusive: b("exclusive"),
            is_builtin: b("isBuiltin"),
        }
    }
}

impl LanguageSelector {
    /// A list of `IDocumentFilterDto`s.
    pub fn from_dtos(v: &Value) -> LanguageSelector {
        LanguageSelector::Many(
            v.as_array()
                .into_iter()
                .flatten()
                .map(|d| match d {
                    Value::String(s) => LanguageSelector::Language(s.clone()),
                    other => LanguageSelector::Filter(LanguageFilter::from_dto(other)),
                })
                .collect(),
        )
    }
}

/// `path.normalize` for POSIX paths.
fn normalize(p: &str) -> String {
    let abs = p.starts_with('/');
    let mut out: Vec<&str> = vec![];
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if out.last().is_some_and(|l| *l != "..") {
                    out.pop();
                } else if !abs {
                    out.push("..");
                }
            }
            s => out.push(s),
        }
    }
    let joined = out.join("/");
    if abs { format!("/{joined}") } else { joined }
}

fn glob_matches(pattern: &str, path: &str) -> bool {
    let Ok(glob) = GlobBuilder::new(pattern).literal_separator(true).build() else {
        return false;
    };
    let m = glob.compile_matcher();
    if m.is_match(path) {
        return true;
    }
    // A pattern with no separator matches the base name.
    !pattern.contains('/') && m.is_match(path.rsplit('/').next().unwrap_or(path))
}

fn pattern_matches(pattern: &Pattern, fs_path: &str) -> bool {
    match pattern {
        Pattern::Glob(p) => p == fs_path || glob_matches(p, fs_path),
        Pattern::Relative { base, pattern } => {
            let base = normalize(base);
            let base = base.trim_end_matches('/');
            match fs_path.strip_prefix(base) {
                Some(rest) if rest.starts_with('/') => {
                    glob_matches(pattern, rest.trim_start_matches('/'))
                }
                _ => false,
            }
        }
    }
}

/// `score(selector, candidateUri, candidateLanguage, candidateIsSynchronized, candidateNotebookUri,
/// candidateNotebookType)`. A missing selector (`undefined`/`null`) scores 0.
pub fn score(
    selector: Option<&LanguageSelector>,
    candidate_uri: &Uri,
    candidate_language: &str,
    candidate_is_synchronized: bool,
    candidate_notebook_uri: Option<&Uri>,
    candidate_notebook_type: Option<&str>,
) -> u32 {
    let Some(selector) = selector else { return 0 };
    match selector {
        LanguageSelector::Many(list) => {
            let mut ret = 0;
            for f in list {
                let v = score(
                    Some(f),
                    candidate_uri,
                    candidate_language,
                    candidate_is_synchronized,
                    candidate_notebook_uri,
                    candidate_notebook_type,
                );
                if v == 10 {
                    return v;
                }
                ret = ret.max(v);
            }
            ret
        }
        LanguageSelector::Language(s) => {
            if !candidate_is_synchronized {
                0
            } else if s == "*" {
                5
            } else if s == candidate_language {
                10
            } else {
                0
            }
        }
        LanguageSelector::Filter(f) => {
            if !candidate_is_synchronized && !f.has_access_to_all_models {
                return 0;
            }
            // A selector that targets a notebook uses the notebook's uri.
            let uri = match (&f.notebook_type, candidate_notebook_uri) {
                (Some(_), Some(nb)) => nb,
                _ => candidate_uri,
            };
            let mut ret = 0;
            if let Some(scheme) = &f.scheme {
                if *scheme == uri.scheme {
                    ret = 10;
                } else if scheme == "*" {
                    ret = 5;
                } else {
                    return 0;
                }
            }
            if let Some(language) = &f.language {
                if language == candidate_language {
                    ret = 10;
                } else if language == "*" {
                    ret = ret.max(5);
                } else {
                    return 0;
                }
            }
            if let Some(nt) = &f.notebook_type {
                if Some(nt.as_str()) == candidate_notebook_type {
                    ret = 10;
                } else if nt == "*" && candidate_notebook_type.is_some() {
                    ret = ret.max(5);
                } else {
                    return 0;
                }
            }
            if let Some(p) = &f.pattern {
                if pattern_matches(p, &uri.fs_path()) {
                    ret = 10;
                } else {
                    return 0;
                }
            }
            ret
        }
    }
}

/// `targetsNotebooks`.
pub fn targets_notebooks(selector: &LanguageSelector) -> bool {
    match selector {
        LanguageSelector::Language(_) => false,
        LanguageSelector::Many(l) => l.iter().any(targets_notebooks),
        LanguageSelector::Filter(f) => f.notebook_type.is_some(),
    }
}

/// `selectLanguageIds`.
pub fn select_language_ids(selector: &LanguageSelector, into: &mut BTreeSet<String>) {
    match selector {
        LanguageSelector::Language(s) => {
            into.insert(s.clone());
        }
        LanguageSelector::Many(l) => l.iter().for_each(|s| select_language_ids(s, into)),
        LanguageSelector::Filter(f) => {
            if let Some(l) = &f.language {
                into.insert(l.clone());
            }
        }
    }
}
