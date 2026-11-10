//! upstream: src/vs/editor/contrib/comment/test/browser/lineCommentCommand.test.ts @ 50e1350fc78d
//!
//! Ports of the line comment command tests, run through `Editor::line_comment`. The harness is
//! upstream's `testCommand`: lines, a selection, the command, the expected lines and selection
//! (`Selection(anchorLine, anchorColumn, activeLine, activeColumn)`). Not ported: the
//! "Line Comment in mixed modes" suite (embedded languages: JSX in a JavaScript file).

use std::sync::Arc;

use vsr_editor::comments::{CommentType, LinePreflight, analyze_lines, haystack_has_needle_at_offset, normalize_insertion_point};
use vsr_editor::{Editor, EditorOptions, Position, Selection};
use vsr_languages::config::{CommentRule, LanguageConfiguration, LanguageSupport};
use vsr_text::{DefaultEol, TextBuffer};

fn sel(a: usize, b: usize, c: usize, d: usize) -> Selection {
    Selection { anchor: Position::new(a, b), active: Position::new(c, d) }
}

fn comments(line: Option<&str>, no_indent: bool, block: Option<(&str, &str)>) -> CommentRule {
    CommentRule { line_comment: line.map(String::from), line_comment_no_indent: no_indent, block_comment: block.map(|(a, b)| (a.to_string(), b.to_string())) }
}

#[allow(clippy::too_many_arguments)]
fn run(rule: &CommentRule, lines: &[&str], selection: Selection, kind: CommentType, insert_space: bool, ignore_empty: bool, expected: &[&str], expected_selection: Selection) {
    let mut e = Editor::new(TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    e.set_language_support(Some(Arc::new(LanguageSupport::new(LanguageConfiguration { comments: Some(rule.clone()), ..Default::default() }))));
    e.set_selection(selection);
    e.line_comment(kind, insert_space, ignore_empty);
    assert_eq!(e.buffer().value(), expected.join("\n"));
    assert_eq!(e.selection(), expected_selection);
}

fn standard() -> CommentRule {
    comments(Some("!@#"), false, Some(("<!@#", "#@!>")))
}

fn toggle(lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    run(&standard(), lines, selection, CommentType::Toggle, true, true, expected, expected_selection);
}

fn add(lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    run(&standard(), lines, selection, CommentType::ForceAdd, true, true, expected, expected_selection);
}

fn simple_model(lines: &'static [&'static str]) -> impl Fn(usize) -> String {
    move |n| lines[n - 1].to_string()
}

fn basic_preflight(tokens: &[&str]) -> Vec<LinePreflight> {
    tokens.iter().map(|t| LinePreflight { ignore: false, comment_str: t.to_string(), comment_str_offset: 0, comment_str_length: t.len() }).collect()
}

const TEXT5: [&str; 5] = ["first", "\tsecond line", "third line", "fourth line", "fifth"];

// upstream: Editor Contrib - Line Comment Command > comment single line
#[test]
fn comment_single_line() {
    toggle(&["some text", "\tsome more text"], sel(1, 1, 1, 1), &["!@# some text", "\tsome more text"], sel(1, 5, 1, 5));
}

// upstream: Editor Contrib - Line Comment Command > case insensitive
#[test]
fn case_insensitive() {
    run(&comments(Some("rem"), false, None), &["REM some text"], sel(1, 1, 1, 1), CommentType::Toggle, true, true, &["some text"], sel(1, 1, 1, 1));
}

// upstream: Editor Contrib - Line Comment Command > comment with token column fixed
#[test]
fn comment_with_token_column_fixed() {
    run(
        &comments(Some("!@#"), true, Some(("<!@#", "#@!>"))),
        &["some text", "\tsome more text"],
        sel(2, 1, 2, 1),
        CommentType::Toggle,
        true,
        true,
        &["some text", "!@# \tsome more text"],
        sel(2, 5, 2, 5),
    );
}

// upstream: Editor Contrib - Line Comment Command > _analyzeLines
#[test]
fn analyze_lines_test() {
    static LINES_1: [&str; 4] = ["\t\t", "    ", "    c", "\t\td"];
    let r = analyze_lines(CommentType::Toggle, true, &simple_model(&LINES_1), basic_preflight(&["//", "rem", "!@#", "!@#"]), 1, true, false, false);
    assert!(!r.should_remove_comments);
    // Does not change `commentStr`
    let strs: Vec<&str> = r.lines.iter().map(|l| l.comment_str.as_str()).collect();
    assert_eq!(strs, ["//", "rem", "!@#", "!@#"]);
    // Fills in `ignore`
    assert_eq!(r.lines.iter().map(|l| l.ignore).collect::<Vec<_>>(), [true, true, false, false]);
    // Fills in `commentStrOffset`
    assert_eq!(r.lines.iter().map(|l| l.comment_str_offset).collect::<Vec<_>>(), [2, 4, 4, 2]);

    static LINES_2: [&str; 4] = ["\t\t", "    rem ", "    !@# c", "\t\t!@#d"];
    let r = analyze_lines(CommentType::Toggle, true, &simple_model(&LINES_2), basic_preflight(&["//", "rem", "!@#", "!@#"]), 1, true, false, false);
    assert!(r.should_remove_comments);
    let strs: Vec<&str> = r.lines.iter().map(|l| l.comment_str.as_str()).collect();
    assert_eq!(strs, ["//", "rem", "!@#", "!@#"]);
    assert_eq!(r.lines.iter().map(|l| l.ignore).collect::<Vec<_>>(), [true, false, false, false]);
    assert_eq!(r.lines.iter().map(|l| l.comment_str_offset).collect::<Vec<_>>(), [2, 4, 4, 2]);
    // Fills in `commentStrLength`
    assert_eq!(r.lines.iter().map(|l| l.comment_str_length).collect::<Vec<_>>(), [2, 4, 4, 3]);
}

// upstream: Editor Contrib - Line Comment Command > _normalizeInsertionPoint
#[test]
fn normalize_insertion_point_test() {
    let run_test = |mixed: &[(&'static str, usize)], tab_size: usize, expected: &[usize], name: &str| {
        let texts: Vec<String> = mixed.iter().map(|m| m.0.to_string()).collect();
        let model = move |n: usize| texts[n - 1].clone();
        let mut offsets: Vec<(bool, usize)> = mixed.iter().map(|m| (false, m.1)).collect();
        normalize_insertion_point(&model, &mut offsets, 1, tab_size);
        let actual: Vec<usize> = offsets.iter().map(|o| o.1).collect();
        assert_eq!(actual, expected, "{name}");
    };
    // Bug 16696:[comment] comments not aligned in this case
    run_test(&[("  XX", 2), ("    YY", 4)], 4, &[0, 0], "Bug 16696");
    run_test(&[("\t\t\tXX", 3), ("    \tYY", 5), ("        ZZ", 8), ("\t\tTT", 2)], 4, &[2, 5, 8, 2], "Test1");
    run_test(&[("\t\t\t   XX", 6), ("    \t\t\t\tYY", 8), ("        ZZ", 8), ("\t\t    TT", 6)], 4, &[2, 5, 8, 2], "Test2");
    run_test(&[("\t\t", 2), ("\t\t\t", 3), ("\t\t\t\t", 4), ("\t\t\t", 3)], 4, &[2, 2, 2, 2], "Test3");
    run_test(&[("\t\t", 2), ("\t\t\t", 3), ("\t\t\t\t", 4), ("\t\t\t", 3), ("    ", 4)], 2, &[2, 2, 2, 2, 4], "Test4");
    run_test(&[("\t\t", 2), ("\t\t\t", 3), ("\t\t\t\t", 4), ("\t\t\t", 3), ("    ", 4)], 4, &[1, 1, 1, 1, 4], "Test5");
    run_test(&[(" \t", 2), ("  \t", 3), ("   \t", 4), ("    ", 4), ("\t", 1)], 4, &[2, 3, 4, 4, 1], "Test6");
    run_test(&[(" \t\t", 3), ("  \t\t", 4), ("   \t\t", 5), ("    \t", 5), ("\t", 1)], 4, &[2, 3, 4, 4, 1], "Test7");
    run_test(&[("\t", 1), ("    ", 4)], 4, &[1, 4], "Test8:4");
    run_test(&[("\t", 1), ("   ", 3)], 4, &[0, 0], "Test8:3");
    run_test(&[("\t", 1), ("  ", 2)], 4, &[0, 0], "Test8:2");
    run_test(&[("\t", 1), (" ", 1)], 4, &[0, 0], "Test8:1");
    run_test(&[("\t", 1), ("", 0)], 4, &[0, 0], "Test8:0");
    assert!(haystack_has_needle_at_offset("REM x", "rem", 0));
}

// upstream: Editor Contrib - Line Comment Command > detects indentation
#[test]
fn detects_indentation() {
    toggle(&["\tsome text", "\tsome more text"], sel(2, 2, 1, 1), &["\t!@# some text", "\t!@# some more text"], sel(2, 2, 1, 1));
}

// upstream: Editor Contrib - Line Comment Command > detects mixed indentation
#[test]
fn detects_mixed_indentation() {
    toggle(&["\tsome text", "    some more text"], sel(2, 2, 1, 1), &["\t!@# some text", "    !@# some more text"], sel(2, 2, 1, 1));
}

// upstream: Editor Contrib - Line Comment Command > ignores whitespace lines
#[test]
fn ignores_whitespace_lines() {
    toggle(
        &["\tsome text", "\t   ", "", "\tsome more text"],
        sel(4, 2, 1, 1),
        &["\t!@# some text", "\t   ", "", "\t!@# some more text"],
        sel(4, 2, 1, 1),
    );
}

// upstream: Editor Contrib - Line Comment Command > removes its own
#[test]
fn removes_its_own() {
    toggle(
        &["\t!@# some text", "\t   ", "\t\t!@# some more text"],
        sel(3, 2, 1, 1),
        &["\tsome text", "\t   ", "\t\tsome more text"],
        sel(3, 2, 1, 1),
    );
}

// upstream: Editor Contrib - Line Comment Command > works in only whitespace
#[test]
fn works_in_only_whitespace() {
    toggle(&["\t    ", "\t", "\t\tsome more text"], sel(3, 1, 1, 1), &["\t!@#     ", "\t!@# ", "\t\tsome more text"], sel(3, 1, 1, 1));
}

// upstream: Editor Contrib - Line Comment Command > bug 9697 - whitespace before comment token
#[test]
fn bug_9697_whitespace_before_comment_token() {
    toggle(&["\t !@#first", "\tsecond line"], sel(1, 1, 1, 1), &["\t first", "\tsecond line"], sel(1, 1, 1, 1));
}

// upstream: Editor Contrib - Line Comment Command > bug 10162 - line comment before caret
#[test]
fn bug_10162_line_comment_before_caret() {
    toggle(&["first!@#", "\tsecond line"], sel(1, 1, 1, 1), &["!@# first!@#", "\tsecond line"], sel(1, 5, 1, 5));
}

// upstream: Editor Contrib - Line Comment Command > comment single line - leading whitespace
#[test]
fn comment_single_line_leading_whitespace() {
    toggle(&["first!@#", "\tsecond line"], sel(2, 3, 2, 1), &["first!@#", "\t!@# second line"], sel(2, 7, 2, 1));
}

// upstream: Editor Contrib - Line Comment Command > ignores invisible selection
#[test]
fn ignores_invisible_selection() {
    toggle(&TEXT5, sel(2, 1, 1, 1), &["!@# first", "\tsecond line", "third line", "fourth line", "fifth"], sel(2, 1, 1, 5));
}

// upstream: Editor Contrib - Line Comment Command > multiple lines
#[test]
fn multiple_lines() {
    toggle(&TEXT5, sel(2, 4, 1, 1), &["!@# first", "!@# \tsecond line", "third line", "fourth line", "fifth"], sel(2, 8, 1, 5));
}

// upstream: Editor Contrib - Line Comment Command > multiple modes on multiple lines
#[test]
fn multiple_modes_on_multiple_lines() {
    toggle(&TEXT5, sel(4, 4, 3, 1), &["first", "\tsecond line", "!@# third line", "!@# fourth line", "fifth"], sel(4, 8, 3, 5));
}

// upstream: Editor Contrib - Line Comment Command > toggle single line
#[test]
fn toggle_single_line() {
    toggle(&TEXT5, sel(1, 1, 1, 1), &["!@# first", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 5, 1, 5));
    toggle(
        &["!@# first", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(1, 4, 1, 4),
        &["first", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(1, 1, 1, 1),
    );
}

// upstream: Editor Contrib - Line Comment Command > toggle multiple lines
#[test]
fn toggle_multiple_lines() {
    toggle(&TEXT5, sel(2, 4, 1, 1), &["!@# first", "!@# \tsecond line", "third line", "fourth line", "fifth"], sel(2, 8, 1, 5));
    toggle(
        &["!@# first", "!@# \tsecond line", "third line", "fourth line", "fifth"],
        sel(2, 7, 1, 4),
        &["first", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(2, 3, 1, 1),
    );
}

// upstream: Editor Contrib - Line Comment Command > issue #5964: Ctrl+/ to create comment when cursor is at the beginning of the line puts the cursor in a strange position
#[test]
fn issue_5964_cursor_at_the_beginning_of_the_line() {
    toggle(&TEXT5, sel(1, 1, 1, 1), &["!@# first", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 5, 1, 5));
}

// upstream: Editor Contrib - Line Comment Command > issue #35673: Comment hotkeys throws the cursor before the comment
#[test]
fn issue_35673_comment_hotkeys_throws_the_cursor_before_the_comment() {
    toggle(
        &["first", "", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(2, 1, 2, 1),
        &["first", "!@# ", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(2, 5, 2, 5),
    );
    toggle(
        &["first", "\t", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(2, 2, 2, 2),
        &["first", "\t!@# ", "\tsecond line", "third line", "fourth line", "fifth"],
        sel(2, 6, 2, 6),
    );
}

// upstream: Editor Contrib - Line Comment Command > issue #2837 "Add Line Comment" fault when blank lines involved
#[test]
fn issue_2837_add_line_comment_fault_when_blank_lines_involved() {
    add(
        &[
            "    if displayName == \"\":",
            "        displayName = groupName",
            "    description = getAttr(attributes, \"description\")",
            "    mailAddress = getAttr(attributes, \"mail\")",
            "",
            "    print \"||Group name|%s|\" % displayName",
            "    print \"||Description|%s|\" % description",
            "    print \"||Email address|[mailto:%s]|\" % mailAddress`",
        ],
        sel(1, 1, 8, 56),
        &[
            "    !@# if displayName == \"\":",
            "    !@#     displayName = groupName",
            "    !@# description = getAttr(attributes, \"description\")",
            "    !@# mailAddress = getAttr(attributes, \"mail\")",
            "",
            "    !@# print \"||Group name|%s|\" % displayName",
            "    !@# print \"||Description|%s|\" % description",
            "    !@# print \"||Email address|[mailto:%s]|\" % mailAddress`",
        ],
        sel(1, 1, 8, 60),
    );
}

// upstream: Editor Contrib - Line Comment Command > issue #47004: Toggle comments shouldn't move cursor
#[test]
fn issue_47004_toggle_comments_shouldnt_move_cursor() {
    add(&["    A line", "    Another line"], sel(2, 7, 1, 1), &["    !@# A line", "    !@# Another line"], sel(2, 11, 1, 1));
}

// upstream: Editor Contrib - Line Comment Command > insertSpace false
#[test]
fn insert_space_false() {
    run(&comments(Some("!@#"), false, None), &["some text"], sel(1, 1, 1, 1), CommentType::Toggle, false, true, &["!@#some text"], sel(1, 4, 1, 4));
}

// upstream: Editor Contrib - Line Comment Command > insertSpace false does not remove space
#[test]
fn insert_space_false_does_not_remove_space() {
    run(&comments(Some("!@#"), false, None), &["!@#    some text"], sel(1, 1, 1, 1), CommentType::Toggle, false, true, &["    some text"], sel(1, 1, 1, 1));
}

// ---- ignoreEmptyLines false ---------------------------------------------------------------------

fn toggle_keep_empty(lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    run(&standard(), lines, selection, CommentType::Toggle, true, false, expected, expected_selection);
}

// upstream: ignoreEmptyLines false > does not ignore whitespace lines
#[test]
fn does_not_ignore_whitespace_lines() {
    toggle_keep_empty(
        &["\tsome text", "\t   ", "", "\tsome more text"],
        sel(4, 2, 1, 1),
        &["!@# \tsome text", "!@# \t   ", "!@# ", "!@# \tsome more text"],
        sel(4, 6, 1, 5),
    );
}

// upstream: ignoreEmptyLines false > removes its own
#[test]
fn ignore_empty_lines_false_removes_its_own() {
    toggle_keep_empty(
        &["\t!@# some text", "\t   ", "\t\t!@# some more text"],
        sel(3, 2, 1, 1),
        &["\tsome text", "\t   ", "\t\tsome more text"],
        sel(3, 2, 1, 1),
    );
}

// upstream: ignoreEmptyLines false > works in only whitespace
#[test]
fn ignore_empty_lines_false_works_in_only_whitespace() {
    toggle_keep_empty(&["\t    ", "\t", "\t\tsome more text"], sel(3, 1, 1, 1), &["\t!@#     ", "\t!@# ", "\t\tsome more text"], sel(3, 1, 1, 1));
}

// upstream: ignoreEmptyLines false > comments single line
#[test]
fn ignore_empty_lines_false_comments_single_line() {
    toggle_keep_empty(&["some text", "\tsome more text"], sel(1, 1, 1, 1), &["!@# some text", "\tsome more text"], sel(1, 5, 1, 5));
}

// upstream: ignoreEmptyLines false > detects indentation
#[test]
fn ignore_empty_lines_false_detects_indentation() {
    toggle_keep_empty(&["\tsome text", "\tsome more text"], sel(2, 2, 1, 1), &["\t!@# some text", "\t!@# some more text"], sel(2, 2, 1, 1));
}

// ---- Line Comment As Block Comment ------------------------------------------------------------

fn as_block(lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    run(&comments(Some(""), false, Some(("(", ")"))), lines, selection, CommentType::Toggle, true, true, expected, expected_selection);
}

// upstream: Editor Contrib - Line Comment As Block Comment > fall back to block comment command
#[test]
fn fall_back_to_block_comment_command() {
    as_block(&TEXT5, sel(1, 1, 1, 1), &["( first )", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 3, 1, 3));
}

// upstream: Editor Contrib - Line Comment As Block Comment > fall back to block comment command - toggle
#[test]
fn fall_back_to_block_comment_command_toggle() {
    as_block(&["(first)", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 7, 1, 2), &TEXT5, sel(1, 6, 1, 1));
}

// upstream: Editor Contrib - Line Comment As Block Comment > bug 9513 - expand single line to uncomment auto block
#[test]
fn bug_9513_expand_single_line_to_uncomment_auto_block() {
    as_block(&TEXT5, sel(1, 1, 1, 1), &["( first )", "\tsecond line", "third line", "fourth line", "fifth"], sel(1, 3, 1, 3));
}

// upstream: Editor Contrib - Line Comment As Block Comment > bug 9691 - always expand selection to line boundaries
#[test]
fn bug_9691_always_expand_selection_to_line_boundaries() {
    as_block(&TEXT5, sel(3, 2, 1, 3), &["( first", "\tsecond line", "third line )", "fourth line", "fifth"], sel(3, 2, 1, 5));
    as_block(&["(first", "\tsecond line", "third line)", "fourth line", "fifth"], sel(3, 11, 1, 2), &TEXT5, sel(3, 11, 1, 1));
}

// ---- Line Comment As Block Comment 2 ----------------------------------------------------------

fn as_block_2(lines: &[&str], selection: Selection, expected: &[&str], expected_selection: Selection) {
    run(&comments(None, false, Some(("<!@#", "#@!>"))), lines, selection, CommentType::Toggle, true, true, expected, expected_selection);
}

const BLOCK_2: [&str; 5] = ["\t\tfirst\t    ", "\t\tsecond line", "\tthird line", "fourth line", "\t\t<!@#fifth#@!>\t\t"];

// upstream: Editor Contrib - Line Comment As Block Comment 2 > no selection => uses indentation
#[test]
fn no_selection_uses_indentation() {
    as_block_2(&BLOCK_2, sel(1, 1, 1, 1), &["\t\t<!@# first\t     #@!>", "\t\tsecond line", "\tthird line", "fourth line", "\t\t<!@#fifth#@!>\t\t"], sel(1, 1, 1, 1));
    as_block_2(
        &["\t\t<!@#first\t    #@!>", "\t\tsecond line", "\tthird line", "fourth line", "\t\t<!@#fifth#@!>\t\t"],
        sel(1, 1, 1, 1),
        &["\t\tfirst\t   ", "\t\tsecond line", "\tthird line", "fourth line", "\t\t<!@#fifth#@!>\t\t"],
        sel(1, 1, 1, 1),
    );
}

// upstream: Editor Contrib - Line Comment As Block Comment 2 > can remove
#[test]
fn can_remove() {
    let expected = ["\t\tfirst\t    ", "\t\tsecond line", "\tthird line", "fourth line", "\t\tfifth\t\t"];
    as_block_2(&BLOCK_2, sel(5, 1, 5, 1), &expected, sel(5, 1, 5, 1));
    as_block_2(&BLOCK_2, sel(5, 3, 5, 3), &expected, sel(5, 3, 5, 3));
    as_block_2(&BLOCK_2, sel(5, 4, 5, 4), &expected, sel(5, 3, 5, 3));
    as_block_2(&BLOCK_2, sel(5, 16, 5, 3), &expected, sel(5, 8, 5, 3));
    as_block_2(&BLOCK_2, sel(5, 12, 5, 7), &expected, sel(5, 8, 5, 3));
    as_block_2(&BLOCK_2, sel(5, 18, 5, 18), &expected, sel(5, 10, 5, 10));
}

// upstream: Editor Contrib - Line Comment As Block Comment 2 > issue #993: Remove comment does not work consistently in HTML
#[test]
fn issue_993_remove_comment_does_not_work_consistently_in_html() {
    as_block_2(&["     asd qwe", "     asd qwe", ""], sel(1, 1, 3, 1), &["     <!@# asd qwe", "     asd qwe #@!>", ""], sel(1, 1, 3, 1));
    as_block_2(&["     <!@#asd qwe", "     asd qwe#@!>", ""], sel(1, 1, 3, 1), &["     asd qwe", "     asd qwe", ""], sel(1, 1, 3, 1));
}
