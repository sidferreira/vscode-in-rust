//! upstream: src/vs/editor/contrib/suggest/browser/suggestController.ts @ 50e1350fc78d
//!
//! What accepting a completion inserts (ADR-0084): the range it replaces around the cursor now, the
//! text (a snippet expanded), where the caret goes, and the provider's additional edits.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggestController.ts (`_insertSuggestion`,
//! `getOverwriteInfo`) and src/vs/editor/contrib/snippet/browser/snippetController2.ts (`insert`).
//!
//! Columns are 1-based UTF-16, as the editor counts them. The item's own columns are those of the
//! line when the request was made; the user may have typed since, so the range is taken relative to
//! the cursor (`overwriteBefore` / `overwriteAfter`), as upstream does.

use vsr_exthost::language_features::dto::{INSERT_TEXT_AS_SNIPPET, INSERT_TEXT_KEEP_WHITESPACE};

use crate::item::{Position, Span, SuggestItem};
use crate::snippet;

/// `editor.suggest.insertMode`: what a completion does with the word's text after the cursor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum InsertMode {
    /// Insert before the word's rest (the default).
    #[default]
    Insert,
    /// Replace the word's rest.
    Replace,
}

/// The edit that accepts an item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcceptPlan {
    /// The cursor line, and the columns the text replaces on it.
    pub line: i64,
    pub start_column: i64,
    pub end_column: i64,
    pub text: String,
    /// The selection afterwards: UTF-16 offsets into `text`.
    pub selection: (usize, usize),
    /// The provider's other edits (a missing import).
    pub additional: Vec<(Span, String)>,
}

/// Adds `indent` after every line break of `text`: the lines of a multi-line completion start at the
/// indentation of the line it is inserted in (upstream's `adjustWhitespace`).
pub fn indent_lines(text: &str, indent: &str) -> String {
    if indent.is_empty() {
        return text.to_string();
    }
    text.replace('\n', &format!("\n{indent}"))
}

/// The plan for accepting `item` with the cursor at `cursor` now. `line_indent` is the leading
/// whitespace of the cursor line.
pub fn plan(item: &SuggestItem, cursor: Position, mode: InsertMode, line_indent: &str) -> AcceptPlan {
    let source = item.source.as_deref();
    let insert_text = source.map_or(item.completion.label.as_str(), |s| s.insert_text.as_str());
    let rules = source.map_or(0, |s| s.insert_text_rules);

    let end = match mode {
        InsertMode::Insert => item.edit_insert_end,
        InsertMode::Replace => item.edit_replace_end,
    };
    let before = (cursor.column - item.edit_start.column).clamp(0, cursor.column - 1);
    let after = (end.column - item.position.column).max(0);

    let adjust = rules & INSERT_TEXT_KEEP_WHITESPACE == 0;
    let expanded = if rules & INSERT_TEXT_AS_SNIPPET != 0 {
        let text = if adjust {
            indent_lines(insert_text, line_indent)
        } else {
            insert_text.to_string()
        };
        snippet::expand(&text)
    } else {
        let text = if adjust {
            indent_lines(insert_text, line_indent)
        } else {
            insert_text.to_string()
        };
        snippet::plain(&text)
    };

    let additional = source
        .map(|s| {
            s.additional_text_edits
                .iter()
                .map(|e| {
                    (
                        Span {
                            start_line: i64::from(e.range.start_line),
                            start_column: i64::from(e.range.start_column),
                            end_line: i64::from(e.range.end_line),
                            end_column: i64::from(e.range.end_column),
                        },
                        e.text.clone(),
                    )
                })
                .collect()
        })
        .unwrap_or_default();

    AcceptPlan {
        line: cursor.line,
        start_column: cursor.column - before,
        end_column: cursor.column + after,
        text: expanded.text,
        selection: expanded.selection,
        additional,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::from_dto;
    use serde_json::json;
    use vsr_exthost::language_features::dto::CompletionList;

    fn item(v: serde_json::Value) -> SuggestItem {
        let list = CompletionList::from_json(&json!({"x": 1, "b": [v]})).expect("a list");
        from_dto(&list.items[0], Position { line: 1, column: 4 }, 0, false)
    }

    fn at(column: i64) -> Position {
        Position { line: 1, column: column }
    }

    #[test]
    fn the_range_follows_the_cursor_not_the_request() {
        // Requested at column 4 (`fo|`... `foo`), the label replaces columns 1..4; one more character typed.
        let it = item(json!({"a": "foobar", "j": {"startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 4}}));
        let p = plan(&it, at(5), InsertMode::Insert, "");
        assert_eq!((p.start_column, p.end_column), (1, 5));
        assert_eq!((p.text.as_str(), p.selection), ("foobar", (6, 6)));
    }

    #[test]
    fn replace_mode_takes_the_rest_of_the_word() {
        let it = item(json!({"a": "foobar", "j": {"insert": {"startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 4},
            "replace": {"startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 7}}}));
        let insert = plan(&it, at(4), InsertMode::Insert, "");
        let replace = plan(&it, at(4), InsertMode::Replace, "");
        assert_eq!(insert.end_column, 4);
        assert_eq!(replace.end_column, 7);
    }

    #[test]
    fn a_snippet_expands_and_indents_its_lines() {
        let it = item(json!({"a": "if", "h": "if (${1:cond}) {\n\t$0\n}", "i": 4,
            "j": {"startLineNumber": 1, "startColumn": 3, "endLineNumber": 1, "endColumn": 4}}));
        let p = plan(&it, at(4), InsertMode::Insert, "  ");
        assert_eq!(p.text, "if (cond) {\n  \t\n  }");
        assert_eq!(p.selection, (4, 8), "the first placeholder is selected");
    }

    #[test]
    fn keep_whitespace_leaves_the_lines_alone() {
        let it = item(json!({"a": "x", "h": "a\nb", "i": 1,
            "j": {"startLineNumber": 1, "startColumn": 4, "endLineNumber": 1, "endColumn": 4}}));
        assert_eq!(plan(&it, at(4), InsertMode::Insert, "  ").text, "a\nb");
    }

    #[test]
    fn a_plain_dollar_is_not_a_snippet() {
        let it = item(json!({"a": "$x", "j": {"startLineNumber": 1, "startColumn": 4, "endLineNumber": 1, "endColumn": 4}}));
        let p = plan(&it, at(4), InsertMode::Insert, "");
        assert_eq!((p.text.as_str(), p.selection), ("$x", (2, 2)));
    }

    #[test]
    fn the_label_is_the_text_without_a_source_item() {
        let mut it = item(json!({"a": "word", "j": {"startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 4}}));
        it.source = None;
        let p = plan(&it, at(4), InsertMode::Insert, "");
        assert_eq!((p.text.as_str(), p.start_column), ("word", 1));
    }

    #[test]
    fn additional_edits_come_along() {
        let it = item(json!({"a": "x", "j": {"startLineNumber": 1, "startColumn": 4, "endLineNumber": 1, "endColumn": 4},
            "l": [{"range": {"startLineNumber": 1, "startColumn": 1, "endLineNumber": 1, "endColumn": 1}, "text": "import x;\n"}]}));
        let p = plan(&it, at(4), InsertMode::Insert, "");
        assert_eq!(p.additional.len(), 1);
        assert_eq!(p.additional[0].1, "import x;\n");
    }
}
