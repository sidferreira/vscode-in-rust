//! upstream: src/vs/editor/test/common/model/textModel.test.ts @ 50e1350fc78d
//!
//! Ports of the indentation guessing tests of `Editor Model - TextModel` (`guess indentation 1`, the
//! `mixed whitespace` cases and the issue regressions). Generated from the upstream test file; each
//! `assert_guess` / `test_guess_indentation` call is one upstream call. `assertGuess` runs the guess
//! with default (true, 13370) and (false, 13371), as upstream does.

use vsr_editor::indentation::guess_indentation;
use vsr_text::{DefaultEol, TextBuffer};

fn test_guess_indentation(default_insert_spaces: bool, default_tab_size: usize, expected_insert_spaces: bool, expected_tab_size: usize, text: &[&str], msg: &str) {
    let buffer = TextBuffer::new(&text.join("\n"), DefaultEol::Lf, true);
    let r = guess_indentation(&buffer, default_tab_size, default_insert_spaces);
    assert_eq!(r.insert_spaces, expected_insert_spaces, "{msg}");
    assert_eq!(r.tab_size, expected_tab_size, "{msg}");
}

/// The expected tab size of `assertGuess`: unknown, a number, or `[number]` (only guessed with spaces).
#[derive(Clone, Copy)]
enum Size {
    Undefined,
    N(usize),
    OnlyWithSpaces(usize),
}

fn assert_guess(expected_insert_spaces: Option<bool>, expected_tab_size: Size, text: &[&str], msg: &str) {
    match expected_insert_spaces {
        None => match expected_tab_size {
            Size::Undefined => {
                test_guess_indentation(true, 13370, true, 13370, text, msg);
                test_guess_indentation(false, 13371, false, 13371, text, msg);
            }
            Size::N(n) => {
                test_guess_indentation(true, 13370, true, n, text, msg);
                test_guess_indentation(false, 13371, false, n, text, msg);
            }
            Size::OnlyWithSpaces(n) => {
                test_guess_indentation(true, 13370, true, n, text, msg);
                test_guess_indentation(false, 13371, false, 13371, text, msg);
            }
        },
        Some(insert) => match expected_tab_size {
            Size::Undefined => {
                test_guess_indentation(true, 13370, insert, 13370, text, msg);
                test_guess_indentation(false, 13371, insert, 13371, text, msg);
            }
            Size::N(n) => {
                test_guess_indentation(true, 13370, insert, n, text, msg);
                test_guess_indentation(false, 13371, insert, n, text, msg);
            }
            Size::OnlyWithSpaces(n) => {
                if insert {
                    test_guess_indentation(true, 13370, insert, n, text, msg);
                    test_guess_indentation(false, 13371, insert, n, text, msg);
                } else {
                    test_guess_indentation(true, 13370, insert, 13370, text, msg);
                    test_guess_indentation(false, 13371, insert, 13371, text, msg);
                }
            }
        },
    }
}

// upstream: Editor Model - TextModel > guess indentation 1
#[test]
fn guess_indentation_1() {
    assert_guess(None, Size::Undefined, &[
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
    ], "no clues");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
    ], "no spaces, 1xTAB");
    assert_guess(Some(true), Size::N(2), &[
        "  x",
        "x",
        "x",
        "x",
        "x",
        "x",
        "x",
    ], "1x2");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "\tx",
        "\tx",
        "\tx",
        "\tx",
        "\tx",
        "\tx",
    ], "7xTAB");
    assert_guess(None, Size::OnlyWithSpaces(2), &[
        "\tx",
        "  x",
        "\tx",
        "  x",
        "\tx",
        "  x",
        "\tx",
        "  x",
    ], "4x2, 4xTAB");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        " x",
        "\tx",
        " x",
        "\tx",
        " x",
        "\tx",
        " x",
    ], "4x1, 4xTAB");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "\tx",
        "  x",
        "\tx",
        "  x",
        "\tx",
        "  x",
        "\tx",
        "  x",
    ], "4x2, 5xTAB");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "\tx",
        "x",
        "\tx",
        "x",
        "\tx",
        "x",
        "\tx",
        "  x",
    ], "1x2, 5xTAB");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "\tx",
        "x",
        "\tx",
        "x",
        "\tx",
        "x",
        "\tx",
        "    x",
    ], "1x4, 5xTAB");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "\tx",
        "x",
        "\tx",
        "x",
        "\tx",
        "  x",
        "\tx",
        "    x",
    ], "1x2, 1x4, 5xTAB");
    assert_guess(None, Size::Undefined, &[
        "x",
        " x",
        " x",
        " x",
        " x",
        " x",
        " x",
        " x",
    ], "7x1 - 1 space is never guessed as an indentation");
    assert_guess(Some(true), Size::Undefined, &[
        "x",
        "          x",
        " x",
        " x",
        " x",
        " x",
        " x",
        " x",
    ], "1x10, 6x1");
    assert_guess(None, Size::Undefined, &[
        "",
        "  ",
        "    ",
        "      ",
        "        ",
        "          ",
        "            ",
        "              ",
    ], "whitespace lines don't count");
    assert_guess(Some(true), Size::N(3), &[
        "x",
        "   x",
        "   x",
        "    x",
        "x",
        "   x",
        "   x",
        "    x",
        "x",
        "   x",
        "   x",
        "    x",
    ], "6x3, 3x4");
    assert_guess(Some(true), Size::N(5), &[
        "x",
        "     x",
        "     x",
        "    x",
        "x",
        "     x",
        "     x",
        "    x",
        "x",
        "     x",
        "     x",
        "    x",
    ], "6x5, 3x4");
    assert_guess(Some(true), Size::N(7), &[
        "x",
        "       x",
        "       x",
        "     x",
        "x",
        "       x",
        "       x",
        "    x",
        "x",
        "       x",
        "       x",
        "    x",
    ], "6x7, 1x5, 2x4");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "  x",
        "  x",
        "  x",
        "x",
        "  x",
        "  x",
        "  x",
        "  x",
    ], "8x2");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "  x",
        "x",
        "  x",
        "  x",
        "x",
        "  x",
        "  x",
        "x",
        "  x",
        "  x",
    ], "8x2");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "    x",
        "x",
        "  x",
        "    x",
        "x",
        "  x",
        "    x",
        "x",
        "  x",
        "    x",
    ], "4x2, 4x4");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "  x",
        "    x",
        "x",
        "  x",
        "  x",
        "    x",
        "x",
        "  x",
        "  x",
        "    x",
    ], "6x2, 3x4");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "  x",
        "    x",
        "    x",
        "x",
        "  x",
        "  x",
        "    x",
        "    x",
    ], "4x2, 4x4");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "    x",
        "    x",
        "x",
        "  x",
        "    x",
        "    x",
    ], "2x2, 4x4");
    assert_guess(Some(true), Size::N(4), &[
        "x",
        "    x",
        "    x",
        "x",
        "    x",
        "    x",
        "x",
        "    x",
        "    x",
        "x",
        "    x",
        "    x",
    ], "8x4");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "    x",
        "    x",
        "      x",
        "x",
        "  x",
        "    x",
        "    x",
        "      x",
    ], "2x2, 4x4, 2x6");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  x",
        "    x",
        "    x",
        "      x",
        "      x",
        "        x",
    ], "1x2, 2x4, 2x6, 1x8");
    assert_guess(Some(true), Size::N(4), &[
        "x",
        "    x",
        "    x",
        "    x",
        "     x",
        "        x",
        "x",
        "    x",
        "    x",
        "    x",
        "     x",
        "        x",
    ], "6x4, 2x5, 2x8");
    assert_guess(Some(true), Size::N(4), &[
        "x",
        "    x",
        "    x",
        "    x",
        "     x",
        "        x",
        "        x",
    ], "3x4, 1x5, 2x8");
    assert_guess(Some(true), Size::N(4), &[
        "x",
        "x",
        "    x",
        "    x",
        "     x",
        "        x",
        "        x",
        "x",
        "x",
        "    x",
        "    x",
        "     x",
        "        x",
        "        x",
    ], "6x4, 2x5, 4x8");
    assert_guess(Some(true), Size::N(3), &[
        "x",
        " x",
        " x",
        " x",
        " x",
        " x",
        "x",
        "   x",
        "    x",
        "    x",
    ], "5x1, 2x0, 1x3, 2x4");
    assert_guess(Some(false), Size::Undefined, &[
        "\t x",
        " \t x",
        "\tx",
    ], "mixed whitespace 1");
    assert_guess(Some(false), Size::Undefined, &[
        "\tx",
        "\t    x",
    ], "mixed whitespace 2");
}

// upstream: Editor Model - TextModel > issue #44991: Wrong indentation size auto-detection
#[test]
fn issue_44991_wrong_indentation_size_auto_detection() {
    assert_guess(Some(true), Size::N(4), &[
        "a = 10             # 0 space indent",
        "b = 5              # 0 space indent",
        "if a > 10:         # 0 space indent",
        "    a += 1         # 4 space indent      delta 4 spaces",
        "    if b > 5:      # 4 space indent",
        "        b += 1     # 8 space indent      delta 4 spaces",
        "        b += 1     # 8 space indent",
        "        b += 1     # 8 space indent",
        "# comment line 1   # 0 space indent      delta 8 spaces",
        "# comment line 2   # 0 space indent",
        "# comment line 3   # 0 space indent",
        "        b += 1     # 8 space indent      delta 8 spaces",
        "        b += 1     # 8 space indent",
        "        b += 1     # 8 space indent",
    ], "");
}

// upstream: Editor Model - TextModel > issue #55818: Broken indentation detection
#[test]
fn issue_55818_broken_indentation_detection() {
    assert_guess(Some(true), Size::N(2), &[
        "",
        "/* REQUIRE */",
        "",
        "const foo = require ( 'foo' ),",
        "      bar = require ( 'bar' );",
        "",
        "/* MY FN */",
        "",
        "function myFn () {",
        "",
        "  const asd = 1,",
        "        dsa = 2;",
        "",
        "  return bar ( foo ( asd ) );",
        "",
        "}",
        "",
        "/* EXPORT */",
        "",
        "module.exports = myFn;",
        "",
    ], "");
}

// upstream: Editor Model - TextModel > issue #70832: Broken indentation detection
#[test]
fn issue_70832_broken_indentation_detection() {
    assert_guess(Some(false), Size::Undefined, &[
        "x",
        "x",
        "x",
        "x",
        "\tx",
        "\t\tx",
        "    x",
        "\t\tx",
        "\tx",
        "\t\tx",
        "\tx",
        "\tx",
        "\tx",
        "\tx",
        "x",
    ], "");
}

// upstream: Editor Model - TextModel > issue #62143: Broken indentation detection
#[test]
fn issue_62143_broken_indentation_detection() {
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "x",
        "  x",
        "  x",
    ], "");
    assert_guess(Some(true), Size::N(2), &[
        "x",
        "  - item2",
        "  - item3",
    ], "");
    test_guess_indentation(true, 2, true, 2, &[
        "x x",
        "  x",
        "  x",
    ], "");
    test_guess_indentation(true, 2, true, 2, &[
        "x x",
        "  x",
        "  x",
        "    x",
    ], "");
    test_guess_indentation(true, 2, true, 2, &[
        "<!--test1.md -->",
        "- item1",
        "  - item2",
        "    - item3",
    ], "");
}

// upstream: Editor Model - TextModel > issue #84217: Broken indentation detection
#[test]
fn issue_84217_broken_indentation_detection() {
    assert_guess(Some(true), Size::N(4), &[
        "def main():",
        "    print('hello')",
    ], "");
    assert_guess(Some(true), Size::N(4), &[
        "def main():",
        "    with open('foo') as fp:",
        "        print(fp.read())",
    ], "");
}

// upstream: Editor Model - TextModel > issue #65668: YAML file indented with 2 spaces
#[test]
fn issue_65668_yaml_file_indented_with_2_spaces() {
    assert_guess(Some(true), Size::N(2), &[
        "version: 2",
        "",
        "jobs:",
        "  build:",
        "    docker:",
        "      - circleci/golang:1.11",
        "",
        "  environment:",
        "    TEST_RESULTS: /tmp/test-results",
        "",
        "  steps:",
        "    - checkout",
        "    - run: mkdir -p $TEST_RESULTS",
        "",
        "    - restore_cache:",
        "        keys:",
        "          - v1-pkg-cache",
        "",
        "    - run:",
        "        name: dep ensure",
        "        command: dep ensure -v",
        "",
        "    - run:",
        "        name: Run unit tests",
        "        command: |",
        "          trap \"go-junit-report <${TEST_RESULTS}/go-test.out > ${TEST_RESULTS}/go-test-report.xml\" EXIT",
        "          go test -v ./... | tee ${TEST_RESULTS}/go-test.out",
        "",
        "    - run:",
        "        name: Build",
        "        command: go build -v",
        "",
        "    - save_cache:",
        "        key: v1-pkg-cache",
        "        paths:",
        "          - \"/go/pkg\"",
        "",
        "    - store_artifacts:",
        "        path: /tmp/test-results",
        "        destination: raw-test-output",
        "",
        "    - store_test_results:",
        "        path: /tmp/test-results",
    ], "");
}

// upstream: Editor Model - TextModel > issue #249040: 4-space indent should win over 2-space when predominant
#[test]
fn issue_249040_4_space_indent_should_win_over_2_space_when_predominant() {
    assert_guess(Some(true), Size::N(4), &[
        "function foo() {",
        "    let a = 1;",
        "    let b = 2;",
        "    if (true) {",
        "        console.log(a);",
        "        console.log(b);",
        "    }",
        "    const obj = {",
        "      x: 1,",
        "      y: 2",
        "    };",
        "    return obj;",
        "}",
    ], "");
}
