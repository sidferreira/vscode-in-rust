//! upstream: src/vs/editor/test/browser/controller/cursor.test.ts @ 50e1350fc78d
//!
//! Ports of the auto closing, over typing, auto surround and electric character cases of
//! `Editor Controller` in cursor.test.ts. The tests use upstream's `autoClosingLanguage`: its
//! language configuration and its small tokenizer (strings, line and block comments, nested
//! braces), written as a `TokenSource`. Not ported: the cases that need several cursors, the
//! composition (IME) cases, decorations listeners and the `#338693` undo-stop cases (a
//! multi-character auto close is one undo step here, see `typing.rs`).

use std::sync::Arc;

use vsr_editor::{
    AutoClosing, AutoSurround, Editor, EditorOptions, LineTokens, PlainTokens, Position, Selection, StandardTokenType, TokenSource, TypingOptions,
};
use vsr_languages::config::{AutoClosingPairConfig, CommentRule, LanguageConfiguration, LanguageSupport};
use vsr_text::{DefaultEol, TextBuffer};

// ---- the autoClosingLanguage ---------------------------------------------------------------------

fn auto_closing_config(before: Option<&str>, with_begin: bool) -> LanguageConfiguration {
    let mut pairs = vec![
        AutoClosingPairConfig::new("{", "}"),
        AutoClosingPairConfig::new("[", "]"),
        AutoClosingPairConfig::new("(", ")"),
        AutoClosingPairConfig::new("'", "'").not_in(&["string", "comment"]),
        AutoClosingPairConfig::new("\"", "\"").not_in(&["string"]),
        AutoClosingPairConfig::new("`", "`").not_in(&["string", "comment"]),
        AutoClosingPairConfig::new("/**", " */").not_in(&["string"]),
    ];
    if with_begin {
        pairs.push(AutoClosingPairConfig::new("begin", "end").not_in(&["string"]));
    }
    LanguageConfiguration {
        comments: Some(CommentRule { block_comment: Some(("/*".into(), "*/".into())), ..Default::default() }),
        auto_closing_pairs: Some(pairs),
        auto_close_before: before.map(String::from),
        ..Default::default()
    }
}

#[derive(Clone)]
enum State {
    Base(Option<Box<State>>),
    Str(char, Box<State>),
    Block(Box<State>),
}

/// upstream's `tokenizeEncoded` of the test language for one line.
fn tokenize(line: &str, mut state: State) -> (Vec<(usize, StandardTokenType)>, State) {
    use StandardTokenType::{Comment, Other, String as Str};
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    let mut tokens: Vec<(usize, StandardTokenType)> = Vec::new(); // (length, type)
    let push = |len: usize, ty: StandardTokenType, tokens: &mut Vec<(usize, StandardTokenType)>| {
        match tokens.last_mut() {
            Some(last) if last.1 == ty => last.0 += len,
            _ => tokens.push((len, ty)),
        }
    };
    while i < chars.len() {
        let rest = &chars[i..];
        match state.clone() {
            State::Base(parent) => {
                let run = rest.iter().take_while(|c| !"'\"`{}/".contains(**c)).count();
                if run > 0 {
                    push(run, Other, &mut tokens);
                    i += run;
                } else if "'\"`".contains(rest[0]) {
                    push(1, Str, &mut tokens);
                    state = State::Str(rest[0], Box::new(State::Base(parent)));
                    i += 1;
                } else if rest[0] == '{' {
                    push(1, Other, &mut tokens);
                    state = State::Base(Some(Box::new(State::Base(parent))));
                    i += 1;
                } else if rest[0] == '}' {
                    push(1, Other, &mut tokens);
                    state = parent.map_or(State::Base(None), |p| *p);
                    i += 1;
                } else if rest.starts_with(&['/', '/']) {
                    push(rest.len(), Comment, &mut tokens);
                    i += rest.len();
                } else if rest.starts_with(&['/', '*']) {
                    push(2, Comment, &mut tokens);
                    state = State::Block(Box::new(State::Base(parent)));
                    i += 2;
                } else {
                    push(1, Other, &mut tokens);
                    i += 1;
                }
            }
            State::Str(ch, parent) => {
                let run = rest.iter().take_while(|c| !"\\'\"`$".contains(**c)).count();
                if run > 0 {
                    push(run, Str, &mut tokens);
                    i += run;
                } else if rest[0] == '\\' {
                    let n = rest.len().min(2);
                    push(n, Str, &mut tokens);
                    i += n;
                } else if rest[0] == ch {
                    push(1, Str, &mut tokens);
                    state = *parent;
                    i += 1;
                } else if rest.starts_with(&['$', '{']) {
                    push(2, Other, &mut tokens);
                    state = State::Base(Some(Box::new(State::Str(ch, parent))));
                    i += 2;
                } else {
                    push(1, Other, &mut tokens);
                    i += 1;
                }
            }
            State::Block(parent) => {
                let run = rest.iter().take_while(|c| **c != '*').count();
                if run > 0 {
                    push(run, Str, &mut tokens);
                    i += run;
                } else if rest.starts_with(&['*', '/']) {
                    push(2, Comment, &mut tokens);
                    state = *parent;
                    i += 2;
                } else {
                    push(1, Other, &mut tokens);
                    i += 1;
                }
            }
        }
    }
    (tokens, state)
}

struct AutoClosingTokens;

impl AutoClosingTokens {
    fn start_state(buffer: &TextBuffer, line: usize) -> State {
        let mut state = State::Base(None);
        for l in 1..line {
            state = tokenize(buffer.line_content(l), state).1;
        }
        state
    }

    fn to_line_tokens(tokens: Vec<(usize, StandardTokenType)>) -> LineTokens {
        let mut end = 0;
        LineTokens::new(&tokens.iter().map(|&(len, ty)| {
            end += len;
            (end, ty)
        }).collect::<Vec<_>>())
    }
}

impl TokenSource for AutoClosingTokens {
    fn line_tokens(&self, buffer: &TextBuffer, line: usize) -> Option<LineTokens> {
        let state = Self::start_state(buffer, line);
        Some(Self::to_line_tokens(tokenize(buffer.line_content(line), state).0))
    }

    fn token_type_if_inserting(&self, buffer: &TextBuffer, line: usize, column: usize, ch: char) -> Option<StandardTokenType> {
        let content = buffer.line_content(line);
        let before: String = content.chars().take(column - 1).collect();
        let after: String = content.chars().skip(column - 1).collect();
        let text = format!("{before}{ch}{after}");
        let tokens = Self::to_line_tokens(tokenize(&text, Self::start_state(buffer, line)).0);
        Some(tokens.type_at_offset(column as isize - 1))
    }
}

// ---- helpers ------------------------------------------------------------------------------------

fn editor_with(lines: &[&str], config: LanguageConfiguration, typing: TypingOptions) -> Editor {
    let mut e = Editor::new(TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces: true });
    e.set_language_support(Some(Arc::new(LanguageSupport::new(config))));
    e.set_typing_options(typing);
    e
}

fn auto_closing_editor(lines: &[&str], typing: TypingOptions) -> Editor {
    editor_with(lines, auto_closing_config(None, true), typing)
}

fn move_to(e: &mut Editor, line: usize, column: usize) {
    e.set_cursor(Position::new(line, column), false);
}

/// Types with no tokenizer: upstream's `autoClosingLanguage` has none registered unless a test
/// calls `setupAutoClosingLanguageTokenization()`, so every token is `Other`.
fn type_keys(e: &mut Editor, text: &str) {
    e.type_text_with(text, &PlainTokens);
}

fn type_keys_tokenized(e: &mut Editor, text: &str) {
    e.type_text_with(text, &AutoClosingTokens);
}

fn line(e: &Editor, n: usize) -> String {
    e.buffer().line_content(n).to_string()
}

/// upstream `assertType`: type `chr` at (line, column), check the line, undo.
fn assert_type(e: &mut Editor, line_number: usize, column: usize, chr: &str, expected_insert: &str, message: &str) {
    assert_type_with(e, line_number, column, chr, expected_insert, message, false);
}

fn assert_type_with(e: &mut Editor, line_number: usize, column: usize, chr: &str, expected_insert: &str, message: &str, tokenized: bool) {
    let content = line(e, line_number);
    let units: Vec<u16> = content.encode_utf16().collect();
    let head = String::from_utf16_lossy(&units[..column - 1]);
    let tail = String::from_utf16_lossy(&units[column - 1..]);
    let expected = format!("{head}{expected_insert}{tail}");
    move_to(e, line_number, column);
    if tokenized { type_keys_tokenized(e, chr) } else { type_keys(e, chr) }
    assert_eq!(line(e, line_number), expected, "{message}");
    assert!(e.undo(), "{message}: nothing to undo");
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Col {
    Normal,
    Special1,
    Special2,
}

fn extract_special_columns(max_column: usize, annotated: &str) -> Vec<Col> {
    let mut result = vec![Col::Normal; max_column + 1];
    let mut column = 1;
    for c in annotated.chars() {
        match c {
            '|' => result[column] = Col::Special1,
            '!' => result[column] = Col::Special2,
            _ => column += 1,
        }
    }
    result
}

const AUTO_CLOSE_TEXT: [&str; 8] = [
    "var a = [];",
    "var b = `asd`;",
    "var c = 'asd';",
    "var d = \"asd\";",
    "var e = /*3*/\t3;",
    "var f = /** 3 */3;",
    "var g = (3+5);",
    "var h = { a: 'value' };",
];

fn check_open_parens(e: &mut Editor, positions: &[&str]) {
    for (i, annotated) in positions.iter().enumerate() {
        let line_number = i + 1;
        let max = e.buffer().line_max_column(line_number);
        let columns = extract_special_columns(max, annotated);
        for column in 1..columns.len() {
            if columns[column] == Col::Special1 {
                assert_type(e, line_number, column, "(", "()", &format!("auto closes @ ({line_number}, {column})"));
            } else {
                assert_type(e, line_number, column, "(", "(", &format!("does not auto close @ ({line_number}, {column})"));
            }
        }
    }
}

// ---- tests ---------------------------------------------------------------------------------------

// upstream: Editor Controller > issue #61070: backtick (`) should auto-close after a word character
#[test]
fn issue_61070_backtick_should_auto_close_after_a_word_character() {
    let mut e = auto_closing_editor(&["const markup = highlight"], TypingOptions::default());
    assert_type(&mut e, 1, 25, "`", "``", "auto closes ` @ (1, 25)");
}

// upstream: Editor Controller > issue #132912: quotes should not auto-close if they are closing a string
#[test]
fn issue_132912_quotes_should_not_auto_close_if_they_are_closing_a_string() {
    let mut e = auto_closing_editor(&["const t2 = `something ${t1}"], TypingOptions::default());
    assert_type_with(&mut e, 1, 28, "`", "`", "does not auto close ` @ (1, 28)", true);
}

// upstream: Editor Controller > autoClosingPairs - open parens: default
#[test]
fn auto_closing_pairs_open_parens_default() {
    let mut e = auto_closing_editor(&AUTO_CLOSE_TEXT, TypingOptions::default());
    check_open_parens(
        &mut e,
        &[
            "var| a| |=| [|]|;|",
            "var| b| |=| |`asd|`|;|",
            "var| c| |=| |'asd|'|;|",
            "var| d| |=| |\"asd|\"|;|",
            "var| e| |=| /*3*/|\t3|;|",
            "var| f| |=| /**| 3| */3|;|",
            "var| g| |=| (3+5|)|;|",
            "var| h| |=| {| a|:| |'value|'| |}|;|",
        ],
    );
}

// upstream: Editor Controller > autoClosingPairs - open parens: whitespace
#[test]
fn auto_closing_pairs_open_parens_whitespace() {
    let typing = TypingOptions { auto_closing_brackets: AutoClosing::BeforeWhitespace, ..Default::default() };
    let mut e = auto_closing_editor(&AUTO_CLOSE_TEXT, typing);
    check_open_parens(
        &mut e,
        &[
            "var| a| =| [|];|",
            "var| b| =| `asd`;|",
            "var| c| =| 'asd';|",
            "var| d| =| \"asd\";|",
            "var| e| =| /*3*/|\t3;|",
            "var| f| =| /**| 3| */3;|",
            "var| g| =| (3+5|);|",
            "var| h| =| {| a:| 'value'| |};|",
        ],
    );
}

// upstream: Editor Controller > autoClosingPairs - open parens disabled/enabled open quotes enabled/disabled
#[test]
fn auto_closing_pairs_open_parens_disabled_enabled_open_quotes_enabled_disabled() {
    let typing = TypingOptions { auto_closing_brackets: AutoClosing::BeforeWhitespace, auto_closing_quotes: AutoClosing::Never, ..Default::default() };
    let mut e = auto_closing_editor(&["var a = [];"], typing);
    let columns = extract_special_columns(e.buffer().line_max_column(1), "var| a| =| [|];|");
    for column in 1..columns.len() {
        let expected = if columns[column] == Col::Special1 { "()" } else { "(" };
        assert_type(&mut e, 1, column, "(", expected, &format!("@ (1, {column})"));
        assert_type(&mut e, 1, column, "'", "'", &format!("does not auto close @ (1, {column})"));
    }

    let typing = TypingOptions { auto_closing_brackets: AutoClosing::Never, auto_closing_quotes: AutoClosing::BeforeWhitespace, ..Default::default() };
    let mut e = auto_closing_editor(&["var b = [];"], typing);
    let columns = extract_special_columns(e.buffer().line_max_column(1), "var b =| [|];|");
    for column in 1..columns.len() {
        let expected = if columns[column] == Col::Special1 { "''" } else { "'" };
        assert_type(&mut e, 1, column, "'", expected, &format!("@ (1, {column})"));
        assert_type(&mut e, 1, column, "(", "(", &format!("does not auto close @ (1, {column})"));
    }
}

// upstream: Editor Controller > autoClosingPairs - configurable open parens
#[test]
fn auto_closing_pairs_configurable_open_parens() {
    let mut e = editor_with(&AUTO_CLOSE_TEXT, auto_closing_config(Some("abc"), false), TypingOptions::default());
    check_open_parens(
        &mut e,
        &[
            "v|ar |a = [|];|",
            "v|ar |b = `|asd`;|",
            "v|ar |c = '|asd';|",
            "v|ar d = \"|asd\";|",
            "v|ar e = /*3*/\t3;|",
            "v|ar f = /** 3| */3;|",
            "v|ar g = (3+5|);|",
            "v|ar h = { |a: 'v|alue' |};|",
        ],
    );
}

// upstream: Editor Controller > autoClosingPairs - auto-pairing can be disabled
#[test]
fn auto_closing_pairs_auto_pairing_can_be_disabled() {
    let typing = TypingOptions { auto_closing_brackets: AutoClosing::Never, auto_closing_quotes: AutoClosing::Never, ..Default::default() };
    let mut e = auto_closing_editor(&AUTO_CLOSE_TEXT, typing);
    for (i, text) in AUTO_CLOSE_TEXT.iter().enumerate() {
        let line_number = i + 1;
        for column in 1..=e.buffer().line_max_column(line_number) {
            assert_type(&mut e, line_number, column, "(", "(", &format!("does not auto close @ ({line_number}, {column}) {text}"));
            assert_type(&mut e, line_number, column, "\"", "\"", &format!("does not auto close @ ({line_number}, {column}) {text}"));
        }
    }
}

// upstream: Editor Controller > autoClosingPairs - auto wrapping is configurable
// (the first block of the upstream test selects with two cursors; this port covers the three single
// cursor blocks and the default with one selection)
#[test]
fn auto_closing_pairs_auto_wrapping_is_configurable() {
    let select = |e: &mut Editor| e.set_selection(Selection { anchor: Position::new(1, 1), active: Position::new(1, 4) });

    let mut e = auto_closing_editor(&["var a = asd"], TypingOptions::default());
    select(&mut e);
    type_keys(&mut e, "`");
    assert_eq!(e.buffer().value(), "`var` a = asd");
    type_keys(&mut e, "(");
    assert_eq!(e.buffer().value(), "`(var)` a = asd");

    let mut e = auto_closing_editor(&["var a = asd"], TypingOptions { auto_surround: AutoSurround::Never, ..Default::default() });
    select(&mut e);
    type_keys(&mut e, "`");
    assert_eq!(e.buffer().value(), "` a = asd");

    let mut e = auto_closing_editor(&["var a = asd"], TypingOptions { auto_surround: AutoSurround::Quotes, ..Default::default() });
    select(&mut e);
    type_keys(&mut e, "`");
    assert_eq!(e.buffer().value(), "`var` a = asd");
    type_keys(&mut e, "(");
    assert_eq!(e.buffer().value(), "`(` a = asd");

    let mut e = auto_closing_editor(&["var a = asd"], TypingOptions { auto_surround: AutoSurround::Brackets, ..Default::default() });
    select(&mut e);
    type_keys(&mut e, "(");
    assert_eq!(e.buffer().value(), "(var) a = asd");
    type_keys(&mut e, "`");
    assert_eq!(e.buffer().value(), "(`) a = asd");
}

// upstream: Editor Controller > autoClosingPairs - quote
#[test]
fn auto_closing_pairs_quote() {
    let mut e = auto_closing_editor(&AUTO_CLOSE_TEXT, TypingOptions::default());
    let positions = [
        "var a |=| [|]|;|",
        "var b |=| `asd`|;|",
        "var c |=| 'asd'|;|",
        "var d |=| \"asd\"|;|",
        "var e |=| /*3*/|\t3;|",
        "var f |=| /**| 3 */3;|",
        "var g |=| (3+5)|;|",
        "var h |=| {| a:| 'value'| |}|;|",
    ];
    for (i, annotated) in positions.iter().enumerate() {
        let line_number = i + 1;
        let columns = extract_special_columns(e.buffer().line_max_column(line_number), annotated);
        for column in 1..columns.len() {
            match columns[column] {
                Col::Special1 => assert_type(&mut e, line_number, column, "'", "''", &format!("auto closes @ ({line_number}, {column})")),
                Col::Special2 => assert_type(&mut e, line_number, column, "'", "", &format!("over types @ ({line_number}, {column})")),
                Col::Normal => assert_type(&mut e, line_number, column, "'", "'", &format!("does not auto close @ ({line_number}, {column})")),
            }
        }
    }
}

// upstream: Editor Controller > autoClosingPairs - multi-character autoclose
#[test]
fn auto_closing_pairs_multi_character_autoclose() {
    let mut e = auto_closing_editor(&["begi"], TypingOptions::default());
    move_to(&mut e, 1, 5);
    type_keys(&mut e, "n");
    assert_eq!(line(&e, 1), "beginend");

    let mut e = auto_closing_editor(&["/*"], TypingOptions::default());
    move_to(&mut e, 1, 3);
    type_keys(&mut e, "*");
    assert_eq!(line(&e, 1), "/** */");
}

// upstream: Editor Controller > issue #37315 - overtypes only those characters that it inserted
#[test]
fn issue_37315_overtypes_only_those_characters_that_it_inserted() {
    let mut e = auto_closing_editor(&["", "y=();"], TypingOptions::default());
    type_keys(&mut e, "x=(");
    assert_eq!(line(&e, 1), "x=()");
    type_keys(&mut e, "asd");
    assert_eq!(line(&e, 1), "x=(asd)");
    // overtype!
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 1), "x=(asd)");
    // do not overtype!
    move_to(&mut e, 2, 4);
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 2), "y=());");
}

// upstream: Editor Controller > issue #37315 - stops overtyping once cursor leaves area
#[test]
fn issue_37315_stops_overtyping_once_cursor_leaves_area() {
    let mut e = auto_closing_editor(&["", "y=();"], TypingOptions::default());
    type_keys(&mut e, "x=(");
    assert_eq!(line(&e, 1), "x=()");
    move_to(&mut e, 1, 5);
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 1), "x=())");
}

// upstream: Editor Controller > issue #37315 - it overtypes only once
#[test]
fn issue_37315_it_overtypes_only_once() {
    let mut e = auto_closing_editor(&["", "y=();"], TypingOptions::default());
    type_keys(&mut e, "x=(");
    assert_eq!(line(&e, 1), "x=()");
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 1), "x=()");
    move_to(&mut e, 1, 4);
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 1), "x=())");
}

// upstream: Editor Controller > issue #37315 - it can remember multiple auto-closed instances
#[test]
fn issue_37315_it_can_remember_multiple_auto_closed_instances() {
    let mut e = auto_closing_editor(&["", "y=();"], TypingOptions::default());
    type_keys(&mut e, "x=(");
    assert_eq!(line(&e, 1), "x=()");
    type_keys(&mut e, "(");
    assert_eq!(line(&e, 1), "x=(())");
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 1), "x=(())");
    type_keys(&mut e, ")");
    assert_eq!(line(&e, 1), "x=(())");
}

// upstream: Editor Controller > issue #118270 - auto closing deletes only those characters that it inserted
#[test]
fn issue_118270_auto_closing_deletes_only_those_characters_that_it_inserted() {
    let mut e = auto_closing_editor(&["", "y=();"], TypingOptions::default());
    type_keys(&mut e, "x=(");
    assert_eq!(line(&e, 1), "x=()");
    type_keys(&mut e, "asd");
    assert_eq!(line(&e, 1), "x=(asd)");
    e.backspace();
    e.backspace();
    e.backspace();
    assert_eq!(line(&e, 1), "x=()");
    // delete closing char!
    e.backspace();
    assert_eq!(line(&e, 1), "x=");
    // do not delete closing char!
    move_to(&mut e, 2, 4);
    e.backspace();
    assert_eq!(line(&e, 2), "y=);");
}

// upstream: Editor Controller > issue #78527 - does not close quote on odd count
#[test]
fn issue_78527_does_not_close_quote_on_odd_count() {
    let mut e = auto_closing_editor(&["std::cout << '\"' << entryMap"], TypingOptions::default());
    move_to(&mut e, 1, 29);
    type_keys(&mut e, "[");
    assert_eq!(line(&e, 1), "std::cout << '\"' << entryMap[]");
    type_keys(&mut e, "\"");
    assert_eq!(line(&e, 1), "std::cout << '\"' << entryMap[\"\"]");
    type_keys(&mut e, "a");
    assert_eq!(line(&e, 1), "std::cout << '\"' << entryMap[\"a\"]");
    type_keys(&mut e, "\"");
    assert_eq!(line(&e, 1), "std::cout << '\"' << entryMap[\"a\"]");
    type_keys(&mut e, "]");
    assert_eq!(line(&e, 1), "std::cout << '\"' << entryMap[\"a\"]");
}

// upstream: Editor Controller > issue #6841: Auto closing brackets should balance brackets
// (the multi-cursor block of the upstream test is not ported)
#[test]
fn issue_6841_auto_closing_brackets_should_balance_brackets() {
    let brackets = vec![("{".to_string(), "}".to_string()), ("[".to_string(), "]".to_string()), ("(".to_string(), ")".to_string())];
    let balanced = LanguageConfiguration {
        brackets: Some(brackets.clone()),
        auto_closing_pairs: Some(vec![AutoClosingPairConfig::new("{", "}"), AutoClosingPairConfig::new("[", "]"), AutoClosingPairConfig::new("(", ")")]),
        ..Default::default()
    };
    let cases: [(&str, (usize, usize), &str, &str); 10] = [
        ("\n}", (1, 1), "{", "{\n}"),
        ("", (1, 1), "{", "{}"),
        ("{}", (1, 2), "{", "{{}}"),
        ("{\n", (2, 1), "{", "{\n{}"),
        ("\n]", (1, 1), "{", "{}\n]"),
        (")\n", (2, 1), "(", ")\n()"),
        ("function foo() {\n\n}", (2, 1), "{", "function foo() {\n{}\n}"),
        ("someFunction);", (1, 13), "(", "someFunction();"),
        ("someFunction;", (1, 13), "(", "someFunction();"),
        ("someFunctionsomeParam);", (1, 13), "(", "someFunction(someParam);"),
    ];
    for (text, (l, c), typed, expected) in cases {
        let lines: Vec<&str> = text.split('\n').collect();
        let mut e = editor_with(&lines, balanced.clone(), TypingOptions::default());
        move_to(&mut e, l, c);
        type_keys(&mut e, typed);
        assert_eq!(e.buffer().value(), expected, "{text:?}");
    }

    let always = TypingOptions { auto_closing_brackets: AutoClosing::Always, ..Default::default() };
    let mut e = editor_with(&["", ")"], balanced, always);
    type_keys(&mut e, "(");
    assert_eq!(e.buffer().value(), "()\n)");

    // Brackets in strings and comments are not counted.
    let only_braces = LanguageConfiguration { brackets: Some(vec![("{".to_string(), "}".to_string())]), ..auto_closing_config(None, true) };
    let mut e = editor_with(&["", "\"}\""], only_braces.clone(), TypingOptions::default());
    type_keys_tokenized(&mut e, "{");
    assert_eq!(e.buffer().value(), "{}\n\"}\"");
    let mut e = editor_with(&["", "\"}\"", "// }"], only_braces, TypingOptions::default());
    type_keys_tokenized(&mut e, "{");
    assert_eq!(e.buffer().value(), "{}\n\"}\"\n// }");
}
