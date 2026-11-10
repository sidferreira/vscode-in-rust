//! upstream: src/vs/editor/contrib/suggest/browser/suggest.ts @ 50e1350fc78d
//!
//! Sort order of suggestions for the snippet placement setting (upstream `getSuggestionComparator`
//! in `suggest.ts`). The score-based ordering used after filtering lives in `model`.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggest.ts (defaultComparator, snippetUpComparator, snippetDownComparator).

use std::cmp::Ordering;

use crate::item::{SuggestItem, kind};

/// Where snippets go among the other suggestions (upstream `SnippetSortOrder`; the setting's
/// `top`, `inline` and `bottom`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnippetSortOrder {
    Top,
    Inline,
    Bottom,
}

/// The comparator for a snippet placement (upstream `getSuggestionComparator`).
pub fn suggestion_comparator(
    order: SnippetSortOrder,
) -> fn(&SuggestItem, &SuggestItem) -> Ordering {
    match order {
        SnippetSortOrder::Top => snippet_up_comparator,
        SnippetSortOrder::Bottom => snippet_down_comparator,
        SnippetSortOrder::Inline => default_comparator,
    }
}

/// Compares two strings as JavaScript does: by UTF-16 code units.
fn utf16_cmp(a: &str, b: &str) -> Ordering {
    a.encode_utf16().cmp(b.encode_utf16())
}

/// Upstream `defaultComparator`: sort text first, then label, then kind.
fn default_comparator(a: &SuggestItem, b: &SuggestItem) -> Ordering {
    if let (Some(sa), Some(sb)) = (&a.sort_text_low, &b.sort_text_low) {
        match utf16_cmp(sa, sb) {
            Ordering::Equal => {}
            other => return other,
        }
    }
    match utf16_cmp(&a.text_label, &b.text_label) {
        Ordering::Equal => {}
        other => return other,
    }
    a.completion.kind.cmp(&b.completion.kind)
}

/// Upstream `snippetUpComparator`: snippets first.
fn snippet_up_comparator(a: &SuggestItem, b: &SuggestItem) -> Ordering {
    if a.completion.kind != b.completion.kind {
        if a.completion.kind == kind::SNIPPET {
            return Ordering::Less;
        } else if b.completion.kind == kind::SNIPPET {
            return Ordering::Greater;
        }
    }
    default_comparator(a, b)
}

/// Upstream `snippetDownComparator`: snippets last.
fn snippet_down_comparator(a: &SuggestItem, b: &SuggestItem) -> Ordering {
    if a.completion.kind != b.completion.kind {
        if a.completion.kind == kind::SNIPPET {
            return Ordering::Greater;
        } else if b.completion.kind == kind::SNIPPET {
            return Ordering::Less;
        }
    }
    default_comparator(a, b)
}
