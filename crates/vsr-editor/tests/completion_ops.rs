//! upstream: src/vs/editor/contrib/suggest/browser/suggestController.ts @ 50e1350fc78d
//!
//! Behaviour: `_insertSuggestion`.
//!
//! `Editor::apply_completion`: the completion, additional edits and the caret as one undo step.

use vsr_editor::{Editor, EditorOptions, Position, Range, Selection};
use vsr_text::{DefaultEol, TextBuffer};

fn editor(text: &str) -> Editor {
    Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true })
}

fn r(l1: usize, c1: usize, l2: usize, c2: usize) -> Range {
    Range::new(Position::new(l1, c1), Position::new(l2, c2))
}

#[test]
fn accepting_is_one_undo_step_and_undo_restores_what_was_typed() {
    let mut ed = editor("fo");
    ed.set_selection(Selection::caret(Position::new(1, 3)));
    ed.type_text("o");
    assert_eq!(ed.apply_completion((r(1, 1, 1, 4), "foobar".into()), (6, 6), &[]), 1);
    assert_eq!(ed.buffer().value(), "foobar");
    assert_eq!(ed.selection().active, Position::new(1, 7));
    assert!(ed.undo());
    assert_eq!(ed.buffer().value(), "foo", "the typing is its own step");
    assert!(ed.redo());
    assert_eq!(ed.buffer().value(), "foobar");
    assert_eq!(ed.selection().active, Position::new(1, 7), "redo restores the caret");
}

#[test]
fn a_placeholder_is_selected_and_additional_edits_move_it() {
    let mut ed = editor("x\nfn");
    ed.set_selection(Selection::caret(Position::new(2, 3)));
    // `fn` becomes `fn name()` with `name` selected; a line is added above.
    let n = ed.apply_completion((r(2, 1, 2, 3), "fn name()".into(), ), (3, 7), &[(r(1, 1, 1, 1), "use a;\n".into())]);
    assert_eq!(n, 2);
    assert_eq!(ed.buffer().value(), "use a;\nx\nfn name()");
    let s = ed.selection();
    assert_eq!((s.anchor, s.active), (Position::new(3, 4), Position::new(3, 8)));
    assert!(ed.undo());
    assert_eq!(ed.buffer().value(), "x\nfn");
    assert_eq!(ed.selection().active, Position::new(2, 3));
}

#[test]
fn an_identical_completion_changes_nothing() {
    let mut ed = editor("foo");
    ed.set_selection(Selection::caret(Position::new(1, 4)));
    assert_eq!(ed.apply_completion((r(1, 1, 1, 4), "foo".into()), (3, 3), &[]), 0);
    assert!(!ed.is_dirty());
}

#[test]
fn a_multi_line_insert_puts_the_caret_in_the_new_text() {
    let mut ed = editor("  a");
    ed.set_selection(Selection::caret(Position::new(1, 4)));
    ed.apply_completion((r(1, 3, 1, 4), "if (x) {\n    \n  }".into()), (13, 13), &[]);
    assert_eq!(ed.buffer().value(), "  if (x) {\n    \n  }");
    assert_eq!(ed.selection().active, Position::new(2, 5));
}
