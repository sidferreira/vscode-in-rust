//! Headless end-to-end tests for languages and syntax highlighting (ADR-0038, docs/testing.md
//! tier 2): the language of an opened file, `files.associations`, Change Language Mode, token
//! colours from the Dark Modern theme, `editor.maxTokenizationLineLength` and
//! `files.trimTrailingWhitespaceInRegexAndStrings`.
//!
//! upstream: src/vs/workbench/browser/parts/editor/editorStatus.ts @ 50e1350fc78d
//! Temp folders and a temp settings file only; the real user settings are never touched.

use std::fs;
use std::time::Duration;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use vsr_quickinput::pick::PickRow;
use vsr_textmate::Rgba;

use super::e2e::{open_file, set, workbench};
use crate::Workbench;
use crate::language_mode::ChangeLanguageMode;

fn language(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (String, String) {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        (e.language_id().to_string(), wb.editors.language_name(e))
    })
}

/// Waits for the background tokenizer of the active editor.
fn tokenized(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    let idle = wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        e.sync_syntax();
        e.syntax().is_none_or(|d| d.tokenizer.wait_idle(Duration::from_secs(20)))
    });
    assert!(idle, "the tokenizer finished");
}

fn foreground_of(wb: &Entity<Workbench>, cx: &mut VisualTestContext, line: usize, text: &str) -> Option<Rgba> {
    wb.read_with(cx, |wb, _| {
        let e = wb.editors.active_editor().unwrap();
        let content = e.buffer().line_content(line).to_string();
        let runs = e.syntax()?.style_runs(line, &content)?;
        runs.iter().find(|r| content[r.start..r.end].trim() == text).and_then(|r| r.style.foreground)
    })
}

fn rows(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    let pick = wb.read_with(cx, |wb, _| wb.lang.picker().cloned()).expect("the language picker is open");
    pick.read_with(cx, |p, _| {
        p.list()
            .rows()
            .iter()
            .map(|r| match r {
                PickRow::Separator(s) => format!("--{s}"),
                PickRow::Item(i) => i.label.clone(),
            })
            .collect()
    })
}

#[gpui_kit::test]
fn a_rust_file_has_the_rust_language_and_keyword_colors(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "syntax-rust", None, &[("a.rs", "fn main() { let s = \"x\"; } // note\n")]);
    open_file(&wb, cx, &f.dir.join("a.rs"));
    assert_eq!(language(&wb, cx), ("rust".to_string(), "Rust".to_string()));
    tokenized(&wb, cx);
    // Dark+: keyword #569CD6, function name #DCDCAA, string #CE9178, comment #6A9955.
    assert_eq!(foreground_of(&wb, cx, 1, "fn"), Some(Rgba([0x56, 0x9c, 0xd6, 255])));
    assert_eq!(foreground_of(&wb, cx, 1, "main"), Some(Rgba([0xdc, 0xdc, 0xaa, 255])));
    assert_eq!(foreground_of(&wb, cx, 1, "\"x\""), Some(Rgba([0xce, 0x91, 0x78, 255])));
    assert_eq!(foreground_of(&wb, cx, 1, "// note"), Some(Rgba([0x6a, 0x99, 0x55, 255])));
}

#[gpui_kit::test]
fn editing_retokenizes_and_the_colors_follow(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "syntax-edit", None, &[("a.rs", "x\n")]);
    open_file(&wb, cx, &f.dir.join("a.rs"));
    tokenized(&wb, cx);
    assert_eq!(foreground_of(&wb, cx, 1, "let"), None);
    cx.simulate_keystrokes("cmd-up"); // caret to the start of the file
    cx.simulate_input("let ");
    cx.run_until_parked();
    tokenized(&wb, cx);
    assert_eq!(foreground_of(&wb, cx, 1, "let"), Some(Rgba([0x56, 0x9c, 0xd6, 255])));
}

#[gpui_kit::test]
fn files_associations_map_a_pattern_to_a_language_live(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "syntax-assoc",
        Some(r#"{ "files.associations": { "*.foo": "json" } }"#),
        &[("x.foo", "{\"a\": 1}\n"), ("y.bar", "{\"a\": 1}\n")],
    );
    open_file(&wb, cx, &f.dir.join("x.foo"));
    assert_eq!(language(&wb, cx), ("json".to_string(), "JSON".to_string()));
    tokenized(&wb, cx);
    assert_eq!(foreground_of(&wb, cx, 1, "1"), Some(Rgba([0xb5, 0xce, 0xa8, 255])), "numbers in JSON are #B5CEA8");

    open_file(&wb, cx, &f.dir.join("y.bar"));
    assert_eq!(language(&wb, cx), ("plaintext".to_string(), "Plain Text".to_string()));
    set(cx, "files.associations", json!({ "*.bar": "python", "*.foo": 3 }));
    assert_eq!(language(&wb, cx), ("python".to_string(), "Python".to_string()));
}

#[gpui_kit::test]
fn change_language_mode_picks_a_language_and_auto_detect_undoes_it(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "syntax-pick", None, &[("a.txt", "print('hi')\n")]);
    cx.update(|_, cx| crate::language_mode::init(cx));
    open_file(&wb, cx, &f.dir.join("a.txt"));
    assert_eq!(language(&wb, cx).0, "plaintext");
    assert!(cx.update(|_, cx| cx.global::<crate::commands::Commands>().execution("workbench.action.editor.changeLanguageMode").is_some()));

    cx.dispatch_action(ChangeLanguageMode);
    cx.run_until_parked();
    let listed = rows(&wb, cx);
    assert_eq!(listed[0], "Auto Detect");
    assert_eq!(listed[1], "--languages (identifier)");
    assert!(listed.contains(&"Python".to_string()));

    cx.simulate_input("python");
    cx.run_until_parked();
    assert_eq!(rows(&wb, cx)[0], "Python");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert_eq!(language(&wb, cx), ("python".to_string(), "Python".to_string()));
    assert!(wb.read_with(cx, |wb, _| wb.lang.picker().is_none()), "the picker closed");
    tokenized(&wb, cx);
    assert_eq!(foreground_of(&wb, cx, 1, "print"), Some(Rgba([0xdc, 0xdc, 0xaa, 255])));

    // A picked language is kept when settings change; Auto Detect goes back to the file name.
    set(cx, "files.associations", json!({}));
    assert_eq!(language(&wb, cx).0, "python");
    cx.dispatch_action(ChangeLanguageMode);
    cx.run_until_parked();
    cx.simulate_keystrokes("enter"); // the first row: Auto Detect
    cx.run_until_parked();
    assert_eq!(language(&wb, cx).0, "plaintext");
}

#[gpui_kit::test]
fn max_tokenization_line_length_leaves_long_lines_plain(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "syntax-maxlen", None, &[("a.rs", "let a = 1; let b = 2; let c = 3;\nlet d = 4;\n")]);
    open_file(&wb, cx, &f.dir.join("a.rs"));
    tokenized(&wb, cx);
    assert!(foreground_of(&wb, cx, 1, "let").is_some());
    set(cx, "editor.maxTokenizationLineLength", json!(10));
    tokenized(&wb, cx);
    assert_eq!(foreground_of(&wb, cx, 1, "let"), None, "line 1 (32 bytes) is over the cap");
    assert!(foreground_of(&wb, cx, 2, "let").is_some(), "line 2 (10 bytes) is not");
    set(cx, "editor.maxTokenizationLineLength", json!(20000));
    tokenized(&wb, cx);
    assert!(foreground_of(&wb, cx, 1, "let").is_some(), "back under the cap");
}

#[gpui_kit::test]
fn trailing_whitespace_in_strings_is_kept_when_the_setting_is_off(cx: &mut TestAppContext) {
    let text = "let s = \"abc  \nabc\";  \nlet x = 1;  \n";
    let (wb, cx, f) = workbench(
        cx,
        "syntax-trim-off",
        Some(r#"{ "files.trimTrailingWhitespace": true, "files.trimTrailingWhitespaceInRegexAndStrings": false }"#),
        &[("a.rs", text)],
    );
    open_file(&wb, cx, &f.dir.join("a.rs"));
    tokenized(&wb, cx);
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(fs::read_to_string(f.dir.join("a.rs")).unwrap(), "let s = \"abc  \nabc\";\nlet x = 1;\n");
}

#[gpui_kit::test]
fn trailing_whitespace_in_strings_is_trimmed_by_default(cx: &mut TestAppContext) {
    let text = "let s = \"abc  \nabc\";  \nlet x = 1;  \n";
    let (wb, cx, f) = workbench(cx, "syntax-trim-on", Some(r#"{ "files.trimTrailingWhitespace": true }"#), &[("a.rs", text)]);
    open_file(&wb, cx, &f.dir.join("a.rs"));
    tokenized(&wb, cx);
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(fs::read_to_string(f.dir.join("a.rs")).unwrap(), "let s = \"abc\nabc\";\nlet x = 1;\n");
}
