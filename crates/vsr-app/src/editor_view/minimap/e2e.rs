//! Headless end-to-end tests of the minimap (docs/testing.md tier 2): a real `Workbench` in gpui's
//! test platform with an injected temp settings.json (never the user's). The test platform's text
//! system is a no-op, so these assert layout bounds and the minimap model (`Frame`), not pixels.
//!
//! Scenario source: tests/e2e-scenarios/preferences.md ("Minimap"). Our own tests.

use std::fs;
use std::path::PathBuf;

use gpui_kit::{Bounds, Entity, MouseButton, Pixels, TestAppContext, VisualTestContext, point, px};
use serde_json::json;

use super::{Frame, MinimapAutohide, MinimapSide, MinimapSize};
use crate::Workbench;
use crate::config::AppConfig;

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-minimap-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn numbered_lines(n: usize) -> String {
    (1..=n).map(|i| format!("    let line_{i} = compute({i}); // lorem ipsum dolor\n")).collect()
}

/// A workbench over a temp folder with `a.txt` (`lines` lines) open, and its settings in a temp file.
fn open<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    lines: usize,
    settings: &str,
) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let dir = unique_dir(name);
    let settings_path = dir.join("settings.json");
    fs::write(&settings_path, settings).unwrap();
    fs::write(dir.join("a.txt"), numbered_lines(lines)).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        AppConfig::install(cx, Some(settings_path));
    });
    let root = dir.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    let file = dir.join("a.txt");
    wb.update(vcx, |wb, cx| {
        wb.open(&file);
        cx.notify();
    });
    vcx.run_until_parked();
    (wb, vcx)
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

fn frame(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Frame> {
    wb.read_with(cx, |wb, _| wb.editor_view.minimap.frame().cloned())
}

fn minimap(cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.debug_bounds("minimap")
}

fn slider(cx: &mut VisualTestContext) -> Option<Bounds<Pixels>> {
    cx.debug_bounds("minimap-slider")
}

/// The editor's text area (list and caret surface), as last painted.
fn text_area(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    wb.read_with(cx, |wb, _| wb.editor_view.bounds.get())
}

fn scroll_top(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> f32 {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        let s = wb.editor_view.scrolls.get(&e.path).expect("scroll handle");
        -f32::from(s.0.borrow().base_handle.offset().y)
    })
}

fn near(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

// ---- presence, enabled, side ---------------------------------------------------------------------

#[gpui_kit::test]
fn minimap_is_shown_by_default_on_the_right(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "default", 1000, "");
    let m = minimap(cx).expect("the minimap is rendered by default");
    let text = text_area(&wb, cx);
    let f = frame(&wb, cx).expect("a minimap frame");
    assert_eq!(f.side, MinimapSide::Right);
    assert!(m.left() >= text.right() - px(1.0), "right of the text: {m:?} vs {text:?}");
    assert!(f32::from(m.size.width) > 8.0 && f.info.width > 8.0, "has a width: {m:?}");
    assert!(near(f32::from(m.size.width), f.info.width as f32, 0.5));
    assert_eq!(f.size, MinimapSize::Proportional);
    // Everything fits the editor's full width: text + minimap.
    let editor = cx.debug_bounds("editor-group").unwrap();
    assert!(near(f32::from(text.size.width + m.size.width), f32::from(editor.size.width), 2.0), "{text:?} {m:?} {editor:?}");
    // 1000 lines do not fit the editor, so there is a slider covering the visible part.
    let s = slider(cx).expect("slider");
    assert!(f.layout.slider_needed);
    assert!(s.top() >= m.top() && s.bottom() < m.bottom());
    assert!(near(f32::from(s.size.height), f.layout.slider_height as f32, 0.5));
}

#[gpui_kit::test]
fn disabling_the_minimap_removes_it_and_gives_the_editor_its_width_back(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "enabled", 200, "");
    let with = text_area(&wb, cx);
    let m = minimap(cx).unwrap();
    set(cx, "editor.minimap.enabled", json!(false));
    assert!(minimap(cx).is_none(), "no minimap element");
    assert!(frame(&wb, cx).is_none());
    let without = text_area(&wb, cx);
    assert!(
        near(f32::from(without.size.width - with.size.width), f32::from(m.size.width), 1.0),
        "the text area grew by the minimap width: {with:?} -> {without:?} ({m:?})"
    );
    set(cx, "editor.minimap.enabled", json!(true));
    assert!(minimap(cx).is_some());
    assert!(near(f32::from(text_area(&wb, cx).size.width), f32::from(with.size.width), 1.0));
}

#[gpui_kit::test]
fn side_left_puts_the_minimap_before_the_editor(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "side", 200, r#"{ "editor.minimap.side": "left" }"#);
    let (m, text) = (minimap(cx).unwrap(), text_area(&wb, cx));
    assert!(m.right() <= text.left() + px(1.0), "left of the text: {m:?} vs {text:?}");
    assert_eq!(frame(&wb, cx).unwrap().side, MinimapSide::Left);
    // Live: back to the right.
    set(cx, "editor.minimap.side", json!("right"));
    let (m, text) = (minimap(cx).unwrap(), text_area(&wb, cx));
    assert!(m.left() >= text.right() - px(1.0), "{m:?} vs {text:?}");
}

// ---- click, drag, slider -------------------------------------------------------------------------

#[gpui_kit::test]
fn clicking_the_minimap_centres_the_clicked_line(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "click", 1000, "");
    assert_eq!(scroll_top(&wb, cx), 0.0);
    let (m, f) = (minimap(cx).unwrap(), frame(&wb, cx).unwrap());
    let css_line_height = (f.info.line_height / f.info.canvas_inner_height * f.info.canvas_outer_height) as f32;
    let at_y = 300.0_f32;
    let expected_line = ((at_y / css_line_height).floor() as f64 + f.layout.start_line - f.layout.top_padding_line_count) as f32;
    let click = point(m.left() + px(20.0), m.top() + px(at_y));
    cx.simulate_mouse_move(click, None, Default::default());
    cx.simulate_click(click, Default::default());
    cx.run_until_parked();

    let lh = f.line_height;
    let expected = (expected_line - 1.0) * lh + lh / 2.0 - f.outer_height / 2.0;
    let got = scroll_top(&wb, cx);
    assert!(expected > 1000.0, "the click is far from the top: {expected}");
    assert!(near(got, expected, 1.0), "scrolled to centre line {expected_line}: {got} vs {expected}");
    // The slider follows: it now sits further down.
    let after = frame(&wb, cx).unwrap();
    assert!(after.layout.slider_top > f.layout.slider_top, "{:?} -> {:?}", f.layout, after.layout);
    assert!(near(after.scroll_top, got, 1.0));
}

#[gpui_kit::test]
fn the_slider_tracks_scrolling_and_stays_inside_the_minimap(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "track", 1000, "");
    let first = slider(cx).unwrap();
    let m = minimap(cx).unwrap();
    let height = first.size.height;
    assert!(near(f32::from(first.top() - m.top()), 0.0, 0.5), "at the top of the minimap at scroll 0");

    cx.simulate_keystrokes("cmd-down"); // caret to the end: the editor scrolls to the last line
    cx.run_until_parked();
    cx.run_until_parked();
    let f = frame(&wb, cx).unwrap();
    let last = slider(cx).unwrap();
    assert!(scroll_top(&wb, cx) > 10_000.0, "scrolled: {}", scroll_top(&wb, cx));
    assert!(last.top() > first.top() + px(100.0), "slider moved down: {first:?} -> {last:?}");
    assert!(near(f32::from(last.size.height), f32::from(height), 0.5), "stable height");
    // Inside the minimap, up to part of a line: the aligned top (upstream sliderTopAligned) may run past
    // the bottom by a fraction of a minimap line when the viewport is not a whole number of lines.
    assert!(last.bottom() <= m.bottom() + px(2.0), "inside the minimap: {last:?} in {m:?}");
    assert!(near(f32::from(last.top() - m.top()), f.layout.slider_top as f32, 0.5), "bounds match the layout");

    cx.simulate_keystrokes("cmd-up");
    cx.run_until_parked();
    cx.run_until_parked();
    let top = slider(cx).unwrap();
    assert!(near(f32::from(top.top() - m.top()), 0.0, 0.5), "back at the top: {top:?}");
}

#[gpui_kit::test]
fn dragging_the_slider_scrolls_the_editor(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "drag", 1000, "");
    let (s, f) = (slider(cx).unwrap(), frame(&wb, cx).unwrap());
    let start = point(s.center().x, s.center().y);
    cx.simulate_mouse_move(start, None, Default::default());
    cx.simulate_mouse_down(start, MouseButton::Left, Default::default());
    cx.run_until_parked();
    let to = point(start.x, start.y + px(50.0));
    cx.simulate_mouse_move(to, Some(MouseButton::Left), Default::default());
    cx.run_until_parked();
    let expected = (50.0 / f.layout.slider_ratio()).round() as f32;
    assert!(near(scroll_top(&wb, cx), expected, 1.0), "{} vs {expected}", scroll_top(&wb, cx));
    cx.simulate_mouse_up(to, MouseButton::Left, Default::default());
    cx.run_until_parked();
    // After the release the slider no longer follows the pointer.
    let held = scroll_top(&wb, cx);
    cx.simulate_mouse_move(point(to.x, to.y + px(80.0)), Some(MouseButton::Left), Default::default());
    cx.run_until_parked();
    assert_eq!(scroll_top(&wb, cx), held);
}

#[gpui_kit::test]
fn short_documents_have_no_slider(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "short", 5, "");
    assert!(minimap(cx).is_some());
    assert!(!frame(&wb, cx).unwrap().layout.slider_needed);
    assert!(slider(cx).is_none());
}

// ---- the other settings ----------------------------------------------------------------------------

#[gpui_kit::test]
fn render_characters_scale_and_max_column_change_the_layout(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "opts", 300, "");
    let base = frame(&wb, cx).unwrap();
    // Device pixels: the configured scale is doubled at a pixel ratio of 2 or more.
    let device = |configured: f64| compute_scale(configured, base.pixel_ratio);
    assert_eq!((base.info.render, base.info.scale, base.info.line_height), (super::RenderMinimap::Text, device(1.0), 2.0 * device(1.0)));

    set(cx, "editor.minimap.renderCharacters", json!(false));
    let blocks = frame(&wb, cx).unwrap();
    assert_eq!((blocks.info.render, blocks.info.line_height), (super::RenderMinimap::Blocks, 3.0 * device(1.0)), "blocks are 3px lines");

    set(cx, "editor.minimap.scale", json!(2));
    let scaled = frame(&wb, cx).unwrap();
    assert_eq!((scaled.info.scale, scaled.info.line_height), (device(2.0), 3.0 * device(2.0)));
    set(cx, "editor.minimap.renderCharacters", json!(true));
    assert_eq!(frame(&wb, cx).unwrap().info.line_height, 2.0 * device(2.0), "2px * scale");

    set(cx, "editor.minimap.maxColumn", json!(30));
    let narrow = frame(&wb, cx).unwrap();
    let char_width = device(2.0) / base.pixel_ratio;
    assert!(narrow.info.width <= (30.0 * char_width).floor() && narrow.info.width < base.info.width, "{} < {}", narrow.info.width, base.info.width);
    assert!(near(f32::from(minimap(cx).unwrap().size.width), narrow.info.width as f32, 0.5));
}

/// `_computeMinimapScale`.
fn compute_scale(configured: f64, pixel_ratio: f64) -> f64 {
    if pixel_ratio >= 2.0 { (configured * 2.0).round() } else { configured.max((configured * pixel_ratio).floor()) }
}

#[gpui_kit::test]
fn size_fill_and_fit_stretch_the_minimap_over_the_editor_height(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "size", 60, "");
    let normal = frame(&wb, cx).unwrap();
    assert!(!normal.info.height_is_editor_height);

    set(cx, "editor.minimap.size", json!("fill"));
    let fill = frame(&wb, cx).unwrap();
    assert!(fill.info.height_is_editor_height && !fill.info.is_sampling);
    assert!(fill.info.line_height > normal.info.line_height, "lines are taller to fill the height: {fill:?}");

    set(cx, "editor.minimap.size", json!("fit"));
    let fit = frame(&wb, cx).unwrap();
    assert_eq!(fit.size, MinimapSize::Fit);
    assert!(!fit.info.height_is_editor_height || fit.info.line_height >= normal.info.line_height);
}

#[gpui_kit::test]
fn size_fill_samples_long_documents_and_clicks_drag_from_the_slider_centre(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "sample", 3000, r#"{ "editor.minimap.size": "fill" }"#);
    let f = frame(&wb, cx).unwrap();
    assert!(f.info.is_sampling && f.info.line_height == 1.0, "{f:?}");
    // The whole document is mapped on the editor height: end line is capped by the canvas height.
    assert!(f.layout.end_line <= f.info.canvas_inner_height);
    // A click far below the slider (non proportional) moves the slider's centre to the pointer.
    let m = minimap(cx).unwrap();
    let click = point(m.left() + px(10.0), m.top() + m.size.height * 0.75);
    cx.simulate_mouse_move(click, None, Default::default());
    cx.simulate_click(click, Default::default());
    cx.run_until_parked();
    let after = frame(&wb, cx).unwrap();
    let centre = m.top() + px((after.layout.slider_top + after.layout.slider_height / 2.0) as f32);
    assert!(near(f32::from(centre), f32::from(click.y), 3.0), "slider centred on the click: {centre:?} vs {click:?}");
}

#[gpui_kit::test]
fn show_slider_and_autohide_follow_hover(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "hover", 1000, "");
    // mouseover (default): the slider is invisible until the pointer is over the minimap.
    assert!(!frame(&wb, cx).unwrap().slider_visible);
    let m = minimap(cx).unwrap();
    let over = point(m.center().x, m.top() + px(400.0));
    cx.simulate_mouse_move(over, None, Default::default());
    cx.run_until_parked();
    assert!(frame(&wb, cx).unwrap().slider_visible, "visible on hover");
    cx.simulate_mouse_move(point(m.left() - px(200.0), m.center().y), None, Default::default());
    cx.run_until_parked();
    assert!(!frame(&wb, cx).unwrap().slider_visible, "hidden again");

    set(cx, "editor.minimap.showSlider", json!("always"));
    assert!(frame(&wb, cx).unwrap().slider_visible);

    // autohide mouseover: the minimap itself is only visible on hover.
    set(cx, "editor.minimap.autohide", json!("mouseover"));
    assert!(!frame(&wb, cx).unwrap().minimap_visible);
    let m = minimap(cx).unwrap();
    cx.simulate_mouse_move(point(m.center().x, m.top() + px(100.0)), None, Default::default());
    cx.run_until_parked();
    assert!(frame(&wb, cx).unwrap().minimap_visible);
    // Still occupies its width while hidden (it fades, it does not collapse).
    cx.simulate_mouse_move(point(m.left() - px(200.0), m.center().y), None, Default::default());
    cx.run_until_parked();
    assert!(!frame(&wb, cx).unwrap().minimap_visible);
    assert!(minimap(cx).is_some());
}

#[gpui_kit::test]
fn autohide_scroll_shows_the_minimap_only_after_scrolling(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "scrollhide", 1000, r#"{ "editor.minimap.autohide": "scroll" }"#);
    assert_eq!(wb.read_with(cx, |wb, _| wb.editor_view.cfg.minimap.autohide), MinimapAutohide::Scroll);
    assert!(!frame(&wb, cx).unwrap().minimap_visible);
    cx.simulate_keystrokes("cmd-down");
    cx.run_until_parked();
    cx.run_until_parked();
    assert!(frame(&wb, cx).unwrap().minimap_visible, "shown while scrolling");
    cx.executor().advance_clock(std::time::Duration::from_millis(700));
    cx.run_until_parked();
    assert!(!frame(&wb, cx).unwrap().minimap_visible, "hidden again after the delay");
}

// ---- performance -------------------------------------------------------------------------------------

#[gpui_kit::test]
fn a_10k_line_file_draws_only_the_lines_that_fit_and_stays_responsive(cx: &mut TestAppContext) {
    let (wb, cx) = open(cx, "big", 10_000, "");
    let f = frame(&wb, cx).unwrap();
    let drawn = f.layout.end_line - f.layout.start_line + 1.0;
    let fitting = (f.info.canvas_inner_height / f.info.line_height).floor();
    assert!(drawn <= fitting && drawn < 1000.0, "only the visible minimap lines: {drawn} of 10000 (fit {fitting})");
    // Best of three batches: the suite runs in parallel and agents may be compiling, so one batch
    // is noisy; the minimum still catches per-key work that grows with the file.
    let mut per_key = std::time::Duration::MAX;
    for _ in 0..3 {
        let start = std::time::Instant::now();
        for _ in 0..5 {
            cx.simulate_keystrokes("pagedown pagedown pagedown pagedown");
        }
        per_key = per_key.min(start.elapsed() / 20);
    }
    eprintln!("10k-line file with minimap: {per_key:?} per page-down, best of 3 (debug build, incl. layout and paint)");
    assert!(per_key < std::time::Duration::from_millis(100), "{per_key:?} per key");
    assert!(frame(&wb, cx).unwrap().layout.start_line > 1.0, "the minimap window moved with the editor");
    assert_eq!(MinimapAutohide::None, wb.read_with(cx, |wb, _| wb.editor_view.cfg.minimap.autohide));
}
