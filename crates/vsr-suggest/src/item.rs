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
    pub const TEXT: u32 = 18;
    pub const TOOL: u32 = 27;
    /// The highest value (upstream uses it for compare).
    pub const SNIPPET: u32 = 28;

    /// The codicon of a kind (upstream `CompletionItemKinds.toIcon`): its name and code point. An
    /// unknown kind is a property.
    pub fn icon(kind: u32) -> (&'static str, char) {
        match kind {
            0 => ("symbol-method", '\u{ea8c}'),
            1 => ("symbol-function", '\u{ea8c}'),
            2 => ("symbol-constructor", '\u{ea8c}'),
            3 => ("symbol-field", '\u{eb5f}'),
            4 => ("symbol-variable", '\u{ea88}'),
            5 => ("symbol-class", '\u{eb5b}'),
            6 => ("symbol-struct", '\u{ea91}'),
            7 => ("symbol-interface", '\u{eb61}'),
            8 => ("symbol-module", '\u{ea8b}'),
            10 => ("symbol-event", '\u{ea86}'),
            11 => ("symbol-operator", '\u{eb64}'),
            12 => ("symbol-unit", '\u{ea96}'),
            13 => ("symbol-value", '\u{ea95}'),
            14 => ("symbol-constant", '\u{eb5d}'),
            15 => ("symbol-enum", '\u{ea95}'),
            16 => ("symbol-enum-member", '\u{eb5e}'),
            17 => ("symbol-keyword", '\u{eb62}'),
            18 => ("symbol-text", '\u{ea93}'),
            19 => ("symbol-color", '\u{eb5c}'),
            20 => ("symbol-file", '\u{eb60}'),
            21 => ("symbol-reference", '\u{ea94}'),
            22 => ("symbol-customcolor", '\u{eb5c}'),
            23 => ("symbol-folder", '\u{ea83}'),
            24 => ("symbol-type-parameter", '\u{ea92}'),
            25 => ("account", '\u{eb99}'),
            26 => ("issues", '\u{eb0c}'),
            27 => ("tools", '\u{eb6d}'),
            28 => ("symbol-snippet", '\u{eb66}'),
            _ => ("symbol-property", '\u{eb65}'),
        }
    }
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

    /// The provider's item as the host sent it (insert text, edits, commit characters, ...), for
    /// the widget (ADR-0084). `None` for items the app makes itself (word based suggestions).
    pub source: Option<Box<vsr_exthost::language_features::dto::CompletionItem>>,
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
            source: None,
        }
    }
}
