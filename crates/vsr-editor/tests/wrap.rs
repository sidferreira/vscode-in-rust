//! Word wrap in the editing core: the projection stays in step with edits, and the cursor moves
//! by view lines. Own tests (the upstream ports are in `upstream_monospace_line_breaks.rs` and
//! `upstream_model_line_projection.rs`). Expectations follow upstream's view-line cursor rules.
//! upstream: src/vs/editor/common/cursor/cursorMoveCommands.ts @ 50e1350fc78d

use vsr_editor::wrap::{PositionAffinity, ViewLines, WrapParams, WrappingIndent};
use vsr_editor::{Editor, EditorOptions, Position};
use vsr_text::{DefaultEol, TextBuffer};

fn editor(text: &str, column: i64, indent: WrappingIndent) -> Editor {
    let e = Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    e.set_wrapping(Some(WrapParams::monospace(4, column, indent)));
    e
}

fn pos(e: &Editor) -> (usize, usize) {
    let p = e.selection().active;
    (p.line, p.column)
}

const LONG: &str = "aaaa bbbb cccc dddd eeee ffff gggg";

#[test]
fn view_line_count_follows_the_column() {
    let e = editor("short\nlong line here that wraps\nx", 10, WrappingIndent::None);
    assert_eq!(e.view_line_count(), 5);
    e.set_wrapping(Some(WrapParams::monospace(4, 80, WrappingIndent::None)));
    assert_eq!(e.view_line_count(), 3);
    e.set_wrapping(None);
    assert!(!e.is_wrapping());
    assert_eq!(e.view_line_count(), 3);
}

#[test]
fn incremental_updates_match_a_fresh_projection() {
    let mut e = editor(&format!("{LONG}\nmid\n{LONG}\n{LONG}"), 12, WrappingIndent::Same);
    let fresh = |e: &Editor| {
        let mut v = ViewLines::new();
        v.sync(e.buffer(), &WrapParams::monospace(4, 12, WrappingIndent::Same));
        (0..v.view_line_count()).map(|i| v.view_line_content(e.buffer(), i)).collect::<Vec<_>>()
    };
    let current = |e: &Editor| {
        let v = e.view_lines().unwrap();
        (0..v.view_line_count()).map(|i| v.view_line_content(e.buffer(), i)).collect::<Vec<_>>()
    };
    assert_eq!(current(&e), fresh(&e));
    e.set_cursor(Position::new(2, 4), false);
    e.type_text(" now much longer than before");
    assert_eq!(current(&e), fresh(&e));
    e.set_cursor(Position::new(1, 1), false);
    e.enter();
    e.enter();
    assert_eq!(current(&e), fresh(&e));
    e.undo();
    assert_eq!(current(&e), fresh(&e));
    e.undo();
    assert_eq!(current(&e), fresh(&e));
    e.set_cursor(Position::new(4, 1), false);
    e.select_all();
    e.type_text("x");
    assert_eq!(current(&e), fresh(&e));
    assert_eq!(e.view_line_count(), 1);
}

#[test]
fn down_moves_within_a_wrapped_line_and_keeps_the_desired_column() {
    // "aaaa bbbb |cccc dddd |eeee ffff |gggg" at 10 columns
    let mut e = editor(LONG, 10, WrappingIndent::None);
    assert_eq!(e.view_line_count(), 4);
    e.set_cursor(Position::new(1, 3), false);
    e.move_down(false);
    assert_eq!(pos(&e), (1, 13)); // second view line, same visible column
    e.move_down(false);
    assert_eq!(pos(&e), (1, 23));
    e.move_up(false);
    e.move_up(false);
    assert_eq!(pos(&e), (1, 3));
    // up from the first view line goes to the start of the document
    e.move_up(false);
    assert_eq!(pos(&e), (1, 1));
}

#[test]
fn down_crosses_into_the_next_model_line() {
    let mut e = editor("aaaa bbbb cccc\nzz", 10, WrappingIndent::None);
    e.set_cursor(Position::new(1, 12), false); // second view line
    e.move_down(false);
    assert_eq!(pos(&e), (2, 2)); // desired visible column 2 on "zz"
}

#[test]
fn continuation_indent_counts_for_the_desired_column() {
    // two spaces of indent, "same": the continuation starts at visible column 2
    let mut e = editor("  aaaa bbbb cccc dddd", 10, WrappingIndent::Same);
    e.set_cursor(Position::new(1, 5), false); // visible column 4
    e.move_down(false);
    let v = e.view_lines().unwrap();
    let (vline, vcol) = v.view_position_of(e.selection().active, PositionAffinity::None);
    drop(v);
    assert_eq!(vline, 1);
    assert_eq!(vcol, 5, "visible column 4 on the continuation line (indent included)");
}

#[test]
fn home_and_end_go_to_the_view_line_then_the_model_line() {
    let mut e = editor(LONG, 10, WrappingIndent::None);
    e.set_cursor(Position::new(1, 15), false); // second view line
    e.move_home(false);
    assert_eq!(pos(&e), (1, 11)); // start of the view line
    e.move_home(false);
    assert_eq!(pos(&e), (1, 1)); // then the model line
    e.set_cursor(Position::new(1, 3), false);
    e.move_end(false);
    assert_eq!(pos(&e), (1, 11)); // end of the first view line ...
    assert!(e.caret_trailing(), "... drawn at its end, not at the start of the next");
    assert_eq!(e.caret_view_position(), Some((0, 11)));
    e.move_end(false);
    assert_eq!(pos(&e), (1, 35)); // End again, already at the end of the view line: the model line's end
    assert!(!e.caret_trailing());
    e.move_up(false);
    assert_eq!(pos(&e), (1, 31), "the sticky end column: up lands at the end of the previous view line");
    assert!(e.caret_trailing());
}

#[test]
fn page_moves_count_view_lines() {
    let mut e = editor(LONG, 10, WrappingIndent::None);
    e.page_down(2, false);
    assert_eq!(pos(&e), (1, 21));
    e.page_up(1, false);
    assert_eq!(pos(&e), (1, 11));
}

#[test]
fn no_wrapping_means_model_lines() {
    let mut e = editor(LONG, 10, WrappingIndent::None);
    e.set_wrapping(None);
    e.set_cursor(Position::new(1, 3), false);
    e.move_down(false);
    assert_eq!(pos(&e), (1, 35), "last line: down goes to its end");
}

#[test]
fn word_separators_change_word_moves() {
    let mut e = Editor::new(TextBuffer::new("foo.bar baz", DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    e.move_word_right(false);
    assert_eq!(pos(&e), (1, 4), "'.' separates by default");
    e.set_word_separators("");
    e.set_cursor(Position::new(1, 1), false);
    e.move_word_right(false);
    assert_eq!(pos(&e), (1, 9), "no separators: 'foo.bar' is one word");
    e.set_word_separators("`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?");
    e.set_cursor(Position::new(1, 1), false);
    e.move_word_right(false);
    assert_eq!(pos(&e), (1, 4));
}
