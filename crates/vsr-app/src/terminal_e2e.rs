//! Headless end-to-end tests of the integrated terminal (docs/testing.md tier 2): real `/bin/sh`
//! processes on real PTYs behind a headless workbench window, no window on screen.
//!
//! Upstream source: src/vs/workbench/contrib/terminal/browser/terminalActions.ts,
//! terminalInstance.ts, terminalConfigurationService.ts

use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext, point, px};
use serde_json::json;
use vsr_terminal::Snapshot;

use crate::Workbench;
use crate::config::AppConfig;
use crate::panel::PanelView;
use crate::panel_e2e::{bounds, workbench};

pub(crate) fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// Makes the next terminal run `/bin/sh -c script`.
pub(crate) fn shell(cx: &mut VisualTestContext, script: &str) {
    set(cx, "terminal.integrated.profiles.osx", json!({ "t": { "path": "/bin/sh", "args": ["-c", script] } }));
    set(cx, "terminal.integrated.defaultProfile.osx", json!("t"));
}

fn wait_until(cx: &mut VisualTestContext, what: &str, mut done: impl FnMut(&mut VisualTestContext) -> bool) {
    cx.executor().allow_parking();
    for _ in 0..400 {
        cx.executor().advance_clock(Duration::from_millis(20)); // the view polls the shell's output on a timer
        cx.run_until_parked();
        if done(cx) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("timed out waiting for {what}");
}

pub(crate) fn snapshot(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Snapshot> {
    wb.read_with(cx, |w, cx| w.panel.terminals.get(w.panel.active_terminal).and_then(|t| t.view.read(cx).snapshot()))
}

pub(crate) fn wait_for_text(wb: &Entity<Workbench>, cx: &mut VisualTestContext, text: &str) -> Snapshot {
    let wb = wb.clone();
    let mut last = None;
    wait_until(cx, &format!("{text:?} on the terminal screen"), |cx| {
        last = snapshot(&wb, cx);
        last.as_ref().is_some_and(|s| s.text().contains(text))
    });
    last.unwrap()
}

fn terminal_count(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> usize {
    wb.read_with(cx, |w, _| w.panel.terminals.len())
}

#[gpui_kit::test]
fn ctrl_backtick_opens_the_panel_on_a_terminal_showing_the_shells_output(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-open", None);
    shell(cx, "printf 'hello from sh'; cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    assert!(cx.debug_bounds("panel").is_some());
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active), PanelView::Terminal);
    assert_eq!(terminal_count(&wb, cx), 1);
    wait_for_text(&wb, cx, "hello from sh");
    assert!(cx.debug_bounds("terminal-grid").is_some());
    assert!(cx.debug_bounds("terminal-cursor").is_some());
    cx.simulate_keystrokes("ctrl-`"); // hides the panel, keeps the terminal
    assert!(cx.debug_bounds("panel").is_none());
    assert_eq!(terminal_count(&wb, cx), 1);
    cx.simulate_keystrokes("ctrl-`");
    assert!(cx.debug_bounds("panel").is_some());
    assert_eq!(terminal_count(&wb, cx), 1, "shown again, not created again");
}

#[gpui_kit::test]
fn the_shell_starts_in_the_workspace_folder(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-cwd", None);
    shell(cx, "pwd; cat");
    cx.simulate_keystrokes("ctrl-`");
    let root = wb.read_with(cx, |w, _| w.root.canonicalize().unwrap());
    wait_for_text(&wb, cx, root.to_str().unwrap());
}

#[gpui_kit::test]
fn typed_text_and_enter_reach_the_shell(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-type", None);
    shell(cx, "read line; printf 'got:%s' \"$line\"; cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    cx.simulate_keystrokes("a b c enter");
    wait_for_text(&wb, cx, "got:abc");
}

#[gpui_kit::test]
fn control_and_arrow_keys_are_sent_as_terminal_sequences(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-keys", None);
    shell(cx, "stty raw -echo; printf ready; cat -vt");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "ready");
    cx.simulate_keystrokes("ctrl-a up left ctrl-left escape tab shift-tab");
    let s = wait_for_text(&wb, cx, "^[[Z");
    assert!(s.text().contains("^A^[[A^[[D^[[1;5D^[^I^[[Z"), "{:?}", s.text());
}

#[gpui_kit::test]
fn ctrl_g_goes_to_the_shell_not_to_go_to_line(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-ctrl-g", None);
    shell(cx, "stty raw -echo; printf ready; cat -v");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "ready");
    cx.simulate_keystrokes("ctrl-g");
    wait_for_text(&wb, cx, "^G");
}

#[gpui_kit::test]
fn workbench_keys_still_work_with_the_terminal_focused(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-wb-keys", None);
    shell(cx, "cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-b");
    assert!(cx.debug_bounds("side-bar").is_none(), "cmd-b toggled the side bar");
    cx.simulate_keystrokes("cmd-j");
    assert!(cx.debug_bounds("panel").is_none(), "cmd-j hid the panel");
    let _ = wb;
}

#[gpui_kit::test]
fn a_clean_exit_closes_the_terminal_and_hides_the_panel(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-exit0", None);
    shell(cx, "printf done");
    cx.simulate_keystrokes("ctrl-`");
    let wbc = wb.clone();
    wait_until(cx, "the terminal to close", |cx| terminal_count(&wbc, cx) == 0);
    assert!(cx.debug_bounds("panel").is_none(), "terminal.integrated.hideOnLastClosed");
}

#[gpui_kit::test]
fn hide_on_last_closed_false_keeps_the_panel(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-keep", None);
    set(cx, "terminal.integrated.hideOnLastClosed", json!(false));
    shell(cx, "true");
    cx.simulate_keystrokes("ctrl-`");
    let wbc = wb.clone();
    wait_until(cx, "the terminal to close", |cx| terminal_count(&wbc, cx) == 0);
    assert!(cx.debug_bounds("panel").is_some());
}

#[gpui_kit::test]
fn a_failing_shell_stays_with_its_exit_code(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-exit3", None);
    shell(cx, "printf failing; exit 3");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "failing");
    wait_until(cx, "the exit message", |cx| {
        cx.run_until_parked();
        cx.debug_bounds("terminal-exit-message").is_some()
    });
    assert_eq!(terminal_count(&wb, cx), 1);
}

#[gpui_kit::test]
fn new_terminal_adds_a_tab_and_kill_removes_the_active_one(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-new-kill", None);
    shell(cx, "cat");
    cx.simulate_keystrokes("ctrl-`");
    assert!(cx.debug_bounds("terminal-tabs").is_none(), "a single terminal shows no list");
    cx.simulate_keystrokes("ctrl-shift-`");
    cx.run_until_parked();
    assert_eq!(terminal_count(&wb, cx), 2);
    assert!(cx.debug_bounds("terminal-tab:0").is_some() && cx.debug_bounds("terminal-tab:1").is_some());
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active_terminal), 1);
    // Select the first one by clicking its tab.
    let tab = bounds(cx, "terminal-tab:0").center();
    cx.simulate_mouse_move(tab, None, Default::default());
    cx.simulate_click(tab, Default::default());
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active_terminal), 0);
    // The kill button of the title bar.
    let kill = bounds(cx, "terminal-kill").center();
    cx.simulate_mouse_move(kill, None, Default::default());
    cx.simulate_click(kill, Default::default());
    cx.run_until_parked();
    assert_eq!(terminal_count(&wb, cx), 1);
    assert!(cx.debug_bounds("terminal-tabs").is_none());
    // The New button.
    let new = bounds(cx, "terminal-new").center();
    cx.simulate_mouse_move(new, None, Default::default());
    cx.simulate_click(new, Default::default());
    cx.run_until_parked();
    assert_eq!(terminal_count(&wb, cx), 2);
}

#[gpui_kit::test]
fn the_terminal_grid_follows_the_size_of_the_panel(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-size", None);
    shell(cx, "sleep 0.5; stty size; cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    cx.run_until_parked();
    let s = snapshot(&wb, cx).unwrap();
    let grid = bounds(cx, "terminal-grid");
    let (cw, lh) = wb.read_with(cx, |w, cx| w.panel.terminals[0].view.read(cx).cell_size());
    assert_eq!(s.cols, (f32::from(grid.size.width) / cw).floor() as usize);
    assert_eq!(s.rows, (f32::from(grid.size.height) / lh).floor() as usize);
    wait_for_text(&wb, cx, &format!("{} {}", s.rows, s.cols));
}

#[gpui_kit::test]
fn shift_page_up_scrolls_into_the_history_and_typing_returns_to_the_bottom(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-scroll", None);
    shell(cx, "i=1; while [ $i -le 120 ]; do echo line$i; i=$((i+1)); done; cat");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "line120");
    assert_eq!(snapshot(&wb, cx).unwrap().display_offset, 0);
    cx.simulate_keystrokes("shift-pageup");
    cx.run_until_parked();
    let s = snapshot(&wb, cx).unwrap();
    assert!(s.display_offset > 0 && !s.text().contains("line120"), "offset {}", s.display_offset);
    cx.simulate_keystrokes("cmd-end");
    assert_eq!(snapshot(&wb, cx).unwrap().display_offset, 0);
    cx.simulate_keystrokes("shift-pageup a");
    assert_eq!(snapshot(&wb, cx).unwrap().display_offset, 0, "typing scrolls to the bottom");
}

#[gpui_kit::test]
fn dragging_selects_text_and_cmd_c_copies_it(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-copy", None);
    shell(cx, "printf 'copy this text'; cat");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "copy this text");
    let grid = bounds(cx, "terminal-grid");
    let (cw, lh) = wb.read_with(cx, |w, cx| w.panel.terminals[0].view.read(cx).cell_size());
    let at = |col: f32| point(grid.origin.x + px(col * cw + cw / 2.), grid.origin.y + px(lh / 2.));
    cx.simulate_mouse_move(at(0.), None, Default::default());
    cx.simulate_mouse_down(at(0.), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(at(3.), Some(gpui::MouseButton::Left), Default::default());
    cx.simulate_mouse_up(at(3.), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("copy"));
}

#[gpui_kit::test]
fn cmd_v_pastes_the_clipboard_into_the_shell(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-paste", None);
    shell(cx, "read line; printf 'pasted:%s' \"$line\"; cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("xyz\n".to_string()));
    cx.simulate_keystrokes("cmd-v");
    wait_for_text(&wb, cx, "pasted:xyz");
}

#[gpui_kit::test]
fn the_cursor_is_drawn_at_the_shells_cursor_cell(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-cursor", None);
    shell(cx, "printf 'abc'; cat");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "abc");
    cx.run_until_parked();
    let (grid, cursor) = (bounds(cx, "terminal-grid"), bounds(cx, "terminal-cursor"));
    let (cw, lh) = wb.read_with(cx, |w, cx| w.panel.terminals[0].view.read(cx).cell_size());
    assert!((f32::from(cursor.origin.x - grid.origin.x) - 3. * cw).abs() < 0.6);
    assert!((f32::from(cursor.size.height) - lh).abs() < 0.6);
}

fn active_view(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Entity<crate::terminal_view::TerminalView> {
    wb.read_with(cx, |w, _| w.panel.terminals[w.panel.active_terminal].view.clone())
}

fn three_terminals(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    shell(cx, "cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.simulate_keystrokes("ctrl-shift-`");
    cx.simulate_keystrokes("ctrl-shift-`");
    cx.run_until_parked();
    assert_eq!(terminal_count(wb, cx), 3);
}

#[gpui_kit::test]
fn removing_a_terminal_before_the_active_one_keeps_the_active_one(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-remove-below", None);
    three_terminals(&wb, cx);
    let third = active_view(&wb, cx);
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active_terminal), 2);
    wb.update(cx, |w, cx| w.remove_terminal(0, cx));
    assert_eq!(wb.read_with(cx, |w, _| w.panel.active_terminal), 1);
    assert_eq!(active_view(&wb, cx), third, "the same terminal stays active");
    // Removing the active last one selects the new last one; removing a later one changes nothing.
    wb.update(cx, |w, _| w.panel.active_terminal = 0);
    let first = active_view(&wb, cx);
    wb.update(cx, |w, cx| w.remove_terminal(1, cx));
    assert_eq!(active_view(&wb, cx), first);
}

#[gpui_kit::test]
fn a_drag_below_the_grid_selects_within_it(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-drag-below", None);
    shell(cx, "printf 'one\\r\\ntwo'; cat");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "two");
    let grid = bounds(cx, "terminal-grid");
    let (cw, lh) = wb.read_with(cx, |w, cx| w.panel.terminals[0].view.read(cx).cell_size());
    let start = point(grid.origin.x + px(cw / 2.), grid.origin.y + px(lh / 2.));
    let outside = point(grid.origin.x + grid.size.width + px(500.), grid.origin.y + grid.size.height + px(500.));
    cx.simulate_mouse_move(start, None, Default::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(outside, Some(gpui::MouseButton::Left), Default::default());
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    let s = snapshot(&wb, cx).unwrap();
    let ((r0, c0), (r1, c1), _) = s.selection.expect("a selection");
    assert!(r0.max(r1) < s.rows && c0.max(c1) < s.cols, "{:?}", s.selection);
    cx.simulate_keystrokes("cmd-c");
    let copied = cx.read_from_clipboard().and_then(|c| c.text()).unwrap_or_default();
    assert!(copied.starts_with("one") && copied.contains("two") && copied.len() < 4000, "{copied:?}");
}

#[gpui_kit::test]
fn focus_moves_on_when_the_focused_terminal_is_removed(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-focus-removed", None);
    shell(cx, "cat");
    cx.simulate_keystrokes("ctrl-`");
    cx.simulate_keystrokes("ctrl-shift-`");
    cx.run_until_parked();
    let (first, second) = wb.read_with(cx, |w, _| (w.panel.terminals[0].view.clone(), w.panel.terminals[1].view.clone()));
    assert!(cx.update(|window, cx| second.read(cx).is_focused(window)));
    wb.update(cx, |w, cx| w.kill_terminal(cx));
    cx.run_until_parked();
    assert!(cx.update(|window, cx| first.read(cx).is_focused(window)), "the focus moved to the remaining terminal");
    // The last one: the focus goes back to the workbench.
    wb.update(cx, |w, cx| w.kill_terminal(cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |w, _| w.panel.terminals.is_empty()));
    let root = wb.read_with(cx, |w, _| w.focus.clone().expect("workbench focus"));
    assert!(cx.update(|window, cx| root.contains_focused(window, cx) || root.is_focused(window)), "the workbench has the focus");
}

#[gpui_kit::test]
fn a_release_outside_the_grid_ends_the_selection(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-release-outside", None);
    shell(cx, "printf 'abc'; cat");
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "abc");
    let grid = bounds(cx, "terminal-grid");
    let start = point(grid.origin.x + px(3.), grid.origin.y + px(3.));
    cx.simulate_mouse_move(start, None, Default::default());
    cx.simulate_mouse_down(start, gpui::MouseButton::Left, Default::default());
    assert!(active_view(&wb, cx).read_with(cx, |v, _| v.is_selecting()));
    let outside = point(px(2.), px(2.));
    cx.simulate_mouse_up(outside, gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert!(!active_view(&wb, cx).read_with(cx, |v, _| v.is_selecting()));
}

#[gpui_kit::test]
fn a_terminal_that_failed_to_start_stops_polling_when_closed(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "term-poll-stop", None);
    set(cx, "terminal.integrated.profiles.osx", json!({ "t": { "path": "/nonexistent/shell" } }));
    set(cx, "terminal.integrated.defaultProfile.osx", json!("t"));
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    let probe = active_view(&wb, cx).read_with(cx, |v, _| v.inbox_probe.clone());
    wb.update(cx, |w, cx| w.kill_terminal(cx));
    for _ in 0..5 {
        cx.executor().advance_clock(Duration::from_millis(20));
        cx.run_until_parked();
    }
    assert!(probe.upgrade().is_none(), "the poll task ended");
}
