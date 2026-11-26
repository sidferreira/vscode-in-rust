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

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use vsr_exthost::diagnostics::{ERROR, WARNING};
use vsr_exthost::language_features::dto::Range;
use vsr_exthost::language_features::outline::{
    Marker, OutlineModel, OutlineService, ProviderSymbols, Request, compare_ranges_using_starts, severity,
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
    /// The models with the document's diagnostics marked on their symbols (ADR-0094), per document.
    marked: HashMap<String, OutlineModel>,
    /// The diagnostics of each document as markers, parsed once per snapshot generation.
    markers: HashMap<String, Vec<Marker>>,
    /// The snapshot generation `markers` was parsed for.
    markers_generation: Option<u64>,
    /// How many times diagnostics were parsed into markers (tests: the parse is once per change).
    #[cfg(test)]
    pub parses: usize,
    /// Answers given in-process when no host runs (tests only).
    #[cfg(test)]
    pub fake: Option<FakeSource>,
}

/// The diagnostics of a document as the outline marks them: errors and warnings (hints and
/// infos are not counted), with 1-based ranges, sorted by start (`updateMarker` consumes them).
pub fn markers_from_json(values: &[Value]) -> Vec<Marker> {
    let mut out: Vec<Marker> = values
        .iter()
        .filter_map(|m| {
            let n = |k: &str| m.get(k).and_then(Value::as_u64).map(|v| v as u32);
            let severity = match m.get("severity").and_then(Value::as_u64)? {
                ERROR => severity::ERROR,
                WARNING => severity::WARNING,
                _ => return None,
            };
            let range = Range::new(n("startLineNumber")?, n("startColumn")?, n("endLineNumber")?, n("endColumn")?);
            Some(Marker { range, severity })
        })
        .collect();
    out.sort_by(|a, b| compare_ranges_using_starts(&a.range, &b.range));
    out
}

pub(crate) fn key(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

impl OutlineState {
    /// The state of a new workbench: the section's keys go to `focus`.
    pub fn with_focus(focus: gpui_kit::FocusHandle) -> Self {
        let mut state = Self::default();
        state.view.focus = Some(focus);
        state
    }

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
            self.remark(path);
        }
        accepted
    }

    /// Marks the diagnostics of `path` on its symbols (ADR-0094). They are parsed once per snapshot
    /// generation: `parse` runs only when the document has no markers for this generation, so a
    /// frame that finds them cached parses nothing.
    pub fn sync_markers(&mut self, path: &Path, generation: u64, parse: impl FnOnce() -> Vec<Marker>) {
        if self.markers_generation != Some(generation) {
            self.markers.clear();
            self.markers_generation = Some(generation);
        }
        let k = key(path);
        if self.markers.get(&k).is_none() {
            #[cfg(test)]
            {
                self.parses += 1;
            }
            let markers = parse();
            self.markers.insert(k, markers);
            self.remark(path);
        }
    }

    /// Rebuilds the marked model of `path` from its cached markers and its plain model. No markers,
    /// or no model yet, means no marked model (the view then shows the plain one).
    fn remark(&mut self, path: &Path) {
        let k = key(path);
        let (Some(markers), Some(plain)) = (self.markers.get(&k), self.service.cached(&k)) else {
            self.marked.remove(&k);
            return;
        };
        if markers.is_empty() {
            self.marked.remove(&k);
            return;
        }
        let mut model = plain.clone();
        let mut markers = markers.clone();
        model.update_marker(&mut markers);
        self.marked.insert(k, model);
    }

    /// The model the Outline view shows: with the diagnostics marked, when there are any.
    pub fn view_model(&self, path: &Path) -> Option<&OutlineModel> {
        let k = key(path);
        self.marked.get(&k).or_else(|| self.service.cached(&k))
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

    /// Drops everything kept for the documents not in `open` (they closed): their models, marked
    /// models and answer counts, and any request still waiting for them (ADR-0086).
    pub fn retain_open(&mut self, open: &HashSet<String>) {
        if self.seen.as_ref().is_some_and(|(p, _)| !open.contains(&key(p))) {
            self.seen = None;
        }
        let closed: HashSet<String> = self
            .answered
            .keys()
            .chain(self.marked.keys())
            .chain(self.markers.keys())
            .filter(|k| !open.contains(*k))
            .cloned()
            .collect();
        for k in closed {
            self.service.forget(&k);
            self.answered.remove(&k);
            self.marked.remove(&k);
            self.markers.remove(&k);
        }
    }
}

impl Workbench {
    /// Keeps the outline of the active document asked for. Called every frame.
    pub(crate) fn outline_sync(&mut self) {
        self.outline_sync_at(Instant::now());
    }

    pub(crate) fn outline_sync_at(&mut self, now: Instant) {
        let open: HashSet<String> = self.editors.open.iter().map(|e| key(&e.path)).collect();
        self.outline.retain_open(&open);
        let active = self
            .editor_surface_active()
            .then(|| self.editors.active_editor())
            .flatten()
            .filter(|e| e.path.is_absolute() && !e.is_untitled())
            .map(|e| (e.path.clone(), e.editor.version()));
        if let Some((path, version)) = &active {
            self.outline.observe(path, *version, now);
        }
        if let Some((path, version)) = self.outline.take_due(now) {
            let started = matches!(self.outline.service.request(&key(&path), version), Request::Started(_));
            if started {
                if self.ext.is_running() {
                    self.ext_send(ToHost::Symbols { path, version });
                } else {
                    self.outline_answer_in_process(&path, version);
                }
            }
        }
        // The diagnostics of the active document are marked on its symbols (ADR-0094); parsed only
        // when the snapshot changed.
        if let Some((path, _)) = &active {
            let generation = self.ext.generation;
            let diagnostics = &self.ext.snapshot.diagnostics;
            let resource = crate::extensions::snapshot::resource_key(path);
            self.outline.sync_markers(path, generation, || {
                diagnostics.get(&resource).map(|v| markers_from_json(v)).unwrap_or_default()
            });
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
    use serde_json::json;
    use vsr_exthost::language_features::dto::DocumentSymbol;

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

    #[test]
    fn diagnostics_become_markers_for_errors_and_warnings_sorted_by_start() {
        let values = vec![
            json!({"severity": 8, "startLineNumber": 5, "startColumn": 4, "endLineNumber": 5, "endColumn": 8}),
            json!({"severity": 2, "startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 2}),
            json!({"severity": 4, "startLineNumber": 2, "startColumn": 3, "endLineNumber": 2, "endColumn": 8}),
            json!({"severity": 1, "startLineNumber": 3, "startColumn": 1, "endLineNumber": 3, "endColumn": 2}),
        ];
        let markers = markers_from_json(&values);
        assert_eq!(markers.len(), 2, "hints and infos are not counted");
        assert_eq!(markers[0].severity, severity::WARNING);
        assert_eq!(markers[0].range, Range::new(2, 3, 2, 8));
        assert_eq!(markers[1].severity, severity::ERROR);
    }

    #[test]
    fn marking_a_model_is_kept_until_the_next_answer() {
        let mut s = OutlineState::default();
        let path = Path::new("/w/a.rs");
        let main = DocumentSymbol {
            name: "main".into(),
            detail: String::new(),
            kind: 11,
            tags: vec![],
            container_name: None,
            range: Range::new(1, 1, 1, 13),
            selection_range: Range::new(1, 1, 1, 13),
            children: vec![],
        };
        s.service.request(&key(path), 1);
        s.deliver(path, 1, vec![ProviderSymbols::new(Some("fake"), vec![main])]);
        let error = Marker { range: Range::new(1, 5, 1, 9), severity: severity::ERROR };
        s.sync_markers(path, 1, || vec![error]);
        let marked = s.view_model(path).expect("a model");
        assert_eq!(marked.marker(marked.top_level()[0]).map(|m| m.top_severity), Some(severity::ERROR));
        s.sync_markers(path, 1, || panic!("the same snapshot is not parsed again"));
        assert_eq!(s.parses, 1, "one parse for one generation");
        s.service.request(&key(path), 2);
        s.deliver(path, 2, vec![ProviderSymbols::new(Some("fake"), vec![])]);
        assert_eq!(s.view_model(path).map(|m| m.top_level().len()), Some(0), "a new answer replaces it");
        s.sync_markers(path, 2, Vec::new);
        assert!(s.view_model(path).is_some(), "no markers, the plain model");
    }
}
