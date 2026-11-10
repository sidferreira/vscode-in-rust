//! Headless end-to-end tests of code folding (docs/testing.md tier 2; tests/e2e-scenarios/preferences.md,
//! "Folding"). The real `Workbench` runs in gpui's test platform with an injected temp
//! settings.json. Our own tests (upstream's folding tests are unit tests, ported in `vsr-editor`).
//!
//! upstream: src/vs/editor/contrib/folding/browser/folding.ts @ 50e1350fc78d

use std::fs;
use std::path::{Path, PathBuf};

use gpui::{Entity, TestAppContext, VisualTestContext, point};
use serde_json::json;

use crate::Workbench;
use crate::config::AppConfig;

fn unique_dir(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-fold-e2e-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

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

/// (model lines, view lines)
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

fn hidden(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<(usize, usize)> {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.hidden_ranges())
}

fn click(cx: &mut VisualTestContext, at: gpui::Point<gpui::Pixels>, mods: gpui::Modifiers) {
    cx.simulate_mouse_move(at, None, mods);
    cx.simulate_click(at, mods);
}

const CODE: &str = "fn a() {\n    let x = 1;\n    let y = 2;\n}\nfn b() {\n    if x {\n        z();\n    }\n}\nend\n";

#[gpui::test]
fn fold_at_a_header_hides_lines_and_unfold_restores_them(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "fold", "{}", &[("a.rs", CODE)]);
    assert_eq!(counts(&wb, cx), (11, 11));
    assert!(cx.debug_bounds("fold-placeholder:1").is_none());
    cx.simulate_keystrokes("alt-cmd-[");
    assert_eq!(counts(&wb, cx), (11, 9), "lines 2-3 are hidden");
    assert_eq!(hidden(&wb, cx), [(2, 3)]);
    // The folded line has its `⋯`, the highlight, and the collapsed control; line numbers skip the hidden lines.
    assert!(cx.debug_bounds("fold-placeholder:1").is_some());
    assert!(cx.debug_bounds("fold-bg:1").is_some());
    assert!(cx.debug_bounds("fold:1").is_some(), "a collapsed region always shows its control");
    assert!(cx.debug_bounds("gutter:2").is_none() && cx.debug_bounds("gutter:3").is_none());
    assert!(cx.debug_bounds("gutter:4").is_some());
    cx.simulate_keystrokes("alt-cmd-]");
    assert_eq!(counts(&wb, cx), (11, 11));
    assert!(cx.debug_bounds("fold-placeholder:1").is_none());
    assert!(cx.debug_bounds("gutter:2").is_some());
}

#[gpui::test]
fn fold_all_and_unfold_all_use_their_chords(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "all", "{}", &[("a.rs", CODE)]);
    cx.simulate_keystrokes("cmd-k cmd-0");
    assert_eq!(hidden(&wb, cx), [(2, 3), (6, 8)]);
    assert_eq!(counts(&wb, cx), (11, 6));
    cx.simulate_keystrokes("cmd-k cmd-j");
    assert_eq!(counts(&wb, cx), (11, 11));
    // Fold level 1 leaves the region the caret is in alone.
    cx.simulate_keystrokes("cmd-down cmd-k cmd-1");
    assert_eq!(hidden(&wb, cx), [(2, 3), (6, 8)]);
}

#[gpui::test]
fn the_caret_skips_a_fold_and_editing_inside_one_unfolds_it(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "caret", "{}", &[("a.rs", CODE)]);
    cx.simulate_keystrokes("alt-cmd-[");
    cx.simulate_keystrokes("end");
    assert_eq!(caret(&wb, cx), (1, 9));
    cx.simulate_keystrokes("down");
    assert_eq!(caret(&wb, cx), (4, 2), "down skips the hidden lines (the closing brace is one column wide)");
    cx.simulate_keystrokes("up end right");
    assert_eq!(caret(&wb, cx), (4, 1), "right at the end of the folded line goes past the fold");
    // Enter at the end of the folded line adds a line inside the fold, which unfolds it.
    cx.simulate_keystrokes("up end enter");
    assert_eq!(hidden(&wb, cx), []);
    assert_eq!(counts(&wb, cx), (12, 12));
}

#[gpui::test]
fn region_markers_fold_in_rust_and_typescript_files(cx: &mut TestAppContext) {
    let rs = "// #region helpers\nfn a() {}\nfn b() {}\n// #endregion\nfn c() {}\n";
    let ts = "//#region helpers\nconst a = 1;\nconst b = 2;\n//#endregion\nconst c = 3;\n";
    let (wb, cx, dir) = workbench(cx, "markers", "{}", &[("a.rs", rs), ("b.ts", ts)]);
    cx.simulate_keystrokes("alt-cmd-[");
    assert_eq!(hidden(&wb, cx), [(2, 4)], "rust: the region folds although nothing is indented");
    open(&wb, cx, &dir.join("b.ts"));
    cx.simulate_keystrokes("alt-cmd-[");
    assert_eq!(hidden(&wb, cx), [(2, 4)], "typescript");
    cx.simulate_keystrokes("cmd-k cmd-9");
    assert_eq!(hidden(&wb, cx), [], "Unfold All Regions");
    cx.simulate_keystrokes("cmd-k cmd-8");
    assert_eq!(hidden(&wb, cx), [(2, 4)], "Fold All Regions");
}

#[gpui::test]
fn folding_off_and_the_maximum_follow_the_settings(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "settings", "{}", &[("a.rs", CODE)]);
    cx.simulate_keystrokes("alt-cmd-[");
    assert_eq!(counts(&wb, cx).1, 9);
    set(cx, "editor.folding", json!(false));
    assert_eq!(counts(&wb, cx).1, 11, "turning folding off shows every line");
    assert!(cx.debug_bounds("fold:5").is_none());
    cx.simulate_keystrokes("alt-cmd-[");
    assert_eq!(counts(&wb, cx).1, 11, "and the commands do nothing");
    set(cx, "editor.folding", json!(true));
    let regions = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.fold_region_count());
    assert_eq!(regions(&wb, cx), 3);
    set(cx, "editor.foldingMaximumRegions", json!(10));
    assert_eq!(regions(&wb, cx), 3, "below the maximum");
    // `auto` and `indentation` both compute indentation ranges.
    set(cx, "editor.foldingStrategy", json!("indentation"));
    assert_eq!(regions(&wb, cx), 3);
}

#[gpui::test]
fn show_folding_controls_always_never_and_mouseover(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "controls", "{}", &[("a.rs", CODE)]);
    let _ = &wb;
    // mouseover (the default): expanded regions show their control while the mouse is over the gutter.
    assert!(cx.debug_bounds("fold:1").is_none());
    let gutter = cx.debug_bounds("gutter:1").expect("gutter");
    cx.simulate_mouse_move(gutter.center(), None, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("fold:1").is_some(), "mouse over the gutter");
    cx.simulate_mouse_move(point(gutter.center().x + gpui::px(400.), gutter.center().y), None, gpui::Modifiers::default());
    cx.run_until_parked();
    assert!(cx.debug_bounds("fold:1").is_none(), "mouse away from the gutter");
    set(cx, "editor.showFoldingControls", json!("always"));
    assert!(cx.debug_bounds("fold:1").is_some() && cx.debug_bounds("fold:5").is_some() && cx.debug_bounds("fold:2").is_none());
    set(cx, "editor.showFoldingControls", json!("never"));
    assert!(cx.debug_bounds("fold:1").is_none());
    cx.simulate_keystrokes("alt-cmd-[");
    assert!(cx.debug_bounds("fold:1").is_none(), "never: not even for a collapsed region");
    assert!(cx.debug_bounds("fold-placeholder:1").is_some(), "the `⋯` stays");
    set(cx, "editor.foldingHighlight", json!(false));
    assert!(cx.debug_bounds("fold-bg:1").is_none());
}

#[gpui::test]
fn clicking_the_control_toggles_the_fold(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "click", r#"{"editor.showFoldingControls":"always"}"#, &[("a.rs", CODE)]);
    let control = cx.debug_bounds("fold:5").expect("control of line 5");
    click(cx, control.center(), gpui::Modifiers::default());
    assert_eq!(hidden(&wb, cx), [(6, 8)]);
    let control = cx.debug_bounds("fold:5").expect("still there, collapsed");
    click(cx, control.center(), gpui::Modifiers::default());
    assert_eq!(hidden(&wb, cx), []);
    // A shift click folds the children first (line 6 inside line 5), then the next click the region.
    let control = cx.debug_bounds("fold:5").unwrap();
    click(cx, control.center(), gpui::Modifiers { shift: true, ..Default::default() });
    assert_eq!(hidden(&wb, cx), [(7, 7)], "recursive: the child folds first");
}

#[gpui::test]
fn clicking_the_placeholder_unfolds(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "placeholder", "{}", &[("a.rs", CODE)]);
    cx.simulate_keystrokes("alt-cmd-[");
    let ph = cx.debug_bounds("fold-placeholder:1").expect("placeholder");
    click(cx, ph.center(), gpui::Modifiers::default());
    assert_eq!(hidden(&wb, cx), [], "a click on the `⋯` unfolds");
    // (A click after the end of the line is `folding::unfolds_on_click`, unit tested: the test
    // platform's text system cannot map far-right x to the end column.)
}

#[gpui::test]
fn folds_follow_edits_above_them(cx: &mut TestAppContext) {
    let (wb, cx, _) = workbench(cx, "follow", "{}", &[("a.rs", CODE)]);
    cx.simulate_keystrokes("down down down down alt-cmd-[");
    assert_eq!(hidden(&wb, cx), [(6, 8)]);
    cx.simulate_keystrokes("cmd-up enter");
    assert_eq!(hidden(&wb, cx), [(7, 9)], "still folded after a line was added above");
}

#[gpui::test]
fn folding_a_10k_line_file_stays_responsive(cx: &mut TestAppContext) {
    let text: String = (1..=2_500).map(|i| format!("fn f{i}() {{\n    let a = {i};\n    let b = a;\n}}\n")).collect();
    let (wb, cx, _) = workbench(cx, "perf", "{}", &[("big.rs", &text)]);
    assert_eq!(counts(&wb, cx).0, 10_001);
    cx.simulate_keystrokes("alt-cmd-[");
    assert_eq!(hidden(&wb, cx).len(), 1);
    // Best of three batches of typing at the end with one region collapsed (the view-line projection
    // and the folding regions are rebuilt on each edit).
    cx.simulate_keystrokes("cmd-down");
    let mut per_key = std::time::Duration::MAX;
    for _ in 0..3 {
        let start = std::time::Instant::now();
        for _ in 0..7 {
            cx.simulate_input("ab");
            cx.simulate_keystrokes("enter");
        }
        per_key = per_key.min(start.elapsed() / 21);
    }
    eprintln!("10k-line file with a fold: {per_key:?} per key, best of 3 (debug build, incl. layout and paint)");
    assert_eq!(hidden(&wb, cx).len(), 1, "the fold survived the edits");
    assert!(per_key < std::time::Duration::from_millis(100), "{per_key:?} per key");
    // Fold all and unfold all on 2,500 regions.
    let start = std::time::Instant::now();
    cx.simulate_keystrokes("cmd-k cmd-0");
    let fold_all = start.elapsed();
    assert_eq!(hidden(&wb, cx).len(), 2_500);
    cx.simulate_keystrokes("cmd-k cmd-j");
    eprintln!("fold all of 2,500 regions: {fold_all:?}");
    assert!(fold_all < std::time::Duration::from_secs(2), "{fold_all:?}");
}
