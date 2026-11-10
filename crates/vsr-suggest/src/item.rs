//! upstream: src/vs/editor/contrib/suggest/browser/suggest.ts @ 50e1350fc78d
//!
//! The suggestion item (upstream `CompletionItem` in `suggest.ts`): a provider's completion, the
//! position it was requested at, and the derived values the model filters and sorts with.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggest.ts (class CompletionItem).
//!
//! Positions are 1-based line and column, as in upstream. Columns may be negative when a
//! completion's range starts before the line (the model tolerates that).

use crate::filters::FuzzyScore;

/// Upstream `languages.CompletionItemKind` values (numeric, like the language feature DTOs).
pub mod kind {
    pub const PROPERTY: u32 = 9;
    pub const OPERATOR: u32 = 11;
    pub const KEYWORD: u32 = 17;
    pub const SNIPPET: u32 = 27;
}

/// A 1-based line and column (upstream `IPosition`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: i64,
    pub column: i64,
}

/// A range from one position to another (upstream `IRange`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub start_line: i64,
    pub start_column: i64,
    pub end_line: i64,
    pub end_column: i64,
}

impl Span {
    /// Upstream `Range.spansMultipleLines`.
    pub fn spans_multiple_lines(&self) -> bool {
        self.end_line > self.start_line
    }
}

/// The text a completion replaces: one range, or an insert range and a replace range (upstream
/// `CompletionItem.range`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemRange {
    Single(Span),
    InsertReplace { insert: Span, replace: Span },
}

/// The part of a provider's completion item the model reads (upstream `languages.CompletionItem`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    pub label: String,
    pub kind: u32,
    pub filter_text: Option<String>,
    pub sort_text: Option<String>,
    pub range: ItemRange,
}

/// A completion item with what the model derives from it.
#[derive(Clone, Debug)]
pub struct SuggestItem {
    pub position: Position,
    pub completion: Completion,
    /// Identifies the provider that offered the item.
    pub provider: u32,
    /// The provider's container was marked incomplete (`CompletionList.isIncomplete`).
    pub incomplete: bool,

    pub text_label: String,
    pub label_low: String,
    pub sort_text_low: Option<String>,
    pub filter_text_low: Option<String>,

    /// The label is empty or the range is not on the cursor line (upstream `isInvalid`).
    pub is_invalid: bool,
    pub edit_start: Position,
    pub edit_insert_end: Position,
    pub edit_replace_end: Position,

    /// Set by filtering: the score, the word it was scored against and the sort inputs.
    pub score: FuzzyScore,
    pub distance: i64,
    pub idx: usize,
    pub word: Option<String>,
}

impl SuggestItem {
    /// Upstream `new CompletionItem(position, completion, container, provider)`.
    pub fn new(
        position: Position,
        completion: Completion,
        provider: u32,
        incomplete: bool,
    ) -> Self {
        let text_label = completion.label.clone();
        let label_low = text_label.to_lowercase();
        let sort_text_low = completion
            .sort_text
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| s.to_lowercase());
        let filter_text_low = completion.filter_text.as_deref().map(|s| s.to_lowercase());

        let mut is_invalid = text_label.is_empty();
        let (edit_start, edit_insert_end, edit_replace_end) = match completion.range {
            ItemRange::Single(range) => {
                is_invalid =
                    is_invalid || range.spans_multiple_lines() || range.start_line != position.line;
                (
                    Position {
                        line: range.start_line,
                        column: range.start_column,
                    },
                    Position {
                        line: range.end_line,
                        column: range.end_column,
                    },
                    Position {
                        line: range.end_line,
                        column: range.end_column,
                    },
                )
            }
            ItemRange::InsertReplace { insert, replace } => {
                is_invalid = is_invalid
                    || insert.spans_multiple_lines()
                    || replace.spans_multiple_lines()
                    || insert.start_line != position.line
                    || replace.start_line != position.line
                    || insert.start_column != replace.start_column;
                (
                    Position {
                        line: insert.start_line,
                        column: insert.start_column,
                    },
                    Position {
                        line: insert.end_line,
                        column: insert.end_column,
                    },
                    Position {
                        line: replace.end_line,
                        column: replace.end_column,
                    },
                )
            }
        };

        Self {
            position,
            completion,
            provider,
            incomplete,
            text_label,
            label_low,
            sort_text_low,
            filter_text_low,
            is_invalid,
            edit_start,
            edit_insert_end,
            edit_replace_end,
            score: FuzzyScore::default_score(),
            distance: 0,
            idx: 0,
            word: None,
        }
    }
}
