//! Headless end-to-end tests of editing through the real workbench (docs/testing.md tier 2).
//!
//! These drive gpui's `TestAppContext`/`VisualTestContext`: a real `Workbench` in a headless
//! window, real key dispatch (key contexts, bindings, the platform input handler), real layout.
//! They live here rather than in `tests/` because `vsr-app` is a binary crate and the
//! workbench is private to it.

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext, point};

use crate::Workbench;

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("vsr-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workbench over `dir` with `file` open and focused, and one frame painted.
fn open_workbench<'a>(
    cx: &'a mut TestAppContext,
    dir: &Path,
    file: &str,
) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    cx.update(super::init);
    let root = dir.to_path_buf();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    let path = dir.join(file);
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
    (wb, cx)
}

/// Move the pointer there first: gpui hit-tests mouse downs against the last mouse-move.
fn click(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, mods: Modifiers) {
    cx.simulate_mouse_move(at, None, mods);
    cx.simulate_click(at, mods);
}

fn value(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> String {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().value())
}

fn dirty(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().is_dirty())
}

fn caret(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (usize, usize) {
    wb.read_with(cx, |wb, _| {
        let p = wb.editors.active_editor().unwrap().editor.selection().active;
        (p.line, p.column)
    })
}

#[gpui::test]
fn typing_enter_undo_and_save(cx: &mut TestAppContext) {
    let dir = unique_dir("save");
    fs::write(dir.join("a.txt"), "world\n").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");
    assert!(!dirty(&wb, cx));

    cx.simulate_keystrokes("h e l l o enter");
    assert_eq!(value(&wb, cx), "hello\nworld\n");
    assert!(dirty(&wb, cx));
    assert_eq!(caret(&wb, cx), (2, 1));

    cx.simulate_keystrokes("cmd-z"); // undoes Enter only (typing is a separate undo step)
    assert_eq!(value(&wb, cx), "helloworld\n");

    cx.simulate_keystrokes("cmd-s");
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "helloworld\n");
    assert!(!dirty(&wb, cx));

    cx.simulate_keystrokes("cmd-z"); // back past the saved state
    assert_eq!(value(&wb, cx), "world\n");
    assert!(dirty(&wb, cx));
    cx.simulate_keystrokes("cmd-shift-z");
    assert_eq!(value(&wb, cx), "helloworld\n");
    assert!(!dirty(&wb, cx));
}

#[gpui::test]
fn navigation_selection_and_deletion(cx: &mut TestAppContext) {
    let dir = unique_dir("nav");
    fs::write(dir.join("a.txt"), "one two\nthree\n").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");

    cx.simulate_keystrokes("alt-right");
    assert_eq!(caret(&wb, cx), (1, 4)); // end of "one"
    cx.simulate_keystrokes("cmd-right");
    assert_eq!(caret(&wb, cx), (1, 8));
    cx.simulate_keystrokes("down");
    assert_eq!(caret(&wb, cx), (2, 6));
    cx.simulate_keystrokes("cmd-up");
    assert_eq!(caret(&wb, cx), (1, 1));
    cx.simulate_keystrokes("cmd-down");
    assert_eq!(caret(&wb, cx), (3, 1));

    cx.simulate_keystrokes("cmd-up shift-right shift-right delete");
    assert_eq!(value(&wb, cx), "e two\nthree\n");
    cx.simulate_keystrokes("right backspace");
    assert_eq!(value(&wb, cx), " two\nthree\n");
    cx.simulate_keystrokes("tab");
    assert_eq!(value(&wb, cx), "     two\nthree\n"); // 4 spaces (insertSpaces) to the tab stop
}

#[gpui::test]
fn clipboard_cut_copy_paste_and_select_all(cx: &mut TestAppContext) {
    let dir = unique_dir("clip");
    fs::write(dir.join("a.txt"), "abc\n").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");

    cx.simulate_keystrokes("shift-right shift-right cmd-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("ab"));
    cx.simulate_keystrokes("cmd-down cmd-v"); // paste at end of document
    assert_eq!(value(&wb, cx), "abc\nab");
    cx.simulate_keystrokes("cmd-a cmd-x");
    assert_eq!(value(&wb, cx), "");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("abc\nab"));
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(value(&wb, cx), "abc\nab");
}

#[gpui::test]
fn typing_replaces_selection_and_handles_unicode(cx: &mut TestAppContext) {
    let dir = unique_dir("uni");
    fs::write(dir.join("a.txt"), "xy").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");

    cx.simulate_input("é😀");
    assert_eq!(value(&wb, cx), "é😀xy");
    assert_eq!(caret(&wb, cx), (1, 4)); // UTF-16 columns: é=1, 😀=2
    cx.simulate_keystrokes("cmd-a z");
    assert_eq!(value(&wb, cx), "z");
}

#[gpui::test]
fn save_preserves_crlf_and_cmd_w_closes_the_tab(cx: &mut TestAppContext) {
    let dir = unique_dir("crlf");
    fs::write(dir.join("a.txt"), "a\r\nb\r\n").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");

    cx.simulate_keystrokes("x enter cmd-s");
    assert_eq!(fs::read_to_string(dir.join("a.txt")).unwrap(), "x\r\na\r\nb\r\n");

    cx.simulate_keystrokes("cmd-w");
    assert!(wb.read_with(cx, |wb, _| wb.editors.open.is_empty()));
}

#[gpui::test]
fn click_places_the_caret_and_shift_click_extends(cx: &mut TestAppContext) {
    let dir = unique_dir("mouse");
    fs::write(dir.join("a.txt"), "first line\nsecond\nthird\n").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");

    let (bounds, gutter) = wb.read_with(cx, |wb, _| (wb.editor_view.bounds.get(), wb.editor_view.gutter));
    assert!(f32::from(bounds.size.height) > 100.0, "editor laid out: {bounds:?}");
    let row_y = |line: usize| bounds.origin.y + gpui::px((line as f32 - 0.5) * 18.0);
    let text_x = bounds.origin.x + gpui::px(gutter);

    // Far left of line 2 is column 1; far right of line 3 is its end.
    click(cx, point(text_x + gpui::px(1.0), row_y(2)), Modifiers::none());
    assert_eq!(caret(&wb, cx), (2, 1));
    click(cx, point(text_x + gpui::px(900.0), row_y(3)), Modifiers::none());
    assert_eq!(caret(&wb, cx), (3, 6));
    // Shift-click extends from the existing anchor.
    click(cx, point(text_x + gpui::px(900.0), row_y(1)), Modifiers::shift());
    let sel = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection());
    assert_eq!((sel.anchor.line, sel.active.line), (3, 1));
    // Typing after a click lands at the clicked position.
    click(cx, point(text_x + gpui::px(1.0), row_y(2)), Modifiers::none());
    cx.simulate_input("Z");
    assert_eq!(value(&wb, cx), "first line\nZsecond\nthird\n");
}

#[gpui::test]
fn status_texts_follow_the_caret(cx: &mut TestAppContext) {
    let dir = unique_dir("status");
    fs::write(dir.join("a.txt"), "\tab\ncd\n").unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "a.txt");

    let status = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| {
        wb.read_with(cx, |wb, _| {
            let e = wb.editors.active_editor().unwrap();
            (super::cursor_status(e), super::indent_status(e))
        })
    };
    assert_eq!(status(&wb, cx), ("Ln 1, Col 1".to_string(), "Spaces: 4".to_string()));
    cx.simulate_keystrokes("right"); // after the tab: visual column 5
    assert_eq!(status(&wb, cx).0, "Ln 1, Col 5");
    cx.simulate_keystrokes("shift-down");
    assert_eq!(status(&wb, cx).0, "Ln 2, Col 3 (5 selected)");
}

#[gpui::test]
fn editing_a_10k_line_file_stays_responsive(cx: &mut TestAppContext) {
    let dir = unique_dir("big");
    let text: String = (1..=10_000).map(|i| format!("line {i} lorem ipsum dolor sit amet\n")).collect();
    fs::write(dir.join("big.txt"), &text).unwrap();
    let (wb, cx) = open_workbench(cx, &dir, "big.txt");

    cx.simulate_keystrokes("cmd-down");
    assert_eq!(caret(&wb, cx), (10_001, 1));
    // Best of three batches: the full suite runs tests in parallel and agents may be compiling,
    // so a single batch is noisy; the minimum still catches O(n) per-key regressions.
    let mut per_key = std::time::Duration::MAX;
    for _ in 0..3 {
        let start = std::time::Instant::now();
        for _ in 0..7 {
            cx.simulate_input("ab");
            cx.simulate_keystrokes("enter");
        }
        per_key = per_key.min(start.elapsed() / 21);
    }
    eprintln!("10k-line file: {per_key:?} per key, best of 3 (debug build, incl. layout and paint)");
    assert!(value(&wb, cx).ends_with("ab\nab\n"));
    // Generous bound for an unoptimised build on a loaded machine; catches O(n^2) regressions.
    assert!(per_key < std::time::Duration::from_millis(100), "{per_key:?} per key");
}
