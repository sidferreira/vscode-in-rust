//! Headless end-to-end tests of the sash grippers (docs/testing.md tier 2; tests/e2e-scenarios/workbench.md,
//! "Sash grippers"). The grip is found by its debug selector, and the idle timer runs on the test clock.
//!
//! upstream: src/vs/workbench/contrib/sash/browser/sash.ts @ 50e1350fc78d

use std::time::Duration;

use gpui::{TestAppContext, VisualTestContext, point, px};
use serde_json::json;

use super::e2e::{set, workbench};

const GRIP: &str = "sidebar-resize-gripper";

fn advance(cx: &mut VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn move_mouse(cx: &mut VisualTestContext) {
    let p = point(px(300.), px(300.));
    cx.simulate_mouse_move(p, None, Default::default());
    cx.run_until_parked();
}

#[gpui_kit::test]
fn the_grip_is_drawn_on_the_sidebar_sash(cx: &mut TestAppContext) {
    // Upstream source: sash.contribution.ts (`workbench.sash.autoHideGrippers` default true; grippers shown).
    let (_wb, cx, _f) = workbench(cx, "grip", None, &[("a.txt", "x")]);
    cx.run_until_parked();
    assert!(
        cx.debug_bounds(GRIP).is_some(),
        "the grip is drawn when the sidebar is shown"
    );
}

#[gpui_kit::test]
fn auto_hide_recedes_the_grip_after_the_delay_and_mouse_movement_brings_it_back(
    cx: &mut TestAppContext,
) {
    // Upstream source: sash.ts `SashGrippersController.showGrippers` (idle timeout, shown again on mouse move).
    let (_wb, cx, _f) = workbench(cx, "grip-idle", None, &[("a.txt", "x")]);
    move_mouse(cx);
    advance(cx, 1000);
    assert!(
        cx.debug_bounds(GRIP).is_some(),
        "still shown inside the 2000 ms delay"
    );
    advance(cx, 1500);
    assert!(
        cx.debug_bounds(GRIP).is_none(),
        "receded after the idle delay"
    );
    move_mouse(cx);
    assert!(
        cx.debug_bounds(GRIP).is_some(),
        "shown again when the mouse moves"
    );
}

#[gpui_kit::test]
fn the_delay_setting_is_read_live(cx: &mut TestAppContext) {
    let (wb, cx, _f) = workbench(cx, "grip-delay", None, &[("a.txt", "x")]);
    set(cx, "workbench.sash.autoHideGrippersDelay", json!(300));
    wb.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    move_mouse(cx);
    advance(cx, 400);
    assert!(
        cx.debug_bounds(GRIP).is_none(),
        "receded after the shortened 300 ms delay"
    );
}

#[gpui_kit::test]
fn disabling_auto_hide_keeps_the_grip_visible(cx: &mut TestAppContext) {
    let (_wb, cx, _f) = workbench(
        cx,
        "grip-off",
        Some(r#"{ "workbench.sash.autoHideGrippers": false }"#),
        &[("a.txt", "x")],
    );
    move_mouse(cx);
    advance(cx, 10_000);
    assert!(
        cx.debug_bounds(GRIP).is_some(),
        "the grip stays visible when auto-hide is off"
    );
}
