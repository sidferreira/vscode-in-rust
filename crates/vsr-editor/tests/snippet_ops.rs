//! The editor side of a snippet session (ADR-0120): the change log of the buffer, the mirror edits
//! joined to the typed edit's undo step, and the completion's insertion start.
//!
//! upstream: src/vs/editor/contrib/snippet/browser/snippetSession.ts @ 50e1350fc78d

use vsr_editor::{Editor, EditorOptions, Position, Range, Selection};
use vsr_text::{DefaultEol, TextBuffer, TextChange};

fn editor(text: &str) -> Editor {
    Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true })
}

#[test]
fn the_change_log_follows_typing_and_undo() {
    let mut ed = editor("ab");
    ed.track_changes(true);
    ed.set_selection(Selection::caret(Position::new(1, 2)));
    ed.type_text("X");
    assert_eq!(ed.take_changes(), vec![TextChange { offset: 1, removed: 0, inserted: 1 }]);
    assert!(ed.undo());
    assert_eq!(ed.take_changes(), vec![TextChange { offset: 1, removed: 1, inserted: 0 }]);
}

#[test]
fn mirrors_undo_with_the_typed_character() {
    // "ab" with stop 1 covering "a" at 0..1, and a mirror of it at 1..2 (the "b" stands in for it).
    let mut ed = editor("ab");
    ed.set_selection(Selection::caret(Position::new(1, 2)));
    ed.type_text("q");
    // The user typed "q" at the end of the placeholder (offset 1); the mirror at 2..3 is rewritten.
    let edits = vec![(2usize, 3usize, "q".to_string())];
    assert_eq!(ed.replace_offsets_joined(&edits), 1);
    assert_eq!(ed.buffer().value(), "aqq");
    assert!(ed.undo());
    assert_eq!(ed.buffer().value(), "ab", "one undo reverts the typed character and its mirror edit");
}

#[test]
fn a_mirror_before_the_caret_moves_the_caret() {
    let mut ed = editor("xy");
    ed.set_selection(Selection::caret(Position::new(1, 3)));
    // Replace the "x" at 0..1 with "long": the caret after "y" keeps its place after the text.
    assert_eq!(ed.replace_offsets_joined(&[(0, 1, "long".to_string())]), 1);
    assert_eq!(ed.buffer().value(), "longy");
    assert_eq!(ed.selection().active, Position::new(1, 6));
}

#[test]
fn without_a_last_step_the_mirrors_are_a_step_of_their_own() {
    let mut ed = editor("ab");
    assert_eq!(ed.replace_offsets_joined(&[(0, 1, "Z".to_string())]), 1);
    assert_eq!(ed.buffer().value(), "Zb");
    assert!(ed.undo());
    assert_eq!(ed.buffer().value(), "ab");
}

#[test]
fn no_edits_change_nothing() {
    let mut ed = editor("ab");
    assert_eq!(ed.replace_offsets_joined(&[]), 0);
    assert!(!ed.undo());
}

#[test]
fn the_completion_reports_where_its_text_starts() {
    let mut ed = editor("x\nfo");
    ed.set_selection(Selection::caret(Position::new(2, 3)));
    let main = (Range::new(Position::new(2, 1), Position::new(2, 3)), "foobar".to_string());
    // An additional edit before the text (an import line) moves the start by its length.
    let extra = [(Range::new(Position::new(1, 1), Position::new(1, 1)), "use a;\n".to_string())];
    let (n, start) = ed.apply_completion_at(main, (6, 6), &extra);
    assert_eq!(n, 2);
    assert_eq!(start, 9, "the 7-unit import line and the 2-unit \"x\\n\" come before it");
    assert_eq!(ed.buffer().value(), "use a;\nx\nfoobar");
}
