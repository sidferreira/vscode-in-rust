//! upstream: src/vs/editor/contrib/quickAccess/test/browser/gotoLineQuickAccess.test.ts @ 50e1350fc78d
//!
//! Ports of the `AbstractGotoLineQuickAccessProvider` tests onto
//! `vsr_quickinput::goto_line::parse_position`. `withTestCodeEditor` becomes a [`LinesModel`]
//! (default tab size 4); the test provider's `useZeroBasedOffset` is the `zero_based` argument.

use vsr_quickinput::goto_line::{LinesModel, parse_position};

fn run_test(input: &str, expected_line: usize, expected_column: Option<usize>, zero_based: bool) {
    let model = LinesModel::new(&["line 1", "line 2", "line 3", "line 4", "line 5"], 4);
    let p = parse_position(Some(&model), input, zero_based);
    assert_eq!(p.line, Some(expected_line), "line of {input:?}");
    assert_eq!(p.column.unwrap_or(1), expected_column.unwrap_or(1), "column of {input:?}");
}

fn t(input: &str, line: usize) {
    run_test(input, line, None, false);
}

fn tc(input: &str, line: usize, column: usize) {
    run_test(input, line, Some(column), false);
}

// upstream: AbstractGotoLineQuickAccessProvider > parsePosition works as expected
#[test]
fn parse_position_works_as_expected() {
    // :line
    t("-100", 1);
    t("-5", 1);
    t("-1", 5);
    t("0", 1);
    t("1", 1);
    t("2", 2);
    t("5", 5);
    t("6", 5);
    t("7", 5);
    t("100", 5);

    // :line,column
    tc("2:-100", 2, 1);
    tc("2:-5", 2, 2);
    tc("2:-1", 2, 6);
    tc("2:0", 2, 1);
    tc("2:1", 2, 1);
    tc("2:2", 2, 2);
    tc("2:6", 2, 6);
    tc("2:7", 2, 7);
    tc("2:8", 2, 7);
    tc("2:100", 2, 7);

    // ::offset (1-based)
    tc(":-1000", 1, 1);
    tc(":-10", 4, 5);
    tc(":-1", 5, 7);
    tc(":0", 1, 1);
    tc(":1", 1, 1);
    tc(":10", 2, 3);
    tc(":1000", 5, 7);

    // offset (0-based)
    run_test(":-1000", 1, Some(1), true);
    run_test(":-10", 4, Some(4), true);
    run_test(":-1", 5, Some(6), true);
    run_test(":0", 1, Some(1), true);
    run_test(":1", 1, Some(2), true);
    run_test(":10", 2, Some(4), true);
    run_test(":1000", 5, Some(7), true);

    // :line#column
    // :line,column
    // spaces
    tc("-1#6", 5, 6);
    tc("2,4", 2, 4);
    tc("  2  :  3  ", 2, 3);
}

fn run_tab_test(input: &str, expected_line: usize, expected_column: usize) {
    let model = LinesModel::new(&["\tline 1", "\t\tline 2", "\tline 3"], 4);
    let p = parse_position(Some(&model), input, false);
    assert_eq!(p.line, Some(expected_line), "line of {input:?}");
    assert_eq!(p.column.unwrap_or(1), expected_column, "column of {input:?}");
}

// upstream: AbstractGotoLineQuickAccessProvider > parsePosition works with tabs
#[test]
fn parse_position_works_with_tabs() {
    // :line,column
    for (input, line, column) in [
        ("1:1", 1, 1),
        ("1:2", 1, 1),
        ("1:3", 1, 1),
        ("1:4", 1, 2),
        ("1:5", 1, 2),
        ("1:6", 1, 3),
        ("1:11", 1, 8),
        ("1:12", 1, 8),
        ("1:-5", 1, 3),
        ("1:-6", 1, 2),
        ("1:-7", 1, 2),
        ("1:-8", 1, 1),
        ("1:-9", 1, 1),
        ("1:-10", 1, 1),
        ("1:-11", 1, 1),
        ("2:1", 2, 1),
        ("2:2", 2, 1),
        ("2:3", 2, 1),
        ("2:4", 2, 2),
        ("2:5", 2, 2),
        ("2:8", 2, 3),
        ("2:9", 2, 3),
        ("2:11", 2, 5),
    ] {
        run_tab_test(input, line, column);
    }
}

#[test]
fn prompts_when_nothing_is_typed() {
    // Not an upstream test: the label texts of `parsePosition`.
    let model = LinesModel::new(&["line 1", "line 2", "line 3", "line 4", "line 5"], 4);
    let label = |v: &str| parse_position(Some(&model), v, false).label;
    assert_eq!(label(""), "Type a line number to go to (from 1 to 5).");
    assert_eq!(label("3"), "Press 'Enter' to go to line 3 or enter colon : to add a column number.");
    assert_eq!(label("3:"), "Press 'Enter' to go to line 3 or enter a column number (from 1 to 7).");
    assert_eq!(label("3:2"), "Press 'Enter' to go to line 3 at column 2.");
    assert_eq!(label(":"), "Type a character position to go to (from 1 to 34).");
    assert_eq!(parse_position(None, "3", false).label, "Open a text editor first to go to a line or an offset.");
}
