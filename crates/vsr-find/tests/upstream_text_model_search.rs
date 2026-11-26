//! upstream: src/vs/editor/test/common/model/textModelSearch.test.ts @ 50e1350fc78d
//!
//! Ported through `vsr_find::compile` + `Compiled::find_matches` (the `findMatches` half of
//! `assertFindMatches`; the CRLF run is a buffer built with CRLF line endings, as upstream's
//! `setEOL`). Whole word is `wordSeparators` + `isValidMatch`, not `\b`.
//!
//! Not ported: the `findNextMatch` / `findPreviousMatch` halves of `assertFindMatches` and the nine
//! tests that only exercise them (FindModel's navigation covers those, see
//! `upstream_find_model.rs`), the three `parseSearchRequest` tests (JS `RegExp` flags and
//! `SearchData`) and the two `isMultilineRegexSource` tests (a private helper of `vsr-editor`).

use vsr_find::{SearchParams, compile};
use vsr_text::{DefaultEol, TextBuffer};

const USUAL_WORD_SEPARATORS: &str = vsr_editor::USUAL_WORD_SEPARATORS;

type R = (usize, usize, usize, usize);

fn found(text: &str, p: &SearchParams, eol_crlf: bool) -> Vec<R> {
    let text = if eol_crlf { text.replace('\n', "\r\n") } else { text.to_string() };
    let buf = TextBuffer::new(&text, if eol_crlf { DefaultEol::CrLf } else { DefaultEol::Lf }, true);
    let c = compile(p).unwrap().expect("a search");
    c.find_matches(&buf, None, false, 1000)
        .iter()
        .map(|m| (m.range.start.line, m.range.start.column, m.range.end.line, m.range.end.column))
        .collect()
}

fn assert_find_matches(text: &str, search: &str, is_regex: bool, match_case: bool, separators: Option<&str>, expected: &[R]) {
    let p = SearchParams {
        search_string: search.into(),
        is_regex,
        match_case,
        whole_word: separators.is_some(),
        word_separators: separators.map(str::to_string),
    };
    assert_eq!(found(text, &p, false), expected, "LF");
    assert_eq!(found(text, &p, true), expected, "CRLF");
}

fn regular_text() -> String {
    [
        "This is some foo - bar text which contains foo and bar - as in Barcelona.",
        "Now it begins a word fooBar and now it is caps Foo-isn't this great?",
        "And here's a dull line with nothing interesting in it",
        "It is also interesting if it's part of a word like amazingFooBar",
        "Again nothing interesting here",
    ]
    .join("\n")
}

const CAPTURE_TEXT: &str = "one line line\ntwo line\nthree";

// upstream: TextModelSearch > Simple find
#[test]
fn simple_find() {
    assert_find_matches(
        &regular_text(),
        "foo",
        false,
        false,
        None,
        &[(1, 14, 1, 17), (1, 44, 1, 47), (2, 22, 2, 25), (2, 48, 2, 51), (4, 59, 4, 62)],
    );
}

// upstream: TextModelSearch > Case sensitive find
#[test]
fn case_sensitive_find() {
    assert_find_matches(&regular_text(), "foo", false, true, None, &[(1, 14, 1, 17), (1, 44, 1, 47), (2, 22, 2, 25)]);
}

// upstream: TextModelSearch > Whole words find
#[test]
fn whole_words_find() {
    assert_find_matches(
        &regular_text(),
        "foo",
        false,
        false,
        Some(USUAL_WORD_SEPARATORS),
        &[(1, 14, 1, 17), (1, 44, 1, 47), (2, 48, 2, 51)],
    );
}

// upstream: TextModelSearch > /^/ find
#[test]
fn caret_find() {
    assert_find_matches(&regular_text(), "^", true, false, None, &[(1, 1, 1, 1), (2, 1, 2, 1), (3, 1, 3, 1), (4, 1, 4, 1), (5, 1, 5, 1)]);
}

// upstream: TextModelSearch > /$/ find
#[test]
fn dollar_find() {
    assert_find_matches(&regular_text(), "$", true, false, None, &[(1, 74, 1, 74), (2, 69, 2, 69), (3, 54, 3, 54), (4, 65, 4, 65), (5, 31, 5, 31)]);
}

// upstream: TextModelSearch > /.*/ find
#[test]
fn dot_star_find() {
    assert_find_matches(&regular_text(), ".*", true, false, None, &[(1, 1, 1, 74), (2, 1, 2, 69), (3, 1, 3, 54), (4, 1, 4, 65), (5, 1, 5, 31)]);
}

// upstream: TextModelSearch > /^$/ find
#[test]
fn empty_lines_find() {
    let text = [
        "This is some foo - bar text which contains foo and bar - as in Barcelona.",
        "",
        "And here's a dull line with nothing interesting in it",
        "",
        "Again nothing interesting here",
    ]
    .join("\n");
    assert_find_matches(&text, "^$", true, false, None, &[(2, 1, 2, 1), (4, 1, 4, 1)]);
}

const JUST: [&str; 4] = ["Just some text text", "Just some text text", "some text again", "again some text"];

// upstream: TextModelSearch > multiline find 1
#[test]
fn multiline_find_1() {
    assert_find_matches(&JUST.join("\n"), "text\\n", true, false, None, &[(1, 16, 2, 1), (2, 16, 3, 1)]);
}

// upstream: TextModelSearch > multiline find 2
#[test]
fn multiline_find_2() {
    assert_find_matches(&JUST.join("\n"), "text\\nJust", true, false, None, &[(1, 16, 2, 5)]);
}

// upstream: TextModelSearch > multiline find 3
#[test]
fn multiline_find_3() {
    assert_find_matches(&JUST.join("\n"), "\\nagain", true, false, None, &[(3, 16, 4, 6)]);
}

// upstream: TextModelSearch > multiline find 4
#[test]
fn multiline_find_4() {
    assert_find_matches(&JUST.join("\n"), ".*\\nJust.*\\n", true, false, None, &[(1, 1, 3, 1)]);
}

// upstream: TextModelSearch > multiline find with line beginning regex
#[test]
fn multiline_find_with_line_beginning_regex() {
    assert_find_matches(&["if", "else", "", "if", "else"].join("\n"), "^if\\nelse", true, false, None, &[(1, 1, 2, 5), (4, 1, 5, 5)]);
}

// upstream: TextModelSearch > matching empty lines using boundary expression
#[test]
fn matching_empty_lines_using_boundary_expression() {
    assert_find_matches(
        &["if", "", "else", "  ", "if", " ", "else"].join("\n"),
        "^\\s*$\\n",
        true,
        false,
        None,
        &[(2, 1, 3, 1), (4, 1, 5, 1), (6, 1, 7, 1)],
    );
}

// upstream: TextModelSearch > matching lines starting with A and ending with B
#[test]
fn matching_lines_starting_with_a_and_ending_with_b() {
    assert_find_matches(&["a if b", "a", "ab", "eb"].join("\n"), "^a.*b$", true, false, None, &[(1, 1, 1, 7), (3, 1, 3, 3)]);
}

// upstream: TextModelSearch > multiline find with line ending regex
#[test]
fn multiline_find_with_line_ending_regex() {
    assert_find_matches(&["if", "else", "", "if", "elseif", "else"].join("\n"), "if\\nelse$", true, false, None, &[(1, 1, 2, 5), (5, 5, 6, 5)]);
}

// upstream: TextModelSearch > issue #4836 - ^.*$
#[test]
fn issue_4836() {
    assert_find_matches(
        &["Just some text text", "", "some text again", "", "again some text"].join("\n"),
        "^.*$",
        true,
        false,
        None,
        &[(1, 1, 1, 20), (2, 1, 2, 1), (3, 1, 3, 16), (4, 1, 4, 1), (5, 1, 5, 16)],
    );
}

// upstream: TextModelSearch > multiline find for non-regex string
#[test]
fn multiline_find_for_non_regex_string() {
    assert_find_matches(
        &["Just some text text", "some text text", "some text again", "again some text", "but not some"].join("\n"),
        "text\nsome",
        false,
        false,
        None,
        &[(1, 16, 2, 5), (2, 11, 3, 5)],
    );
}

// upstream: TextModelSearch > issue #3623: Match whole word does not work for not latin characters
#[test]
fn issue_3623_whole_word_non_latin() {
    assert_find_matches(
        &["я", "компилятор", "обфускация", ":я-я"].join("\n"),
        "я",
        false,
        false,
        Some(USUAL_WORD_SEPARATORS),
        &[(1, 1, 1, 2), (4, 2, 4, 3), (4, 4, 4, 5)],
    );
}

// upstream: TextModelSearch > issue #27459: Match whole words regression
#[test]
fn issue_27459_whole_words_regression() {
    assert_find_matches(
        &[
            "this._register(this._textAreaInput.onKeyDown((e: IKeyboardEvent) => {",
            "\tthis._viewController.emitKeyDown(e);",
            "}));",
        ]
        .join("\n"),
        "((e: ",
        false,
        false,
        Some(USUAL_WORD_SEPARATORS),
        &[(1, 45, 1, 50)],
    );
}

// upstream: TextModelSearch > issue #27594: Search results disappear
#[test]
fn issue_27594_search_results_disappear() {
    assert_find_matches("this.server.listen(0);", "listen(", false, false, Some(USUAL_WORD_SEPARATORS), &[(1, 13, 1, 20)]);
}

fn with_captures(text: &str, search: &str) -> Vec<(R, Vec<String>)> {
    let buf = TextBuffer::new(text, DefaultEol::Lf, true);
    let p = SearchParams { search_string: search.into(), is_regex: true, ..SearchParams::default() };
    compile(&p)
        .unwrap()
        .unwrap()
        .find_matches(&buf, None, true, 100)
        .into_iter()
        .map(|m| ((m.range.start.line, m.range.start.column, m.range.end.line, m.range.end.column), m.matches.unwrap()))
        .collect()
}

fn strings(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

// upstream: TextModelSearch > findMatches with capturing matches
#[test]
fn find_matches_with_capturing_matches() {
    assert_eq!(
        with_captures(CAPTURE_TEXT, "(l(in)e)"),
        [
            ((1, 5, 1, 9), strings(&["line", "line", "in"])),
            ((1, 10, 1, 14), strings(&["line", "line", "in"])),
            ((2, 5, 2, 9), strings(&["line", "line", "in"])),
        ]
    );
}

// upstream: TextModelSearch > findMatches multiline with capturing matches
#[test]
fn find_matches_multiline_with_capturing_matches() {
    assert_eq!(
        with_captures(CAPTURE_TEXT, "(l(in)e)\\n"),
        [((1, 10, 2, 1), strings(&["line\n", "line", "in"])), ((2, 5, 3, 1), strings(&["line\n", "line", "in"]))]
    );
}

// upstream: TextModelSearch > \n matches \r\n
#[test]
fn newline_matches_crlf() {
    let text = ["a", "b", "c", "d", "e", "f", "g", "h", "i"].join("\r\n");
    let buf = TextBuffer::new(&text, DefaultEol::CrLf, true);
    let first = |search: &str| {
        let p = SearchParams { search_string: search.into(), is_regex: true, ..SearchParams::default() };
        let m = compile(&p).unwrap().unwrap().find_matches(&buf, None, true, 1000).remove(0);
        ((m.range.start.line, m.range.start.column, m.range.end.line, m.range.end.column), m.matches.unwrap())
    };
    assert_eq!(first("h\\n"), ((8, 1, 9, 1), strings(&["h\n"])));
    assert_eq!(first("g\\nh\\n"), ((7, 1, 9, 1), strings(&["g\nh\n"])));
    assert_eq!(first("\\ni"), ((8, 2, 9, 2), strings(&["\ni"])));
}

// upstream: TextModelSearch > \r can never be found
#[test]
fn carriage_return_can_never_be_found() {
    let text = ["a", "b", "c"].join("\r\n");
    let buf = TextBuffer::new(&text, DefaultEol::CrLf, true);
    let p = SearchParams { search_string: "\\r\\n".into(), is_regex: true, ..SearchParams::default() };
    assert!(compile(&p).unwrap().unwrap().find_matches(&buf, None, true, 1000).is_empty());
}

// upstream: TextModelSearch > issue #53415. W should match line break.
#[test]
fn issue_53415_non_word_matches_line_break() {
    assert_find_matches(&["text", "180702-", "180703-180704"].join("\n"), "\\d{6}-\\W", true, false, None, &[(2, 1, 3, 1)]);
    assert_find_matches(
        &["Just some text", "", "Just"].join("\n"),
        "\\W",
        true,
        false,
        None,
        &[(1, 5, 1, 6), (1, 10, 1, 11), (1, 15, 2, 1), (2, 1, 3, 1)],
    );
    assert_find_matches(
        &["Just some text", "\tJust", "Just"].join("\n"),
        "\\W",
        true,
        false,
        None,
        &[(1, 5, 1, 6), (1, 10, 1, 11), (1, 15, 2, 1), (2, 1, 2, 2), (2, 6, 3, 1)],
    );
    assert_find_matches(&["Just  some text", "", "Just"].join("\n"), "\\W{2}", true, false, None, &[(1, 5, 1, 7), (1, 16, 3, 1)]);
}

// upstream: TextModelSearch > Simple find using unicode escape sequences
#[test]
fn simple_find_using_unicode_escape_sequences() {
    assert_find_matches(
        &regular_text(),
        "\\u{0066}\\u006f\\u006F",
        true,
        false,
        None,
        &[(1, 14, 1, 17), (1, 44, 1, 47), (2, 22, 2, 25), (2, 48, 2, 51), (4, 59, 4, 62)],
    );
}

// upstream: TextModelSearch > issue #74715. \d* finds empty string and stops searching.
#[test]
fn issue_74715_empty_matches_do_not_stop_the_search() {
    let got: Vec<_> = with_captures("10.243.30.10", "\\d*").into_iter().map(|(r, m)| (r.1, r.3, m[0].clone())).collect();
    assert_eq!(
        got,
        [(1, 3, "10".to_string()), (3, 3, String::new()), (4, 7, "243".into()), (7, 7, String::new()), (8, 10, "30".into()), (10, 10, String::new()), (11, 13, "10".into())]
    );
}

// upstream: TextModelSearch > issue #100134. Zero-length matches should properly step over surrogate pairs
#[test]
fn issue_100134_zero_length_matches_step_over_surrogate_pairs() {
    // 1[Laptop]1: no matches inside the emoji.
    assert_find_matches("1\u{1F4BB}1", "()", true, false, None, &[(1, 1, 1, 1), (1, 2, 1, 2), (1, 4, 1, 4), (1, 5, 1, 5)]);
    // 1[Cat Face][ZWJ][Laptop]1: matches between the emoji and the ZWJ, none inside an emoji.
    assert_find_matches(
        "1\u{1F431}\u{200D}\u{1F4BB}1",
        "()",
        true,
        false,
        None,
        &[(1, 1, 1, 1), (1, 2, 1, 2), (1, 4, 1, 4), (1, 5, 1, 5), (1, 7, 1, 7), (1, 8, 1, 8)],
    );
}
