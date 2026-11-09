//! Word-wise caret moves. Expectations are our own, written for the simplified
//! classifier documented in src/word_ops.rs (no upstream test is ported exactly here).
//! upstream: src/vs/editor/test/browser/controller/cursor.test.ts @ 50e1350fc78d

use vsr_editor::{Editor, EditorOptions, Position, Selection};
use vsr_text::{DefaultEol, TextBuffer};

fn editor(text: &str) -> Editor {
    Editor::new(
        TextBuffer::new(text, DefaultEol::Lf, false),
        EditorOptions { tab_size: 4, insert_spaces: true },
    )
}

fn caret(line: usize, column: usize) -> Selection {
    Selection::caret(Position::new(line, column))
}

#[test]
// own: word right stops at the start of the next word, skipping a run of separators' class
fn word_right_stops_at_next_word_start() {
    let mut ed = editor("foo bar.baz");
    ed.move_word_right(false);
    assert_eq!(ed.selection(), caret(1, 5));
    ed.move_word_right(false);
    assert_eq!(ed.selection(), caret(1, 8), "the separator run '.' is one step");
    ed.move_word_right(false);
    assert_eq!(ed.selection(), caret(1, 9));
    ed.move_word_right(false);
    assert_eq!(ed.selection(), caret(1, 12), "end of line");
}

#[test]
// own: word left goes back over whitespace and then to the start of the previous word
fn word_left_stops_at_previous_word_start() {
    let mut ed = editor("foo  bar");
    ed.set_cursor(Position::new(1, 9), false);
    ed.move_word_left(false);
    assert_eq!(ed.selection(), caret(1, 6));
    ed.move_word_left(false);
    assert_eq!(ed.selection(), caret(1, 1));
    ed.move_word_left(false);
    assert_eq!(ed.selection(), caret(1, 1));
}

#[test]
// own: word moves extend the selection when asked
fn word_right_with_extend_keeps_the_anchor() {
    let mut ed = editor("alpha beta");
    ed.move_word_right(true);
    assert_eq!(ed.selection(), Selection {
        anchor: Position::new(1, 1),
        active: Position::new(1, 7),
    });
}

#[test]
// own: word left at column 1 wraps to the previous line's end, like a character move
fn word_left_wraps_at_line_start() {
    let mut ed = editor("ab\ncd");
    ed.set_cursor(Position::new(2, 1), false);
    ed.move_word_left(false);
    assert_eq!(ed.selection(), caret(1, 3));
}
