//! Words of a line and the word-based completion provider (ADR-0083).
//!
//! upstream: src/vs/editor/common/services/editorWebWorker.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/services/editorWorkerService.ts @ 50e1350fc78d
//!
//! Upstream source: src/vs/editor/common/core/wordHelper.ts (the default word definition),
//! src/vs/editor/common/services/editorWebWorker.ts (`$textualSuggest`) and
//! src/vs/editor/browser/services/editorWorkerService.ts (`WordBasedCompletionItemProvider`).
//!
//! A word is a run of characters that are neither whitespace nor one of the default word
//! separators (the editor's `word_at` reads the same set). Upstream's default definition also
//! matches numbers with a dot (`-?\d*\.\d\w*`); here `1.5` is two words, which the suggest rules
//! treat the same (a numeric word never triggers and is never offered). Columns are 1-based UTF-16.

use crate::item::{Completion, ItemRange, Position, Span, SuggestItem, kind};

/// Upstream `DEFAULT_WORD_REGEXP`'s separators.
pub const WORD_SEPARATORS: &str = "`~!@#%^&*()-=+[{]}\\|;:'\",.<>/?";

/// Upstream `_suggestionsLimit`.
pub const SUGGESTIONS_LIMIT: usize = 10_000;

pub fn is_word_char(c: char) -> bool {
    !c.is_whitespace() && !WORD_SEPARATORS.contains(c)
}

/// A word on a line with its 1-based UTF-16 columns (`IWordAtPosition`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Word {
    pub word: String,
    pub start_column: i64,
    pub end_column: i64,
}

/// The words of a line, in order, with their columns.
pub fn line_words(line: &str) -> Vec<Word> {
    let mut out = Vec::new();
    let mut units = 0i64;
    let mut run: Option<(i64, String)> = None;
    for c in line.chars() {
        if is_word_char(c) {
            run.get_or_insert_with(|| (units, String::new())).1.push(c);
        } else if let Some((start, word)) = run.take() {
            out.push(Word {
                end_column: start + 1 + word.encode_utf16().count() as i64,
                start_column: start + 1,
                word,
            });
        }
        units += c.len_utf16() as i64;
    }
    if let Some((start, word)) = run {
        out.push(Word {
            end_column: start + 1 + word.encode_utf16().count() as i64,
            start_column: start + 1,
            word,
        });
    }
    out
}

/// Upstream `getWordAtPosition`: the word that touches `column` on either side.
pub fn word_at(line: &str, column: i64) -> Option<Word> {
    line_words(line)
        .into_iter()
        .find(|w| w.start_column <= column && column <= w.end_column)
}

/// Upstream `getWordUntilPosition`: the part of the word before `column`; empty at `column` when
/// there is none.
pub fn word_until(line: &str, column: i64) -> Word {
    match word_at(line, column) {
        Some(w) => {
            let take = (column - w.start_column).max(0) as usize;
            let units: Vec<u16> = w.word.encode_utf16().take(take).collect();
            Word {
                word: String::from_utf16_lossy(&units),
                start_column: w.start_column,
                end_column: column,
            }
        }
        None => Word {
            word: String::new(),
            start_column: column,
            end_column: column,
        },
    }
}

/// Upstream `!isNaN(Number(word))` for the words a line can hold: a word that parses as a number.
pub fn is_numeric(word: &str) -> bool {
    !word.is_empty() && word.parse::<f64>().is_ok()
}

/// `$textualSuggest`: the distinct words of `texts` in order of appearance, without `leading_word`
/// and without numbers, at most [`SUGGESTIONS_LIMIT`].
pub fn textual_words<'a>(
    texts: impl IntoIterator<Item = &'a str>,
    leading_word: Option<&str>,
) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    'outer: for text in texts {
        for line in text.split('\n') {
            for w in line_words(line.strip_suffix('\r').unwrap_or(line)) {
                if Some(w.word.as_str()) == leading_word || is_numeric(&w.word) {
                    continue;
                }
                if seen.insert(w.word.clone()) {
                    out.push(w.word);
                    if out.len() > SUGGESTIONS_LIMIT {
                        break 'outer;
                    }
                }
            }
        }
    }
    out
}

/// The provider handle word based items carry (a real provider's handle is a small number the
/// host assigned; this one never is).
pub const WORD_PROVIDER: u32 = u32::MAX;

/// `WordBasedCompletionItemProvider.provideCompletionItems`: one `Text` item per word. `line` is
/// the text of the cursor line, `texts` the documents to read words from (the active one first).
pub fn word_items(line_no: i64, line: &str, column: i64, texts: &[&str]) -> Vec<SuggestItem> {
    let word = word_at(line, column);
    let (start, end) = match &word {
        Some(w) => (w.start_column, w.end_column),
        None => (column, column),
    };
    let position = Position {
        line: line_no,
        column,
    };
    let span = |end_column: i64| Span {
        start_line: line_no,
        start_column: start,
        end_line: line_no,
        end_column,
    };
    textual_words(
        texts.iter().copied(),
        word.as_ref().map(|w| w.word.as_str()),
    )
    .into_iter()
    .map(|w| {
        SuggestItem::new(
            position,
            Completion {
                sort_text: Some(w.clone()),
                label: w,
                kind: kind::TEXT,
                filter_text: None,
                range: ItemRange::InsertReplace {
                    insert: span(column),
                    replace: span(end),
                },
            },
            WORD_PROVIDER,
            false,
        )
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_touch_the_cursor_on_either_side() {
        let w = word_at("foo.bar", 4).expect("foo ends at 4");
        assert_eq!(
            (w.word.as_str(), w.start_column, w.end_column),
            ("foo", 1, 4)
        );
        let w = word_at("foo.bar", 5).expect("bar starts at 5");
        assert_eq!(w.word, "bar");
        assert!(word_at("a  b", 3).is_none());
    }

    #[test]
    fn the_word_until_the_cursor_is_cut_there() {
        let w = word_until("hello", 3);
        assert_eq!(
            (w.word.as_str(), w.start_column, w.end_column),
            ("he", 1, 3)
        );
        let w = word_until("a. ", 3);
        assert_eq!((w.word.as_str(), w.start_column, w.end_column), ("", 3, 3));
    }

    #[test]
    fn columns_count_utf16_units() {
        let words = line_words("\u{1F600} abc");
        assert_eq!(words[0].start_column, 1);
        assert_eq!(words[0].end_column, 3, "an astral character is two units");
        assert_eq!(words[1].start_column, 4);
    }

    #[test]
    fn textual_words_skip_the_leading_word_numbers_and_repeats() {
        let words = textual_words(["foo bar 12 foo\nbaz foo", "qux bar"], Some("foo"));
        assert_eq!(words, ["bar", "baz", "qux"]);
    }

    #[test]
    fn word_items_replace_the_whole_word_and_insert_up_to_the_cursor() {
        let items = word_items(1, "ab abc", 6, &["ab abc other"]);
        let labels: Vec<_> = items.iter().map(|i| i.text_label.as_str()).collect();
        assert_eq!(
            labels,
            ["ab", "other"],
            "the word under the cursor is left out"
        );
        match items[0].completion.range {
            ItemRange::InsertReplace { insert, replace } => {
                assert_eq!((insert.start_column, insert.end_column), (4, 6));
                assert_eq!((replace.start_column, replace.end_column), (4, 7));
            }
            ItemRange::Single(_) => panic!("insert and replace"),
        }
        assert_eq!(items[0].completion.kind, kind::TEXT);
    }
}
