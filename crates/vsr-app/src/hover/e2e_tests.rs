//! Headless E2E tests of the hover widget (ADR-0109). No extension host runs here: the providers'
//! answer comes from `HoverState::fake`, and the clock is passed in, so every delay is exact.
//!
//! Upstream source: src/vs/editor/contrib/hover/browser/hoverController.ts

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{Entity, Modifiers, ScrollDelta, ScrollEvent, ScrollSource, TestAppContext, TouchPhase, VisualTestContext, point as gpoint, px};
use vsr_exthost::language_features::dto::{Hover, MarkdownString, Range};

use super::{LONG_LINE_WARNING, Point};
use crate::Workbench;
use crate::config::AppConfig;

/// A workbench over `<tmp>/proj` (`src/main.rs`, `fn main() {}`), with `settings` as the user settings file.
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    workbench_text(cx, name, settings, "fn main() {}\n")
}

/// A workbench over `<tmp>/proj` (`src/main.rs` with `text`), with `settings` as the user settings file.
fn workbench_text<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str, text: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = vsr_test_scratch::dir!().join(format!("rscode-hover-e2e-{name}-{}-{nanos}", std::process::id()));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), text).unwrap();
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

/// The pointer rests on `line`, `column` at `at` (window coordinates) at `now`.
fn rest_at(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path, now: Instant, line: u32, column: u32, at: (f32, f32)) {
    let point = Point { path: path.to_path_buf(), line, column };
    wb.update(cx, |wb, cx| {
        let s = crate::hover::settings(cx);
        wb.editor_view.hover.pointer(now, Some((point, at)), s);
    });
}

/// The word box `(x, top, bottom)` of `line`, `column` in the editor surface.
fn char_box(wb: &Entity<Workbench>, cx: &mut VisualTestContext, line: usize, column: usize) -> (f32, f32, f32) {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        wb.editor_view.anchor_at(e, line, column)
    })
}

/// A window point on `line`, `column` (its row's middle), for real mouse events.
fn window_point_at(wb: &Entity<Workbench>, cx: &mut VisualTestContext, line: usize, column: usize) -> gpui_kit::Point<gpui_kit::Pixels> {
    let origin = wb.read_with(cx, |wb, _| wb.editor_view.bounds.get().origin);
    let (x, top, _) = char_box(wb, cx, line, column);
    gpoint(origin.x + px(x + 1.0), origin.y + px(top + 9.0))
}

fn hover_shown(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    wb.read_with(cx, |wb, _| wb.editor_view.hover.shown.is_some())
}

/// The widget is shown on `main` (line 1, column 5) after its delay, with the answer "fn main()".
fn show_on_main(wb: &Entity<Workbench>, cx: &mut VisualTestContext, file: &Path) {
    answer_with(wb, cx, "fn main()");
    let t0 = Instant::now();
    rest_on_word(wb, cx, file, t0);
    frame_at(wb, cx, t0 + Duration::from_millis(300));
    assert!(hover_shown(wb, cx), "shown before the event");
}

#[gpui::test]
fn the_widget_is_anchored_at_the_word_not_the_pointer(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "anchor", "{}");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    answer_with(&wb, cx, "fn main()");
    let t0 = Instant::now();
    // The pointer rests on the word's fifth column, but far to the right.
    rest_at(&wb, cx, &file, t0, 1, 5, (200.0, 300.0));
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    let word = wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        wb.editor_view.hover_word_box(e, wb.editor_view.hover_drawn().expect("shown"))
    });
    assert_eq!(word, char_box(&wb, cx, 1, 4), "anchored at the word's start (column 4)");
    assert_ne!(word.0, 200.0, "not at the pointer");
    let gutter = wb.read_with(cx, |wb, _| wb.editor_view.gutter);
    let advance = char_box(&wb, cx, 1, 5).0 - char_box(&wb, cx, 1, 4).0;
    assert!((word.0 - (gutter + 3.0 * advance)).abs() < 0.5, "gutter plus three character advances");
}

#[gpui::test]
fn a_word_after_a_tab_is_anchored_after_the_tab_stop(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench_text(cx, "tab-anchor", "{}", "\tfn main() {}\n");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    // The answer's range is `fn`, chars 2 to 4 of the line.
    wb.update(cx, |wb, _| {
        wb.editor_view.hover.fake = Some(Box::new(|_, _, _| vec![Hover { range: Some(Range::new(1, 2, 1, 4)), ..hover_of("fn") }]));
    });
    let t0 = Instant::now();
    rest_at(&wb, cx, &file, t0, 1, 3, (10.0, 20.0));
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    let word = wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        wb.editor_view.hover_word_box(e, wb.editor_view.hover_drawn().expect("shown"))
    });
    assert_eq!(word, char_box(&wb, cx, 1, 2), "the word's start is column 2");
    // A tab is four cells (`editor.tabSize`): the word starts four advances after the tab's start.
    let advance = char_box(&wb, cx, 1, 3).0 - char_box(&wb, cx, 1, 2).0;
    let tab_start = char_box(&wb, cx, 1, 1).0;
    assert!((word.0 - tab_start - 4.0 * advance).abs() < 0.5, "four cells after the tab");
}

#[gpui::test]
fn a_mouse_down_dismisses_the_widget(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "mouse-dismiss", "{}");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    show_on_main(&wb, cx, &file);
    let at = window_point_at(&wb, cx, 1, 1);
    cx.simulate_mouse_move(at, None, Modifiers::none());
    cx.simulate_click(at, Modifiers::none());
    cx.run_until_parked();
    assert!(!hover_shown(&wb, cx), "a click dismisses the hover");
}

#[gpui::test]
fn a_key_press_and_typing_dismiss_the_widget(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "key-dismiss", "{}");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    show_on_main(&wb, cx, &file);
    cx.simulate_keystrokes("right");
    cx.run_until_parked();
    assert!(!hover_shown(&wb, cx), "a bound key dismisses the hover");
    show_on_main(&wb, cx, &file);
    cx.simulate_input("x");
    cx.run_until_parked();
    assert!(!hover_shown(&wb, cx), "typed text dismisses the hover");
}

#[gpui::test]
fn a_wheel_scroll_dismisses_the_widget(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench_text(cx, "wheel-dismiss", "{}", &"fn main() {}\n".repeat(300));
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    show_on_main(&wb, cx, &file);
    let top_before = char_box(&wb, cx, 1, 4).1;
    let at = window_point_at(&wb, cx, 5, 1);
    cx.simulate_event(ScrollEvent {
        position: at,
        delta: ScrollDelta::Pixels(gpoint(px(0.0), px(-180.0))),
        modifiers: Modifiers::none(),
        touch_phase: TouchPhase::Moved,
        source: ScrollSource::Pointer,
    });
    cx.run_until_parked();
    assert_ne!(char_box(&wb, cx, 1, 4).1, top_before, "the wheel scrolled the document");
    assert!(!hover_shown(&wb, cx), "the scroll dismissed the hover");
}

#[gpui::test]
fn the_long_line_warning_is_shown_on_a_skipped_line_without_provider_hovers(cx: &mut TestAppContext) {
    // `editor.maxTokenizationLineLength` 5: the 12-character line is one the tokenizer skips.
    let (wb, cx, proj) = workbench(cx, "long-line", r#"{"editor.maxTokenizationLineLength": 5}"#);
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    let t0 = Instant::now();
    // No provider answer: the warning alone is shown.
    rest_on_word(&wb, cx, &file, t0);
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    assert_eq!(shown_lines(&wb, cx), Some(vec![LONG_LINE_WARNING.to_string()]));
}

#[gpui::test]
fn editor_hover_show_long_line_warning_off_shows_no_warning(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "long-line-off", r#"{"editor.maxTokenizationLineLength": 5, "editor.hover.showLongLineWarning": false}"#);
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    let t0 = Instant::now();
    rest_on_word(&wb, cx, &file, t0);
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    assert_eq!(shown_lines(&wb, cx), None, "editor.hover.showLongLineWarning = false shows nothing");
}

#[gpui::test]
fn the_widget_is_not_drawn_while_the_suggest_list_is_open(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "suggest-overlap", "{}");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    show_on_main(&wb, cx, &file);
    wb.update(cx, |wb, _| wb.editor_view.suggest_visible = true);
    assert!(wb.read_with(cx, |wb, _| wb.editor_view.hover_drawn().is_none()), "the suggest list covers it");
    assert!(hover_shown(&wb, cx), "the answer is kept for when the list closes");
}

/// Replaces the whole text of the active editor without a key, mouse or scroll event (an extension's
/// edit, a revert, an undo from the palette), and paints.
fn edit_outside_the_keys(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &str) {
    let text = text.to_string();
    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.unwrap();
        let e = &mut wb.editors.open[i].editor;
        e.select_all();
        e.type_text(&text);
        cx.notify();
    });
    cx.run_until_parked();
}

#[gpui::test]
fn a_hover_on_a_line_removed_by_a_non_key_edit_is_dropped_and_paint_does_not_panic(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench_text(cx, "line-removed", "{}", "a\nb\nfn main() {}\n");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    let point = Point { path: file.clone(), line: 3, column: 5 };
    let t0 = Instant::now();
    answer_with(&wb, cx, "fn main()");
    wb.update(cx, |wb, cx| {
        let s = crate::hover::settings(cx);
        wb.editor_view.hover.pointer(t0, Some((point, (10.0, 20.0))), s);
    });
    frame_at(&wb, cx, t0 + Duration::from_millis(300));
    wb.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert!(hover_shown(&wb, cx), "shown on the last line");
    edit_outside_the_keys(&wb, cx, "x");
    assert!(!hover_shown(&wb, cx), "the edit drops the hover");
    // The anchor of a line past the end clamps instead of panicking.
    let b = char_box(&wb, cx, 99, 1);
    assert!(b.1 >= 0.0);
}

#[gpui::test]
fn a_hover_is_dropped_when_another_tab_becomes_active(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "other-tab", "{}");
    let file = proj.join("src/main.rs");
    let other = proj.join("src/other.rs");
    fs::write(&other, "fn other() {}\n").unwrap();
    open(&wb, cx, &file);
    show_on_main(&wb, cx, &file);
    wb.update(cx, |wb, cx| {
        wb.open(&other);
        cx.notify();
    });
    cx.run_until_parked();
    assert!(!hover_shown(&wb, cx), "the hover belongs to the other document");
}
