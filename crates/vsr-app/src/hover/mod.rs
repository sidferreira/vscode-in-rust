//! The hover widget (ADR-0109): the pointer rests on a word for `editor.hover.delay`, the providers
//! are asked (`ToHost::Hover`), and the answer is shown above or below the word until the pointer
//! leaves it (after `editor.hover.hidingDelay` when `editor.hover.sticky` keeps it).
//!
//! Upstream source: src/vs/editor/contrib/hover/browser/hoverController.ts
//! Upstream source: src/vs/editor/contrib/hover/browser/contentHoverController.ts
//!
//! Flow: the editor's mouse move calls [`HoverState::pointer`] and, when a delay starts, a timer
//! wakes the editor. The frame calls [`Workbench::hover_sync`], which returns the due request and
//! sends it to the worker. The answer comes back as `FromHost::Hovers` into
//! [`HoverState::deliver`]; an answer to a request that is no longer awaited is dropped. Every
//! time-based step takes the instant as an argument, so the tests drive the clock.
//!
//! Differences: the markdown is shown as plain lines (no code blocks, links or theme icons), the
//! widget is anchored at the pointer rather than at the word's box, and there is no verbosity
//! control or status bar.

mod overlay;

#[cfg(test)]
mod e2e_tests;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use gpui_kit::Task;
use vsr_exthost::language_features::dto::{Hover, Range};
use vsr_settings::Configuration;

pub use overlay::overlay;

use crate::Workbench;
use crate::config::AppConfig;
use crate::extensions::worker::ToHost;
use crate::navigation::contains;

/// What the hover settings say, read when the pointer moves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    /// `editor.hover.enabled`: only `on` shows the hover for the mouse (`off` and `onKeyboardModifier` do not).
    pub enabled: bool,
    /// `editor.hover.delay` (milliseconds).
    pub delay: Duration,
    /// `editor.hover.hidingDelay` (milliseconds).
    pub hiding_delay: Duration,
    /// `editor.hover.sticky`: the widget stays while the pointer is over it.
    pub sticky: bool,
    /// `editor.hover.above`: the widget shows above the word, else below.
    pub above: bool,
}

impl Settings {
    pub fn read(c: &Configuration) -> Settings {
        let enabled = c.get_value("editor.hover.enabled").and_then(|v| v.as_str()) == Some("on");
        let ms = |key: &str, default: u64| Duration::from_millis(c.get_value(key).and_then(|v| v.as_u64()).unwrap_or(default));
        Settings {
            enabled,
            delay: ms("editor.hover.delay", 300),
            hiding_delay: ms("editor.hover.hidingDelay", 300),
            sticky: c.get_bool("editor.hover.sticky").unwrap_or(true),
            above: c.get_bool("editor.hover.above").unwrap_or(true),
        }
    }
}

/// Hovers a test answers with, instead of a running host: the hovers at a path, line and column.
#[cfg(test)]
pub type FakeHovers = Box<dyn Fn(&std::path::Path, u32, u32) -> Vec<Hover>>;

/// A word the pointer rests on: 1-based line and `char` column, in the document at `path`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    pub path: PathBuf,
    pub line: u32,
    pub column: u32,
}

/// A request that was sent and not answered yet. `at` is the pointer's position in the view.
#[derive(Debug, Clone, PartialEq)]
pub struct Pending {
    pub id: u64,
    pub point: Point,
    pub at: (f32, f32),
}

/// The widget on screen: its lines, the word's range (chars) and where it is anchored.
#[derive(Debug, Clone, PartialEq)]
pub struct Shown {
    pub point: Point,
    pub range: Option<Range>,
    pub lines: Vec<String>,
    pub at: (f32, f32),
    pub above: bool,
}

/// A word the pointer rests on, waiting for the delay.
#[derive(Debug, Clone, PartialEq)]
struct Armed {
    point: Point,
    due: Instant,
    at: (f32, f32),
}

#[derive(Default)]
pub struct HoverState {
    next_id: u64,
    settings: Option<Settings>,
    armed: Option<Armed>,
    awaited: Option<Pending>,
    pub shown: Option<Shown>,
    /// When the widget goes away, while the pointer is off the word (sticky).
    hide_at: Option<Instant>,
    /// The pointer is over the widget itself.
    on_widget: bool,
    /// The timer that wakes the editor when the delay ends.
    pub timer: Option<Task<()>>,
    /// Tests: the answer the providers give, instead of a running host.
    #[cfg(test)]
    pub fake: Option<FakeHovers>,
}

impl HoverState {
    /// The pointer moved: `None` when it left the text. Returns the delay to wake up after, when a
    /// new word started its delay.
    pub fn pointer(&mut self, now: Instant, point: Option<(Point, (f32, f32))>, s: Settings) -> Option<Duration> {
        self.settings = Some(s);
        if !s.enabled {
            self.armed = None;
            self.shown = None;
            self.hide_at = None;
            return None;
        }
        let Some((p, at)) = point else {
            self.armed = None;
            self.leave(now, s);
            return None;
        };
        if self.shown.as_ref().is_some_and(|sh| covers(sh, &p)) {
            self.hide_at = None;
            self.armed = None;
            return None;
        }
        if self.armed.as_ref().is_some_and(|a| a.point == p) {
            return None;
        }
        self.leave(now, s);
        self.armed = Some(Armed { point: p, due: now + s.delay, at });
        Some(s.delay)
    }

    /// The pointer is over the widget (`inside`) or not. Sticky keeps the widget while it is.
    pub fn over_widget(&mut self, now: Instant, inside: bool) {
        self.on_widget = inside;
        if inside {
            self.hide_at = None;
        } else if let Some(s) = self.settings {
            self.leave(now, s);
        }
    }

    /// The pointer is off the word: the widget hides now (not sticky) or after the hiding delay.
    fn leave(&mut self, now: Instant, s: Settings) {
        if self.shown.is_none() || self.on_widget {
            return;
        }
        if s.sticky {
            self.hide_at.get_or_insert(now + s.hiding_delay);
        } else {
            self.shown = None;
            self.hide_at = None;
        }
    }

    /// Time moved on: hides the widget when its hiding delay ended, and returns the request whose
    /// delay ended (it is then the one awaited; an older one is no longer).
    pub fn tick(&mut self, now: Instant) -> Option<Pending> {
        if self.hide_at.is_some_and(|t| now >= t) {
            self.hide_at = None;
            self.shown = None;
        }
        if self.armed.as_ref().is_some_and(|a| now >= a.due) {
            let a = self.armed.take()?;
            self.next_id += 1;
            let p = Pending { id: self.next_id, point: a.point, at: a.at };
            self.awaited = Some(p.clone());
            return Some(p);
        }
        None
    }

    /// The answer of request `id`. Returns false (and changes nothing) when it is not the one awaited.
    pub fn deliver(&mut self, id: u64, hovers: Vec<Hover>) -> bool {
        let Some(p) = self.awaited.take_if(|p| p.id == id) else { return false };
        let lines: Vec<String> = hovers.iter().flat_map(|h| h.contents.iter()).flat_map(|m| plain_lines(&m.value)).collect();
        let range = hovers.iter().find_map(|h| h.range);
        self.shown = (!lines.is_empty()).then(|| Shown {
            above: self.settings.is_none_or(|s| s.above),
            point: p.point,
            range,
            lines,
            at: p.at,
        });
        self.hide_at = None;
        true
    }
}

/// Whether the shown widget already covers the word at `p`.
fn covers(sh: &Shown, p: &Point) -> bool {
    if sh.point.path != p.path {
        return false;
    }
    match sh.range {
        Some(r) => contains(r, p.line, p.column),
        None => sh.point.line == p.line && sh.point.column == p.column,
    }
}

/// The markdown as plain lines: fences, emphasis and heading marks are dropped; the text is kept.
pub fn plain_lines(markdown: &str) -> Vec<String> {
    markdown
        .lines()
        .filter(|l| !l.trim_start().starts_with("```"))
        .map(|l| l.trim_start_matches('#').trim_start().replace("**", "").replace('`', ""))
        .filter(|l| !l.trim().is_empty())
        .collect()
}

impl Workbench {
    /// Every frame: sends the hover request whose delay ended, to the worker (or answers in process
    /// without one).
    pub(crate) fn hover_sync(&mut self) {
        self.hover_sync_at(Instant::now());
    }

    pub(crate) fn hover_sync_at(&mut self, now: Instant) {
        let Some(req) = self.editor_view.hover.tick(now) else { return };
        if self.ext.is_running() {
            let p = req.point;
            self.ext_send(ToHost::Hover { id: req.id, path: p.path, line: p.line, column: p.column });
        } else {
            self.hover_answer_in_process(&req);
        }
    }

    #[cfg(test)]
    fn hover_answer_in_process(&mut self, req: &Pending) {
        let hovers = self.editor_view.hover.fake.as_ref().map(|f| f(&req.point.path, req.point.line, req.point.column)).unwrap_or_default();
        self.editor_view.hover.deliver(req.id, hovers);
    }

    #[cfg(not(test))]
    fn hover_answer_in_process(&mut self, req: &Pending) {
        // No host is running: nothing to show.
        self.editor_view.hover.deliver(req.id, Vec::new());
    }
}

/// The settings the hover reads, from the app's configuration.
pub fn settings(cx: &gpui_kit::App) -> Settings {
    Settings::read(AppConfig::get(cx).configuration())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(line: u32, column: u32) -> Point {
        Point { path: PathBuf::from("/w/a.rs"), line, column }
    }

    fn on() -> Settings {
        Settings { enabled: true, delay: Duration::from_millis(300), hiding_delay: Duration::from_millis(300), sticky: true, above: true }
    }

    fn doc(text: &str, range: Option<Range>) -> Hover {
        Hover {
            contents: vec![vsr_exthost::language_features::dto::MarkdownString { value: text.into(), ..Default::default() }],
            range,
            id: None,
            can_increase_verbosity: false,
            can_decrease_verbosity: false,
        }
    }

    #[test]
    fn the_request_waits_for_the_delay_and_no_earlier() {
        let t0 = Instant::now();
        let mut h = HoverState::default();
        assert_eq!(h.pointer(t0, Some((point(3, 4), (10.0, 20.0))), on()), Some(Duration::from_millis(300)));
        assert!(h.tick(t0 + Duration::from_millis(299)).is_none(), "not before the delay");
        let req = h.tick(t0 + Duration::from_millis(300)).expect("due at the delay");
        assert_eq!((req.point.line, req.point.column), (3, 4));
        assert!(h.tick(t0 + Duration::from_secs(9)).is_none(), "one request per rest");
    }

    #[test]
    fn a_newer_word_drops_the_older_answer() {
        let t0 = Instant::now();
        let mut h = HoverState::default();
        h.pointer(t0, Some((point(1, 1), (0.0, 0.0))), on());
        let first = h.tick(t0 + Duration::from_millis(300)).unwrap();
        h.pointer(t0 + Duration::from_millis(400), Some((point(2, 1), (0.0, 0.0))), on());
        let second = h.tick(t0 + Duration::from_millis(700)).unwrap();
        assert!(!h.deliver(first.id, vec![doc("old", None)]), "the first is no longer awaited");
        assert!(h.deliver(second.id, vec![doc("new", None)]));
        assert_eq!(h.shown.as_ref().unwrap().lines, vec!["new".to_string()]);
    }

    #[test]
    fn enabled_off_or_keyboard_only_never_shows_the_widget() {
        let t0 = Instant::now();
        let mut h = HoverState::default();
        let off = Settings { enabled: false, ..on() };
        assert_eq!(h.pointer(t0, Some((point(1, 1), (0.0, 0.0))), off), None);
        assert!(h.tick(t0 + Duration::from_secs(1)).is_none());
    }

    #[test]
    fn sticky_keeps_the_widget_until_the_hiding_delay_unless_the_pointer_is_over_it() {
        let t0 = Instant::now();
        let mut h = HoverState::default();
        h.pointer(t0, Some((point(1, 1), (0.0, 0.0))), on());
        let req = h.tick(t0 + Duration::from_millis(300)).unwrap();
        h.deliver(req.id, vec![doc("text", None)]);
        let left = t0 + Duration::from_millis(400);
        h.pointer(left, None, on());
        assert!(h.shown.is_some(), "still shown during the hiding delay");
        assert!(h.tick(left + Duration::from_millis(299)).is_none());
        h.over_widget(left + Duration::from_millis(100), true);
        assert!(h.tick(left + Duration::from_secs(5)).is_none());
        assert!(h.shown.is_some(), "the pointer is over the widget");
        h.over_widget(left + Duration::from_secs(5), false);
        h.tick(left + Duration::from_secs(6));
        assert!(h.shown.is_none(), "hidden once the pointer left it and the delay ended");
    }

    #[test]
    fn not_sticky_hides_the_widget_as_the_pointer_leaves() {
        let t0 = Instant::now();
        let mut h = HoverState::default();
        let ns = Settings { sticky: false, ..on() };
        h.pointer(t0, Some((point(1, 1), (0.0, 0.0))), ns);
        let req = h.tick(t0 + Duration::from_millis(300)).unwrap();
        h.deliver(req.id, vec![doc("text", None)]);
        h.pointer(t0 + Duration::from_millis(400), None, ns);
        assert!(h.shown.is_none());
    }

    #[test]
    fn the_widget_is_placed_above_or_below_the_word_as_set() {
        let t0 = Instant::now();
        for above in [true, false] {
            let mut h = HoverState::default();
            let s = Settings { above, ..on() };
            h.pointer(t0, Some((point(1, 1), (0.0, 0.0))), s);
            let req = h.tick(t0 + Duration::from_millis(300)).unwrap();
            h.deliver(req.id, vec![doc("x", None)]);
            assert_eq!(h.shown.as_ref().unwrap().above, above);
        }
    }

    #[test]
    fn moving_within_the_shown_word_keeps_the_widget() {
        let t0 = Instant::now();
        let mut h = HoverState::default();
        h.pointer(t0, Some((point(1, 2), (0.0, 0.0))), on());
        let req = h.tick(t0 + Duration::from_millis(300)).unwrap();
        h.deliver(req.id, vec![doc("x", Some(Range::new(1, 1, 1, 4)))]);
        assert_eq!(h.pointer(t0 + Duration::from_secs(1), Some((point(1, 3), (0.0, 0.0))), on()), None);
        assert!(h.shown.is_some());
    }

    fn config(user: &str) -> Configuration {
        Configuration::new(&vsr_settings::SettingsRegistry::load_default(), vsr_settings::UserSettings::parse(user).unwrap())
    }

    #[test]
    fn the_default_settings_are_the_upstream_defaults() {
        let s = Settings::read(&config("{}"));
        assert_eq!(s, Settings { enabled: true, delay: Duration::from_millis(300), hiding_delay: Duration::from_millis(300), sticky: true, above: true });
    }

    #[test]
    fn editor_hover_enabled_off_and_keyboard_modifier_hide_the_mouse_hover() {
        assert!(Settings::read(&config(r#"{"editor.hover.enabled": "on"}"#)).enabled);
        assert!(!Settings::read(&config(r#"{"editor.hover.enabled": "off"}"#)).enabled);
        assert!(!Settings::read(&config(r#"{"editor.hover.enabled": "onKeyboardModifier"}"#)).enabled);
    }

    #[test]
    fn editor_hover_delay_and_hiding_delay_are_read_in_milliseconds() {
        let s = Settings::read(&config(r#"{"editor.hover.delay": 750, "editor.hover.hidingDelay": 1200}"#));
        assert_eq!(s.delay, Duration::from_millis(750));
        assert_eq!(s.hiding_delay, Duration::from_millis(1200));
    }

    #[test]
    fn editor_hover_sticky_and_above_are_read() {
        let s = Settings::read(&config(r#"{"editor.hover.sticky": false, "editor.hover.above": false}"#));
        assert!(!s.sticky);
        assert!(!s.above);
    }

    #[test]
    fn markdown_becomes_plain_lines() {
        assert_eq!(plain_lines("```rust\nfn f()\n```\n**bold** `x`\n# Title"), vec!["fn f()", "bold x", "Title"]);
    }
}
