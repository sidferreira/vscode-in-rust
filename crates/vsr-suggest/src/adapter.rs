//! The extension host's completion items as the suggest model reads them (ADR-0082).
//!
//! Upstream source: src/vs/workbench/api/common/extHostTypeConverters.ts (`CompletionItemKind`
//! defaults to `Property` when a provider gives none) and src/vs/editor/contrib/suggest/browser/
//! suggest.ts (the model's input).
//!
//! The host counts columns in UTF-16 units; the editor counts `char`s (ADR-0065). The conversion
//! needs the text of the document the columns point into, so the worker converts a list with
//! [`list_to_chars`] before it sends it, and nothing downstream sees a protocol column.
//!
//! Columns are 1-based everywhere. A column inside a surrogate pair rounds down to the character
//! that starts there; a column past the end of the line keeps counting one unit per position.

use vsr_exthost::language_features::dto::{CompletionItem, CompletionList, CompletionRange, Range};

use crate::item::{Completion, ItemRange, Position, Span, SuggestItem, kind};

/// The text of 1-based `line` of `text`, without its line break.
pub fn line_text(text: &str, line: u32) -> &str {
    let l = text
        .split('\n')
        .nth(line.saturating_sub(1) as usize)
        .unwrap_or("");
    l.strip_suffix('\r').unwrap_or(l)
}

/// A 1-based UTF-16 column of `line` as the 1-based `char` column it falls on.
pub fn utf16_to_char_column(line: &str, column: u32) -> u32 {
    let target = column.saturating_sub(1) as usize;
    let mut units = 0usize;
    for (k, c) in line.chars().enumerate() {
        let len = c.len_utf16();
        if target < units + len {
            return k as u32 + 1;
        }
        units += len;
    }
    let chars = line.chars().count();
    (chars + 1 + target.saturating_sub(units)) as u32
}

/// A 1-based `char` column of `line` as the 1-based UTF-16 column the host expects.
pub fn char_to_utf16_column(line: &str, column: u32) -> u32 {
    let want = column.saturating_sub(1) as usize;
    let mut units = 0usize;
    let mut taken = 0usize;
    for c in line.chars().take(want) {
        units += c.len_utf16();
        taken += 1;
    }
    (units + (want - taken) + 1) as u32
}

/// Converts the columns of `range` from UTF-16 to `char`s, reading the lines from `text`.
pub fn range_to_chars(text: &str, range: Range) -> Range {
    Range {
        start_column: utf16_to_char_column(line_text(text, range.start_line), range.start_column),
        end_column: utf16_to_char_column(line_text(text, range.end_line), range.end_column),
        ..range
    }
}

/// Converts every range of a list (item ranges and the edits that come with them) to `char`
/// columns, in place.
pub fn list_to_chars(list: &mut CompletionList, text: &str) {
    for item in &mut list.items {
        item.range = match &item.range {
            CompletionRange::Single(r) => CompletionRange::Single(range_to_chars(text, *r)),
            CompletionRange::InsertReplace { insert, replace } => CompletionRange::InsertReplace {
                insert: range_to_chars(text, *insert),
                replace: range_to_chars(text, *replace),
            },
        };
        for edit in &mut item.additional_text_edits {
            edit.range = range_to_chars(text, edit.range);
        }
    }
}

fn span(r: Range) -> Span {
    Span {
        start_line: i64::from(r.start_line),
        start_column: i64::from(r.start_column),
        end_line: i64::from(r.end_line),
        end_column: i64::from(r.end_column),
    }
}

/// The suggest item of one provider's completion, requested at `position` (1-based, `char`
/// columns; see [`list_to_chars`]). `provider` identifies the provider, `incomplete` is its
/// list's `isIncomplete`.
pub fn from_dto(
    item: &CompletionItem,
    position: Position,
    provider: u32,
    incomplete: bool,
) -> SuggestItem {
    let range = match item.range {
        CompletionRange::Single(r) => ItemRange::Single(span(r)),
        CompletionRange::InsertReplace { insert, replace } => ItemRange::InsertReplace {
            insert: span(insert),
            replace: span(replace),
        },
    };
    SuggestItem::new(
        position,
        Completion {
            label: item.label.clone(),
            kind: item.kind.unwrap_or(kind::PROPERTY),
            filter_text: item.filter_text.clone(),
            sort_text: item.sort_text.clone(),
            range,
        },
        provider,
        incomplete,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn list(v: serde_json::Value) -> CompletionList {
        CompletionList::from_json(&v).expect("a completion list")
    }

    #[test]
    fn ascii_columns_are_the_same_in_both_units() {
        assert_eq!(utf16_to_char_column("let x", 5), 5);
        assert_eq!(char_to_utf16_column("let x", 5), 5);
        assert_eq!(utf16_to_char_column("", 1), 1);
    }

    #[test]
    fn an_emoji_before_the_cursor_counts_two_units_and_one_char() {
        // "a😀b": offsets a=0, 😀=1..3, b=3; the host's column 4 is before `b`.
        let line = "a\u{1F600}b";
        assert_eq!(
            utf16_to_char_column(line, 4),
            3,
            "before b is char column 3"
        );
        assert_eq!(
            char_to_utf16_column(line, 3),
            4,
            "char column 3 is UTF-16 column 4"
        );
        assert_eq!(utf16_to_char_column(line, 2), 2, "before the emoji");
        assert_eq!(char_to_utf16_column(line, 2), 2);
    }

    #[test]
    fn a_column_inside_a_surrogate_pair_rounds_down_to_its_char() {
        let line = "a\u{1F600}b";
        assert_eq!(
            utf16_to_char_column(line, 3),
            2,
            "between the two halves of the emoji"
        );
    }

    #[test]
    fn columns_past_the_end_keep_counting() {
        assert_eq!(utf16_to_char_column("ab", 3), 3, "the end of the line");
        assert_eq!(utf16_to_char_column("ab", 5), 5);
        assert_eq!(char_to_utf16_column("ab", 5), 5);
        // Accented letters are one unit each; CJK too; only astral characters are two.
        assert_eq!(utf16_to_char_column("\u{e9}\u{4e2d}x", 3), 3);
    }

    #[test]
    fn line_text_reads_one_line_without_its_break() {
        let text = "one\r\ntwo\nthree";
        assert_eq!(line_text(text, 1), "one");
        assert_eq!(line_text(text, 2), "two");
        assert_eq!(line_text(text, 3), "three");
        assert_eq!(line_text(text, 4), "");
    }

    #[test]
    fn a_list_is_converted_to_char_columns_in_items_and_edits() {
        // Line 1 is "a😀b.", the completion replaces `b.` (UTF-16 columns 4..6, chars 3..5).
        let text = "a\u{1F600}b.\n";
        let mut l = list(json!({
            "b": [{
                "a": "bar",
                "b": 2,
                "j": { "startLineNumber": 1, "startColumn": 4, "endLineNumber": 1, "endColumn": 6 },
                "l": [{ "range": { "startLineNumber": 1, "startColumn": 2, "endLineNumber": 1, "endColumn": 3 },
                        "text": "" }]
            }]
        }));
        list_to_chars(&mut l, text);
        let item = &l.items[0];
        assert_eq!(item.range, CompletionRange::Single(Range::new(1, 3, 1, 5)));
        assert_eq!(item.additional_text_edits[0].range, Range::new(1, 2, 1, 2));
    }

    #[test]
    fn default_ranges_reach_items_that_carry_none_and_are_converted_once() {
        let text = "x\u{1F600}yz\n";
        let mut l = list(json!({
            "a": {
                "insert": { "startLineNumber": 1, "startColumn": 2, "endLineNumber": 1, "endColumn": 4 },
                "replace": { "startLineNumber": 1, "startColumn": 2, "endLineNumber": 1, "endColumn": 5 }
            },
            "b": [{ "a": "one" }, { "a": "two" }],
            "c": true,
            "x": 7
        }));
        list_to_chars(&mut l, text);
        assert!(l.is_incomplete);
        assert_eq!(l.cache_id, Some(7));
        for item in &l.items {
            // "x😀yz": the insert range is columns 2..4 (before the emoji, before `y`), which is
            // chars 2..3; the replace range ends at column 5 (before `z`), char 4.
            assert_eq!(
                item.range,
                CompletionRange::InsertReplace {
                    insert: Range::new(1, 2, 1, 3),
                    replace: Range::new(1, 2, 1, 4),
                }
            );
        }
    }

    #[test]
    fn an_item_maps_to_a_suggest_item_with_its_fields() {
        let mut l = list(json!({
            "b": [{
                "a": "push",
                "b": 17,
                "e": "0002",
                "f": "push()",
                "j": { "startLineNumber": 3, "startColumn": 5, "endLineNumber": 3, "endColumn": 8 }
            }],
            "c": true
        }));
        list_to_chars(&mut l, "\n\n    pus\n");
        let s = from_dto(&l.items[0], Position { line: 3, column: 8 }, 4, true);
        assert_eq!(s.text_label, "push");
        assert_eq!(s.completion.kind, kind::KEYWORD);
        assert_eq!(s.completion.filter_text.as_deref(), Some("push()"));
        assert_eq!(s.completion.sort_text.as_deref(), Some("0002"));
        assert_eq!(s.provider, 4);
        assert!(s.incomplete);
        assert!(!s.is_invalid);
        assert_eq!(
            s.completion.range,
            ItemRange::Single(Span {
                start_line: 3,
                start_column: 5,
                end_line: 3,
                end_column: 8
            })
        );
    }

    #[test]
    fn a_missing_kind_is_property_and_a_range_on_another_line_is_invalid() {
        let mut l = list(json!({
            "b": [{
                "a": "later",
                "j": { "startLineNumber": 2, "startColumn": 1, "endLineNumber": 2, "endColumn": 3 }
            }]
        }));
        list_to_chars(&mut l, "x\ny\n");
        let s = from_dto(&l.items[0], Position { line: 1, column: 2 }, 0, false);
        assert_eq!(s.completion.kind, kind::PROPERTY);
        assert!(
            s.is_invalid,
            "the range starts on line 2, the cursor is on line 1"
        );
    }
}
