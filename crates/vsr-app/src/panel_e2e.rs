//! Headless end-to-end tests of the panel (docs/testing.md tier 2).
//!
//! Upstream source: src/vs/workbench/browser/parts/panel/panelPart.ts, panelActions.ts

use std::fs;

use gpui::{Bounds, Entity, Pixels, TestAppContext, VisualTestContext, point, px};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;
use crate::panel::PanelView;

pub(crate) fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: Option<&str>) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = vsr_test_scratch::dir!().join(format!("rscode-panel-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.json");
    if let Some(text) = settings {
        fs::write(&path, text).unwrap();
    }
    fs::write(dir.join("a.txt"), "x").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, Some(path));
    });
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(dir, cx));
    vcx.run_until_parked();
    (wb, vcx)
}

pub(crate) fn bounds(cx: &mut VisualTestContext, sel: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(sel).unwrap_or_else(|| panic!("{sel} is not rendered"))
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn cmd_j_toggles_the_panel(cx: &mut TestAppContext) {
    let (_wb, cx) = workbench(cx, "toggle", None);
    assert!(cx.debug_bounds("panel").is_none(), "hidden at first");
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    let (panel, editor) = (bounds(cx, "panel"), bounds(cx, "editor-group"));
    assert!(editor.bottom() <= panel.top() + px(1.), "the panel sits under the editor group");
    assert_eq!(panel.size.height, px(300.));
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    assert!(cx.debug_bounds("panel").is_none());
}

#[gpui_kit::test]
fn tabs_switch_views_and_commands_show_them(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tabs", None);
    cx.simulate_keystrokes("cmd-shift-u");
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active), PanelView::Output);
    let tab = bounds(cx, "panel-tab:problems");
    cx.simulate_mouse_move(tab.center(), None, Default::default());
    cx.simulate_click(tab.center(), Default::default());
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active), PanelView::Problems);
    wb.update(cx, |w, cx| w.step_panel_view(1, cx));
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active), PanelView::Output);
}

#[gpui_kit::test]
fn dragging_the_sash_resizes_the_panel(cx: &mut TestAppContext) {
    let (_wb, cx) = workbench(cx, "drag", None);
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    let sash = bounds(cx, "panel-sash").center();
    cx.simulate_mouse_move(sash, None, Default::default());
    cx.simulate_mouse_down(sash, gpui::MouseButton::Left, Default::default());
    let up = point(sash.x, sash.y - px(100.));
    cx.simulate_mouse_move(up, Some(gpui::MouseButton::Left), Default::default());
    cx.run_until_parked();
    assert_eq!(bounds(cx, "panel").size.height, px(400.));
    cx.simulate_mouse_up(up, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(point(sash.x, sash.y + px(50.)), Some(gpui::MouseButton::Left), Default::default());
    cx.run_until_parked();
    assert_eq!(bounds(cx, "panel").size.height, px(400.), "released: no more resizing");
}

#[gpui_kit::test]
fn default_location_places_the_panel(cx: &mut TestAppContext) {
    let (_wb, cx) = workbench(cx, "location", None);
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    for (loc, ok) in [("top", 0), ("left", 1), ("right", 2), ("bottom", 3)] {
        set(cx, "workbench.panel.defaultLocation", json!(loc));
        let (p, e) = (bounds(cx, "panel"), bounds(cx, "editor-group"));
        let good = match ok {
            0 => p.bottom() <= e.top() + px(1.),
            1 => p.right() <= e.left() + px(1.),
            2 => e.right() <= p.left() + px(1.),
            _ => e.bottom() <= p.top() + px(1.),
        };
        assert!(good, "{loc}: {p:?} {e:?}");
    }
}

#[gpui_kit::test]
fn opens_maximized_always_hides_the_editor_group(cx: &mut TestAppContext) {
    let (_wb, cx) = workbench(cx, "max", Some(r#"{ "workbench.panel.opensMaximized": "always" }"#));
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    assert!(cx.debug_bounds("editor-group").is_none());
    assert!(bounds(cx, "panel").size.height > px(500.));
    cx.simulate_keystrokes("cmd-j"); // hide
    set(cx, "workbench.panel.opensMaximized", json!("never"));
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    assert!(cx.debug_bounds("editor-group").is_some(), "never: opens restored");
}

#[gpui_kit::test]
fn show_labels_false_shows_icons_only(cx: &mut TestAppContext) {
    let (_wb, cx) = workbench(cx, "labels", None);
    cx.simulate_keystrokes("cmd-j");
    cx.run_until_parked();
    let with_label = bounds(cx, "panel-tab:problems").size.width;
    set(cx, "workbench.panel.showLabels", json!(false));
    let icon_only = bounds(cx, "panel-tab:problems").size.width;
    assert!(icon_only < with_label, "{icon_only:?} < {with_label:?}");
}
