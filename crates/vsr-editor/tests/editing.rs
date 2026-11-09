//! Typing, indentation, deletion, clipboard, undo/redo and dirty tracking.
//! Upstream-ported cases carry a `// upstream: <suite> > <test>` comment. The rest
//! are our own checks of behaviour the port documents (see the src/ module docs).
//! upstream: src/vs/editor/test/browser/controller/cursor.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/browser/controller/cursorMoveCommand.test.ts @ 50e1350fc78d

use vsr_editor::{Editor, EditorOptions, Selection};
use vsr_text::{DefaultEol, Position, TextBuffer};

fn editor(text: &str, tab_size: usize, insert_spaces: bool) -> Editor {
    Editor::new(
        TextBuffer::new(text, DefaultEol::Lf, false),
        EditorOptions { tab_size, insert_spaces },
    )
}

fn value(ed: &Editor) -> String {
    ed.buffer().value()
}

fn line(ed: &Editor, n: usize) -> String {
    ed.buffer().line_content(n).to_string()
}

fn caret(line: usize, column: usize) -> Selection {
    Selection { anchor: Position::new(line, column), active: Position::new(line, column) }
}

fn set(ed: &mut Editor, line: usize, column: usize) {
    ed.set_cursor(Position::new(line, column), false);
}

const LINE1: &str = "    \tMy First Line\t ";
const LINE2: &str = "\tMy Second Line";
const LINE3: &str = "    Third Line🐶";
const LINE4: &str = "";
const LINE5: &str = "1";

fn fixture() -> Editor {
    let text = format!("{LINE1}\r\n{LINE2}\n{LINE3}\n{LINE4}\r\n{LINE5}");
    editor(&text, 4, true)
}

// ---- tab -------------------------------------------------------------------

#[test]
// upstream: Editor Controller > issue #3463: pressing tab adds spaces, but not as many as for a tab
fn tab_after_tab_adds_spaces_to_next_visible_tab_stop() {
    let mut ed = editor("function a() {\n\tvar a = {\n\t\tx: 3\n\t};\n}", 4, true);
    set(&mut ed, 3, 2);
    ed.tab();
    assert_eq!(line(&ed, 3), "\t    \tx: 3");
}

#[test]
// upstream: Editor Controller > issue #4312: trying to type a tab character over a sequence of spaces results in unexpected behaviour
fn tab_over_selected_spaces_inserts_one_tab_without_insert_spaces() {
    let mut ed = editor(
        "var foo = 123;       // this is a comment\nvar bar = 4;       // another comment",
        4,
        false,
    );
    set(&mut ed, 1, 15);
    ed.set_cursor(Position::new(1, 22), true);
    ed.tab();
    assert_eq!(line(&ed, 1), "var foo = 123;\t// this is a comment");
}

#[test]
// upstream: Editor Controller > Cursor honors insertSpaces configuration on tab
fn tab_honors_tab_size_when_inserting_spaces() {
    let mut ed = editor(
        "    \tMy First Line\t \nMy Second Line123\n    Third Line\n\n1",
        13,
        true,
    );
    set(&mut ed, 2, 1);
    ed.tab();
    assert_eq!(line(&ed, 2), "             My Second Line123");
}

#[test]
// upstream: Editor Controller > Bug #16657: [editor] Tab on empty line of zero indentation moves cursor to position (1,1)
fn tab_on_empty_line_moves_cursor_past_the_tab() {
    let mut ed = editor(
        "function baz() {\n\tfunction hello() { // something here\n\t\n\n\t}\n}\n",
        4,
        false,
    );
    set(&mut ed, 7, 1);
    assert_eq!(ed.selection(), caret(7, 1));
    ed.tab();
    assert_eq!(line(&ed, 7), "\t");
    assert_eq!(ed.selection(), caret(7, 2));
}

// ---- enter -----------------------------------------------------------------

#[test]
// upstream: Editor Controller > Cursor honors insertSpaces configuration on new line
fn enter_indents_with_spaces_when_insert_spaces() {
    let mut ed = fixture();
    ed.set_cursor(Position::new(1, 21), false);
    ed.enter();
    assert_eq!(line(&ed, 1), LINE1);
    assert_eq!(line(&ed, 2), "        ");
    assert_eq!(ed.selection(), caret(2, 9));
}

#[test]
// upstream: Editor Controller > issue #148256: Pressing Enter creates line with bad indent with insertSpaces: true
fn enter_normalizes_indent_to_spaces() {
    let mut ed = editor("  \t", 4, true);
    set(&mut ed, 1, 4);
    ed.enter();
    assert_eq!(value(&ed), "  \t\n    ");
}

#[test]
// upstream: Editor Controller > issue #148256: Pressing Enter creates line with bad indent with insertSpaces: false
fn enter_normalizes_indent_to_tabs() {
    let mut ed = editor("  \t", 4, false);
    set(&mut ed, 1, 4);
    ed.enter();
    assert_eq!(value(&ed), "  \t\n\t");
}

#[test]
// own: Enter in leading whitespace takes only the indent before the caret (upstream _enter, no indent rules)
fn enter_in_leading_whitespace_keeps_only_the_indent_before_the_caret() {
    let mut ed = editor("    return true;", 4, true);
    set(&mut ed, 1, 3);
    ed.enter();
    assert_eq!(value(&ed), "  \n    return true;");
    assert_eq!(ed.selection(), caret(2, 3));
}

// ---- backspace / delete -----------------------------------------------------

#[test]
// upstream: Editor Controller > Backspace removes whitespaces with tab size
fn backspace_removes_whitespace_up_to_tab_stops() {
    let mut ed = editor(" \t \t     x\n        a    \n    ", 4, true);

    // DeleteLeft does not remove a tab stop when text exists before the cursor.
    set(&mut ed, 2, 14);
    ed.backspace();
    assert_eq!(line(&ed, 2), "        a   ");

    // DeleteLeft removes up to the previous tab stop (4) inside indentation.
    set(&mut ed, 2, 9);
    ed.backspace();
    assert_eq!(line(&ed, 2), "    a   ");

    ed.backspace();
    assert_eq!(line(&ed, 2), "a   ");

    // Two consecutive DeleteLefts form one undo step.
    assert!(ed.undo());
    assert_eq!(line(&ed, 2), "        a   ");

    // Nothing is broken when cursor is in (1,1).
    set(&mut ed, 1, 1);
    ed.backspace();
    assert_eq!(line(&ed, 1), " \t \t     x");

    // DeleteLeft stops at tab stops even in mixed whitespace.
    set(&mut ed, 1, 10);
    ed.backspace();
    assert_eq!(line(&ed, 1), " \t \t    x");
    ed.backspace();
    assert_eq!(line(&ed, 1), " \t \tx");
    ed.backspace();
    assert_eq!(line(&ed, 1), " \tx");
    ed.backspace();
    assert_eq!(line(&ed, 1), "x");

    // DeleteLeft on the last line, then across the line break.
    set(&mut ed, 3, 5);
    ed.backspace();
    assert_eq!(line(&ed, 3), "");
    ed.backspace();
    assert_eq!(value(&ed), "x\n        a   ");

    // A selection is deleted as selected.
    set(&mut ed, 2, 3);
    ed.set_cursor(Position::new(2, 4), true);
    ed.backspace();
    assert_eq!(line(&ed, 2), "       a   ");
}

#[test]
// upstream: Editor Controller > issue #99629: Emoji modifiers in text treated separately when using backspace (simplified: a surrogate pair is one unit)
fn backspace_removes_a_surrogate_pair_as_one_unit() {
    let mut ed = editor("ab🐶", 4, true);
    set(&mut ed, 1, 5);
    ed.backspace();
    assert_eq!(value(&ed), "ab");
    assert_eq!(ed.selection(), caret(1, 3));
}

#[test]
// own: Delete (forward) removes one unit and joins lines at the end of a line
fn delete_forward_removes_next_unit_and_joins_lines() {
    let mut ed = editor("a🐶\nb", 4, true);
    set(&mut ed, 1, 2);
    ed.delete();
    assert_eq!(value(&ed), "a\nb");
    set(&mut ed, 1, 2);
    ed.delete();
    assert_eq!(value(&ed), "ab");
    set(&mut ed, 1, 3);
    ed.delete();
    assert_eq!(value(&ed), "ab", "delete at the end of the document is a no-op");
}

// ---- typing -----------------------------------------------------------------

#[test]
// own: typing replaces the selection and leaves the caret after the inserted text
fn type_text_replaces_selection() {
    let mut ed = editor("hello world", 4, true);
    set(&mut ed, 1, 1);
    ed.set_cursor(Position::new(1, 6), true);
    ed.type_text("HEY\nyo");
    assert_eq!(value(&ed), "HEY\nyo world");
    assert_eq!(ed.selection(), caret(2, 3));
}

#[test]
// own: a surrogate pair advances the caret by two UTF-16 columns
fn type_text_counts_utf16_columns() {
    let mut ed = editor("", 4, true);
    ed.type_text("🐶x");
    assert_eq!(ed.selection(), caret(1, 4));
}

// ---- cut / paste / select ----------------------------------------------------

#[test]
// upstream: Editor Controller > bug #16740: [editor] Cut line doesn't quite cut the last line
fn cut_on_empty_selection_removes_the_line() {
    let mut ed = editor("asdasd\nqwerty", 4, true);
    set(&mut ed, 2, 1);
    ed.cut();
    assert_eq!(ed.buffer().line_count(), 1);
    assert_eq!(line(&ed, 1), "asdasd");

    let mut ed = editor("asdasd\n", 4, true);
    set(&mut ed, 2, 1);
    ed.cut();
    assert_eq!(ed.buffer().line_count(), 1);
    assert_eq!(line(&ed, 1), "asdasd");
    ed.cut();
    assert_eq!(ed.buffer().line_count(), 1);
    assert_eq!(line(&ed, 1), "");
}

#[test]
// own: selected text is returned by selected_text and removed by cut; paste inserts at the caret
fn selected_text_cut_and_paste() {
    let mut ed = editor("one two", 4, true);
    set(&mut ed, 1, 1);
    ed.set_cursor(Position::new(1, 4), true);
    assert_eq!(ed.selected_text(), "one");
    assert_eq!(ed.cut(), "one");
    assert_eq!(value(&ed), " two");
    ed.paste("ONE");
    assert_eq!(value(&ed), "ONE two");
    assert_eq!(ed.selection(), caret(1, 4));
}

// ---- undo / redo / dirty ----------------------------------------------------

#[test]
// own: consecutive typed characters are one undo step; redo restores them
fn consecutive_typing_is_one_undo_step() {
    let mut ed = editor("", 4, true);
    ed.type_text("a");
    ed.type_text("b");
    ed.type_text("c");
    assert!(ed.undo());
    assert_eq!(value(&ed), "");
    assert_eq!(ed.selection(), caret(1, 1));
    assert!(ed.redo());
    assert_eq!(value(&ed), "abc");
    assert_eq!(ed.selection(), caret(1, 4));
    assert!(!ed.redo());
}

#[test]
// own: a cursor move closes the typing group
fn cursor_move_breaks_typing_group() {
    let mut ed = editor("", 4, true);
    ed.type_text("ab");
    ed.move_left(false);
    ed.type_text("X");
    assert_eq!(value(&ed), "aXb");
    assert!(ed.undo());
    assert_eq!(value(&ed), "ab");
    assert!(ed.undo());
    assert_eq!(value(&ed), "");
    assert!(!ed.undo());
}

#[test]
// own: undo restores the selection from before the step and redo the one after it
fn undo_restores_selection_before_and_redo_after() {
    let mut ed = editor("abcdef", 4, true);
    set(&mut ed, 1, 2);
    ed.set_cursor(Position::new(1, 4), true);
    ed.type_text("X");
    assert_eq!(value(&ed), "aXdef");
    assert!(ed.undo());
    assert_eq!(value(&ed), "abcdef");
    assert_eq!(ed.selection(), Selection {
        anchor: Position::new(1, 2),
        active: Position::new(1, 4),
    });
    assert!(ed.redo());
    assert_eq!(value(&ed), "aXdef");
    assert_eq!(ed.selection(), caret(1, 3));
}

#[test]
// own: undo and redo of multi-line edits (enter, cut of a line) restore lines and carets
fn undo_redo_of_line_breaks_restores_lines() {
    let mut ed = editor("ab\ncd", 4, true);
    set(&mut ed, 1, 2);
    ed.enter();
    assert_eq!(value(&ed), "a\nb\ncd");
    assert!(ed.undo());
    assert_eq!(value(&ed), "ab\ncd");
    assert_eq!(ed.selection(), caret(1, 2));
    assert!(ed.redo());
    assert_eq!(value(&ed), "a\nb\ncd");
    assert_eq!(ed.selection(), caret(2, 1));

    set(&mut ed, 3, 1);
    assert_eq!(ed.cut(), "cd\n");
    assert_eq!(value(&ed), "a\nb");
    assert!(ed.undo());
    assert_eq!(value(&ed), "a\nb\ncd");
    assert_eq!(ed.selection(), caret(3, 1));
}

#[test]
// own: a new edit after undo discards the redo stack
fn new_edit_clears_redo() {
    let mut ed = editor("", 4, true);
    ed.type_text("a");
    ed.undo();
    ed.type_text("b");
    assert!(!ed.redo());
    assert_eq!(value(&ed), "b");
}

#[test]
// own: dirty is relative to the saved version; undoing back to the saved state is clean
fn dirty_tracks_the_saved_version() {
    let mut ed = editor("", 4, true);
    assert!(!ed.is_dirty());
    ed.type_text("a");
    assert!(ed.is_dirty());
    ed.mark_saved();
    assert!(!ed.is_dirty());
    ed.type_text("b");
    assert!(ed.is_dirty());
    ed.undo();
    assert!(!ed.is_dirty(), "undo back to the saved state is not dirty");
    ed.redo();
    assert!(ed.is_dirty());
}

#[test]
// own: after saving, undoing past the save point and then making a different edit stays dirty
fn dirty_after_undo_and_a_different_edit() {
    let mut ed = editor("", 4, true);
    ed.type_text("a");
    ed.mark_saved();
    ed.undo();
    ed.type_text("z");
    assert!(ed.is_dirty());
    assert_eq!(value(&ed), "z");
}

#[test]
// own: version changes with every content change, and returns to an earlier value on undo
fn version_returns_to_the_earlier_value_on_undo() {
    let mut ed = editor("", 4, true);
    let v0 = ed.version();
    ed.type_text("a");
    let v1 = ed.version();
    assert_ne!(v0, v1);
    set(&mut ed, 1, 1);
    assert_eq!(ed.version(), v1, "cursor moves do not change the version");
    ed.undo();
    assert_eq!(ed.version(), v0);
}
