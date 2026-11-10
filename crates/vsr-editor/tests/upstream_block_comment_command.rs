//! upstream: src/vs/editor/contrib/comment/test/browser/blockCommentCommand.test.ts @ 50e1350fc78d
//!
//! Ports of the block comment command tests, run through `Editor::block_comment` with the
//! language `{ lineComment: '!@#', blockComment: ['<0', '0>'] }`.

use std::sync::Arc;

use vsr_editor::{Editor, EditorOptions, Position, Selection};
use vsr_languages::config::{CommentRule, LanguageConfiguration, LanguageSupport};
use vsr_text::{DefaultEol, TextBuffer};

fn sel(a: usize, b: usize, c: usize, d: usize) -> Selection {
    Selection { anchor: Position::new(a, b), active: Position::new(c, d) }
}

fn run(insert_space: bool, lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    let mut e = Editor::new(TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    let rule = CommentRule { line_comment: Some("!@#".into()), line_comment_no_indent: false, block_comment: Some(("<0".into(), "0>".into())) };
    e.set_language_support(Some(Arc::new(LanguageSupport::new(LanguageConfiguration { comments: Some(rule), ..Default::default() }))));
    e.set_selection(selection);
    e.block_comment(insert_space);
    assert_eq!(e.buffer().value(), expected.join("\n"));
    assert_eq!(e.selection(), expected_selection);
}

fn test_block_comment(lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    run(true, lines, selection, expected, expected_selection);
}

const TEXT5: [&str; 5] = ["first", "\tsecond line", "third line", "fourth line", "fifth"];

// upstream: Editor Contrib - Block Comment Command > empty selection wraps itself
#[test]
fn empty_selection_wraps_itself() {
    test_block_comment(&TEXT5, sel(1, 3, 1, 3), &["fi<0  0>rst", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 6, 1, 6));
}

// upstream: Editor Contrib - Block Comment Command > invisible selection ignored
#[test]
fn invisible_selection_ignored() {
    test_block_comment(&TEXT5, sel(2, 1, 1, 1), &["<0 first", " 0>\tsecond line", "third line", "fourth line", "fifth"], sel(1, 4, 2, 1));
}

// upstream: Editor Contrib - Block Comment Command > bug9511
#[test]
fn bug9511() {
    test_block_comment(&TEXT5, sel(1, 6, 1, 1), &["<0 first 0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 4, 1, 9));
    test_block_comment(&["<0first0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 8, 1, 3), &TEXT5, sel(1, 1, 1, 6));
}

// upstream: Editor Contrib - Block Comment Command > one line selection
#[test]
fn one_line_selection() {
    test_block_comment(&TEXT5, sel(1, 6, 1, 3), &["fi<0 rst 0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 6, 1, 9));
}

// upstream: Editor Contrib - Block Comment Command > one line selection toggle
#[test]
fn one_line_selection_toggle() {
    test_block_comment(&TEXT5, sel(1, 6, 1, 3), &["fi<0 rst 0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 6, 1, 9));
    test_block_comment(&["fi<0rst0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 8, 1, 5), &TEXT5, sel(1, 3, 1, 6));
    test_block_comment(&["<0 first 0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 10, 1, 1), &TEXT5, sel(1, 1, 1, 6));
    test_block_comment(&["<0 first0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 9, 1, 1), &TEXT5, sel(1, 1, 1, 6));
    test_block_comment(&["<0first 0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 9, 1, 1), &TEXT5, sel(1, 1, 1, 6));
    test_block_comment(&["fi<0rst0>", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 8, 1, 5), &TEXT5, sel(1, 3, 1, 6));
}

// upstream: Editor Contrib - Block Comment Command > multi line selection
#[test]
fn multi_line_selection() {
    test_block_comment(&TEXT5, sel(2, 4, 1, 1), &["<0 first", "\tse 0>cond line", "third line", "fourth line", "fifth"], sel(1, 4, 2, 4));
}

// upstream: Editor Contrib - Block Comment Command > multi line selection toggle
#[test]
fn multi_line_selection_toggle() {
    test_block_comment(&TEXT5, sel(2, 4, 1, 1), &["<0 first", "\tse 0>cond line", "third line", "fourth line", "fifth"], sel(1, 4, 2, 4));
    test_block_comment(&["<0first", "\tse0>cond line", "third line", "fourth line", "fifth"], sel(2, 4, 1, 3), &TEXT5, sel(1, 1, 2, 4));
    test_block_comment(&["<0 first", "\tse0>cond line", "third line", "fourth line", "fifth"], sel(2, 4, 1, 3), &TEXT5, sel(1, 1, 2, 4));
    test_block_comment(&["<0first", "\tse 0>cond line", "third line", "fourth line", "fifth"], sel(2, 4, 1, 3), &TEXT5, sel(1, 1, 2, 4));
    test_block_comment(&["<0 first", "\tse 0>cond line", "third line", "fourth line", "fifth"], sel(2, 4, 1, 3), &TEXT5, sel(1, 1, 2, 4));
}

// upstream: Editor Contrib - Block Comment Command > fuzzy removes
#[test]
fn fuzzy_removes() {
    for s in [sel(2, 5, 1, 7), sel(2, 5, 1, 6), sel(2, 5, 1, 5), sel(2, 5, 1, 11), sel(2, 1, 1, 11), sel(2, 7, 1, 11)] {
        test_block_comment(&["asd <0 qwe", "asd 0> qwe"], s, &["asd qwe", "asd qwe"], sel(1, 5, 2, 4));
    }
}

// upstream: Editor Contrib - Block Comment Command > bug #30358
#[test]
fn bug_30358() {
    test_block_comment(&["<0 start 0> middle end"], sel(1, 20, 1, 23), &["<0 start 0> middle <0 end 0>"], sel(1, 23, 1, 26));
    test_block_comment(&["<0 start 0> middle <0 end 0>"], sel(1, 13, 1, 19), &["<0 start 0> <0 middle 0> <0 end 0>"], sel(1, 16, 1, 22));
}

// upstream: Editor Contrib - Block Comment Command > issue #34618
#[test]
fn issue_34618() {
    test_block_comment(&["<0  0> middle end"], sel(1, 4, 1, 4), &[" middle end"], sel(1, 1, 1, 1));
}

// upstream: Editor Contrib - Block Comment Command > insertSpace false
#[test]
fn insert_space_false() {
    run(false, &["some text"], sel(1, 1, 1, 5), &["<0some0> text"], sel(1, 3, 1, 7));
}

// upstream: Editor Contrib - Block Comment Command > insertSpace false does not remove space
#[test]
fn insert_space_false_does_not_remove_space() {
    run(false, &["<0 some 0> text"], sel(1, 4, 1, 8), &[" some  text"], sel(1, 1, 1, 7));
}
