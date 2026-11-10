//! upstream: src/vs/editor/contrib/suggest/browser/completionModel.ts @ 50e1350fc78d
//!
//! The sorted, filtered completion view model (upstream `CompletionModel`).
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/completionModel.ts.
//!
//! The model is built from the items of every provider. `set_line_context` changes the word the
//! user has typed so far; the view is then refiltered: incrementally when the word grew (only the
//! previously matching items are scored again), from all items otherwise. The filtered view is
//! cached until the next change, so asking for `items` twice does not refilter.

use std::cmp::Ordering;
use std::collections::BTreeSet;

use crate::filters::{
    FuzzyScore, FuzzyScoreOptions, FuzzyScorer, any_score, fuzzy_score,
    fuzzy_score_graceful_aggressive,
};
use crate::item::{Position, SuggestItem, kind};
use crate::sort::SnippetSortOrder;

/// The text before the cursor on the line, and how much the typed word changed since the last
/// view (upstream `LineContext`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineContext {
    pub leading_line_content: String,
    pub character_count_delta: i64,
}

/// The settings the model reads (the relevant parts of upstream `InternalSuggestOptions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuggestOptions {
    /// `editor.suggest.filterGraceful`: keep a match when the pattern has a typo, and rank by the
    /// best alignment. Ignored for more than 2,000 items (upstream does the same).
    pub filter_graceful: bool,
}

impl Default for SuggestOptions {
    /// Upstream `EditorOptions.suggest.defaultValue` for the fields the model reads.
    fn default() -> Self {
        Self {
            filter_graceful: true,
        }
    }
}

/// The distance of an item from the cursor, used to break score ties (upstream `WordDistance`).
pub trait WordDistance {
    fn distance(&self, position: Position, completion: &crate::item::Completion) -> i64;
}

/// Upstream `WordDistance.None`: every item is equally close.
pub struct NoWordDistance;

impl WordDistance for NoWordDistance {
    fn distance(&self, _position: Position, _completion: &crate::item::Completion) -> i64 {
        0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Refilter {
    Nothing,
    All,
    Incr,
}

pub struct CompletionModel {
    items: Vec<SuggestItem>,
    column: i64,
    line_context: LineContext,
    word_distance: Box<dyn WordDistance>,
    options: SuggestOptions,
    snippet_order: SnippetSortOrder,
    fuzzy_options: FuzzyScoreOptions,

    refilter: Refilter,
    filtered: Option<Vec<SuggestItem>>,
    incomplete_providers: BTreeSet<u32>,
    computations: usize,
}

impl CompletionModel {
    /// Upstream `new CompletionModel(items, column, lineContext, wordDistance, options,
    /// snippetSuggestions, fuzzyScoreOptions)`. `column` is the cursor column.
    pub fn new(
        items: Vec<SuggestItem>,
        column: i64,
        line_context: LineContext,
        word_distance: Box<dyn WordDistance>,
        options: SuggestOptions,
        snippet_order: SnippetSortOrder,
        fuzzy_options: FuzzyScoreOptions,
    ) -> Self {
        Self {
            items,
            column,
            line_context,
            word_distance,
            options,
            snippet_order,
            fuzzy_options,
            refilter: Refilter::All,
            filtered: None,
            incomplete_providers: BTreeSet::new(),
            computations: 0,
        }
    }

    /// Every item the model was made of, unfiltered, in source order (upstream `getItemsByProvider`
    /// groups these by provider).
    pub fn all_items(&self) -> &[SuggestItem] {
        &self.items
    }

    pub fn line_context(&self) -> &LineContext {
        &self.line_context
    }

    /// Upstream `set lineContext`: a changed word refilters the view, incrementally when it grew.
    pub fn set_line_context(&mut self, value: LineContext) {
        if self.line_context.leading_line_content != value.leading_line_content
            || self.line_context.character_count_delta != value.character_count_delta
        {
            self.refilter = if self.line_context.character_count_delta < value.character_count_delta
                && self.filtered.is_some()
            {
                Refilter::Incr
            } else {
                Refilter::All
            };
            self.line_context = value;
        }
    }

    /// The filtered and sorted items (upstream `items`).
    pub fn items(&mut self) -> &[SuggestItem] {
        self.ensure_cached_state();
        self.filtered.as_deref().unwrap_or(&[])
    }

    /// Providers whose items are incomplete: asking them again when the word grows gives more
    /// items (upstream `getIncompleteProvider`).
    pub fn incomplete_providers(&mut self) -> &BTreeSet<u32> {
        self.ensure_cached_state();
        &self.incomplete_providers
    }

    /// How many times the view was rebuilt (upstream compares the cached array by identity).
    pub fn computations(&self) -> usize {
        self.computations
    }

    fn ensure_cached_state(&mut self) {
        if self.refilter != Refilter::Nothing {
            self.create_cached_state();
        }
    }

    fn create_cached_state(&mut self) {
        self.computations += 1;
        self.incomplete_providers.clear();

        let source: Vec<SuggestItem> = if self.refilter == Refilter::All {
            self.items.clone()
        } else {
            self.filtered.take().unwrap_or_default()
        };

        let leading = self.line_context.leading_line_content.clone();
        let delta = self.line_context.character_count_delta;
        let leading_units: Vec<u16> = leading.encode_utf16().collect();

        // Score the cheap way unless the list is long (upstream's switch on the item count).
        let score_fn: FuzzyScorer = if !self.options.filter_graceful || source.len() > 2000 {
            fuzzy_score
        } else {
            fuzzy_score_graceful_aggressive
        };

        let mut seen_providers: BTreeSet<u32> = BTreeSet::new();
        let mut target: Vec<SuggestItem> = Vec::new();
        let mut cached_word: Option<(i64, String)> = None;

        for (i, mut item) in source.into_iter().enumerate() {
            if item.is_invalid {
                continue;
            }

            // Only the first valid item of a provider decides whether it is incomplete.
            if seen_providers.insert(item.provider) && item.incomplete {
                self.incomplete_providers.insert(item.provider);
            }

            // The word is the rest of the line that is filtered and scored against: the text
            // from the start of the replace range to the cursor.
            let overwrite_before = item.position.column - item.edit_start.column;
            let word_len = overwrite_before + delta - (item.position.column - self.column);
            let word = match &cached_word {
                Some((len, word)) if *len == word_len => word.clone(),
                _ => {
                    let word = last_units(&leading_units, word_len);
                    cached_word = Some((word_len, word.clone()));
                    word
                }
            };
            item.word = Some(word.clone());

            if word_len == 0 {
                // Nothing to score against: a constant rank, the fallback order decides.
                item.score = FuzzyScore::default_score();
            } else {
                let word_units: Vec<u16> = word.encode_utf16().collect();
                let word_low = word.to_lowercase();

                // Whitespace before the replace range does not take part in the match.
                let mut word_pos: i64 = 0;
                while word_pos < overwrite_before
                    && matches!(word_units.get(word_pos as usize), Some(&0x20) | Some(&0x09))
                {
                    word_pos += 1;
                }

                if word_pos >= word_len {
                    // Only whitespace before the replace range: the rules of no word apply.
                    item.score = FuzzyScore::default_score();
                } else {
                    let word_pos = word_pos as usize;
                    if let (Some(filter_text), Some(filter_low)) = (
                        item.completion.filter_text.clone(),
                        item.filter_text_low.clone(),
                    ) {
                        // The filter text must match the word; the label then gives the highlights.
                        let Some(matched) = score_fn(
                            &word,
                            &word_low,
                            word_pos,
                            &filter_text,
                            &filter_low,
                            0,
                            self.fuzzy_options,
                        ) else {
                            continue;
                        };
                        if compare_ignore_case(&filter_text, &item.text_label) == Ordering::Equal {
                            item.score = matched;
                        } else {
                            // Re-run on the label for highlights, but keep the rank of the filter match.
                            let mut score = any_score(
                                &word,
                                &word_low,
                                word_pos,
                                &item.text_label,
                                &item.label_low,
                                0,
                            );
                            score.score = matched.score;
                            item.score = score;
                        }
                    } else {
                        // Without a filter text the label is matched.
                        let Some(matched) = score_fn(
                            &word,
                            &word_low,
                            word_pos,
                            &item.text_label,
                            &item.label_low,
                            0,
                            self.fuzzy_options,
                        ) else {
                            continue;
                        };
                        item.score = matched;
                    }
                }
            }

            item.idx = i;
            item.distance = self.word_distance.distance(item.position, &item.completion);
            target.push(item);
        }

        target.sort_by(|a, b| self.compare(a, b));
        self.filtered = Some(target);
        self.refilter = Refilter::Nothing;
    }

    fn compare(&self, a: &SuggestItem, b: &SuggestItem) -> Ordering {
        match self.snippet_order {
            SnippetSortOrder::Top => {
                if a.completion.kind != b.completion.kind {
                    if a.completion.kind == kind::SNIPPET {
                        return Ordering::Less;
                    } else if b.completion.kind == kind::SNIPPET {
                        return Ordering::Greater;
                    }
                }
            }
            SnippetSortOrder::Bottom => {
                if a.completion.kind != b.completion.kind {
                    if a.completion.kind == kind::SNIPPET {
                        return Ordering::Greater;
                    } else if b.completion.kind == kind::SNIPPET {
                        return Ordering::Less;
                    }
                }
            }
            SnippetSortOrder::Inline => {}
        }
        compare_scored(a, b)
    }
}

/// Upstream `_compareCompletionItems`: higher score first, then nearer, then earlier in the source.
fn compare_scored(a: &SuggestItem, b: &SuggestItem) -> Ordering {
    b.score
        .score
        .cmp(&a.score.score)
        .then(a.distance.cmp(&b.distance))
        .then(a.idx.cmp(&b.idx))
}

/// Upstream `leadingLineContent.slice(-wordLen)`, on UTF-16 units. Zero gives the empty word; a
/// negative length counts from the start, as `slice` does with a negated index.
fn last_units(units: &[u16], word_len: i64) -> String {
    let len = units.len() as i64;
    let (start, end) = if word_len == 0 {
        return String::new();
    } else if word_len > 0 {
        ((len - word_len).max(0), len)
    } else {
        ((-word_len).min(len), len)
    };
    String::from_utf16_lossy(&units[start as usize..end as usize])
}

/// Upstream `compareIgnoreCase` for the equality test the model needs.
fn compare_ignore_case(a: &str, b: &str) -> Ordering {
    a.to_lowercase().cmp(&b.to_lowercase())
}
