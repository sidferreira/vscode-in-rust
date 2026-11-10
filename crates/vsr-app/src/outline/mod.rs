//! The outline of the active document (ADR-0086): asks the extension host for its symbols,
//! debounces the asks while typing, and keeps the answers per document. The breadcrumbs
//! (`breadcrumbs/`) and Go to Symbol (`quick_open`) read them.
//!
//! Upstream source: src/vs/editor/contrib/documentSymbols/browser/outlineModel.ts (the service)
//! Upstream source: src/vs/editor/common/services/languageFeatureDebounce.ts (the debounce, fixed here)
//!
//! Flow: [`Workbench::outline_sync_at`] runs every frame. A change to the active document is
//! observed and due [`DEBOUNCE`] later; then the request goes to the worker (`ToHost::Symbols`),
//! and the answer comes back as `FromHost::Symbols` into [`OutlineState::deliver`]. A newer
//! request for the same document replaces the one in flight inside the worker.
//! Without a running host, a test can answer in-process through `OutlineState::fake`.

#[cfg(test)]
mod e2e_tests;
pub mod goto;
pub mod view; // the Outline view under the Explorer tree (ADR-0092)

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vsr_exthost::language_features::outline::{
    OutlineModel, OutlineService, ProviderSymbols, Request,
};

use crate::Workbench;
use crate::extensions::worker::ToHost;

/// How long the document must stay unchanged before its symbols are asked for.
pub const DEBOUNCE: Duration = Duration::from_millis(300);

/// An in-process provider for tests: the symbols of a document.
#[cfg(test)]
pub type FakeSource = Box<dyn Fn(&Path) -> Vec<ProviderSymbols>>;

/// The outline state the workbench keeps.
#[derive(Default)]
pub struct OutlineState {
    service: OutlineService,
    /// The active document and version seen last.
    seen: Option<(PathBuf, u64)>,
    /// The request waiting for the debounce, with the time it becomes due.
    due: Option<(PathBuf, u64, Instant)>,
    /// How many providers answered for each document's last delivered version (0: none has a
    /// provider, the view says so).
    answered: HashMap<String, usize>,
    /// The Outline view's state (section open, toggled items).
    pub view: view::ViewState,
    /// Answers given in-process when no host runs (tests only).
    #[cfg(test)]
    pub fake: Option<FakeSource>,
}

pub(crate) fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl OutlineState {
    /// The active document is at `version`: a change restarts the debounce.
    pub fn observe(&mut self, path: &Path, version: u64, now: Instant) {
        if self
            .seen
            .as_ref()
            .is_some_and(|(p, v)| p == path && *v == version)
        {
            return;
        }
        self.seen = Some((path.to_path_buf(), version));
        self.due = Some((path.to_path_buf(), version, now + DEBOUNCE));
    }

    /// The request whose debounce has ended, if any.
    pub fn take_due(&mut self, now: Instant) -> Option<(PathBuf, u64)> {
        match &self.due {
            Some((_, _, at)) if now >= *at => self.due.take().map(|(p, v, _)| (p, v)),
            _ => None,
        }
    }

    /// The providers answered for `path` at `version`. False when that request is no longer
    /// wanted (a newer version was asked for).
    pub fn deliver(&mut self, path: &Path, version: u64, groups: Vec<ProviderSymbols>) -> bool {
        let providers = groups.len();
        let accepted = self.service.deliver(&key(path), version, groups).is_some();
        if accepted {
            self.answered.insert(key(path), providers);
        }
        accepted
    }

    /// How many providers answered for the last delivered version of `path`; `None` before any answer.
    pub fn answered(&self, path: &Path) -> Option<usize> {
        self.answered.get(&key(path)).copied()
    }

    /// The outline of `path` as last delivered, whatever its version (breadcrumbs keep showing
    /// the previous symbols until the new ones arrive).
    pub fn model(&self, path: &Path) -> Option<&OutlineModel> {
        self.service.cached(&key(path))
    }
}

impl Workbench {
    /// Keeps the outline of the active document asked for. Called every frame.
    pub(crate) fn outline_sync(&mut self) {
        self.outline_sync_at(Instant::now());
    }

    pub(crate) fn outline_sync_at(&mut self, now: Instant) {
        let active = self
            .editor_surface_active()
            .then(|| self.editors.active_editor())
            .flatten()
            .filter(|e| e.path.is_absolute() && !e.is_untitled())
            .map(|e| (e.path.clone(), e.editor.version()));
        if let Some((path, version)) = &active {
            self.outline.observe(path, *version, now);
        }
        let Some((path, version)) = self.outline.take_due(now) else {
            return;
        };
        if let Request::Started(_) = self.outline.service.request(&key(&path), version) {
            if self.ext.is_running() {
                self.ext_send(ToHost::Symbols { path, version });
            } else {
                self.outline_answer_in_process(&path, version);
            }
        }
    }

    #[cfg(test)]
    fn outline_answer_in_process(&mut self, path: &Path, version: u64) {
        if let Some(answer) = self.outline.fake.as_ref().map(|f| f(path)) {
            self.outline.deliver(path, version, answer);
        }
    }

    #[cfg(not(test))]
    fn outline_answer_in_process(&mut self, _path: &Path, _version: u64) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_restarts_the_debounce() {
        let mut s = OutlineState::default();
        let t0 = Instant::now();
        let path = Path::new("/w/a.rs");
        s.observe(path, 1, t0);
        assert_eq!(
            s.take_due(t0 + DEBOUNCE - Duration::from_millis(1)),
            None,
            "still too early"
        );
        assert_eq!(s.take_due(t0 + DEBOUNCE), Some((path.to_path_buf(), 1)));
        // Typing again moves the due time from the change, not from the first keystroke.
        s.observe(path, 2, t0 + Duration::from_millis(200));
        assert_eq!(s.take_due(t0 + DEBOUNCE + Duration::from_millis(10)), None);
        assert_eq!(
            s.take_due(t0 + Duration::from_millis(500)),
            Some((path.to_path_buf(), 2))
        );
    }

    #[test]
    fn the_same_version_is_not_asked_twice() {
        let mut s = OutlineState::default();
        let t0 = Instant::now();
        s.observe(Path::new("/w/a.rs"), 1, t0);
        assert!(s.take_due(t0 + DEBOUNCE).is_some());
        s.observe(Path::new("/w/a.rs"), 1, t0 + DEBOUNCE);
        assert_eq!(s.take_due(t0 + DEBOUNCE * 3), None);
    }
}
