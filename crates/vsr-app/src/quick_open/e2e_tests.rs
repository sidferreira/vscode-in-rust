//! Headless E2E tests for Quick Open (docs/testing.md tier 2; scenarios in
//! tests/e2e-scenarios/quick-open.md): Cmd+P over a temp workspace with the in-memory
//! configuration, typing, navigation, accepting, Escape, `file:line`, excludes.

use std::fs;
use std::path::PathBuf;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_quickinput::pick::PickRow;

use crate::Workbench;
use crate::config::AppConfig;
use crate::quick_input::QuickPick;

fn workspace(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-e2e-quickopen-{name}-{}-{nanos}", std::process::id()));
    for d in ["src", "notes", "target", "node_modules/pkg", "docs"] {
        fs::create_dir_all(dir.join(d)).unwrap();
    }
    fs::write(dir.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(dir.join("src/domain.rs"), "struct D;\n").unwrap();
    fs::write(dir.join("notes/file.rs"), "one\ntwo\nthree\nfour\n").unwrap();
    fs::write(dir.join("docs/readme.md"), "# docs\n").unwrap();
    fs::write(dir.join("target/skip.rs"), "x\n").unwrap();
    fs::write(dir.join("node_modules/pkg/main.js"), "x\n").unwrap();
    fs::write(dir.join("ignored.log"), "x\n").unwrap();
    fs::write(dir.join(".gitignore"), "*.log\ntarget\n").unwrap();
    dir
}

fn start<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        crate::quick_open::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None); // in memory: never the real settings file
    });
    let dir = workspace(name);
    let root = dir.clone();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    cx.run_until_parked();
    (wb, cx, dir)
}

fn picker(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Entity<QuickPick>> {
    wb.read_with(cx, |wb, _| wb.quick.picker().cloned())
}

/// "dir/label" of every item, separators as "--label".
fn rows(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    let p = picker(wb, cx).expect("quick open is open");
    p.read_with(cx, |p, _| {
        p.list()
            .rows()
            .iter()
            .map(|r| match r {
                PickRow::Separator(s) => format!("--{s}"),
                PickRow::Item(i) => match &i.description {
                    Some(d) => format!("{d}/{}", i.label),
                    None => i.label.clone(),
                },
            })
            .collect()
    })
}

fn open_quick_open(cx: &mut VisualTestContext) {
    cx.simulate_keystrokes("cmd-p");
    cx.run_until_parked();
}

fn active_path(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<String> {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.path.to_string_lossy().into_owned()))
}

fn root_focused(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> bool {
    cx.update(|window, app| wb.read(app).focus.as_ref().unwrap().is_focused(window))
}

#[gpui_kit::test]
fn typing_mainrs_ranks_src_main_first_with_highlights(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "rank");
    open_quick_open(cx);
    assert!(picker(&wb, cx).is_some());
    assert_eq!(rows(&wb, cx), Vec::<String>::new(), "no history, empty query: nothing yet");

    cx.simulate_input("mainrs");
    cx.run_until_parked();
    let r = rows(&wb, cx);
    assert_eq!(r[0], "--file results");
    assert_eq!(r[1], "src/main.rs");
    let p = picker(&wb, cx).unwrap();
    let hl = p.read_with(cx, |p, _| {
        let item = p.active_item().unwrap();
        item.label_highlights.iter().flat_map(|m| item.label.chars().skip(m.start).take(m.end - m.start)).collect::<String>()
    });
    assert_eq!(hl, "mainrs");
}

#[gpui_kit::test]
fn down_then_enter_opens_the_second_result_and_focuses_the_editor(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "accept");
    open_quick_open(cx);
    cx.simulate_input("main");
    cx.run_until_parked();
    let r = rows(&wb, cx);
    assert_eq!(&r[1..3], ["src/main.rs", "src/domain.rs"]);

    cx.simulate_keystrokes("down enter");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none(), "accepting closes the picker");
    assert_eq!(active_path(&wb, cx), Some(dir.join("src/domain.rs").to_string_lossy().into_owned()));

    // The editor has focus: typing edits the document.
    cx.simulate_keystrokes("x");
    let text = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().buffer().value());
    assert!(text.starts_with('x'), "typed into the editor, got {text:?}");
}

#[gpui_kit::test]
fn recently_opened_files_come_first_and_the_empty_query_shows_them(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "history");
    wb.update(cx, |wb, cx| {
        wb.open(&dir.join("src/main.rs"));
        wb.open(&dir.join("docs/readme.md"));
        cx.notify();
    });
    open_quick_open(cx);
    assert_eq!(rows(&wb, cx), ["--recently opened", "docs/readme.md", "src/main.rs"]);
    cx.simulate_input("rs");
    cx.run_until_parked();
    let r = rows(&wb, cx);
    assert_eq!(&r[..2], ["--recently opened", "src/main.rs"]);
    assert!(r.contains(&"--file results".to_string()) && r.contains(&"src/domain.rs".to_string()));
}

#[gpui_kit::test]
fn escape_closes_and_returns_focus(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "escape");
    assert!(root_focused(&wb, cx));
    open_quick_open(cx);
    assert!(picker(&wb, cx).is_some());
    assert!(!root_focused(&wb, cx), "the input has focus");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
    assert!(root_focused(&wb, cx), "focus is back");
}

#[gpui_kit::test]
fn file_colon_line_opens_at_that_line(cx: &mut TestAppContext) {
    let (wb, cx, dir) = start(cx, "line");
    open_quick_open(cx);
    cx.simulate_input("file.rs:3");
    cx.run_until_parked();
    assert_eq!(rows(&wb, cx), ["--file results", "notes/file.rs"]);
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(active_path(&wb, cx), Some(dir.join("notes/file.rs").to_string_lossy().into_owned()));
    let line = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection().active.line);
    assert_eq!(line, 3);
}

#[gpui_kit::test]
fn excluded_and_ignored_files_are_not_offered(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "exclude");
    open_quick_open(cx);
    cx.simulate_input("rs");
    cx.run_until_parked();
    let r = rows(&wb, cx);
    assert!(r.contains(&"src/main.rs".to_string()));
    assert!(!r.iter().any(|x| x.contains("skip.rs")), "target is in .gitignore: {r:?}");
    cx.simulate_keystrokes("escape");
    cx.simulate_keystrokes("cmd-p");
    cx.simulate_input("main");
    cx.run_until_parked();
    assert!(!rows(&wb, cx).iter().any(|x| x.contains("node_modules")), "search.exclude default");
    cx.simulate_keystrokes("escape");

    // files.exclude from the live configuration, applied when Quick Open opens again.
    cx.update(|_, cx| {
        AppConfig::set_value(cx, "files.exclude", Some(serde_json::json!({"**/notes": true, "**/.git": true}))).unwrap();
    });
    cx.simulate_keystrokes("cmd-p");
    cx.simulate_input("file");
    cx.run_until_parked();
    assert_eq!(rows(&wb, cx), Vec::<String>::new(), "notes/ is excluded");
}

#[gpui_kit::test]
fn clicking_outside_closes(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "outside");
    open_quick_open(cx);
    let p = gpui_kit::point(gpui_kit::px(5.), gpui_kit::px(500.));
    cx.simulate_mouse_move(p, None, gpui_kit::Modifiers::default());
    cx.simulate_click(p, gpui_kit::Modifiers::default());
    cx.run_until_parked();
    assert!(picker(&wb, cx).is_none());
}

#[gpui_kit::test]
fn preserve_input_restores_the_last_text(cx: &mut TestAppContext) {
    let (wb, cx, _dir) = start(cx, "preserve");
    cx.update(|_, cx| AppConfig::set_value(cx, "workbench.quickOpen.preserveInput", Some(serde_json::json!(true))).unwrap());
    open_quick_open(cx);
    cx.simulate_input("domain");
    cx.simulate_keystrokes("escape");
    open_quick_open(cx);
    let q = picker(&wb, cx).unwrap().read_with(cx, |p, cx| p.query(cx));
    assert_eq!(q, "domain");
}

#[gpui_kit::test]
fn settings_are_read_from_the_configuration(cx: &mut TestAppContext) {
    cx.update(|cx| AppConfig::install(cx, None));
    cx.update(|cx| {
        let s = super::QuickOpenSettings::from_configuration(AppConfig::get(cx).configuration());
        assert!(s.close_on_focus_lost && !s.preserve_input && s.include_history && !s.history_by_recency);
        assert!(s.index.exclude_globs.contains(&"**/node_modules".to_string()));
        AppConfig::set_value(cx, "search.quickOpen.history.filterSortOrder", Some(serde_json::json!("recency"))).unwrap();
        AppConfig::set_value(cx, "search.quickOpen.includeHistory", Some(serde_json::json!(false))).unwrap();
        let s = super::QuickOpenSettings::from_configuration(AppConfig::get(cx).configuration());
        assert!(s.history_by_recency && !s.include_history);
    });
}
