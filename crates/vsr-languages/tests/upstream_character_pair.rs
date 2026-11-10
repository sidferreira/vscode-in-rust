//! upstream: src/vs/editor/test/common/modes/supports/characterPair.test.ts @ 50e1350fc78d
//!
//! Ports of the `CharacterPairSupport` suite. The tests' `TokenText` lines are `LineTokens`.

use vsr_languages::character_pair::{CharacterPairSupport, StandardAutoClosingPair};
use vsr_languages::config::{AutoClosingPairConfig, LanguageConfiguration, LineTokens, StandardTokenType};

fn pair(open: &str, close: &str) -> StandardAutoClosingPair {
    StandardAutoClosingPair::new(&AutoClosingPairConfig::new(open, close))
}

fn ab() -> Vec<(String, String)> {
    vec![("a".to_string(), "b".to_string())]
}

fn pairs(support: &CharacterPairSupport) -> Vec<StandardAutoClosingPair> {
    support.auto_closing_pairs().to_vec()
}

/// A line of `(text, type)` tokens as `LineTokens` (end offsets in UTF-16 units).
fn line(tokens: &[(&str, StandardTokenType)]) -> LineTokens {
    let mut end = 0;
    let spans: Vec<(usize, StandardTokenType)> = tokens
        .iter()
        .map(|(text, ty)| {
            end += text.encode_utf16().count();
            (end, *ty)
        })
        .collect();
    LineTokens::new(&spans)
}

fn not_in_string_comment() -> CharacterPairSupport {
    CharacterPairSupport::new(&LanguageConfiguration {
        auto_closing_pairs: Some(vec![AutoClosingPairConfig::new("{", "}").not_in(&["string", "comment"])]),
        ..Default::default()
    })
}

fn should_auto_close(support: &CharacterPairSupport, tokens: &LineTokens, column: usize) -> bool {
    support.auto_closing_pairs()[0].should_auto_close(tokens, column)
}

use StandardTokenType::{Comment, Other, String as Str};

// upstream: CharacterPairSupport > only autoClosingPairs
#[test]
fn only_auto_closing_pairs() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { auto_closing_pairs: Some(vec![AutoClosingPairConfig::new("a", "b")]), ..Default::default() });
    assert_eq!(pairs(&s), vec![pair("a", "b")]);
    assert_eq!(s.surrounding_pairs(), ab());
}

// upstream: CharacterPairSupport > only empty autoClosingPairs
#[test]
fn only_empty_auto_closing_pairs() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { auto_closing_pairs: Some(vec![]), ..Default::default() });
    assert!(pairs(&s).is_empty());
    assert!(s.surrounding_pairs().is_empty());
}

// upstream: CharacterPairSupport > only brackets
#[test]
fn only_brackets() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { brackets: Some(ab()), ..Default::default() });
    assert_eq!(pairs(&s), vec![pair("a", "b")]);
    assert_eq!(s.surrounding_pairs(), ab());
}

// upstream: CharacterPairSupport > only empty brackets
#[test]
fn only_empty_brackets() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { brackets: Some(vec![]), ..Default::default() });
    assert!(pairs(&s).is_empty());
    assert!(s.surrounding_pairs().is_empty());
}

// upstream: CharacterPairSupport > only surroundingPairs
#[test]
fn only_surrounding_pairs() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { surrounding_pairs: Some(ab()), ..Default::default() });
    assert!(pairs(&s).is_empty());
    assert_eq!(s.surrounding_pairs(), ab());
}

// upstream: CharacterPairSupport > only empty surroundingPairs
#[test]
fn only_empty_surrounding_pairs() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { surrounding_pairs: Some(vec![]), ..Default::default() });
    assert!(pairs(&s).is_empty());
    assert!(s.surrounding_pairs().is_empty());
}

// upstream: CharacterPairSupport > brackets is ignored when having autoClosingPairs
#[test]
fn brackets_is_ignored_when_having_auto_closing_pairs() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { auto_closing_pairs: Some(vec![]), brackets: Some(ab()), ..Default::default() });
    assert!(pairs(&s).is_empty());
    assert!(s.surrounding_pairs().is_empty());
}

// upstream: CharacterPairSupport > shouldAutoClosePair in empty line
#[test]
fn should_auto_close_pair_in_empty_line() {
    let s = not_in_string_comment();
    assert!(should_auto_close(&s, &line(&[]), 1));
}

// upstream: CharacterPairSupport > shouldAutoClosePair in not interesting line 1
#[test]
fn should_auto_close_pair_in_not_interesting_line_1() {
    let s = not_in_string_comment();
    assert!(should_auto_close(&s, &line(&[("do", Other)]), 3));
}

// upstream: CharacterPairSupport > shouldAutoClosePair in not interesting line 2
#[test]
fn should_auto_close_pair_in_not_interesting_line_2() {
    let s = CharacterPairSupport::new(&LanguageConfiguration { auto_closing_pairs: Some(vec![AutoClosingPairConfig::new("{", "}")]), ..Default::default() });
    assert!(should_auto_close(&s, &line(&[("do", Str)]), 3));
}

// upstream: CharacterPairSupport > shouldAutoClosePair in interesting line 1
#[test]
fn should_auto_close_pair_in_interesting_line_1() {
    let s = not_in_string_comment();
    let tokens = line(&[("\"a\"", Str)]);
    for column in 1..=4 {
        assert!(!should_auto_close(&s, &tokens, column), "column {column}");
    }
}

// upstream: CharacterPairSupport > shouldAutoClosePair in interesting line 2
#[test]
fn should_auto_close_pair_in_interesting_line_2() {
    let s = not_in_string_comment();
    let tokens = line(&[("x=", Other), ("\"a\"", Str), (";", Other)]);
    let expected = [true, true, true, false, false, false, true];
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(should_auto_close(&s, &tokens, i + 1), *want, "column {}", i + 1);
    }
}

// upstream: CharacterPairSupport > shouldAutoClosePair in interesting line 3
#[test]
fn should_auto_close_pair_in_interesting_line_3() {
    let s = not_in_string_comment();
    let tokens = line(&[(" ", Other), ("//a", Comment)]);
    let expected = [true, true, false, false, false];
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(should_auto_close(&s, &tokens, i + 1), *want, "column {}", i + 1);
    }
}
