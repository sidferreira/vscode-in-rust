//! Go to Definition, Declaration, Type Definition and Implementation (ADR-0108): F12, Cmd+F12
//! (implementation) and Cmd+click ask the extension host's providers for the targets of the
//! caret, and the editor opens the first one at its range.
//!
//! Upstream source: src/vs/editor/contrib/gotoSymbol/browser/goToCommands.ts
//! Upstream source: src/vs/editor/contrib/gotoSymbol/browser/link/clickLinkGesture.ts
//!
//! Flow: [`Workbench::navigate`] reads the caret and sends `ToHost::Navigate` to the worker; the
//! worker answers `FromHost::Locations` with the providers' links; [`Workbench::navigation_deliver`]
//! turns them into targets and opens one. Without a running host a test answers in-process through
//! `NavState::fake`.
//!
//! Differences: a second target is not shown in a peek view (the peek widget is not built; the
//! first target opens for `multiple*` = `peek` too), and no message is painted when nothing is
//! found (`NavState::message` keeps it).

#[cfg(test)]
mod e2e_tests;

use std::path::{Path, PathBuf};

use vsr_exthost::language_features::dto::{LocationLink, Range};
use vsr_exthost::language_features::main_thread::Kind;
use vsr_suggest::adapter::{line_text, utf16_to_char_column};

use crate::Workbench;
use crate::config::AppConfig;
use crate::config::settings::OpenSource;
use crate::extensions::worker::ToHost;

/// The four go-to commands. Their providers are asked with the same position.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Definition,
    Declaration,
    TypeDefinition,
    Implementation,
}

impl Nav {
    pub fn kind(self) -> Kind {
        match self {
            Nav::Definition => Kind::Definition,
            Nav::Declaration => Kind::Declaration,
            Nav::TypeDefinition => Kind::TypeDefinition,
            Nav::Implementation => Kind::Implementation,
        }
    }

    /// The `$provide*` method on the host (`MainThreadLanguageFeatures`' counterpart).
    pub fn method(self) -> &'static str {
        match self {
            Nav::Definition => "$provideDefinition",
            Nav::Declaration => "$provideDeclaration",
            Nav::TypeDefinition => "$provideTypeDefinition",
            Nav::Implementation => "$provideImplementation",
        }
    }

    /// The command id (the one the Command Palette and the keybindings name).
    pub fn command_id(self) -> &'static str {
        match self {
            Nav::Definition => "editor.action.revealDefinition",
            Nav::Declaration => "editor.action.revealDeclaration",
            Nav::TypeDefinition => "editor.action.goToTypeDefinition",
            Nav::Implementation => "editor.action.goToImplementation",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Nav::Definition => "Go to Definition",
            Nav::Declaration => "Go to Declaration",
            Nav::TypeDefinition => "Go to Type Definition",
            Nav::Implementation => "Go to Implementations",
        }
    }

    /// The word the "not found" message uses.
    pub fn noun(self) -> &'static str {
        match self {
            Nav::Definition => "definition",
            Nav::Declaration => "declaration",
            Nav::TypeDefinition => "type definition",
            Nav::Implementation => "implementation",
        }
    }

    /// `editor.gotoLocation.alternative*Command`: the command run when the caret already sits on the
    /// one target.
    pub fn alternative_setting(self) -> &'static str {
        match self {
            Nav::Definition => "editor.gotoLocation.alternativeDefinitionCommand",
            Nav::Declaration => "editor.gotoLocation.alternativeDeclarationCommand",
            Nav::TypeDefinition => "editor.gotoLocation.alternativeTypeDefinitionCommand",
            Nav::Implementation => "editor.gotoLocation.alternativeImplementationCommand",
        }
    }

    /// The go-to command with this id, if rscode has one (`editor.action.goToReferences` has none).
    pub fn from_command_id(id: &str) -> Option<Nav> {
        Nav::ALL.into_iter().find(|n| n.command_id() == id)
    }

    pub const ALL: [Nav; 4] = [Nav::Definition, Nav::Declaration, Nav::TypeDefinition, Nav::Implementation];
}

/// A target of a go-to: a file and where in it. Positions are 1-based with `char` columns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    pub path: PathBuf,
    /// The whole target.
    pub range: Range,
    /// The part to select: the target's selection range, else its range.
    pub selection: Range,
}

/// The targets of the links: file links only, converted from the protocol's UTF-16 columns to
/// `char` columns with the text of each file (`text_of`; a file without text keeps its columns, which
/// is the same for ASCII), sorted by position and without duplicates.
pub fn targets(links: &[LocationLink], text_of: &dyn Fn(&Path) -> Option<String>) -> Vec<Target> {
    let mut out: Vec<Target> = links
        .iter()
        .filter(|l| l.uri.scheme == "file")
        .map(|l| {
            let path = PathBuf::from(l.uri.fs_path());
            let text = text_of(&path);
            let range = to_chars(text.as_deref(), l.range);
            let selection = to_chars(text.as_deref(), l.target_selection_range.unwrap_or(l.range));
            Target { path, range, selection }
        })
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path).then(key(a.selection).cmp(&key(b.selection))));
    out.dedup();
    out
}

fn key(r: Range) -> (u32, u32, u32, u32) {
    (r.start_line, r.start_column, r.end_line, r.end_column)
}

fn to_chars(text: Option<&str>, r: Range) -> Range {
    let Some(text) = text else { return r };
    let col = |line: u32, c: u32| utf16_to_char_column(line_text(text, line), c);
    Range::new(r.start_line, col(r.start_line, r.start_column), r.end_line, col(r.end_line, r.end_column))
}

/// Whether the 1-based `(line, column)` lies in `range` (both ends included, as upstream's
/// `Range.containsPosition`).
pub fn contains(range: Range, line: u32, column: u32) -> bool {
    let p = (line, column);
    (range.start_line, range.start_column) <= p && p <= (range.end_line, range.end_column)
}

/// Cmd+click follows a link (upstream's `clickLinkGesture` at the default `editor.multiCursorModifier`
/// `alt`: the trigger is Cmd on macOS). A single click with Cmd and no other modifier.
pub fn is_goto_click(click_count: usize, platform: bool, shift: bool, alt: bool) -> bool {
    click_count == 1 && platform && !shift && !alt
}

/// The request in flight: the newest go-to, with the caret it was asked from (1-based, chars).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pending {
    pub id: u64,
    pub nav: Nav,
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

/// Links a test answers with, instead of a running host.
#[cfg(test)]
pub type FakeLinks = Box<dyn Fn(&Path, Nav) -> Vec<LocationLink>>;

/// The go-to state the workbench keeps.
#[derive(Default)]
pub struct NavState {
    next_id: u64,
    pending: Option<Pending>,
    /// What the last go-to said when it found nothing ("No definition found"). Not painted yet.
    pub message: Option<String>,
    #[cfg(test)]
    pub fake: Option<FakeLinks>,
}

impl NavState {
    /// Starts a request from the caret. A request still waiting is replaced: its answer is dropped.
    pub fn start(&mut self, nav: Nav, path: PathBuf, line: u32, column: u32) -> Pending {
        self.next_id += 1;
        let p = Pending { id: self.next_id, nav, path, line, column };
        self.pending = Some(p.clone());
        p
    }

    /// The request `id` when it is the one awaited (taking it), else `None`.
    pub fn take(&mut self, id: u64) -> Option<Pending> {
        match &self.pending {
            Some(p) if p.id == id => self.pending.take(),
            _ => None,
        }
    }
}

impl Workbench {
    /// The keybinding or Cmd+click asks for `nav`: always handled (the caret is in the editor).
    pub(crate) fn navigation_command(&mut self, nav: Nav, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) -> bool {
        self.navigate(nav, window, cx);
        true
    }

    /// Asks the providers of `nav` for the caret's targets.
    pub(crate) fn navigate(&mut self, nav: Nav, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        let Some(e) = self.editors.active_editor().filter(|e| e.path.is_absolute() && !e.is_untitled()) else {
            return;
        };
        // The editor's positions are 1-based (vsr-text), the same numbering the worker expects.
        let caret = e.editor.selection().active;
        let path = e.path.clone();
        let (line, column) = (caret.line as u32, caret.column as u32);
        let pending = self.navigation.start(nav, path, line, column);
        if self.ext.is_running() {
            self.ext_send(ToHost::Navigate { id: pending.id, nav, path: pending.path, line, column });
        } else {
            self.navigation_answer_in_process(&pending, window, cx);
        }
        cx.notify();
    }

    /// The providers answered the request `id`. Stale answers are dropped.
    pub(crate) fn navigation_deliver(
        &mut self,
        id: u64,
        nav: Nav,
        links: Vec<LocationLink>,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(origin) = self.navigation.take(id) else { return };
        if origin.nav != nav {
            return;
        }
        let found = {
            let text_of = |p: &Path| self.navigation_text(p);
            targets(&links, &text_of)
        };
        // Exactly one target and the caret already on it: the alternative command runs (upstream's
        // `editor.gotoLocation.alternative*Command`), so a go-to that finds nothing new does something else.
        if let [only] = found.as_slice()
            && only.path == origin.path
            && contains(only.range, origin.line, origin.column)
        {
            let alternative = AppConfig::get(cx).configuration().get_str(nav.alternative_setting()).unwrap_or("").to_string();
            if let Some(next) = Nav::from_command_id(&alternative) {
                self.navigate(next, window, cx);
            }
            return;
        }
        match found.first() {
            Some(t) => {
                self.navigation.message = None;
                self.select_symbol(&t.path, t.selection, OpenSource::CodeNavigation, window, cx);
            }
            None => {
                self.navigation.message = Some(format!("No {} found", nav.noun()));
            }
        }
        cx.notify();
    }

    /// The text of an open editor, or of the file on disk when it is not open.
    fn navigation_text(&self, path: &Path) -> Option<String> {
        self.editors
            .open
            .iter()
            .find(|e| e.path == path)
            .map(|e| e.buffer().value().to_string())
            .or_else(|| std::fs::read_to_string(path).ok())
    }

    #[cfg(test)]
    fn navigation_answer_in_process(&mut self, pending: &Pending, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        let links = self.navigation.fake.as_ref().map(|f| f(&pending.path, pending.nav)).unwrap_or_default();
        self.navigation_deliver(pending.id, pending.nav, links, window, cx);
    }

    #[cfg(not(test))]
    fn navigation_answer_in_process(&mut self, pending: &Pending, window: &mut gpui_kit::Window, cx: &mut gpui_kit::Context<Self>) {
        // No host is running: nothing can be found.
        self.navigation_deliver(pending.id, pending.nav, Vec::new(), window, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nav_commands_round_trip_and_the_unknown_ids_are_none() {
        for n in Nav::ALL {
            assert_eq!(Nav::from_command_id(n.command_id()), Some(n));
        }
        assert_eq!(Nav::from_command_id("editor.action.goToReferences"), None);
        assert_eq!(Nav::from_command_id(""), None);
    }

    #[test]
    fn cmd_click_follows_a_link_only_for_a_single_plain_click() {
        assert!(is_goto_click(1, true, false, false));
        assert!(!is_goto_click(2, true, false, false), "a double click selects a word");
        assert!(!is_goto_click(1, false, false, false), "a plain click places the caret");
        assert!(!is_goto_click(1, true, true, false), "Cmd+Shift+click extends the selection");
        assert!(!is_goto_click(1, true, false, true), "Cmd+Alt+click is not a go-to");
    }

    #[test]
    fn containment_includes_both_ends() {
        let r = Range::new(2, 5, 2, 9);
        assert!(contains(r, 2, 5));
        assert!(contains(r, 2, 9));
        assert!(!contains(r, 2, 4));
        assert!(!contains(r, 3, 5));
    }

    #[test]
    fn a_newer_request_drops_the_older_answer() {
        let mut s = NavState::default();
        let first = s.start(Nav::Definition, PathBuf::from("/w/a.rs"), 1, 1);
        let second = s.start(Nav::Definition, PathBuf::from("/w/a.rs"), 2, 1);
        assert_eq!(s.take(first.id), None, "the first is no longer awaited");
        assert_eq!(s.take(second.id), Some(second.clone()));
        assert_eq!(s.take(second.id), None, "an answer is taken once");
    }

    fn link(path: &str, range: Range, selection: Option<Range>) -> LocationLink {
        LocationLink {
            origin_selection_range: None,
            uri: vsr_exthost::language_features::uri::Uri::file(path),
            range,
            target_selection_range: selection,
        }
    }

    #[test]
    fn targets_are_sorted_deduplicated_and_skip_other_schemes() {
        let a = link("/w/b.rs", Range::new(3, 1, 3, 4), None);
        let b = link("/w/a.rs", Range::new(5, 1, 5, 4), Some(Range::new(5, 2, 5, 3)));
        let mut web = link("/w/c.rs", Range::new(1, 1, 1, 2), None);
        web.uri.scheme = "untitled".into();
        let out = targets(&[a.clone(), b.clone(), a.clone(), web], &|_| None);
        assert_eq!(out.len(), 2, "the duplicate and the non-file link are dropped");
        assert_eq!(out[0].path, PathBuf::from("/w/a.rs"));
        assert_eq!(out[0].selection, Range::new(5, 2, 5, 3), "the selection range is used when given");
        assert_eq!(out[1].selection, Range::new(3, 1, 3, 4), "else the whole range");
    }

    #[test]
    fn utf16_columns_become_char_columns_with_the_target_text() {
        // The emoji is one char and two UTF-16 units: `x` is UTF-16 column 12 and char column 11.
        let text = "let s = \"😀x\";\n";
        let l = link("/w/s.rs", Range::new(1, 12, 1, 13), None);
        let out = targets(&[l], &|_| Some(text.to_string()));
        assert_eq!(out[0].range, Range::new(1, 11, 1, 12), "the `x` is char column 11");
    }
}
