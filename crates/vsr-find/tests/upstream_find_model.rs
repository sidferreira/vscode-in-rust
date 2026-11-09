//! upstream: src/vs/editor/contrib/find/test/browser/findModel.test.ts @ 50e1350fc78d
//!
//! Ported with these adaptations (see `FindModel`'s docs): there are no decorations, so
//! `assertFindState` compares the model's matches and its highlighted (current) match; a
//! `moveTo` becomes `Editor::set_cursor`; `setValue` becomes a replace of the whole text.
//!
//! Not ported: "find model removes its decorations" (no decorations), the two `selectAllMatches`
//! tests (one cursor), "multi-selection" navigation is ported over `search_scope` ranges, and the
//! five "look ahed regex" replace tests plus the multiline look-ahead one (Rust `regex` has no
//! look-around, ADR-0023).

use vsr_editor::{Editor, EditorOptions, Position, Range};
use vsr_find::{FindModel, FindState};
use vsr_text::{DefaultEol, TextBuffer};

type R = (usize, usize, usize, usize);

const TEXT: [&str; 12] = [
    "// my cool header",
    "#include \"cool.h\"",
    "#include <iostream>",
    "",
    "int main() {",
    "    cout << \"hello world, Hello!\" << endl;",
    "    cout << \"hello world again\" << endl;",
    "    cout << \"Hello world again\" << endl;",
    "    cout << \"helloworld again\" << endl;",
    "}",
    "// blablablaciao",
    "",
];

fn editor_with(text: &str) -> Editor {
    Editor::new(TextBuffer::new(text, DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true })
}

fn fixture() -> Editor {
    editor_with(&TEXT.join("\n"))
}

fn rng((l1, c1, l2, c2): R) -> Range {
    Range::new(Position::new(l1, c1), Position::new(l2, c2))
}

fn from_range(r: Range) -> R {
    (r.start.line, r.start.column, r.end.line, r.end.column)
}

fn state(f: impl FnOnce(&mut FindState)) -> FindState {
    let mut s = FindState::default();
    f(&mut s);
    s
}

fn assert_find_state(ed: &Editor, model: &FindModel, cursor: R, highlighted: Option<R>, find_decorations: &[R]) {
    assert_eq!(from_range(ed.selection().range()), cursor, "cursor");
    assert_eq!(model.highlighted().map(from_range), highlighted, "highlighted");
    let all: Vec<R> = model.matches().iter().copied().map(from_range).collect();
    assert_eq!(all, find_decorations, "findDecorations");
}

fn line(ed: &Editor, n: usize) -> String {
    ed.buffer().line_content(n).to_string()
}

const HELLO_WHOLE: [R; 4] = [(6, 14, 6, 19), (6, 27, 6, 32), (7, 14, 7, 19), (8, 14, 8, 19)];
const HELLO_ALL: [R; 5] = [(6, 14, 6, 19), (6, 27, 6, 32), (7, 14, 7, 19), (8, 14, 8, 19), (9, 14, 9, 19)];

// upstream: FindModel > incremental find from beginning of file
#[test]
fn incremental_find_from_beginning_of_file() {
    let mut ed = fixture();
    ed.set_cursor(Position::new(1, 1), false);
    let mut m = FindModel::new(FindState::default(), &ed);

    // simulate typing the search string
    m.change(&mut ed, true, |s| s.search_string = "H".into());
    assert_find_state(
        &ed,
        &m,
        (1, 12, 1, 13),
        Some((1, 12, 1, 13)),
        &[(1, 12, 1, 13), (2, 16, 2, 17), (6, 14, 6, 15), (6, 27, 6, 28), (7, 14, 7, 15), (8, 14, 8, 15), (9, 14, 9, 15)],
    );

    m.change(&mut ed, true, |s| s.search_string = "He".into());
    assert_find_state(
        &ed,
        &m,
        (1, 12, 1, 14),
        Some((1, 12, 1, 14)),
        &[(1, 12, 1, 14), (6, 14, 6, 16), (6, 27, 6, 29), (7, 14, 7, 16), (8, 14, 8, 16), (9, 14, 9, 16)],
    );

    m.change(&mut ed, true, |s| s.search_string = "Hello".into());
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &HELLO_ALL);

    // simulate toggling on `matchCase`
    m.change(&mut ed, true, |s| s.match_case = true);
    assert_find_state(&ed, &m, (6, 27, 6, 32), Some((6, 27, 6, 32)), &[(6, 27, 6, 32), (8, 14, 8, 19)]);

    m.change(&mut ed, true, |s| s.search_string = "hello".into());
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &[(6, 14, 6, 19), (7, 14, 7, 19), (9, 14, 9, 19)]);

    // simulate toggling on `wholeWord`
    m.change(&mut ed, true, |s| s.whole_word = true);
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &[(6, 14, 6, 19), (7, 14, 7, 19)]);

    // simulate toggling off `matchCase`
    m.change(&mut ed, true, |s| s.match_case = false);
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &HELLO_WHOLE);

    // simulate toggling off `wholeWord`
    m.change(&mut ed, true, |s| s.whole_word = false);
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &HELLO_ALL);

    // simulate adding a search scope
    m.change(&mut ed, true, |s| s.search_scope = Some(vec![rng((8, 1, 10, 1))]));
    assert_find_state(&ed, &m, (8, 14, 8, 19), Some((8, 14, 8, 19)), &[(8, 14, 8, 19), (9, 14, 9, 19)]);

    // simulate removing the search scope
    m.change(&mut ed, true, |s| s.search_scope = None);
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &HELLO_ALL);
}

// upstream: FindModel > find model updates state matchesCount
#[test]
fn find_model_updates_state_matches_count() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| s.search_string = "hello".into()), &ed);
    assert_eq!(m.match_info().count, 5);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_ALL);

    m.change(&mut ed, false, |s| s.search_string = "helloo".into());
    assert_eq!(m.match_info().count, 0);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
}

// upstream: FindModel > find model reacts to position change
#[test]
fn find_model_reacts_to_position_change() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| s.search_string = "hello".into()), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_ALL);

    ed.set_cursor(Position::new(6, 20), false);
    m.sync(&ed);
    assert_find_state(&ed, &m, (6, 20, 6, 20), None, &HELLO_ALL);

    m.change(&mut ed, true, |s| s.search_string = "Hello".into());
    assert_find_state(&ed, &m, (6, 27, 6, 32), Some((6, 27, 6, 32)), &HELLO_ALL);
}

fn next_states(ed: &mut Editor, m: &mut FindModel, all: &[R], expected: &[R]) {
    for &e in expected {
        m.move_to_next_match(ed);
        assert_find_state(ed, m, e, Some(e), all);
    }
}

fn prev_states(ed: &mut Editor, m: &mut FindModel, all: &[R], expected: &[R]) {
    for &e in expected {
        m.move_to_prev_match(ed);
        assert_find_state(ed, m, e, Some(e), all);
    }
}

// upstream: FindModel > find model next
#[test]
fn find_model_next() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| (s.search_string, s.whole_word) = ("hello".into(), true)), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);
    next_states(&mut ed, &mut m, &HELLO_WHOLE, &[(6, 14, 6, 19), (6, 27, 6, 32), (7, 14, 7, 19), (8, 14, 8, 19), (6, 14, 6, 19)]);
}

// upstream: FindModel > find model next stays in scope
#[test]
fn find_model_next_stays_in_scope() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.whole_word = true;
        s.search_scope = Some(vec![rng((7, 1, 9, 1))]);
    });
    let mut m = FindModel::new(st, &ed);
    let all = [(7, 14, 7, 19), (8, 14, 8, 19)];
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &all);
    next_states(&mut ed, &mut m, &all, &[(7, 14, 7, 19), (8, 14, 8, 19), (7, 14, 7, 19)]);
}

// upstream: FindModel > multi-selection find model next stays in scope (overlap)
#[test]
fn multi_selection_find_model_next_stays_in_scope_overlap() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.whole_word = true;
        s.search_scope = Some(vec![rng((7, 1, 8, 2)), rng((8, 1, 9, 1))]);
    });
    let mut m = FindModel::new(st, &ed);
    let all = [(7, 14, 7, 19), (8, 14, 8, 19)];
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &all);
    next_states(&mut ed, &mut m, &all, &[(7, 14, 7, 19), (8, 14, 8, 19), (7, 14, 7, 19)]);
}

// upstream: FindModel > multi-selection find model next stays in scope
#[test]
fn multi_selection_find_model_next_stays_in_scope() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.match_case = true;
        s.search_scope = Some(vec![rng((6, 1, 7, 38)), rng((9, 3, 9, 38))]);
    });
    let mut m = FindModel::new(st, &ed);
    // `match_case: false` would find (6, 27, 6, 32) as well; `whole_word: true` would exclude (9, 14, 9, 19).
    let all = [(6, 14, 6, 19), (7, 14, 7, 19), (9, 14, 9, 19)];
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &all);
    next_states(&mut ed, &mut m, &all, &[(6, 14, 6, 19), (7, 14, 7, 19), (9, 14, 9, 19), (6, 14, 6, 19)]);
}

// upstream: FindModel > find model prev
#[test]
fn find_model_prev() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| (s.search_string, s.whole_word) = ("hello".into(), true)), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);
    prev_states(
        &mut ed,
        &mut m,
        &HELLO_WHOLE,
        &[(8, 14, 8, 19), (7, 14, 7, 19), (6, 27, 6, 32), (6, 14, 6, 19), (8, 14, 8, 19)],
    );
}

// upstream: FindModel > find model prev stays in scope
#[test]
fn find_model_prev_stays_in_scope() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.whole_word = true;
        s.search_scope = Some(vec![rng((7, 1, 9, 1))]);
    });
    let mut m = FindModel::new(st, &ed);
    let all = [(7, 14, 7, 19), (8, 14, 8, 19)];
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &all);
    prev_states(&mut ed, &mut m, &all, &[(8, 14, 8, 19), (7, 14, 7, 19), (8, 14, 8, 19)]);
}

// upstream: FindModel > find model next/prev with no matches
#[test]
fn find_model_next_prev_with_no_matches() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| (s.search_string, s.whole_word) = ("helloo".into(), true)), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    m.move_to_next_match(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    m.move_to_prev_match(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
}

// upstream: FindModel > find model next/prev respects cursor position
#[test]
fn find_model_next_prev_respects_cursor_position() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| (s.search_string, s.whole_word) = ("hello".into(), true)), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);
    ed.set_cursor(Position::new(6, 20), false);
    m.sync(&ed);
    assert_find_state(&ed, &m, (6, 20, 6, 20), None, &HELLO_WHOLE);
    m.move_to_next_match(&mut ed);
    assert_find_state(&ed, &m, (6, 27, 6, 32), Some((6, 27, 6, 32)), &HELLO_WHOLE);
}

fn regex_state(pattern: &str) -> FindState {
    state(|s| {
        s.search_string = pattern.into();
        s.is_regex = true;
    })
}

const LINE_STARTS: [R; 12] = [
    (1, 1, 1, 1),
    (2, 1, 2, 1),
    (3, 1, 3, 1),
    (4, 1, 4, 1),
    (5, 1, 5, 1),
    (6, 1, 6, 1),
    (7, 1, 7, 1),
    (8, 1, 8, 1),
    (9, 1, 9, 1),
    (10, 1, 10, 1),
    (11, 1, 11, 1),
    (12, 1, 12, 1),
];

const LINE_ENDS: [R; 12] = [
    (1, 18, 1, 18),
    (2, 18, 2, 18),
    (3, 20, 3, 20),
    (4, 1, 4, 1),
    (5, 13, 5, 13),
    (6, 43, 6, 43),
    (7, 41, 7, 41),
    (8, 41, 8, 41),
    (9, 40, 9, 40),
    (10, 2, 10, 2),
    (11, 17, 11, 17),
    (12, 1, 12, 1),
];

const WHOLE_LINES: [R; 12] = [
    (1, 1, 1, 18),
    (2, 1, 2, 18),
    (3, 1, 3, 20),
    (4, 1, 4, 1),
    (5, 1, 5, 13),
    (6, 1, 6, 43),
    (7, 1, 7, 41),
    (8, 1, 8, 41),
    (9, 1, 9, 40),
    (10, 1, 10, 2),
    (11, 1, 11, 17),
    (12, 1, 12, 1),
];

// upstream: FindModel > find ^
#[test]
fn find_caret() {
    let mut ed = fixture();
    let mut m = FindModel::new(regex_state("^"), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &LINE_STARTS);
    next_states(&mut ed, &mut m, &LINE_STARTS, &[(2, 1, 2, 1), (3, 1, 3, 1)]);
}

// upstream: FindModel > find $
#[test]
fn find_dollar() {
    let mut ed = fixture();
    let mut m = FindModel::new(regex_state("$"), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &LINE_ENDS);
    next_states(&mut ed, &mut m, &LINE_ENDS, &[(1, 18, 1, 18), (2, 18, 2, 18), (3, 20, 3, 20)]);
}

// upstream: FindModel > find next ^$
#[test]
fn find_next_caret_dollar() {
    let mut ed = fixture();
    let mut m = FindModel::new(regex_state("^$"), &ed);
    let all = [(4, 1, 4, 1), (12, 1, 12, 1)];
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &all);
    next_states(&mut ed, &mut m, &all, &[(4, 1, 4, 1), (12, 1, 12, 1), (4, 1, 4, 1)]);
}

// upstream: FindModel > find .*
#[test]
fn find_dot_star() {
    let ed = fixture();
    let m = FindModel::new(regex_state(".*"), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &WHOLE_LINES);
}

// upstream: FindModel > find next ^.*$
#[test]
fn find_next_caret_dot_star_dollar() {
    let mut ed = fixture();
    let mut m = FindModel::new(regex_state("^.*$"), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &WHOLE_LINES);
    next_states(&mut ed, &mut m, &WHOLE_LINES, &[(1, 1, 1, 18), (2, 1, 2, 18)]);
}

// upstream: FindModel > find prev ^.*$
#[test]
fn find_prev_caret_dot_star_dollar() {
    let mut ed = fixture();
    let mut m = FindModel::new(regex_state("^.*$"), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &WHOLE_LINES);
    prev_states(&mut ed, &mut m, &WHOLE_LINES, &[(12, 1, 12, 1), (11, 1, 11, 17)]);
}

// upstream: FindModel > find prev ^$
#[test]
fn find_prev_caret_dollar() {
    let mut ed = fixture();
    let mut m = FindModel::new(regex_state("^$"), &ed);
    let all = [(4, 1, 4, 1), (12, 1, 12, 1)];
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &all);
    prev_states(&mut ed, &mut m, &all, &[(12, 1, 12, 1), (4, 1, 4, 1), (12, 1, 12, 1)]);
}

// upstream: FindModel > replace hello
#[test]
fn replace_hello() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.replace_string = "hi".into();
        s.whole_word = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);

    ed.set_cursor(Position::new(6, 20), false);
    m.sync(&ed);
    assert_find_state(&ed, &m, (6, 20, 6, 20), None, &HELLO_WHOLE);
    assert_eq!(line(&ed, 6), "    cout << \"hello world, Hello!\" << endl;");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (6, 27, 6, 32), Some((6, 27, 6, 32)), &HELLO_WHOLE);
    assert_eq!(line(&ed, 6), "    cout << \"hello world, Hello!\" << endl;");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (7, 14, 7, 19), Some((7, 14, 7, 19)), &[(6, 14, 6, 19), (7, 14, 7, 19), (8, 14, 8, 19)]);
    assert_eq!(line(&ed, 6), "    cout << \"hello world, hi!\" << endl;");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (8, 14, 8, 19), Some((8, 14, 8, 19)), &[(6, 14, 6, 19), (8, 14, 8, 19)]);
    assert_eq!(line(&ed, 7), "    cout << \"hi world again\" << endl;");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (6, 14, 6, 19), Some((6, 14, 6, 19)), &[(6, 14, 6, 19)]);
    assert_eq!(line(&ed, 8), "    cout << \"hi world again\" << endl;");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (6, 16, 6, 16), None, &[]);
    assert_eq!(line(&ed, 6), "    cout << \"hi world, hi!\" << endl;");

    // Four of the five calls replaced something: four undo steps (the first one only selected).
    for _ in 0..4 {
        assert!(ed.undo());
    }
    assert!(!ed.undo());
    assert_eq!(line(&ed, 6), "    cout << \"hello world, Hello!\" << endl;");
}

// upstream: FindModel > replace bla
#[test]
fn replace_bla() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "bla".into();
        s.replace_string = "ciao".into();
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(11, 4, 11, 7), (11, 7, 11, 10), (11, 10, 11, 13)]);

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (11, 4, 11, 7), Some((11, 4, 11, 7)), &[(11, 4, 11, 7), (11, 7, 11, 10), (11, 10, 11, 13)]);
    assert_eq!(line(&ed, 11), "// blablablaciao");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (11, 8, 11, 11), Some((11, 8, 11, 11)), &[(11, 8, 11, 11), (11, 11, 11, 14)]);
    assert_eq!(line(&ed, 11), "// ciaoblablaciao");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (11, 12, 11, 15), Some((11, 12, 11, 15)), &[(11, 12, 11, 15)]);
    assert_eq!(line(&ed, 11), "// ciaociaoblaciao");

    m.replace(&mut ed);
    assert_find_state(&ed, &m, (11, 16, 11, 16), None, &[]);
    assert_eq!(line(&ed, 11), "// ciaociaociaociao");
}

// upstream: FindModel > replaceAll hello
#[test]
fn replace_all_hello() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.replace_string = "hi".into();
        s.whole_word = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);

    ed.set_cursor(Position::new(6, 20), false);
    m.sync(&ed);
    assert_find_state(&ed, &m, (6, 20, 6, 20), None, &HELLO_WHOLE);

    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (6, 17, 6, 17), None, &[]);
    assert_eq!(line(&ed, 6), "    cout << \"hi world, hi!\" << endl;");
    assert_eq!(line(&ed, 7), "    cout << \"hi world again\" << endl;");
    assert_eq!(line(&ed, 8), "    cout << \"hi world again\" << endl;");

    // One undo step restores all four.
    assert!(ed.undo());
    assert_eq!(line(&ed, 6), "    cout << \"hello world, Hello!\" << endl;");
    assert_eq!(line(&ed, 8), "    cout << \"Hello world again\" << endl;");
    assert!(!ed.undo());
}

// upstream: FindModel > replaceAll two spaces with one space
#[test]
fn replace_all_two_spaces_with_one_space() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "  ".into();
        s.replace_string = " ".into();
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(
        &ed,
        &m,
        (1, 1, 1, 1),
        None,
        &[(6, 1, 6, 3), (6, 3, 6, 5), (7, 1, 7, 3), (7, 3, 7, 5), (8, 1, 8, 3), (8, 3, 8, 5), (9, 1, 9, 3), (9, 3, 9, 5)],
    );
    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(6, 1, 6, 3), (7, 1, 7, 3), (8, 1, 8, 3), (9, 1, 9, 3)]);
    assert_eq!(line(&ed, 6), "  cout << \"hello world, Hello!\" << endl;");
    assert_eq!(line(&ed, 7), "  cout << \"hello world again\" << endl;");
    assert_eq!(line(&ed, 8), "  cout << \"Hello world again\" << endl;");
    assert_eq!(line(&ed, 9), "  cout << \"helloworld again\" << endl;");
}

// upstream: FindModel > replaceAll bla
#[test]
fn replace_all_bla() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "bla".into();
        s.replace_string = "ciao".into();
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(11, 4, 11, 7), (11, 7, 11, 10), (11, 10, 11, 13)]);
    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    assert_eq!(line(&ed, 11), "// ciaociaociaociao");
}

// upstream: FindModel > replaceAll bla with \t\n
#[test]
fn replace_all_bla_with_tab_and_newline() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "bla".into();
        s.replace_string = "<\\n\\t>".into();
        s.is_regex = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(11, 4, 11, 7), (11, 7, 11, 10), (11, 10, 11, 13)]);
    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    assert_eq!(line(&ed, 11), "// <");
    assert_eq!(line(&ed, 12), "\t><");
    assert_eq!(line(&ed, 13), "\t><");
    assert_eq!(line(&ed, 14), "\t>ciao");
}

// upstream: FindModel > issue #3516: "replace all" moves page/cursor/focus/scroll to the place of the last replacement
#[test]
fn issue_3516_replace_all_does_not_move_the_cursor() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "include".into();
        s.replace_string = "bar".into();
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(2, 2, 2, 9), (3, 2, 3, 9)]);
    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    assert_eq!(line(&ed, 2), "#bar \"cool.h\"");
    assert_eq!(line(&ed, 3), "#bar <iostream>");
}

// upstream: FindModel > listens to model content changes
#[test]
fn listens_to_model_content_changes() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.replace_string = "hi".into();
        s.whole_word = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);

    // `model.setValue('hello\nhi')`
    let full = ed.buffer().full_range();
    ed.replace_ranges(&[(full, "hello\nhi".to_string())]);
    ed.set_cursor(Position::new(1, 1), false);
    m.sync(&ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(1, 1, 1, 6)]);
}

// upstream: FindModel > issue #1914: NPE when there is only one find match
#[test]
fn issue_1914_only_one_find_match() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| s.search_string = "cool.h".into()), &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(2, 11, 2, 17)]);
    next_states(&mut ed, &mut m, &[(2, 11, 2, 17)], &[(2, 11, 2, 17), (2, 11, 2, 17)]);
}

// upstream: FindModel > replaceAll preserving case
#[test]
fn replace_all_preserving_case() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.replace_string = "goodbye".into();
        s.preserve_case = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_ALL);
    m.replace_all(&mut ed);
    assert_eq!(line(&ed, 6), "    cout << \"goodbye world, Goodbye!\" << endl;");
    assert_eq!(line(&ed, 7), "    cout << \"goodbye world again\" << endl;");
    assert_eq!(line(&ed, 8), "    cout << \"Goodbye world again\" << endl;");
    assert_eq!(line(&ed, 9), "    cout << \"goodbyeworld again\" << endl;");
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
}

// upstream: FindModel > issue #18711 replaceAll with empty string
#[test]
fn issue_18711_replace_all_with_empty_string() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.replace_string = String::new();
        s.whole_word = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &HELLO_WHOLE);
    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    assert_eq!(line(&ed, 6), "    cout << \" world, !\" << endl;");
    assert_eq!(line(&ed, 7), "    cout << \" world again\" << endl;");
    assert_eq!(line(&ed, 8), "    cout << \" world again\" << endl;");
}

// upstream: FindModel > issue #32522 replaceAll with ^ on more than 1000 matches
#[test]
fn issue_32522_replace_all_with_caret_on_more_than_1000_matches() {
    let initial: String = (0..1100).map(|i| format!("line{i}\n")).collect();
    let mut ed = editor_with(&initial);
    let st = state(|s| {
        s.search_string = "^".into();
        s.replace_string = "a ".into();
        s.is_regex = true;
    });
    let mut m = FindModel::new(st, &ed);
    m.replace_all(&mut ed);
    let mut expected: String = (0..1100).map(|i| format!("a line{i}\n")).collect();
    expected.push_str("a ");
    assert_eq!(ed.buffer().value(), expected);
}

// upstream: FindModel > issue #19740 Find and replace capture group/backreference inserts `undefined` instead of empty string
#[test]
fn issue_19740_unmatched_capture_group_is_empty() {
    let mut ed = fixture();
    let st = state(|s| {
        s.search_string = "hello(z)?".into();
        s.replace_string = "hi$1".into();
        s.is_regex = true;
        s.match_case = true;
    });
    let mut m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(6, 14, 6, 19), (7, 14, 7, 19), (9, 14, 9, 19)]);
    m.replace_all(&mut ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[]);
    assert_eq!(line(&ed, 6), "    cout << \"hi world, Hello!\" << endl;");
    assert_eq!(line(&ed, 7), "    cout << \"hi world again\" << endl;");
    assert_eq!(line(&ed, 9), "    cout << \"hiworld again\" << endl;");
}

// upstream: FindModel > issue #27083. search scope works even if it is a single line
#[test]
fn issue_27083_search_scope_single_line() {
    let ed = fixture();
    let st = state(|s| {
        s.search_string = "hello".into();
        s.whole_word = true;
        s.search_scope = Some(vec![rng((7, 1, 8, 1))]);
    });
    let m = FindModel::new(st, &ed);
    assert_find_state(&ed, &m, (1, 1, 1, 1), None, &[(7, 14, 7, 19)]);
}

fn assert_nav(m: &FindModel, position: usize, forward: bool, back: bool) {
    assert_eq!(m.match_info().position, position);
    assert_eq!(m.can_navigate_forward(), forward);
    assert_eq!(m.can_navigate_back(), back);
}

// upstream: FindModel > issue #3516: Control behavior of "Next" operations (not looping back to beginning)
#[test]
fn issue_3516_next_operations_not_looping() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| (s.search_string, s.loop_) = ("hello".into(), false)), &ed);
    assert_eq!(m.match_info().count, 5);

    // Test next operations
    assert_nav(&m, 0, true, true);
    for (pos, fwd, back) in [(1, true, false), (2, true, true), (3, true, true), (4, true, true), (5, false, true), (5, false, true), (5, false, true)] {
        m.move_to_next_match(&mut ed);
        assert_nav(&m, pos, fwd, back);
    }
    // Test previous operations
    for (pos, fwd, back) in [(4, true, true), (3, true, true), (2, true, true), (1, true, false), (1, true, false), (1, true, false)] {
        m.move_to_prev_match(&mut ed);
        assert_nav(&m, pos, fwd, back);
    }
}

// upstream: FindModel > issue #3516: Control behavior of "Next" operations (looping back to beginning)
#[test]
fn issue_3516_next_operations_looping() {
    let mut ed = fixture();
    let mut m = FindModel::new(state(|s| s.search_string = "hello".into()), &ed);
    assert_eq!(m.match_info().count, 5);

    assert_nav(&m, 0, true, true);
    for pos in [1, 2, 3, 4, 5, 1, 2] {
        m.move_to_next_match(&mut ed);
        assert_nav(&m, pos, true, true);
    }
    for pos in [1, 5, 4, 3, 2, 1] {
        m.move_to_prev_match(&mut ed);
        assert_nav(&m, pos, true, true);
    }
}

// upstream: FindModel > issue #288515: Wrong current index in find widget if matches > 1000
#[test]
fn issue_288515_wrong_current_index_if_matches_over_1000() {
    let text = vec!["hello"; 1001].join("\n");
    let mut ed = editor_with(&text);
    ed.set_cursor(Position::new(900, 1), false);
    ed.set_cursor(Position::new(900, 6), true);
    let m = FindModel::new(state(|s| s.search_string = "hello".into()), &ed);
    assert_eq!(m.match_info().count, 1001);
    // With the cursor selecting 'hello' at line 900, the position is 900.
    assert_eq!(m.match_info().position, 900);
}
