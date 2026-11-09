//! Save-time edits (files.trimTrailingWhitespace, files.insertFinalNewline) and `set_options`.
//! Our own tests of the behaviour in saveParticipants.ts; none is ported 1:1.
//! upstream: src/vs/workbench/contrib/codeEditor/browser/saveParticipants.ts @ 50e1350fc78d

use vsr_editor::{Editor, EditorOptions, Position, Selection};
use vsr_text::{DefaultEol, TextBuffer};

fn editor(text: &str) -> Editor {
    Editor::new(TextBuffer::new(text, DefaultEol::Lf, false), EditorOptions { tab_size: 4, insert_spaces: true })
}

#[test]
fn trims_spaces_and_tabs_at_line_ends_only() {
    let mut ed = editor("a  \n  b\t \n   \nc\n\t\t");
    assert!(ed.trim_trailing_whitespace(&[]));
    assert_eq!(ed.buffer().value(), "a\n  b\n\nc\n");
    // Nothing left to trim.
    assert!(!ed.trim_trailing_whitespace(&[]));
}

#[test]
fn trim_is_one_undo_step_and_redo_restores_it() {
    let mut ed = editor("a  \nb \nc");
    ed.mark_saved();
    assert!(ed.trim_trailing_whitespace(&[]));
    assert_eq!(ed.buffer().value(), "a\nb\nc");
    assert!(ed.is_dirty());
    assert!(ed.undo());
    assert_eq!(ed.buffer().value(), "a  \nb \nc");
    assert!(!ed.is_dirty(), "undo returns to the saved state");
    assert!(!ed.undo(), "a single step");
    assert!(ed.redo());
    assert_eq!(ed.buffer().value(), "a\nb\nc");
}

#[test]
fn trim_keeps_the_selection_clamped_to_the_text() {
    let mut ed = editor("ab   \ncd");
    ed.set_cursor(Position::new(1, 6), false);
    ed.trim_trailing_whitespace(&[]);
    assert_eq!(ed.selection(), Selection::caret(Position::new(1, 3)));
}

#[test]
fn trim_with_cursor_keeps_whitespace_before_the_caret_line_end() {
    // Auto-save semantics: the caret on a line protects the whitespace before its column.
    let mut ed = editor("a   \nb   ");
    ed.trim_trailing_whitespace(&[Position::new(1, 3)]);
    assert_eq!(ed.buffer().value(), "a \nb");
    // Caret at the very end of the line: that line is left alone.
    let mut ed = editor("a   \nb   ");
    ed.trim_trailing_whitespace(&[Position::new(2, 5)]);
    assert_eq!(ed.buffer().value(), "a\nb   ");
}

#[test]
fn inserts_a_final_newline_using_the_buffer_eol() {
    let mut ed = editor("a\nb");
    assert!(ed.insert_final_newline());
    assert_eq!(ed.buffer().value(), "a\nb\n");
    // Already ends with a newline: the last line is empty.
    assert!(!ed.insert_final_newline());

    let mut crlf = Editor::new(TextBuffer::new("a\r\nb", DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    crlf.insert_final_newline();
    assert_eq!(crlf.buffer().value(), "a\r\nb\r\n");
}

#[test]
fn final_newline_is_skipped_for_empty_or_whitespace_last_line_and_undoable() {
    let mut ed = editor("a\n  ");
    assert!(!ed.insert_final_newline());
    assert_eq!(ed.buffer().value(), "a\n  ");

    let mut ed = editor("x");
    ed.set_cursor(Position::new(1, 2), false);
    ed.mark_saved();
    ed.insert_final_newline();
    assert_eq!(ed.selection(), Selection::caret(Position::new(1, 2)));
    ed.undo();
    assert_eq!(ed.buffer().value(), "x");
    assert!(!ed.is_dirty());
}

#[test]
fn set_options_changes_tab_behaviour() {
    let mut ed = editor("");
    ed.set_options(EditorOptions { tab_size: 2, insert_spaces: true });
    ed.tab();
    assert_eq!(ed.buffer().value(), "  ");
    ed.set_options(EditorOptions { tab_size: 0, insert_spaces: false });
    assert_eq!(ed.options().tab_size, 1);
    ed.tab();
    assert_eq!(ed.buffer().value(), "  \t");
}
