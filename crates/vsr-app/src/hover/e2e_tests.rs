//! Headless E2E tests of the hover widget (ADR-0109). No extension host runs here: the providers'
//! answer comes from `HoverState::fake`, and the clock is passed in, so every delay is exact.
//!
//! Upstream source: src/vs/editor/contrib/hover/browser/hoverController.ts

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_exthost::language_features::dto::{Hover, MarkdownString, Range};

use super::Point;
use crate::Workbench;
use crate::config::AppConfig;

/// A workbench over `<tmp>/proj` (`src/main.rs`), with `settings` as the user settings file.
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-hover-e2e-{name}-{}-{nanos}", std::process::id()));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    let settings_file = dir.join("settings.json");
    fs::write(&settings_file, settings).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        crate::breadcrumbs::init(cx);
        AppConfig::install(cx, Some(settings_file));
    });
    let root = proj.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    (wb, vcx, proj)
}

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
}

/// A hover with `text` over the word at columns 4 to 8 of line 1.
fn hover_of(text: &str) -> Hover {
    Hover {
        contents: vec![MarkdownString { value: text.into(), ..Default::default() }],
        range: Some(Range::new(1, 4, 1, 8)),
        id: None,
        can_increase_verbosity: false,
        can_decrease_verbosity: false,
    }
}

/// The pointer rests on column 5 of line 1 at `now`: the delay starts (returned) when the settings
/// allow it.
fn rest_on_word(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path, now: Instant) -> Option<Duration> {
    let point = Point { path: path.to_path_buf(), line: 1, column: 5 };
    wb.update(cx, |wb, cx| {
        let s = crate::hover::settings(cx);
        wb.editor_view.hover.pointer(now, Some((point, (10.0, 20.0))), s)
    })
}

fn leave(wb: &Entity<Workbench>, cx: &mut VisualTestContext, now: Instant) {
    wb.update(cx, |wb, cx| {
        let s = crate::hover::settings(cx);
        wb.editor_view.hover.pointer(now, None, s);
    });
}

fn frame_at(wb: &Entity<Workbench>, cx: &mut VisualTestContext, now: Instant) {
    wb.update(cx, |wb, _| wb.hover_sync_at(now));
}

fn shown_lines(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Vec<String>> {
    wb.read_with(cx, |wb, _| wb.editor_view.hover.shown.as_ref().map(|s| s.lines.clone()))
}

fn answer_with(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &'static str) {
    wb.update(cx, |wb, _| {
        wb.editor_view.hover.fake = Some(Box::new(move |_, _, _| vec![hover_of(text)]));
    });
}

#[gpui::test]
fn resting_on_a_word_shows_the_answer_once_the_default_delay_has_passed(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "default", "{}");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    answer_with(&wb, cx, "fn main()");
    let t0 = Instant::now();
    assert_eq!(rest_on_word(&wb, cx, &file, t0), Some(Duration::from_millis(300)));
    frame_at(&wb, cx, t0 + Duration::from_millis(299));
    assert_eq!(shown_lines(&wb, cx), None, "not before the default delay of 300 ms");
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    assert_eq!(shown_lines(&wb, cx), Some(vec!["fn main()".to_string()]));
}

#[gpui::test]
fn editor_hover_delay_sets_the_delay(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "delay", r#"{"editor.hover.delay": 800}"#);
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    answer_with(&wb, cx, "delayed");
    let t0 = Instant::now();
    assert_eq!(rest_on_word(&wb, cx, &file, t0), Some(Duration::from_millis(800)));
    frame_at(&wb, cx, t0 + Duration::from_millis(799));
    assert_eq!(shown_lines(&wb, cx), None);
    frame_at(&wb, cx, t0 + Duration::from_millis(800));
    assert!(shown_lines(&wb, cx).is_some());
}

#[gpui::test]
fn editor_hover_enabled_off_shows_nothing(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "off", r#"{"editor.hover.enabled": "off"}"#);
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    answer_with(&wb, cx, "never");
    let t0 = Instant::now();
    assert_eq!(rest_on_word(&wb, cx, &file, t0), None, "no delay starts");
    frame_at(&wb, cx, t0 + Duration::from_secs(5));
    assert_eq!(shown_lines(&wb, cx), None);
}

#[gpui::test]
fn editor_hover_hiding_delay_keeps_a_sticky_widget_after_the_pointer_leaves(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "hiding", r#"{"editor.hover.hidingDelay": 1000}"#);
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    answer_with(&wb, cx, "sticky");
    let t0 = Instant::now();
    rest_on_word(&wb, cx, &file, t0);
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    let left = t0 + Duration::from_millis(400);
    leave(&wb, cx, left);
    frame_at(&wb, cx, left + Duration::from_millis(999));
    assert!(shown_lines(&wb, cx).is_some(), "still shown during the hiding delay");
    frame_at(&wb, cx, left + Duration::from_millis(1000));
    assert_eq!(shown_lines(&wb, cx), None, "hidden once the hiding delay ended");
}

#[gpui::test]
fn editor_hover_sticky_off_hides_the_widget_as_the_pointer_leaves(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "not-sticky", r#"{"editor.hover.sticky": false}"#);
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    answer_with(&wb, cx, "gone");
    let t0 = Instant::now();
    rest_on_word(&wb, cx, &file, t0);
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    assert!(shown_lines(&wb, cx).is_some());
    leave(&wb, cx, t0 + Duration::from_millis(400));
    assert_eq!(shown_lines(&wb, cx), None, "hidden at once without sticky");
}

#[gpui::test]
fn editor_hover_above_places_the_widget_above_or_below_the_word(cx: &mut TestAppContext) {
    for (name, above) in [("above-on", true), ("above-off", false)] {
        let settings = format!(r#"{{"editor.hover.above": {above}}}"#);
        let (wb, cx, proj) = workbench(cx, name, &settings);
        let file = proj.join("src/main.rs");
        open(&wb, cx, &file);
        answer_with(&wb, cx, "placed");
        let t0 = Instant::now();
        rest_on_word(&wb, cx, &file, t0);
        frame_at(&wb, cx, t0 + Duration::from_millis(300));
        let placed = wb.read_with(cx, |wb, _| wb.editor_view.hover.shown.as_ref().map(|s| s.above));
        assert_eq!(placed, Some(above), "editor.hover.above = {above}");
    }
}
