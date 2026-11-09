//! Headless end-to-end tests for the `files.*` settings of ADR-0034 (docs/testing.md, tier 2):
//! save participants, the line break of a text without one, auto save (after delay, focus and
//! window triggers, workspace-only), and read-only files. Every assertion reads the file on disk.
//!
//! Temp folders and a temp settings file only; the real user settings are never touched.

use std::fs;
use std::time::Duration;

use gpui_kit::TestAppContext;
use serde_json::json;

use super::e2e::{Fixture, editor_value, open_file, set, unique_dir, workbench};
use crate::Workbench;

/// Runs the test clock forward by `ms` and lets the frame (and any timer it started) settle.
fn advance(cx: &mut gpui_kit::VisualTestContext, ms: u64) {
    cx.executor().advance_clock(Duration::from_millis(ms));
    cx.run_until_parked();
}

fn read(f: &Fixture, name: &str) -> String {
    fs::read_to_string(f.dir.join(name)).unwrap()
}

fn is_dirty(wb: &gpui_kit::Entity<Workbench>, cx: &mut gpui_kit::VisualTestContext) -> bool {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().is_dirty())
}

// ---- files.trimFinalNewlines and files.insertFinalNewline ------------------------------------

#[gpui_kit::test]
fn trim_final_newlines_keeps_exactly_one_line_break_on_save(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "trim-final",
        Some(r#"{ "files.trimFinalNewlines": true }"#),
        &[("a.txt", "a\n\n\n")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(read(&f, "a.txt"), "a\n");
}

#[gpui_kit::test]
fn trim_final_newlines_is_off_by_default(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "trim-final-off", None, &[("a.txt", "a\n\n\n")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(read(&f, "a.txt"), "a\n\n\n");
}

#[gpui_kit::test]
fn trim_final_newlines_runs_after_insert_final_newline(cx: &mut TestAppContext) {
    // Upstream order: insert the final newline, then trim the empty lines after the last content.
    let (wb, cx, f) = workbench(
        cx,
        "trim-and-insert",
        Some(r#"{ "files.trimFinalNewlines": true, "files.insertFinalNewline": true }"#),
        &[("a.txt", "a\n\n")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("cmd-s");
    assert_eq!(read(&f, "a.txt"), "a\n");
}

// ---- files.eol -------------------------------------------------------------------------------

#[gpui_kit::test]
fn eol_crlf_is_the_line_break_of_a_text_without_one(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "eol-crlf",
        Some(r#"{ "files.eol": "\r\n" }"#),
        &[("one.txt", "abc"), ("two.txt", "a\nb")],
    );
    open_file(&wb, cx, &f.dir.join("one.txt"));
    cx.simulate_keystrokes("end enter x cmd-s");
    assert_eq!(read(&f, "one.txt"), "abc\r\nx");
    // A text that already has line breaks keeps its own.
    open_file(&wb, cx, &f.dir.join("two.txt"));
    cx.simulate_keystrokes("down end enter x cmd-s");
    assert_eq!(read(&f, "two.txt"), "a\nb\nx");
}

#[gpui_kit::test]
fn eol_defaults_to_lf_and_can_change_live(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "eol-live",
        None,
        &[("one.txt", "abc"), ("two.txt", "abc")],
    );
    open_file(&wb, cx, &f.dir.join("one.txt"));
    cx.simulate_keystrokes("end enter x cmd-s");
    assert_eq!(read(&f, "one.txt"), "abc\nx", "auto is LF on macOS");
    set(cx, "files.eol", json!("\r\n"));
    open_file(&wb, cx, &f.dir.join("two.txt"));
    cx.simulate_keystrokes("end enter x cmd-s");
    assert_eq!(
        read(&f, "two.txt"),
        "abc\r\nx",
        "applies to the next open without restart"
    );
}

// ---- files.autoSave: afterDelay ---------------------------------------------------------------

#[gpui_kit::test]
fn auto_save_is_off_by_default(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(cx, "autosave-off", None, &[("a.txt", "a")]);
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("x");
    advance(cx, 5000);
    assert_eq!(read(&f, "a.txt"), "a");
    assert!(is_dirty(&wb, cx));
}

#[gpui_kit::test]
fn after_delay_saves_once_the_delay_passes_since_the_last_edit(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "autosave-delay",
        Some(r#"{ "files.autoSave": "afterDelay", "files.autoSaveDelay": 1000 }"#),
        &[("a.txt", "a")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("x");
    advance(cx, 999);
    assert_eq!(read(&f, "a.txt"), "a", "not before the delay");
    advance(cx, 1);
    assert_eq!(read(&f, "a.txt"), "xa", "saved at the delay");
    assert!(!is_dirty(&wb, cx));
}

#[gpui_kit::test]
fn after_delay_restarts_on_every_edit(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "autosave-restart",
        Some(r#"{ "files.autoSave": "afterDelay", "files.autoSaveDelay": 1000 }"#),
        &[("a.txt", "a")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("x");
    advance(cx, 800);
    cx.simulate_keystrokes("y");
    advance(cx, 800); // 1600 ms after the first edit, 800 after the second
    assert_eq!(
        read(&f, "a.txt"),
        "a",
        "the second edit restarted the delay"
    );
    advance(cx, 200);
    assert_eq!(read(&f, "a.txt"), "xya");
}

#[gpui_kit::test]
fn after_delay_uses_the_save_path_and_leaves_the_caret_line_alone(cx: &mut TestAppContext) {
    // `files.trimTrailingWhitespace` on an auto save: the line the caret is on keeps its spaces.
    let (wb, cx, f) = workbench(
        cx,
        "autosave-trim",
        Some(
            r#"{ "files.autoSave": "afterDelay", "files.autoSaveDelay": 500, "files.trimTrailingWhitespace": true }"#,
        ),
        &[("a.txt", "a  \nb  \n")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    // The caret ends up after the trailing space of line 1, so that space stays; line 2 is trimmed.
    cx.simulate_keystrokes("end backspace");
    advance(cx, 500);
    assert_eq!(
        read(&f, "a.txt"),
        "a \nb\n",
        "caret line keeps the space before the caret"
    );
}

#[gpui_kit::test]
fn auto_save_workspace_files_only_skips_files_outside_the_folder(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "autosave-ws",
        Some(
            r#"{ "files.autoSave": "afterDelay", "files.autoSaveDelay": 100, "files.autoSaveWorkspaceFilesOnly": true }"#,
        ),
        &[("inside.txt", "a")],
    );
    let outside = unique_dir("autosave-outside");
    fs::write(outside.join("out.txt"), "o").unwrap();
    open_file(&wb, cx, &outside.join("out.txt"));
    cx.simulate_keystrokes("x");
    advance(cx, 1000);
    assert_eq!(
        fs::read_to_string(outside.join("out.txt")).unwrap(),
        "o",
        "outside the folder: not saved"
    );
    open_file(&wb, cx, &f.dir.join("inside.txt"));
    cx.simulate_keystrokes("x");
    advance(cx, 1000);
    assert_eq!(read(&f, "inside.txt"), "xa", "inside the folder: saved");
}

// ---- files.autoSave: focus and window triggers ------------------------------------------------

#[gpui_kit::test]
fn on_focus_change_saves_the_editor_being_left(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "autosave-focus",
        Some(r#"{ "files.autoSave": "onFocusChange" }"#),
        &[("a.txt", "a"), ("b.txt", "b")],
    );
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("x");
    advance(cx, 5000);
    assert_eq!(read(&f, "a.txt"), "a", "no timer in this mode");
    open_file(&wb, cx, &f.dir.join("b.txt"));
    assert_eq!(
        read(&f, "a.txt"),
        "xa",
        "saved when another editor became active"
    );
    assert_eq!(read(&f, "b.txt"), "b");
}

#[gpui_kit::test]
fn on_window_change_saves_dirty_editors_when_the_window_loses_focus(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "autosave-window",
        Some(r#"{ "files.autoSave": "onWindowChange" }"#),
        &[("a.txt", "a"), ("b.txt", "b")],
    );
    // The test window starts inactive; the trigger needs an active-to-inactive change.
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    open_file(&wb, cx, &f.dir.join("a.txt"));
    cx.simulate_keystrokes("x");
    open_file(&wb, cx, &f.dir.join("b.txt"));
    assert_eq!(
        read(&f, "a.txt"),
        "a",
        "switching editors does not save in this mode"
    );
    cx.simulate_keystrokes("y");
    cx.deactivate_window();
    wb.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(
        read(&f, "b.txt"),
        "yb",
        "the window lost focus: dirty editors are saved"
    );
    assert_eq!(
        read(&f, "a.txt"),
        "xa",
        "every dirty editor, not only the active one"
    );
}

// ---- files.readonly* ------------------------------------------------------------------------

#[gpui_kit::test]
fn readonly_include_opens_matching_files_read_only(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "readonly-include",
        Some(
            r#"{ "files.readonlyInclude": { "**/*.log": true }, "files.readonlyExclude": { "keep.log": true } }"#,
        ),
        &[("ro.log", "log"), ("keep.log", "keep"), ("a.txt", "a")],
    );
    open_file(&wb, cx, &f.dir.join("ro.log"));
    assert!(wb.read_with(cx, |wb, _| {
        wb.editors.active_editor().unwrap().is_read_only()
    }));
    cx.simulate_keystrokes("x enter cmd-z backspace cmd-s");
    assert_eq!(editor_value(&wb, cx), "log", "edits are refused");
    assert_eq!(read(&f, "ro.log"), "log");

    open_file(&wb, cx, &f.dir.join("keep.log"));
    assert!(
        !wb.read_with(cx, |wb, _| wb
            .editors
            .active_editor()
            .unwrap()
            .is_read_only()),
        "excluded: writable"
    );
    cx.simulate_keystrokes("x cmd-s");
    assert_eq!(read(&f, "keep.log"), "xkeep");

    open_file(&wb, cx, &f.dir.join("a.txt"));
    assert!(!wb.read_with(cx, |wb, _| {
        wb.editors.active_editor().unwrap().is_read_only()
    }));
}

#[gpui_kit::test]
fn readonly_from_permissions_follows_the_owner_write_bit(cx: &mut TestAppContext) {
    use std::os::unix::fs::PermissionsExt;
    let (wb, cx, f) = workbench(cx, "readonly-perm", None, &[("locked.txt", "l")]);
    fs::set_permissions(f.dir.join("locked.txt"), fs::Permissions::from_mode(0o444)).unwrap();
    open_file(&wb, cx, &f.dir.join("locked.txt"));
    assert!(
        !wb.read_with(cx, |wb, _| wb
            .editors
            .active_editor()
            .unwrap()
            .is_read_only()),
        "off by default"
    );
    set(cx, "files.readonlyFromPermissions", json!(true));
    assert!(wb.read_with(cx, |wb, _| {
        wb.editors.active_editor().unwrap().is_read_only()
    }));
    fs::set_permissions(f.dir.join("locked.txt"), fs::Permissions::from_mode(0o644)).unwrap();
}

#[gpui_kit::test]
fn search_replace_leaves_read_only_files_alone(cx: &mut TestAppContext) {
    use crate::search_view::replace::{FileRanges, ReplaceRequest};
    use vsr_search::{SearchRange, TextSearchQuery};
    let (wb, cx, f) = workbench(
        cx,
        "readonly-search",
        Some(r#"{ "files.readonlyInclude": { "**/*.log": true } }"#),
        &[("ro.log", "log"), ("w.txt", "log")],
    );
    let query = TextSearchQuery {
        pattern: "log".into(),
        is_regex: false,
        is_case_sensitive: false,
        is_word_match: false,
        is_multiline: false,
    };
    let pattern = vsr_find::SearchReplacePattern::new("X", &query, false).unwrap();
    let request = |file: &str| ReplaceRequest {
        files: vec![FileRanges {
            path: f.dir.join(file),
            ranges: vec![SearchRange::one_line(1, 1, 4)],
        }],
        pattern: pattern.clone(),
        preserve_case: false,
    };
    wb.update(cx, |wb, cx| wb.apply_search_replace(&request("ro.log"), cx));
    assert_eq!(read(&f, "ro.log"), "log", "read-only: not replaced");
    wb.update(cx, |wb, cx| wb.apply_search_replace(&request("w.txt"), cx));
    assert_eq!(read(&f, "w.txt"), "X", "writable: replaced");
}

#[gpui_kit::test]
fn toggle_read_only_in_session_overrides_the_settings(cx: &mut TestAppContext) {
    let (wb, cx, f) = workbench(
        cx,
        "readonly-toggle",
        Some(r#"{ "files.readonlyInclude": { "**/*.log": true } }"#),
        &[("ro.log", "log")],
    );
    open_file(&wb, cx, &f.dir.join("ro.log"));
    wb.update(cx, |wb, cx| wb.toggle_read_only_in_session(cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("x cmd-s");
    assert_eq!(
        read(&f, "ro.log"),
        "xlog",
        "session override makes it writable"
    );
    wb.update(cx, |wb, cx| wb.toggle_read_only_in_session(cx));
    cx.run_until_parked();
    assert!(wb.read_with(cx, |wb, _| {
        wb.editors.active_editor().unwrap().is_read_only()
    }));
}
