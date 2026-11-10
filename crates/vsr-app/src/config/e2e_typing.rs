//! Headless end-to-end tests for the typing rules and the editing settings of ADR-0046
//! (docs/testing.md tier 2): auto closing and over typing, auto surround, auto indent, Cmd+/ and
//! Shift+Alt+A, `editor.detectIndentation`, `trimAutoWhitespace`, `useTabStops`, `indentSize`,
//! `trimWhitespaceOnDelete` and `emptySelectionClipboard`. Every setting is also checked off.
//!
//! upstream: src/vs/editor/common/cursor/cursorTypeOperations.ts @ 50e1350fc78d
//! Temp folders and a temp settings file only; the real user settings are never touched.

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use vsr_editor::{Position, Selection};

use super::e2e::{editor_value, open_file, set, workbench};
use crate::Workbench;
use crate::model::OpenEditor;

/// Runs `f` on the active editor and repaints.
fn with_editor<R>(wb: &Entity<Workbench>, cx: &mut VisualTestContext, f: impl FnOnce(&mut OpenEditor) -> R) -> R {
    let r = wb.update(cx, |wb, cx| {
        let i = wb.editors.active.expect("an editor is open");
        let r = f(&mut wb.editors.open[i]);
        cx.notify();
        r
    });
    cx.run_until_parked();
    r
}

fn caret(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> (usize, usize) {
    wb.read_with(cx, |wb, _| {
        let p = wb.editors.active_editor().unwrap().editor.selection().active;
        (p.line, p.column)
    })
}

fn place_caret(wb: &Entity<Workbench>, cx: &mut VisualTestContext, line: usize, column: usize) {
    with_editor(wb, cx, |e| e.editor.set_cursor(Position::new(line, column), false));
}

fn select(wb: &Entity<Workbench>, cx: &mut VisualTestContext, from: (usize, usize), to: (usize, usize)) {
    with_editor(wb, cx, |e| {
        e.editor.set_selection(Selection { anchor: Position::new(from.0, from.1), active: Position::new(to.0, to.1) });
    });
}

fn setup<'a>(cx: &'a mut TestAppContext, name: &str, file: &str, text: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let (wb, cx, f) = workbench(cx, name, None, &[(file, text)]);
    open_file(&wb, cx, &f.dir.join(file));
    cx.run_until_parked(); // the editor takes focus on the frame after the first
    (wb, cx)
}

#[gpui_kit::test]
fn typing_an_open_bracket_closes_it_and_the_close_bracket_types_over(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "autoclose", "a.rs", "");
    cx.simulate_input("(");
    assert_eq!(editor_value(&wb, cx), "()");
    assert_eq!(caret(&wb, cx), (1, 2), "the caret is between the brackets");
    cx.simulate_input("x");
    cx.simulate_input(")");
    assert_eq!(editor_value(&wb, cx), "(x)", "the closing bracket the editor added is typed over");
    assert_eq!(caret(&wb, cx), (1, 4));
    cx.simulate_input(")");
    assert_eq!(editor_value(&wb, cx), "(x))", "a second one is a new character");

    // `{` and `"` close as well; a quote is not closed in a string.
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("{");
    assert_eq!(editor_value(&wb, cx), "{}");
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("\"");
    assert_eq!(editor_value(&wb, cx), "\"\"");
    cx.simulate_input("a");
    cx.simulate_input("\"");
    assert_eq!(editor_value(&wb, cx), "\"a\"", "the closing quote is typed over");
}

#[gpui_kit::test]
fn backspace_between_a_pair_removes_both(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "autodelete", "a.rs", "");
    cx.simulate_input("[");
    assert_eq!(editor_value(&wb, cx), "[]");
    cx.simulate_keystrokes("backspace");
    assert_eq!(editor_value(&wb, cx), "", "both are removed");
    set(cx, "editor.autoClosingDelete", json!("never"));
    cx.simulate_input("[");
    cx.simulate_keystrokes("backspace");
    assert_eq!(editor_value(&wb, cx), "]", "autoClosingDelete never removes only the opening bracket");
}

#[gpui_kit::test]
fn the_auto_closing_settings_turn_each_rule_off(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "autoclose-off", "a.rs", "");
    set(cx, "editor.autoClosingBrackets", json!("never"));
    cx.simulate_input("(");
    assert_eq!(editor_value(&wb, cx), "(", "autoClosingBrackets never");
    cx.simulate_keystrokes("backspace");
    set(cx, "editor.autoClosingBrackets", json!("languageDefined"));
    set(cx, "editor.autoClosingQuotes", json!("never"));
    cx.simulate_input("\"");
    assert_eq!(editor_value(&wb, cx), "\"", "autoClosingQuotes never");
    cx.simulate_keystrokes("backspace");
    set(cx, "editor.autoClosingQuotes", json!("languageDefined"));
    set(cx, "editor.autoClosingOvertype", json!("never"));
    cx.simulate_input("(");
    cx.simulate_input(")");
    assert_eq!(editor_value(&wb, cx), "())", "autoClosingOvertype never types a new closing bracket");
    cx.simulate_keystrokes("cmd-a backspace");
    set(cx, "editor.autoClosingOvertype", json!("auto"));
    // `beforeWhitespace`: no close before a letter.
    set(cx, "editor.autoClosingBrackets", json!("beforeWhitespace"));
    cx.simulate_input("a");
    place_caret(&wb, cx, 1, 1);
    cx.simulate_input("(");
    assert_eq!(editor_value(&wb, cx), "(a");
}

#[gpui_kit::test]
fn a_selection_is_surrounded_by_a_typed_quote_or_bracket(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "surround", "a.rs", "hello world");
    select(&wb, cx, (1, 1), (1, 6));
    cx.simulate_input("\"");
    assert_eq!(editor_value(&wb, cx), "\"hello\" world");
    let sel = wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.selection());
    assert_eq!((sel.anchor.column, sel.active.column), (2, 7), "the surrounded text stays selected");
    cx.simulate_input("(");
    assert_eq!(editor_value(&wb, cx), "\"(hello)\" world", "and can be surrounded again");

    // autoSurround never: the typed character replaces the selection.
    set(cx, "editor.autoSurround", json!("never"));
    select(&wb, cx, (1, 1), (1, 3));
    cx.simulate_input("x");
    assert_eq!(editor_value(&wb, cx), "xhello)\" world");
}

#[gpui_kit::test]
fn enter_indents_after_an_open_brace_and_splits_a_pair(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "enter-rs", "a.rs", "fn main() {");
    place_caret(&wb, cx, 1, 12);
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "fn main() {\n    ");
    assert_eq!(caret(&wb, cx), (2, 5));

    // `{|}`: Enter puts the close brace on its own line, indented like the opening line.
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("{");
    assert_eq!(editor_value(&wb, cx), "{}");
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "{\n    \n}");
    assert_eq!(caret(&wb, cx), (2, 5));
}

#[gpui_kit::test]
fn enter_indents_after_a_colon_in_python(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "enter-py", "a.py", "def f(x):");
    place_caret(&wb, cx, 1, 10);
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "def f(x):\n    ", "an onEnter rule of the language configuration");
    assert_eq!(caret(&wb, cx), (2, 5));
    // `autoIndent: keep` only copies the indentation of the line.
    set(cx, "editor.autoIndent", json!("keep"));
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("def g(x):");
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "def g(x):\n");
}

#[gpui_kit::test]
fn the_auto_indent_levels(cx: &mut TestAppContext) {
    // none: a plain line break. keep: the indentation of the line. brackets: and after `{`.
    let (wb, cx) = setup(cx, "autoindent", "a.rs", "    a {");
    place_caret(&wb, cx, 1, 8);
    set(cx, "editor.autoIndent", json!("none"));
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "    a {\n");
    cx.simulate_keystrokes("backspace");
    set(cx, "editor.autoIndent", json!("keep"));
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "    a {\n    ");
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("    a {");
    assert_eq!(editor_value(&wb, cx), "    a {}", "the brace closed");
    cx.simulate_keystrokes("delete");
    set(cx, "editor.autoIndent", json!("brackets"));
    cx.simulate_keystrokes("enter");
    assert_eq!(editor_value(&wb, cx), "    a {\n        ", "an open bracket indents at `brackets`");
}

#[gpui_kit::test]
fn typing_a_close_brace_first_on_a_line_outdents_it(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "electric", "a.rs", "fn f() {\n    x;\n    ");
    place_caret(&wb, cx, 3, 5);
    cx.simulate_input("}");
    assert_eq!(editor_value(&wb, cx), "fn f() {\n    x;\n}");
    // At any autoIndent level.
    set(cx, "editor.autoIndent", json!("none"));
    set(cx, "editor.autoClosingBrackets", json!("never"));
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("{");
    cx.simulate_keystrokes("enter");
    cx.simulate_input("   ");
    cx.simulate_input("}");
    assert_eq!(editor_value(&wb, cx), "{\n}", "the line is indented like the one with the matching brace");
}

#[gpui_kit::test]
fn cmd_slash_toggles_line_comments_in_rust_and_python(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "comment-rs", "a.rs", "let a = 1;\n    let b = 2;\n");
    cx.simulate_keystrokes("cmd-/");
    assert_eq!(editor_value(&wb, cx), "// let a = 1;\n    let b = 2;\n");
    assert_eq!(caret(&wb, cx), (1, 4), "the caret follows the inserted comment");
    cx.simulate_keystrokes("cmd-a cmd-/");
    assert_eq!(editor_value(&wb, cx), "// // let a = 1;\n//     let b = 2;\n", "a line that is not commented adds a comment to all, aligned");
    cx.simulate_keystrokes("cmd-/");
    assert_eq!(editor_value(&wb, cx), "// let a = 1;\n    let b = 2;\n", "every line commented: removes one level");

    let (wb, cx) = setup(cx, "comment-py", "b.py", "x = 1\ny = 2");
    cx.simulate_keystrokes("cmd-a cmd-/");
    assert_eq!(editor_value(&wb, cx), "# x = 1\n# y = 2");
    cx.simulate_keystrokes("cmd-/");
    assert_eq!(editor_value(&wb, cx), "x = 1\ny = 2");
    cx.simulate_keystrokes("cmd-z");
    assert_eq!(editor_value(&wb, cx), "# x = 1\n# y = 2", "one undo step");
}

#[gpui_kit::test]
fn comments_settings_insert_space_and_ignore_empty_lines(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "comment-settings", "a.rs", "a\n\nb");
    set(cx, "editor.comments.insertSpace", json!(false));
    cx.simulate_keystrokes("cmd-a cmd-/");
    assert_eq!(editor_value(&wb, cx), "//a\n\n//b", "no space after the marker; the empty line is ignored");
    cx.simulate_keystrokes("cmd-/");
    set(cx, "editor.comments.ignoreEmptyLines", json!(false));
    cx.simulate_keystrokes("cmd-a cmd-/");
    assert_eq!(editor_value(&wb, cx), "//a\n//\n//b", "ignoreEmptyLines off: the empty line gets a marker too");
}

#[gpui_kit::test]
fn shift_alt_a_toggles_a_block_comment(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "block-comment", "a.rs", "foo bar");
    select(&wb, cx, (1, 1), (1, 4));
    cx.simulate_keystrokes("shift-alt-a");
    assert_eq!(editor_value(&wb, cx), "/* foo */ bar");
    cx.simulate_keystrokes("shift-alt-a");
    assert_eq!(editor_value(&wb, cx), "foo bar");
}

#[gpui_kit::test]
fn detect_indentation_picks_tabs_for_a_tab_indented_file(cx: &mut TestAppContext) {
    let text = "fn f() {\n\tlet a = 1;\n\tif a {\n\t\tb();\n\t}\n}\n";
    let (wb, cx) = setup(cx, "detect-tabs", "a.rs", text);
    let options = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| wb.read_with(cx, |wb, _| wb.editors.active_editor().unwrap().editor.options());
    assert!(!options(&wb, cx).insert_spaces, "tabs are detected");
    place_caret(&wb, cx, 6, 1);
    cx.simulate_keystrokes("tab");
    assert_eq!(editor_value(&wb, cx).lines().nth(5), Some("\t}"), "Tab types a tab character");
    cx.simulate_keystrokes("cmd-z");
    // Off: the configured tab size and spaces.
    set(cx, "editor.detectIndentation", json!(false));
    assert!(options(&wb, cx).insert_spaces);
    // And a file indented with 2 spaces is detected as such, when detection is on again.
    set(cx, "editor.detectIndentation", json!(true));
    let (wb2, cx2) = setup(cx, "detect-spaces", "b.rs", "a {\n  b;\n  c {\n    d;\n  }\n}\n");
    let o = options(&wb2, cx2);
    assert!(o.insert_spaces);
    assert_eq!(o.tab_size, 2);
}

#[gpui_kit::test]
fn trim_auto_whitespace_removes_the_indentation_an_enter_left_alone(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "trim-auto", "a.txt", "    x");
    set(cx, "editor.detectIndentation", json!(false));
    place_caret(&wb, cx, 1, 6);
    cx.simulate_keystrokes("enter enter");
    assert_eq!(editor_value(&wb, cx), "    x\n\n    ", "the indentation of the empty line was removed");
    set(cx, "editor.trimAutoWhitespace", json!(false));
    cx.simulate_keystrokes("cmd-a backspace");
    cx.simulate_input("    x");
    cx.simulate_keystrokes("enter enter");
    assert_eq!(editor_value(&wb, cx), "    x\n    \n    ", "with trimAutoWhitespace off it stays");
}

#[gpui_kit::test]
fn use_tab_stops_and_indent_size(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "tabstops", "a.txt", "      x");
    set(cx, "editor.detectIndentation", json!(false)); // one line says nothing about the indent size
    place_caret(&wb, cx, 1, 7);
    cx.simulate_keystrokes("backspace");
    assert_eq!(editor_value(&wb, cx), "    x", "backspace in indentation goes to the previous tab stop");
    set(cx, "editor.useTabStops", json!(false));
    cx.simulate_keystrokes("backspace");
    assert_eq!(editor_value(&wb, cx), "   x", "without tab stops it removes one space");
    set(cx, "editor.indentSize", json!(2));
    set(cx, "editor.useTabStops", json!(true));
    place_caret(&wb, cx, 1, 1);
    cx.simulate_keystrokes("tab");
    assert_eq!(editor_value(&wb, cx), "  " .to_string() + "   x", "Tab inserts an indent size of spaces");
}

#[gpui_kit::test]
fn trim_whitespace_on_delete_joins_lines_smartly(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "trim-delete", "a.txt", "a\n   b");
    place_caret(&wb, cx, 1, 2);
    cx.simulate_keystrokes("delete");
    assert_eq!(editor_value(&wb, cx), "a   b");
    cx.simulate_keystrokes("cmd-z");
    set(cx, "editor.trimWhitespaceOnDelete", json!(true));
    place_caret(&wb, cx, 1, 2);
    cx.simulate_keystrokes("delete");
    assert_eq!(editor_value(&wb, cx), "ab", "the indentation of the next line goes with the line break");
}

#[gpui_kit::test]
fn empty_selection_clipboard_copies_the_line_unless_off(cx: &mut TestAppContext) {
    let (wb, cx) = setup(cx, "clipboard", "a.txt", "one\ntwo\n");
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".to_string()));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("one\n"));
    set(cx, "editor.emptySelectionClipboard", json!(false));
    cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("before".to_string()));
    cx.simulate_keystrokes("cmd-c");
    assert_eq!(cx.read_from_clipboard().and_then(|c| c.text()).as_deref(), Some("before"), "nothing is copied");
    cx.simulate_keystrokes("cmd-x");
    assert_eq!(editor_value(&wb, cx), "one\ntwo\n", "and nothing is cut");
}
