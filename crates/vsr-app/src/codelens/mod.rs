//! Code lens (ADR-0155): the providers' lenses of the active document, asked once its edits stop
//! for `DEBOUNCE` (`editor.codeLens`), and each lens's command resolved when it is asked for.
//!
//! Upstream source: src/vs/editor/contrib/codelens/browser/codelensController.ts
//! Upstream source: src/vs/editor/contrib/codelens/browser/codelens.ts
//!
//! Flow: the frame calls [`Workbench::codelens_sync`], which observes the active document's version.
//! A change starts the debounce; when it ends the request goes to the worker (`ToHost::CodeLenses`)
//! and the answer comes back as `FromHost::CodeLenses` into [`CodeLensState::deliver`]. A lens
//! without a command gets `ToHost::ResolveCodeLens`. Every time-based step takes the instant as an
//! argument, so the tests drive the clock.
//!
//! Not done yet (ADR-0155 step 2): the lens rows in the editor's layout, the click that runs the
//! command, and the font settings.

#[cfg(test)]
mod e2e_tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use vsr_exthost::language_features::dto::{CodeLens, CodeLensList};
use vsr_settings::Configuration;

use crate::Workbench;
use crate::config::AppConfig;
use crate::extensions::worker::ToHost;

/// How long the edits must stop before the lenses are asked for (upstream's debounce).
pub const DEBOUNCE: Duration = Duration::from_millis(250);

/// What the code lens settings say, read every frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// `editor.codeLens`.
    pub enabled: bool,
}

impl Settings {
    pub fn read(c: &Configuration) -> Settings {
        Settings { enabled: c.get_bool("editor.codeLens").unwrap_or(true) }
    }
}

/// The settings from the app's configuration.
pub fn settings(cx: &gpui_kit::App) -> Settings {
    Settings::read(AppConfig::get(cx).configuration())
}

/// A lens and the provider that gave it. Its command is absent until it is resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct Lens {
    pub handle: u32,
    pub lens: CodeLens,
}

/// A request that was sent and not answered yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: u64,
    pub path: PathBuf,
}

/// The lenses of one document, and the provider lists the host holds for them (released when the
/// document's lenses are replaced).
#[derive(Debug, Default, Clone, PartialEq)]
struct Doc {
    lists: Vec<(u32, u64)>,
    lenses: Vec<Lens>,
}

/// Lenses the tests answer with, instead of a running host.
#[cfg(test)]
pub struct FakeLenses {
    /// The provider lists for a document.
    pub lists: Box<dyn Fn(&Path) -> Vec<(u32, CodeLensList)>>,
    /// The command a lens resolves to (`None`: it stays without one).
    pub resolve: Box<dyn Fn(&CodeLens) -> Option<vsr_exthost::language_features::dto::Command>>,
}

#[derive(Default)]
pub struct CodeLensState {
    next_id: u64,
    /// The active document and its version the last time it was observed.
    last: Option<(PathBuf, u64)>,
    /// The document whose request waits for the debounce, and when the debounce ends.
    due: Option<(PathBuf, Instant)>,
    awaited: Option<Pending>,
    docs: HashMap<PathBuf, Doc>,
    /// Resolve requests in flight: the document and the lens asked for.
    resolving: HashMap<u64, (PathBuf, Lens)>,
    #[cfg(test)]
    pub fake: Option<FakeLenses>,
}

/// The `(handle, cache id)` pairs of the lists that were cached by the host.
pub fn cache_ids(groups: &[(u32, CodeLensList)]) -> Vec<(u32, u64)> {
    groups.iter().filter_map(|(h, l)| l.cache_id.map(|c| (*h, c))).collect()
}

impl CodeLensState {
    /// The active document is at `version`. A new version starts (or restarts) the debounce.
    pub fn observe(&mut self, path: &Path, version: u64, now: Instant) {
        if self.last.as_ref().is_some_and(|(p, v)| p == path && *v == version) {
            return;
        }
        self.last = Some((path.to_path_buf(), version));
        self.due = Some((path.to_path_buf(), now + DEBOUNCE));
    }

    /// `editor.codeLens` is off: nothing is asked, and the next observation asks again.
    pub fn disable(&mut self) {
        self.last = None;
        self.due = None;
    }

    /// The request whose debounce ended (it is then the one awaited).
    pub fn tick(&mut self, now: Instant) -> Option<Pending> {
        if self.due.as_ref().is_some_and(|(_, t)| now >= *t) {
            let (path, _) = self.due.take()?;
            self.next_id += 1;
            let p = Pending { id: self.next_id, path };
            self.awaited = Some(p.clone());
            return Some(p);
        }
        None
    }

    /// The providers' lists for request `id`. The document's lenses are replaced and the lists it
    /// held before are returned, to be released. `None` when `id` is not the one awaited: nothing
    /// changes, and the caller releases the new lists (`cache_ids`).
    pub fn deliver(&mut self, id: u64, groups: Vec<(u32, CodeLensList)>) -> Option<Vec<(u32, u64)>> {
        let p = self.awaited.take_if(|p| p.id == id)?;
        let mut lists = Vec::new();
        let mut lenses = Vec::new();
        for (handle, list) in groups {
            if let Some(c) = list.cache_id {
                lists.push((handle, c));
            }
            lenses.extend(list.lenses.into_iter().map(|lens| Lens { handle, lens }));
        }
        lenses.sort_by_key(|l| (l.lens.range.start_line, l.lens.range.start_column));
        let old = self.docs.insert(p.path, Doc { lists, lenses }).map(|d| d.lists).unwrap_or_default();
        Some(old)
    }

    /// The lenses of a document, in position order.
    pub fn lenses(&self, path: &Path) -> &[Lens] {
        self.docs.get(path).map(|d| d.lenses.as_slice()).unwrap_or(&[])
    }

    /// The lenses of `path` without a command that are not being resolved yet: each one gets a
    /// request id, to answer with [`CodeLensState::resolved`].
    pub fn resolve_requests(&mut self, path: &Path) -> Vec<(u64, u32, CodeLens)> {
        let todo: Vec<Lens> = self
            .lenses(path)
            .iter()
            .filter(|l| l.lens.command.is_none())
            .filter(|l| !self.resolving.values().any(|(p, asked)| p == path && asked == *l))
            .cloned()
            .collect();
        todo.into_iter()
            .map(|l| {
                self.next_id += 1;
                let id = self.next_id;
                self.resolving.insert(id, (path.to_path_buf(), l.clone()));
                (id, l.handle, l.lens)
            })
            .collect()
    }

    /// The answer to resolve request `id`: the lens gets its command. True when a lens changed.
    pub fn resolved(&mut self, id: u64, lens: CodeLens) -> bool {
        let Some((path, asked)) = self.resolving.remove(&id) else { return false };
        let Some(command) = lens.command else { return false };
        let Some(doc) = self.docs.get_mut(&path) else { return false };
        match doc.lenses.iter_mut().find(|l| l.handle == asked.handle && l.lens.range == asked.lens.range && l.lens.command.is_none()) {
            Some(slot) => {
                slot.lens.command = Some(command);
                true
            }
            None => false,
        }
    }
}

impl Workbench {
    /// Every frame: observes the active document, sends the request whose debounce ended and the
    /// resolves of its lenses. With `editor.codeLens` off nothing is asked.
    pub(crate) fn codelens_sync(&mut self, cx: &gpui_kit::App) {
        self.codelens_sync_at(Instant::now(), settings(cx));
    }

    pub(crate) fn codelens_sync_at(&mut self, now: Instant, s: Settings) {
        if !s.enabled {
            self.codelens.disable();
            return;
        }
        let active = self
            .editor_surface_active()
            .then(|| self.editors.active_editor())
            .flatten()
            .filter(|e| e.path.is_absolute() && !e.is_untitled())
            .map(|e| (e.path.clone(), e.editor.version()));
        if let Some((path, version)) = &active {
            self.codelens.observe(path, *version, now);
        }
        if let Some(req) = self.codelens.tick(now) {
            if self.ext.is_running() {
                self.ext_send(ToHost::CodeLenses { id: req.id, path: req.path });
            } else {
                self.codelens_answer_in_process(&req);
            }
        }
        if let Some((path, _)) = &active {
            for (id, handle, lens) in self.codelens.resolve_requests(path) {
                if self.ext.is_running() {
                    self.ext_send(ToHost::ResolveCodeLens { id, handle, lens: Box::new(lens) });
                } else {
                    self.codelens_resolve_in_process(id, lens);
                }
            }
        }
    }

    /// The providers' lists for request `id` (from the worker, or the in-process fake). True when
    /// the answer was the one awaited and the lenses changed.
    pub(crate) fn codelens_deliver(&mut self, id: u64, groups: Vec<(u32, CodeLensList)>) -> bool {
        let fresh = cache_ids(&groups);
        match self.codelens.deliver(id, groups) {
            Some(old) => {
                self.codelens_release(old);
                true
            }
            None => {
                self.codelens_release(fresh);
                false
            }
        }
    }

    /// Tells the host to drop the cached lists.
    pub(crate) fn codelens_release(&self, lists: Vec<(u32, u64)>) {
        if !lists.is_empty() && self.ext.is_running() {
            self.ext_send(ToHost::ReleaseCodeLenses(lists));
        }
    }

    #[cfg(test)]
    fn codelens_answer_in_process(&mut self, req: &Pending) {
        let groups = self.codelens.fake.as_ref().map(|f| (f.lists)(&req.path)).unwrap_or_default();
        self.codelens_deliver(req.id, groups);
    }

    #[cfg(not(test))]
    fn codelens_answer_in_process(&mut self, req: &Pending) {
        // No host is running: no provider, so no lenses.
        self.codelens_deliver(req.id, Vec::new());
    }

    #[cfg(test)]
    fn codelens_resolve_in_process(&mut self, id: u64, lens: CodeLens) {
        let resolved = self.codelens.fake.as_ref().and_then(|f| (f.resolve)(&lens));
        let lens = CodeLens { command: resolved.or(lens.command), ..lens };
        self.codelens.resolved(id, lens);
    }

    #[cfg(not(test))]
    fn codelens_resolve_in_process(&mut self, id: u64, lens: CodeLens) {
        self.codelens.resolved(id, lens);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_exthost::language_features::dto::{Command, Range};

    fn at(line: u32) -> CodeLens {
        CodeLens { range: Range::new(line, 1, line, 1), command: None, cache_id: None }
    }

    fn list(lenses: Vec<CodeLens>, cache: Option<u64>) -> CodeLensList {
        CodeLensList { lenses, cache_id: cache }
    }

    fn config(user: &str) -> Configuration {
        Configuration::new(&vsr_settings::SettingsRegistry::load_default(), vsr_settings::UserSettings::parse(user).unwrap())
    }

    #[test]
    fn the_setting_defaults_to_on_and_can_be_turned_off() {
        assert!(Settings::read(&config("{}")).enabled);
        assert!(!Settings::read(&config(r#"{"editor.codeLens": false}"#)).enabled);
    }

    #[test]
    fn the_request_waits_for_the_debounce_and_edits_restart_it() {
        let t0 = Instant::now();
        let p = Path::new("/w/a.rs");
        let mut c = CodeLensState::default();
        c.observe(p, 1, t0);
        assert!(c.tick(t0 + Duration::from_millis(249)).is_none(), "not before the debounce");
        c.observe(p, 2, t0 + Duration::from_millis(200));
        assert!(c.tick(t0 + Duration::from_millis(300)).is_none(), "the edit restarted the debounce");
        let req = c.tick(t0 + Duration::from_millis(450)).expect("due 250 ms after the last edit");
        assert_eq!(req.path, p);
        assert!(c.tick(t0 + Duration::from_secs(9)).is_none(), "one request per edit");
        c.observe(p, 2, t0 + Duration::from_secs(10));
        assert!(c.tick(t0 + Duration::from_secs(11)).is_none(), "the same version asks nothing again");
    }

    #[test]
    fn an_answer_replaces_the_lenses_and_returns_the_lists_to_release() {
        let t0 = Instant::now();
        let p = Path::new("/w/a.rs");
        let mut c = CodeLensState::default();
        c.observe(p, 1, t0);
        let first = c.tick(t0 + DEBOUNCE).unwrap();
        assert_eq!(c.deliver(first.id, vec![(7, list(vec![at(3), at(1)], Some(70)))]), Some(vec![]));
        assert_eq!(c.lenses(p).iter().map(|l| l.lens.range.start_line).collect::<Vec<_>>(), vec![1, 3], "in position order");
        c.observe(p, 2, t0 + Duration::from_secs(1));
        let second = c.tick(t0 + Duration::from_secs(2)).unwrap();
        assert_eq!(c.deliver(second.id, vec![(8, list(vec![at(5)], None))]), Some(vec![(7, 70)]), "the old list is released");
        assert_eq!(c.lenses(p).len(), 1);
    }

    #[test]
    fn an_answer_to_a_request_no_longer_awaited_changes_nothing() {
        let t0 = Instant::now();
        let p = Path::new("/w/a.rs");
        let mut c = CodeLensState::default();
        c.observe(p, 1, t0);
        let stale = c.tick(t0 + DEBOUNCE).unwrap();
        c.observe(p, 2, t0 + Duration::from_secs(1));
        let awaited = c.tick(t0 + Duration::from_secs(2)).unwrap();
        assert!(c.deliver(stale.id, vec![(1, list(vec![at(1)], Some(5)))]).is_none());
        assert!(c.lenses(p).is_empty(), "the stale answer is dropped");
        assert_eq!(cache_ids(&[(1, list(vec![at(1)], Some(5)))]), vec![(1, 5)], "its cache is released by the caller");
        assert_eq!(c.deliver(awaited.id, vec![]), Some(vec![]));
    }

    #[test]
    fn a_lens_without_a_command_is_resolved_once_and_gets_its_command() {
        let t0 = Instant::now();
        let p = Path::new("/w/a.rs");
        let mut c = CodeLensState::default();
        c.observe(p, 1, t0);
        let req = c.tick(t0 + DEBOUNCE).unwrap();
        c.deliver(req.id, vec![(4, list(vec![at(2)], None))]);
        let asks = c.resolve_requests(p);
        assert_eq!(asks.len(), 1);
        assert!(c.resolve_requests(p).is_empty(), "already being resolved");
        let (id, handle, lens) = asks.into_iter().next().unwrap();
        assert_eq!(handle, 4);
        let cmd = Command { id: "x.run".into(), title: "Run".into(), tooltip: None, arguments: vec![], ident: None };
        assert!(c.resolved(id, CodeLens { command: Some(cmd.clone()), ..lens }));
        assert_eq!(c.lenses(p)[0].lens.command, Some(cmd));
        assert!(c.resolve_requests(p).is_empty(), "resolved lenses are not asked again");
    }

    #[test]
    fn disabling_stops_the_requests() {
        let t0 = Instant::now();
        let p = Path::new("/w/a.rs");
        let mut c = CodeLensState::default();
        c.observe(p, 1, t0);
        c.disable();
        assert!(c.tick(t0 + Duration::from_secs(1)).is_none());
    }
}
