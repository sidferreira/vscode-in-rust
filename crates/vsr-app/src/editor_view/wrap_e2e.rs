//! Headless end-to-end tests of word wrap (docs/testing.md tier 2; tests/e2e-scenarios/preferences.md,
//! "Word wrap"). The real `Workbench` runs in gpui's test platform with an injected temp
//! settings.json. The test platform's text system measures nothing, so the viewport width is
//! injected (`EditorView::viewport_columns_override`) for the viewport modes; `wordWrapColumn` needs no
//! measuring. Our own tests (upstream's wrapping tests are unit tests, ported in `vsr-editor`).

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Entity, TestAppContext, VisualTestContext, point};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-wrap-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A workbench over a temp folder with `files`, settings from `settings` (JSON text), the first file open.
fn workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    settings: &str,
    files: &[(&str, &str)],
) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let dir = unique_dir(name);
    let settings_path = dir.join("settings.json");
    fs::write(&settings_path, settings).unwrap();
    for (file, text) in files {
        fs::write(dir.join(file), text).unwrap();
    }
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        AppConfig::install(cx, Some(settings_path));
    });
    let root = dir.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    open(&wb, vcx, &dir.join(files[0].0));
    (wb, vcx, dir)
}

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// Injects the viewport width in columns and repaints.
fn viewport(wb: &Entity<Workbench>, cx: &mut VisualTestContext, columns: usize) {
    wb.update(cx, |wb, cx| {
        wb.editor_view.viewport_columns_override = Some(columns);
        cx.notify();
    });
    cx.run_until_parked();
}

fn counts(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (usize, usize) {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        (e.buffer().line_count(), e.editor.view_line_count())
    })
}

fn caret(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (usize, usize) {
    wb.read_with(cx, |wb, _| {
        let p = wb.editors.active_editor().unwrap().editor.selection().active;
        (p.line, p.column)
    })
}

fn row_texts(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        let v = e.editor.view_lines().expect("wrapping");
        (0..v.view_line_count()).map(|i| v.view_line_content(e.buffer(), i)).collect()
    })
}

const LONG: &str = "aaaa bbbb cccc dddd\nshort\n";

#[gpui::test]
fn word_wrap_column_wraps_a_long_line_into_view_lines(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "column", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", LONG)]);
    // "aaaa bbbb |cccc dddd" / "short" / "" : three model lines, four view lines.
    assert_eq!(counts(&wb, cx), (3, 4));
    assert_eq!(row_texts(&wb, cx), ["aaaa bbbb ", "cccc dddd", "short", ""]);
    // One row per view line, and the line number only on the first row of a line.
    for row in ["row:0", "row:1", "row:2", "row:3"] {
        assert!(cx.debug_bounds(row).is_some(), "{row}");
    }
    assert!(cx.debug_bounds("row:4").is_none());
    assert!(cx.debug_bounds("gutter:1").is_some() && cx.debug_bounds("gutter:2").is_some() && cx.debug_bounds("gutter:3").is_some());
    // Off again: one row per line.
    set(cx, "editor.wordWrap", json!("off"));
    assert_eq!(counts(&wb, cx), (3, 3));
    set(cx, "editor.wordWrap", json!("wordWrapColumn"));
    set(cx, "editor.wordWrapColumn", json!(5));
    assert_eq!(counts(&wb, cx).1, 6, "a narrower column wraps more");
}

#[gpui::test]
fn viewport_modes_wrap_at_the_viewport_or_the_smaller_of_both(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "viewport", r#"{"editor.wordWrap":"on","editor.wordWrapColumn":15}"#, &[("a.txt", LONG)]);
    viewport(&wb, cx, 10);
    assert_eq!(counts(&wb, cx), (3, 4), "on: the viewport width, 10 columns");
    viewport(&wb, cx, 80);
    assert_eq!(counts(&wb, cx), (3, 3), "a wide viewport: nothing wraps");
    set(cx, "editor.wordWrap", json!("bounded"));
    assert_eq!(counts(&wb, cx), (3, 4), "bounded: min(viewport 80, wordWrapColumn 15) wraps the 19 column line");
    set(cx, "editor.wordWrapColumn", json!(40));
    assert_eq!(counts(&wb, cx), (3, 3), "bounded to 40: the line fits");
    viewport(&wb, cx, 10);
    assert_eq!(counts(&wb, cx), (3, 4), "bounded: the viewport is the smaller one");
}

#[gpui::test]
fn down_moves_within_a_wrapped_line_and_home_end_follow_view_lines(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "moves", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", LONG)]);
    assert_eq!(caret(&wb, cx), (1, 1));
    cx.simulate_keystrokes("right right down");
    assert_eq!(caret(&wb, cx), (1, 13), "down: the next view line, same visible column");
    cx.simulate_keystrokes("down");
    assert_eq!(caret(&wb, cx), (2, 3), "then the next model line");
    cx.simulate_keystrokes("up up");
    assert_eq!(caret(&wb, cx), (1, 3));
    cx.simulate_keystrokes("end");
    assert_eq!(caret(&wb, cx), (1, 11), "End: the end of the view line");
    cx.simulate_keystrokes("end");
    assert_eq!(caret(&wb, cx), (1, 20), "End again: the end of the line");
    cx.simulate_keystrokes("home");
    assert_eq!(caret(&wb, cx), (1, 11), "Home: the start of the view line");
    cx.simulate_keystrokes("home");
    assert_eq!(caret(&wb, cx), (1, 1), "Home again: the start of the line");
}

#[gpui::test]
fn alt_z_toggles_word_wrap_for_the_editor(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "altz", "{}", &[("a.txt", LONG), ("b.txt", LONG)]);
    viewport(&wb, cx, 10);
    assert_eq!(counts(&wb, cx), (3, 3), "off by default");
    cx.simulate_keystrokes("alt-z");
    assert_eq!(counts(&wb, cx), (3, 4), "Alt+Z wraps at the viewport");
    cx.simulate_keystrokes("alt-z");
    assert_eq!(counts(&wb, cx), (3, 3), "and again unwraps");
    // The command is in the registry under its VS Code id.
    cx.update(|_, cx| {
        let commands = cx.global::<crate::commands::Commands>();
        assert!(commands.registry.contains("editor.action.toggleWordWrap"));
        assert_eq!(commands.registry.descriptor("editor.action.toggleWordWrap").unwrap().label(), "View: Toggle Word Wrap");
    });
}

#[gpui::test]
fn alt_z_turns_off_a_wrapping_setting_for_that_editor_only(cx: &mut TestAppContext) {
    let (wb, cx, dir) = workbench(cx, "altz2", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", LONG), ("b.txt", LONG)]);
    assert_eq!(counts(&wb, cx).1, 4);
    cx.simulate_keystrokes("alt-z");
    assert_eq!(counts(&wb, cx).1, 3, "wrapped becomes off");
    open(&wb, cx, &dir.join("b.txt"));
    assert_eq!(counts(&wb, cx).1, 4, "another editor still follows the setting");
}

#[gpui::test]
fn wrapping_indent_indents_the_continuation_lines(cx: &mut TestAppContext) {
    let text = "    aaaa bbbb cccc dddd eeee ffff gggg\n";
    let (wb, cx, _) = workbench(cx, "indent", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":19}"#, &[("a.txt", text)]);
    let lead = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| {
        let rows = row_texts(wb, cx);
        assert!(rows[1].trim_start().starts_with("dddd"), "{rows:?}");
        rows[1].len() - rows[1].trim_start().len()
    };
    // default "same": the continuation line keeps the 4 spaces of the line
    assert_eq!(lead(&wb, cx), 4);
    set(cx, "editor.wrappingIndent", json!("indent"));
    assert_eq!(lead(&wb, cx), 8, "indent: one more level");
    set(cx, "editor.wrappingIndent", json!("deepIndent"));
    assert_eq!(lead(&wb, cx), 12, "deepIndent: two more levels");
    set(cx, "editor.wrappingIndent", json!("none"));
    assert_eq!(lead(&wb, cx), 0, "none: wrapped lines start at column 1");
}

#[gpui::test]
fn word_break_keep_all_does_not_break_between_cjk_and_latin(cx: &mut TestAppContext) {
    let text = "你好1111\n";
    let (wb, cx, _) = workbench(cx, "wordbreak", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":5}"#, &[("a.txt", text)]);
    assert_eq!(row_texts(&wb, cx)[0], "你好", "normal: a break opportunity after the ideographs");
    set(cx, "editor.wordBreak", json!("keepAll"));
    assert_eq!(row_texts(&wb, cx)[0], "你好1", "keepAll: no such opportunity, the line breaks where it overflows");
}

#[gpui::test]
fn wrapping_strategy_advanced_breaks_like_simple(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "strategy", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", LONG)]);
    let simple = row_texts(&wb, cx);
    set(cx, "editor.wrappingStrategy", json!("advanced"));
    assert_eq!(row_texts(&wb, cx), simple);
}

#[gpui::test]
fn word_wrap_indicator_marks_rows_that_continue(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "indicator", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", LONG)]);
    let _ = wb;
    assert!(cx.debug_bounds("wrap-indicator:0").is_none(), "off by default");
    set(cx, "editor.wordWrapIndicator", json!(true));
    assert!(cx.debug_bounds("wrap-indicator:0").is_some(), "row 0 continues");
    assert!(cx.debug_bounds("wrap-indicator:1").is_none(), "the last row of a line does not");
}

#[gpui::test]
fn word_separators_apply_to_word_moves(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "separators", "{}", &[("a.txt", "foo.bar baz\n")]);
    cx.simulate_keystrokes("alt-right");
    assert_eq!(caret(&wb, cx), (1, 4), "'.' separates words by default");
    set(cx, "editor.wordSeparators", json!(" "));
    cx.simulate_keystrokes("cmd-left alt-right");
    assert_eq!(caret(&wb, cx), (1, 8), "with only a space as separator 'foo.bar' is one word");
}

#[gpui::test]
fn typing_rewraps_and_clicking_a_continuation_row_places_the_caret_in_it(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "click", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", "aaaa bbbb cccc\n")]);
    assert_eq!(counts(&wb, cx), (2, 3));
    cx.simulate_input("xxxxx ");
    assert!(counts(&wb, cx).1 >= 3);
    let before = counts(&wb, cx).1;
    cx.simulate_input("yyyyy zzzzz ");
    assert!(counts(&wb, cx).1 > before, "typing wraps the line further");
    // A click on the second row lands in the second view line's text.
    let gutter = wb.read_with(cx, |wb, _| wb.editor_view.gutter);
    let row = cx.debug_bounds("row:1").expect("row 1");
    let at = point(row.origin.x + gpui::px(gutter + 1.0), row.origin.y + gpui::px(4.0));
    cx.simulate_mouse_move(at, None, gpui::Modifiers::default());
    cx.simulate_click(at, gpui::Modifiers::default());
    let start = wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        let v = e.editor.view_lines().unwrap();
        v.view_line(1).start_column
    });
    assert_eq!(caret(&wb, cx), (1, start), "the caret is in view line 1, not at the start of the line");
}

#[gpui::test]
fn the_minimap_counts_view_lines(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "minimap", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":10}"#, &[("a.txt", LONG)]);
    let minimap_lines = wb.read_with(cx, |wb, _| wb.editor_view.minimap.frame().map(|f| f.line_count));
    assert_eq!(minimap_lines, Some(4), "four view lines, not three model lines");
    set(cx, "editor.wordWrap", json!("off"));
    let minimap_lines = wb.read_with(cx, |wb, _| wb.editor_view.minimap.frame().map(|f| f.line_count));
    assert_eq!(minimap_lines, Some(3));
}

#[gpui::test]
fn editing_a_10k_line_file_with_word_wrap_stays_responsive(cx: &mut TestAppContext) {
    let text: String = (1..=10_000).map(|i| format!("line {i} lorem ipsum dolor sit amet consectetur\n")).collect();
    let (wb, cx, _) = workbench(cx, "big", r#"{"editor.wordWrap":"wordWrapColumn","editor.wordWrapColumn":20}"#, &[("big.txt", &text)]);
    let (models, views) = counts(&wb, cx);
    assert_eq!(models, 10_001);
    assert!(views >= 2 * models - 1, "every line wraps at least once: {views}");
    cx.simulate_keystrokes("cmd-down");
    let start = std::time::Instant::now();
    for _ in 0..20 {
        cx.simulate_input("ab");
        cx.simulate_keystrokes("enter");
    }
    let per_key = start.elapsed() / 60;
    eprintln!("10k-line file, word wrap on: {per_key:?} per key (debug build, incl. layout and paint)");
    assert!(per_key < std::time::Duration::from_millis(150), "{per_key:?} per key");
    // A resize (a new wrapping column) recomputes every line.
    let start = std::time::Instant::now();
    set(cx, "editor.wordWrapColumn", json!(30));
    eprintln!("10k-line file, word wrap column change: {:?} (debug build, incl. layout and paint)", start.elapsed());
}
