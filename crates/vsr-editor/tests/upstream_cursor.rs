//! Cursor movement behaviour, ported from upstream VS Code's cursor tests.
//! Each test carries a `// upstream: <suite> > <test>` comment naming its source.
//! upstream: src/vs/editor/test/browser/controller/cursor.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/browser/controller/cursorMoveCommand.test.ts @ 50e1350fc78d
//!
//! Fixture: the `Editor Controller - Cursor` document (mixed CRLF/LF, a surrogate pair
//! on line 3). Selections are written `sel((anchor), (active))`, like `Selection`'s
//! `(startLine, startColumn, endLine, endColumn)` in upstream, which is anchor then active.

use vsr_editor::{Editor, EditorOptions, Selection};
use vsr_text::{DefaultEol, Position, TextBuffer};

const LINE1: &str = "    \tMy First Line\t ";
const LINE2: &str = "\tMy Second Line";
const LINE3: &str = "    Third Line🐶";
const LINE4: &str = "";
const LINE5: &str = "1";

fn fixture() -> Editor {
    let text = format!("{LINE1}\r\n{LINE2}\n{LINE3}\n{LINE4}\r\n{LINE5}");
    Editor::new(
        TextBuffer::new(&text, DefaultEol::Lf, false),
        EditorOptions { tab_size: 4, insert_spaces: true },
    )
}

fn caret(line: usize, column: usize) -> Selection {
    sel((line, column), (line, column))
}

fn sel(anchor: (usize, usize), active: (usize, usize)) -> Selection {
    Selection {
        anchor: Position::new(anchor.0, anchor.1),
        active: Position::new(active.0, active.1),
    }
}

fn move_to(ed: &mut Editor, line: usize, column: usize) {
    ed.set_cursor(Position::new(line, column), false);
}

// ---- Editor Controller - Cursor -----------------------------------------------

#[test]
// upstream: Editor Controller - Cursor > move and then select
fn move_and_then_select() {
    let mut ed = fixture();
    move_to(&mut ed, 2, 3);
    assert_eq!(ed.selection(), caret(2, 3));
    ed.set_cursor(Position::new(2, 15), true);
    assert_eq!(ed.selection(), sel((2, 3), (2, 15)));
    ed.set_cursor(Position::new(1, 2), true);
    assert_eq!(ed.selection(), sel((2, 3), (1, 2)));
}

#[test]
// upstream: Editor Controller - Cursor > move left on top left position
fn move_left_on_top_left_position() {
    let mut ed = fixture();
    ed.move_left(false);
    assert_eq!(ed.selection(), caret(1, 1));
}

#[test]
// upstream: Editor Controller - Cursor > move left with surrogate pair
fn move_left_with_surrogate_pair() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 17);
    ed.move_left(false);
    assert_eq!(ed.selection(), caret(3, 15));
}

#[test]
// upstream: Editor Controller - Cursor > move left goes to previous row
fn move_left_goes_to_previous_row() {
    let mut ed = fixture();
    move_to(&mut ed, 2, 1);
    ed.move_left(false);
    assert_eq!(ed.selection(), caret(1, 21));
}

#[test]
// upstream: Editor Controller - Cursor > move left selection
fn move_left_selection() {
    let mut ed = fixture();
    move_to(&mut ed, 2, 1);
    ed.move_left(true);
    assert_eq!(ed.selection(), sel((2, 1), (1, 21)));
}

#[test]
// upstream: Editor Controller - Cursor > move right on bottom right position
fn move_right_on_bottom_right_position() {
    let mut ed = fixture();
    move_to(&mut ed, 5, 2);
    ed.move_right(false);
    assert_eq!(ed.selection(), caret(5, 2));
}

#[test]
// upstream: Editor Controller - Cursor > move right with surrogate pair
fn move_right_with_surrogate_pair() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 15);
    ed.move_right(false);
    assert_eq!(ed.selection(), caret(3, 17));
}

#[test]
// upstream: Editor Controller - Cursor > move right goes to next row
fn move_right_goes_to_next_row() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 21);
    ed.move_right(false);
    assert_eq!(ed.selection(), caret(2, 1));
}

#[test]
// upstream: Editor Controller - Cursor > move right selection
fn move_right_selection() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 21);
    ed.move_right(true);
    assert_eq!(ed.selection(), sel((1, 21), (2, 1)));
}

#[test]
// upstream: Editor Controller - Cursor > move down
fn move_down() {
    let mut ed = fixture();
    for expected in [(2, 1), (3, 1), (4, 1), (5, 1), (5, 2)] {
        ed.move_down(false);
        assert_eq!(ed.selection(), caret(expected.0, expected.1));
    }
}

#[test]
// upstream: Editor Controller - Cursor > move down with selection
fn move_down_with_selection() {
    let mut ed = fixture();
    for expected in [(2, 1), (3, 1), (4, 1), (5, 1), (5, 2)] {
        ed.move_down(true);
        assert_eq!(ed.selection(), sel((1, 1), expected));
    }
}

#[test]
// upstream: Editor Controller - Cursor > move down with tabs
fn move_down_with_tabs() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 5);
    for expected in [(2, 2), (3, 5), (4, 1), (5, 2)] {
        ed.move_down(false);
        assert_eq!(ed.selection(), caret(expected.0, expected.1));
    }
}

#[test]
// upstream: Editor Controller - Cursor > move up
fn move_up() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 5);
    ed.move_up(false);
    assert_eq!(ed.selection(), caret(2, 2));
    ed.move_up(false);
    assert_eq!(ed.selection(), caret(1, 5));
}

#[test]
// upstream: Editor Controller - Cursor > move up with selection
fn move_up_with_selection() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 5);
    ed.move_up(true);
    assert_eq!(ed.selection(), sel((3, 5), (2, 2)));
    ed.move_up(true);
    assert_eq!(ed.selection(), sel((3, 5), (1, 5)));
}

#[test]
// upstream: Editor Controller - Cursor > move up and down with tabs
fn move_up_and_down_with_tabs() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 5);
    for _ in 0..4 {
        ed.move_down(false);
    }
    assert_eq!(ed.selection(), caret(5, 2));
    for expected in [(4, 1), (3, 5), (2, 2), (1, 5)] {
        ed.move_up(false);
        assert_eq!(ed.selection(), caret(expected.0, expected.1));
    }
}

#[test]
// upstream: Editor Controller - Cursor > move up and down with end of lines starting from a long one
fn move_up_and_down_with_end_of_lines_starting_from_a_long_one() {
    let mut ed = fixture();
    ed.move_end(false);
    assert_eq!(ed.selection(), caret(1, 21));
    ed.move_end(false);
    assert_eq!(ed.selection(), caret(1, 21));
    for expected in [(2, 16), (3, 17), (4, 1), (5, 2)] {
        ed.move_down(false);
        assert_eq!(ed.selection(), caret(expected.0, expected.1));
    }
    for _ in 0..4 {
        ed.move_up(false);
    }
    assert_eq!(ed.selection(), caret(1, 21));
}

#[test]
// upstream: Editor Controller - Cursor > move to beginning of line (case: from within line)
fn move_to_beginning_of_line_from_within_line() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 8);
    ed.move_home(false);
    assert_eq!(ed.selection(), caret(1, 6));
    ed.move_home(false);
    assert_eq!(ed.selection(), caret(1, 1));
}

#[test]
// upstream: Editor Controller - Cursor > move to beginning of line (case: from whitespace at beginning of line)
fn move_to_beginning_of_line_from_whitespace_at_beginning_of_line() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 2);
    ed.move_home(false);
    assert_eq!(ed.selection(), caret(1, 6));
    ed.move_home(false);
    assert_eq!(ed.selection(), caret(1, 1));
}

#[test]
// upstream: Editor Controller - Cursor > move to beginning of line (case: within line selection)
fn move_to_beginning_of_line_within_line_selection() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 8);
    ed.move_home(true);
    assert_eq!(ed.selection(), sel((1, 8), (1, 6)));
    ed.move_home(true);
    assert_eq!(ed.selection(), sel((1, 8), (1, 1)));
}

#[test]
// upstream: Editor Controller - Cursor > move to beginning of line with selection multiline forward
fn move_to_beginning_of_line_with_selection_multiline_forward() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 8);
    ed.set_cursor(Position::new(3, 9), true);
    ed.move_home(false);
    assert_eq!(ed.selection(), caret(3, 5));
}

#[test]
// upstream: Editor Controller - Cursor > issue #17011: Shift+home/end now go to the end of the selection start's line, not the selection's end
fn shift_home_goes_to_the_start_of_the_active_line() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 8);
    ed.set_cursor(Position::new(3, 9), true);
    ed.move_home(true);
    assert_eq!(ed.selection(), sel((1, 8), (3, 5)));
}

#[test]
// upstream: Editor Controller - Cursor > move to end of line with selection multiline backward
fn move_to_end_of_line_with_selection_multiline_backward() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 9);
    ed.set_cursor(Position::new(1, 1), true);
    ed.move_end(false);
    assert_eq!(ed.selection(), caret(1, 21));
}

#[test]
// upstream: Editor Controller - Cursor > move to beginning of buffer
fn move_to_beginning_of_buffer_from_within_another_line_selection() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 3);
    ed.move_doc_start(true);
    assert_eq!(ed.selection(), sel((3, 3), (1, 1)));
}

#[test]
// upstream: Editor Controller - Cursor > move to end of buffer
fn move_to_end_of_buffer_from_within_another_line_selection() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 3);
    ed.move_doc_end(true);
    assert_eq!(ed.selection(), sel((3, 3), (5, 2)));
    ed.move_doc_end(false);
    assert_eq!(ed.selection(), caret(5, 2));
}

#[test]
// upstream: Editor Controller - Cursor > select all
fn select_all() {
    let mut ed = fixture();
    ed.select_all();
    assert_eq!(ed.selection(), sel((1, 1), (5, 2)));
}

// ---- Cursor move command test ------------------------------------------------

#[test]
// upstream: Cursor move command test > move left should move to left by n characters
fn move_left_moves_several_characters() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 8);
    for _ in 0..3 {
        ed.move_left(false);
    }
    assert_eq!(ed.selection(), caret(1, 5));
}

#[test]
// upstream: Cursor move command test > move right should move to right by n characters
fn move_right_moves_several_characters() {
    let mut ed = fixture();
    move_to(&mut ed, 1, 2);
    for _ in 0..6 {
        ed.move_right(false);
    }
    assert_eq!(ed.selection(), caret(1, 8));
}

#[test]
// upstream: Cursor move command test > move up by model line cursor move command
fn move_up_by_model_line_cursor_move_command() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 5);
    ed.page_up(2, false);
    assert_eq!(ed.selection(), caret(1, 5));
    ed.page_up(1, false);
    assert_eq!(ed.selection(), caret(1, 1));
}

#[test]
// upstream: Cursor move command test > move down by model line cursor move command
fn move_down_by_model_line_cursor_move_command() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 5);
    ed.page_down(2, false);
    assert_eq!(ed.selection(), caret(5, 2));
    ed.page_down(1, false);
    assert_eq!(ed.selection(), caret(5, 2));
}

#[test]
// upstream: Cursor move command test > move up with selection by cursor move command
fn move_up_with_selection_by_cursor_move_command() {
    let mut ed = fixture();
    move_to(&mut ed, 3, 5);
    ed.move_up(true);
    assert_eq!(ed.selection(), sel((3, 5), (2, 2)));
    ed.move_up(true);
    assert_eq!(ed.selection(), sel((3, 5), (1, 5)));
}
