//! Headless tests of the `terminal.integrated.*` settings the terminal reads (every wired setting has
//! one here), over real shells on real PTYs.
//!
//! Upstream source: src/vs/workbench/contrib/terminal/browser/terminalConfigurationService.ts,
//! src/vs/platform/terminal/common/terminalPlatformConfiguration.ts

use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext, point, px};
use serde_json::json;
use vsr_terminal::Snapshot;

use crate::Workbench;
use crate::panel_e2e::{bounds, workbench};
use crate::terminal_e2e::{set, shell, snapshot, wait_for_text};
use crate::terminal_view::{Palette, TerminalSettings, row_runs};
use crate::theme::Theme;
use crate::ui::to_gpui;

fn cell_size(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (f32, f32) {
    wb.read_with(cx, |w, cx| w.panel.terminals[0].view.read(cx).cell_size())
}

fn open(cx: &mut VisualTestContext, script: &str) {
    shell(cx, script);
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
}

fn settings_from(json: &str) -> TerminalSettings {
    let c = vsr_settings::Configuration::new(&vsr_settings::SettingsRegistry::load_default(), vsr_settings::UserSettings::parse(json).unwrap());
    TerminalSettings::from_configuration(&c)
}

/// A real shell's screen after it ran `script` (no workbench, no window).
fn screen(script: &str) -> Snapshot {
    let config = vsr_terminal::TerminalConfig { shell: Some("/bin/sh".into()), args: Some(vec!["-c".into(), script.into()]), ..Default::default() };
    let t = vsr_terminal::Terminal::spawn(&config, |_| {}).unwrap();
    for _ in 0..200 {
        std::thread::sleep(Duration::from_millis(20));
        if t.exit_status().is_some() {
            std::thread::sleep(Duration::from_millis(150));
            break;
        }
    }
    t.snapshot()
}

#[gpui_kit::test]
fn font_size_and_line_height_set_the_cell_height(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-size", None);
    set(cx, "terminal.integrated.fontSize", json!(20));
    set(cx, "terminal.integrated.lineHeight", json!(1.5));
    open(cx, "printf abc; cat");
    wait_for_text(&wb, cx, "abc");
    let (_, lh) = cell_size(&wb, cx);
    assert_eq!(lh, 30.0);
    assert_eq!(bounds(cx, "terminal-cursor").size.height, px(30.));
    // Live: a smaller font gives more rows.
    let rows = snapshot(&wb, cx).unwrap().rows;
    set(cx, "terminal.integrated.fontSize", json!(10));
    set(cx, "terminal.integrated.lineHeight", json!(1));
    cx.run_until_parked();
    cx.run_until_parked();
    assert!(snapshot(&wb, cx).unwrap().rows > rows);
}

#[gpui_kit::test]
fn font_family_defaults_to_the_editor_font_and_can_be_set(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-font", None);
    open(cx, "printf abc; cat");
    let editor = wb.read_with(cx, |w, _| w.editor_settings.font_family.clone());
    let used = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| wb.read_with(cx, |w, cx| w.panel.terminals[0].view.read(cx).used_family.borrow().clone());
    assert_eq!(used(&wb, cx), editor);
    set(cx, "terminal.integrated.fontFamily", json!("Courier New"));
    assert_eq!(used(&wb, cx), "Courier New");
}

#[test]
fn font_weights_style_normal_and_bold_cells() {
    let snap = screen("printf 'a\\033[1mb'");
    let palette = Palette::new(&Theme::dark_modern(), None);
    let s = settings_from(r#"{ "terminal.integrated.fontWeight": "300", "terminal.integrated.fontWeightBold": "800" }"#);
    let (_, runs) = row_runs(&snap, 0, &palette, &s, true);
    let weights: Vec<f32> = runs.iter().filter_map(|(_, st)| st.font_weight.map(|w| w.0)).collect();
    assert_eq!(weights[..2], [300.0, 800.0]);
}

#[test]
fn bold_text_uses_bright_colours_only_when_draw_bold_text_in_bright_colors_is_on() {
    let snap = screen("printf '\\033[1;31mX'");
    let palette = Palette::new(&Theme::dark_modern(), None);
    let colour = |json: &str| row_runs(&snap, 0, &palette, &settings_from(json), true).1[0].1.color;
    assert_eq!(colour("{}"), Some(to_gpui(palette.ansi[9])));
    assert_eq!(colour(r#"{ "terminal.integrated.drawBoldTextInBrightColors": false }"#), Some(to_gpui(palette.ansi[1])));
}

#[gpui_kit::test]
fn cursor_style_and_width(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-cursor", None);
    open(cx, "printf abc; cat");
    wait_for_text(&wb, cx, "abc");
    let (cw, lh) = cell_size(&wb, cx);
    let cursor = bounds(cx, "terminal-cursor");
    assert!((f32::from(cursor.size.width) - cw).abs() <= 1.0, "block: one cell wide: {cursor:?} cw {cw}");
    set(cx, "terminal.integrated.cursorStyle", json!("line"));
    set(cx, "terminal.integrated.cursorWidth", json!(3));
    let cursor = bounds(cx, "terminal-cursor");
    assert_eq!((cursor.size.width, cursor.size.height), (px(3.), px(lh)));
    set(cx, "terminal.integrated.cursorStyle", json!("underline"));
    let cursor = bounds(cx, "terminal-cursor");
    assert!((f32::from(cursor.size.width) - cw).abs() <= 1.0);
    assert_eq!(cursor.size.height, px(3.));
}

#[gpui_kit::test]
fn scrollback_limits_the_history(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-scrollback", None);
    set(cx, "terminal.integrated.scrollback", json!(40));
    open(cx, "i=1; while [ $i -le 200 ]; do echo line$i; i=$((i+1)); done; cat");
    wait_for_text(&wb, cx, "line200");
    assert_eq!(snapshot(&wb, cx).unwrap().history_size, 40);
}

#[gpui_kit::test]
fn copy_on_selection_copies_when_the_mouse_is_released(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-copysel", None);
    set(cx, "terminal.integrated.copyOnSelection", json!(true));
    open(cx, "printf 'select me'; cat");
    wait_for_text(&wb, cx, "select me");
    let grid = bounds(cx, "terminal-grid");
    let (cw, lh) = cell_size(&wb, cx);
    let at = |col: f32| point(grid.origin.x + px(col * cw + cw / 2.), grid.origin.y + px(lh / 2.));
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("old".to_string()));
    cx.simulate_mouse_move(at(0.), None, Default::default());
    cx.simulate_mouse_down(at(0.), gpui::MouseButton::Left, Default::default());
    cx.simulate_mouse_move(at(5.), Some(gpui::MouseButton::Left), Default::default());
    cx.simulate_mouse_up(at(5.), gpui::MouseButton::Left, Default::default());
    cx.run_until_parked();
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("select"));
}

#[gpui_kit::test]
fn mac_option_is_meta_sends_escape_before_the_key(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-meta", None);
    set(cx, "terminal.integrated.macOptionIsMeta", json!(true));
    open(cx, "stty raw -echo; printf ready; cat -v");
    wait_for_text(&wb, cx, "ready");
    cx.simulate_keystrokes("alt-f");
    wait_for_text(&wb, cx, "^[f");
}

#[gpui_kit::test]
fn without_mac_option_is_meta_option_does_not_send_escape(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-nometa", None);
    open(cx, "stty raw -echo; printf ready; cat -v");
    wait_for_text(&wb, cx, "ready");
    cx.simulate_keystrokes("alt-f");
    cx.run_until_parked();
    std::thread::sleep(Duration::from_millis(100));
    let text = wait_for_text(&wb, cx, "ready").text();
    assert!(!text.contains("^["), "no escape prefix: {text:?}");
}

fn scroll(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, lines: f32, alt: bool) {
    cx.simulate_event(gpui::ScrollEvent {
        position: at,
        delta: gpui::ScrollDelta::Lines(point(0., lines)),
        modifiers: gpui::Modifiers { alt, ..Default::default() },
        touch_phase: gpui::TouchPhase::Moved,
        source: Default::default(),
    });
    cx.run_until_parked();
}

#[gpui_kit::test]
fn wheel_sensitivity_and_fast_scroll_multiply_the_lines(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-wheel", None);
    open(cx, "i=1; while [ $i -le 300 ]; do echo line$i; i=$((i+1)); done; cat");
    wait_for_text(&wb, cx, "line300");
    let at = bounds(cx, "terminal-grid").center();
    scroll(cx, at, 2.0, false);
    assert_eq!(snapshot(&wb, cx).unwrap().display_offset, 2);
    set(cx, "terminal.integrated.mouseWheelScrollSensitivity", json!(3));
    scroll(cx, at, 1.0, false);
    assert_eq!(snapshot(&wb, cx).unwrap().display_offset, 5);
    set(cx, "terminal.integrated.fastScrollSensitivity", json!(10));
    scroll(cx, at, 1.0, true);
    assert_eq!(snapshot(&wb, cx).unwrap().display_offset, 35, "3 x 10 lines with Alt");
}

#[gpui_kit::test]
fn word_separators_decide_where_a_double_click_selection_ends(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-words", None);
    set(cx, "terminal.integrated.wordSeparators", json!(" -"));
    open(cx, "printf 'foo-bar baz'; cat");
    wait_for_text(&wb, cx, "foo-bar");
    let grid = bounds(cx, "terminal-grid");
    let (cw, lh) = cell_size(&wb, cx);
    let p = point(grid.origin.x + px(1. * cw + cw / 2.), grid.origin.y + px(lh / 2.));
    cx.simulate_mouse_move(p, None, Default::default());
    cx.simulate_event(gpui::MouseDownEvent { position: p, modifiers: Default::default(), button: gpui::MouseButton::Left, click_count: 2, first_mouse: false });
    cx.run_until_parked();
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("foo"));
}

#[gpui_kit::test]
fn ignore_bracketed_paste_mode_omits_the_markers(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-paste", None);
    open(cx, "stty raw -echo; printf '\\033[?2004hready'; cat -v");
    wait_for_text(&wb, cx, "ready");
    cx.write_to_clipboard(gpui::ClipboardItem::new_string("hi".to_string()));
    cx.simulate_keystrokes("cmd-v");
    wait_for_text(&wb, cx, "^[[200~hi^[[201~");
    set(cx, "terminal.integrated.ignoreBracketedPasteMode", json!(true));
    cx.simulate_keystrokes("cmd-v");
    wait_for_text(&wb, cx, "^[[201~hi");
}

#[gpui_kit::test]
fn cwd_overrides_the_workspace_folder(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-cwd", None);
    let dir = vsr_test_scratch::dir!().join(format!("rscode-term-cwd-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    set(cx, "terminal.integrated.cwd", json!(dir.to_str().unwrap()));
    open(cx, "pwd; cat");
    wait_for_text(&wb, cx, dir.canonicalize().unwrap().to_str().unwrap());
}

#[gpui_kit::test]
fn env_osx_and_profile_env_reach_the_shell(cx: &mut TestAppContext) {
    let (wb, cx) = workbench(cx, "tset-env", None);
    set(cx, "terminal.integrated.env.osx", json!({ "RS_TERM_A": "from-env" }));
    set(
        cx,
        "terminal.integrated.profiles.osx",
        json!({ "t": { "path": "/bin/sh", "args": ["-c", "echo $RS_TERM_A $RS_TERM_B; cat"], "env": { "RS_TERM_B": "from-profile" } } }),
    );
    set(cx, "terminal.integrated.defaultProfile.osx", json!("t"));
    cx.simulate_keystrokes("ctrl-`");
    wait_for_text(&wb, cx, "from-env from-profile");
}
