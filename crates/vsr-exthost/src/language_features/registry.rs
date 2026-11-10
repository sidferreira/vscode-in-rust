//! upstream: src/vs/editor/common/languageFeatureRegistry.ts @ 50e1350fc78d
//!
//! `LanguageFeatureRegistry<T>`: providers registered with a selector, queried for a model
//! (uri + language) in score order. Differences from upstream: no `onDidChange` emitter (poll
//! [`LanguageFeatureRegistry::version`]), queries take `&mut self` because scoring is cached
//! (`_lastCandidate`) and they return clones of `T`; disposables are [`RegistrationId`]s.

use super::selector::{LanguageSelector, score, select_language_ids};
use super::uri::Uri;
use std::collections::BTreeSet;

/// What a registry needs to know about a document: `ITextModel.uri`, `getLanguageId()` and
/// `shouldSynchronizeModel`.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInfo {
    pub uri: Uri,
    pub language_id: String,
    pub synchronized: bool,
}

impl ModelInfo {
    pub fn new(uri: Uri, language_id: &str) -> ModelInfo {
        ModelInfo {
            uri,
            language_id: language_id.into(),
            synchronized: true,
        }
    }
}

/// `NotebookInfo`.
#[derive(Debug, Clone, PartialEq)]
pub struct NotebookInfo {
    pub uri: Uri,
    pub notebook_type: String,
}

pub type NotebookInfoResolver = Box<dyn Fn(&Uri) -> Option<NotebookInfo>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct RegistrationId(pub u64);

struct Entry<T> {
    id: RegistrationId,
    selector: LanguageSelector,
    provider: T,
    score: u32,
    time: u64,
}

#[derive(Debug, Clone, PartialEq)]
struct MatchCandidate {
    uri: Uri,
    language_id: String,
    synchronized: bool,
    notebook: Option<NotebookInfo>,
    recursive: bool,
}

fn is_exclusive(s: &LanguageSelector) -> bool {
    match s {
        LanguageSelector::Language(_) => false,
        LanguageSelector::Many(l) => l.iter().all(is_exclusive),
        LanguageSelector::Filter(f) => f.exclusive,
    }
}

fn is_builtin(s: &LanguageSelector) -> bool {
    match s {
        LanguageSelector::Language(_) => false,
        LanguageSelector::Many(l) => l.iter().any(is_builtin),
        LanguageSelector::Filter(f) => f.is_builtin,
    }
}

pub struct LanguageFeatureRegistry<T> {
    clock: u64,
    next_id: u64,
    version: u64,
    entries: Vec<Entry<T>>,
    last_candidate: Option<MatchCandidate>,
    notebook_info_resolver: Option<NotebookInfoResolver>,
}

impl<T> Default for LanguageFeatureRegistry<T> {
    fn default() -> Self {
        LanguageFeatureRegistry {
            clock: 0,
            next_id: 0,
            version: 0,
            entries: vec![],
            last_candidate: None,
            notebook_info_resolver: None,
        }
    }
}

impl<T> std::fmt::Debug for LanguageFeatureRegistry<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LanguageFeatureRegistry({} entries)", self.entries.len())
    }
}

impl<T: Clone> LanguageFeatureRegistry<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_notebook_resolver(resolver: NotebookInfoResolver) -> Self {
        LanguageFeatureRegistry {
            notebook_info_resolver: Some(resolver),
            ..Self::default()
        }
    }

    /// Counts every register and unregister (the stand-in for `onDidChange`).
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn register(&mut self, selector: LanguageSelector, provider: T) -> RegistrationId {
        let id = RegistrationId(self.next_id);
        self.next_id += 1;
        self.entries.push(Entry {
            id,
            selector,
            provider,
            score: 0,
            time: self.clock,
        });
        self.clock += 1;
        self.last_candidate = None;
        self.version += 1;
        id
    }

    /// Remove a registration (the disposable of upstream's `register`); false when it is gone.
    pub fn unregister(&mut self, id: RegistrationId) -> bool {
        match self.entries.iter().position(|e| e.id == id) {
            Some(i) => {
                self.entries.remove(i);
                self.last_candidate = None;
                self.version += 1;
                true
            }
            None => false,
        }
    }

    pub fn has(&mut self, model: &ModelInfo) -> bool {
        !self.all(model).is_empty()
    }

    /// Every provider that matches, in registry order (best first once scored).
    pub fn all(&mut self, model: &ModelInfo) -> Vec<T> {
        self.update_scores(model, false);
        self.entries
            .iter()
            .filter(|e| e.score > 0)
            .map(|e| e.provider.clone())
            .collect()
    }

    pub fn all_no_model(&self) -> Vec<T> {
        self.entries.iter().map(|e| e.provider.clone()).collect()
    }

    pub fn registered_language_ids(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for e in &self.entries {
            select_language_ids(&e.selector, &mut out);
        }
        out
    }

    /// Matching providers, best score first; among equals user extensions before built-in ones,
    /// then the most recently registered first.
    pub fn ordered(&mut self, model: &ModelInfo, recursive: bool) -> Vec<T> {
        self.update_scores(model, recursive);
        self.entries
            .iter()
            .filter(|e| e.score > 0)
            .map(|e| e.provider.clone())
            .collect()
    }

    /// [`ordered`](Self::ordered) in buckets of equal score.
    pub fn ordered_groups(&mut self, model: &ModelInfo) -> Vec<Vec<T>> {
        self.update_scores(model, false);
        let mut out: Vec<Vec<T>> = vec![];
        let mut last: Option<u32> = None;
        for e in self.entries.iter().filter(|e| e.score > 0) {
            match (&mut out.last_mut(), last) {
                (Some(bucket), Some(s)) if s == e.score => bucket.push(e.provider.clone()),
                _ => {
                    last = Some(e.score);
                    out.push(vec![e.provider.clone()]);
                }
            }
        }
        out
    }

    fn update_scores(&mut self, model: &ModelInfo, recursive: bool) {
        let notebook = self
            .notebook_info_resolver
            .as_ref()
            .and_then(|r| r(&model.uri));
        let candidate = MatchCandidate {
            uri: model.uri.clone(),
            language_id: model.language_id.clone(),
            synchronized: model.synchronized,
            notebook,
            recursive,
        };
        if self.last_candidate.as_ref() == Some(&candidate) {
            return;
        }
        let nb_uri = candidate.notebook.as_ref().map(|n| &n.uri);
        let nb_type = candidate
            .notebook
            .as_ref()
            .map(|n| n.notebook_type.as_str());
        for i in 0..self.entries.len() {
            let s = score(
                Some(&self.entries[i].selector),
                &candidate.uri,
                &candidate.language_id,
                candidate.synchronized,
                nb_uri,
                nb_type,
            );
            self.entries[i].score = s;
            if is_exclusive(&self.entries[i].selector) && s > 0 {
                if recursive {
                    self.entries[i].score = 0;
                } else {
                    // One exclusive selector overrides every other.
                    for e in &mut self.entries {
                        e.score = 0;
                    }
                    self.entries[i].score = 1000;
                    break;
                }
            }
        }
        self.last_candidate = Some(candidate);
        self.entries.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| is_builtin(&a.selector).cmp(&is_builtin(&b.selector)))
                .then_with(|| b.time.cmp(&a.time))
        });
    }
}
