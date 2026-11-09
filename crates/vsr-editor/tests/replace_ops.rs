//! upstream: src/vs/editor/contrib/find/browser/replaceAllCommand.ts @ 50e1350fc78d
//!
//! `Editor::replace_ranges` (one undo step, selection tracking), `set_selection`, `word_at`.

use vsr_editor::{Editor, EditorOptions, Position, Range, Selection};
use vsr_text::{DefaultEol, TextBuffer};

fn editor(text: &str) -> Editor {
    Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true })
}

fn r(l1: usize, c1: usize, l2: usize, c2: usize) -> Range {
    Range::new(Position::new(l1, c1), Position::new(l2, c2))
}

#[test]
fn replace_ranges_is_one_undo_step_and_tracks_the_selection() {
    let mut ed = editor("foo bar foo\nfoo\n");
    ed.set_selection(Selection::caret(Position::new(2, 4))); // end of line 2
    let n = ed.replace_ranges(&[(r(2, 1, 2, 4), "x".into()), (r(1, 1, 1, 4), "quux".into()), (r(1, 9, 1, 12), "".into())]);
    assert_eq!(n, 3);
    assert_eq!(ed.buffer().value(), "quux bar \nx\n");
    assert_eq!(ed.selection().active, Position::new(2, 2), "caret follows the shrunk line");
    assert!(ed.undo());
    assert_eq!(ed.buffer().value(), "foo bar foo\nfoo\n");
    assert_eq!(ed.selection().active, Position::new(2, 4));
    assert!(!ed.undo(), "all edits were one step");
    assert!(ed.redo());
    assert_eq!(ed.buffer().value(), "quux bar \nx\n");
}

#[test]
fn replace_ranges_skips_noops_and_snaps_inside_points() {
    let mut ed = editor("abcdef");
    assert_eq!(ed.replace_ranges(&[(r(1, 2, 1, 4), "bc".into())]), 0);
    assert!(!ed.is_dirty());
    ed.set_selection(Selection::caret(Position::new(1, 3))); // inside "bcd"
    ed.replace_ranges(&[(r(1, 2, 1, 5), "XY".into())]);
    assert_eq!(ed.buffer().value(), "aXYef");
    assert_eq!(ed.selection().active, Position::new(1, 4), "snaps to the end of the replacement");
}

#[test]
fn replace_ranges_across_lines_and_crlf() {
    let mut ed = editor("a\r\nb\r\nc");
    ed.replace_ranges(&[(r(1, 2, 2, 1), "-".into())]); // the first line break
    assert_eq!(ed.buffer().value(), "a-b\r\nc");
    ed.undo();
    assert_eq!(ed.buffer().value(), "a\r\nb\r\nc");
}

#[test]
fn word_at_finds_the_word_touching_the_position() {
    let ed = editor("foo.bar  baz");
    assert_eq!(ed.word_at(Position::new(1, 2)), Some(r(1, 1, 1, 4)));
    assert_eq!(ed.word_at(Position::new(1, 4)), Some(r(1, 1, 1, 4)), "end of a word still touches it");
    assert_eq!(ed.word_at(Position::new(1, 5)), Some(r(1, 5, 1, 8)));
    assert_eq!(ed.word_at(Position::new(1, 9)), None, "between spaces");
    assert_eq!(ed.word_at(Position::new(1, 13)), Some(r(1, 10, 1, 13)));
}
