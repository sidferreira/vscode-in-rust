//! The quick pick model: rows (items and separators), the active row and navigation.
//!
//! upstream: src/vs/platform/quickinput/browser/quickInputList.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/common/quickInput.ts @ 50e1350fc78d
//!
//! UI free. Navigation follows `QuickInputList.focus`: Up/Down loop (`shouldLoop`) and skip
//! separators, PageUp/PageDown clamp. After the rows change the first item is active.

use crate::fuzzy::{
    ItemAccessor, ItemScore, Match, PreparedQuery, compare_scored, prepare_query, score_item_fuzzy_uncached,
};

/// One selectable entry (`IQuickPickItem`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickItem {
    /// Caller's key (an index into its own data). Reported back on accept.
    pub id: usize,
    pub label: String,
    pub description: Option<String>,
    /// If the item is a file: its path (the scorer prefers identity matches on it).
    pub path: Option<String>,
    /// Char ranges of `label` to highlight.
    pub label_highlights: Vec<Match>,
    /// Char ranges of `description` to highlight.
    pub description_highlights: Vec<Match>,
}

impl PickItem {
    pub fn new(id: usize, label: impl Into<String>) -> Self {
        Self { id, label: label.into(), ..Self::default() }
    }

    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        let d: String = description.into();
        self.description = (!d.is_empty()).then_some(d);
        self
    }
}

/// A row of the list: an item or a separator label (`IQuickPickSeparator`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PickRow {
    Separator(String),
    Item(PickItem),
}

impl PickRow {
    pub fn item(&self) -> Option<&PickItem> {
        match self {
            PickRow::Item(i) => Some(i),
            PickRow::Separator(_) => None,
        }
    }
}

/// The rows and the active (focused) row.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PickList {
    rows: Vec<PickRow>,
    active: Option<usize>,
}

impl PickList {
    pub fn rows(&self) -> &[PickRow] {
        &self.rows
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Number of items (rows that are not separators).
    pub fn item_count(&self) -> usize {
        self.rows.iter().filter(|r| r.item().is_some()).count()
    }

    /// Replace the rows; the first item becomes active.
    pub fn set_rows(&mut self, rows: Vec<PickRow>) {
        self.rows = rows;
        self.active = self.first_item_from(0);
    }

    /// Replace the rows and keep the active item when it is still there (matched by `id`).
    pub fn set_rows_keep_active(&mut self, rows: Vec<PickRow>) {
        let prev = self.active_item().map(|i| i.id);
        self.rows = rows;
        self.active = prev
            .and_then(|id| self.rows.iter().position(|r| r.item().is_some_and(|i| i.id == id)))
            .or_else(|| self.first_item_from(0));
    }

    /// Index (into `rows`) of the active row.
    pub fn active(&self) -> Option<usize> {
        self.active
    }

    pub fn active_item(&self) -> Option<&PickItem> {
        self.active.and_then(|i| self.rows.get(i)).and_then(PickRow::item)
    }

    /// Activate `row` if it is an item.
    pub fn set_active(&mut self, row: usize) {
        if self.rows.get(row).is_some_and(|r| r.item().is_some()) {
            self.active = Some(row);
        }
    }

    fn is_item(&self, i: usize) -> bool {
        self.rows.get(i).is_some_and(|r| r.item().is_some())
    }

    fn first_item_from(&self, start: usize) -> Option<usize> {
        (start..self.rows.len()).find(|&i| self.is_item(i))
    }

    fn last_item_to(&self, end: usize) -> Option<usize> {
        if self.rows.is_empty() {
            return None;
        }
        (0..=end.min(self.rows.len() - 1)).rev().find(|&i| self.is_item(i))
    }

    pub fn first(&mut self) {
        self.active = self.first_item_from(0);
    }

    pub fn last(&mut self) {
        if !self.rows.is_empty() {
            self.active = self.last_item_to(self.rows.len() - 1);
        }
    }

    /// Down: the next item, looping to the first after the last.
    pub fn next(&mut self) {
        self.active = match self.active {
            None => self.first_item_from(0),
            Some(a) => self.first_item_from(a + 1).or_else(|| self.first_item_from(0)),
        };
    }

    /// Up: the previous item, looping to the last before the first.
    pub fn previous(&mut self) {
        let last = self.rows.len().saturating_sub(1);
        self.active = match self.active {
            None => self.last_item_to(last),
            Some(a) => (a > 0).then(|| self.last_item_to(a - 1)).flatten().or_else(|| self.last_item_to(last)),
        };
    }

    /// PageDown: `page` rows down (no loop); the last item when it is a short way.
    pub fn next_page(&mut self, page: usize) {
        let Some(a) = self.active else { return self.first() };
        let target = (a + page.max(1)).min(self.rows.len() - 1);
        self.active = self.first_item_from(target).or_else(|| self.last_item_to(target)).or(self.active);
    }

    /// PageUp: `page` rows up (no loop).
    pub fn previous_page(&mut self, page: usize) {
        let Some(a) = self.active else { return self.first() };
        let target = a.saturating_sub(page.max(1));
        self.active = self.last_item_to(target).or_else(|| self.first_item_from(target)).or(self.active);
    }
}

struct PickAccessor;

impl ItemAccessor<PickItem> for PickAccessor {
    fn label(&self, item: &PickItem) -> Option<String> {
        Some(item.label.clone())
    }
    fn description(&self, item: &PickItem) -> Option<String> {
        item.description.clone()
    }
    fn path(&self, item: &PickItem) -> Option<String> {
        item.path.clone()
    }
}

/// Filter and sort `items` by the fuzzy score of `query` (label and description, matches on the
/// label first, like the quick pick of VS Code with `matchOnDescription`). An empty query keeps
/// every item in its order, without highlights. Returns item rows only.
pub fn filter_items(items: &[PickItem], query: &str) -> Vec<PickRow> {
    let q = prepare_query(query);
    if q.normalized.is_empty() {
        return items
            .iter()
            .cloned()
            .map(|mut i| {
                i.label_highlights.clear();
                i.description_highlights.clear();
                PickRow::Item(i)
            })
            .collect();
    }
    let mut scored = score_items(items, &q, true);
    scored.sort_by(|(a, sa), (b, sb)| compare_scored(a, b, sa, sb, &q, &PickAccessor).cmp(&0));
    scored.into_iter().map(|(i, s)| PickRow::Item(with_highlights(i, s))).collect()
}

/// Score `items`, dropping the ones that do not match.
pub fn score_items(items: &[PickItem], query: &PreparedQuery, allow_non_contiguous: bool) -> Vec<(PickItem, ItemScore)> {
    items
        .iter()
        .filter_map(|i| {
            let s = score_item_fuzzy_uncached(Some(i), query, allow_non_contiguous, &PickAccessor);
            (s.score != 0).then(|| (i.clone(), s))
        })
        .collect()
}

pub fn with_highlights(mut item: PickItem, score: ItemScore) -> PickItem {
    item.label_highlights = score.label_match.unwrap_or_default();
    item.description_highlights = score.description_match.unwrap_or_default();
    item
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(labels: &[&str]) -> Vec<PickRow> {
        labels.iter().enumerate().map(|(i, l)| PickRow::Item(PickItem::new(i, *l))).collect()
    }

    fn with_sep() -> PickList {
        let mut l = PickList::default();
        l.set_rows(vec![
            PickRow::Separator("recent".into()),
            PickRow::Item(PickItem::new(0, "a")),
            PickRow::Item(PickItem::new(1, "b")),
            PickRow::Separator("files".into()),
            PickRow::Item(PickItem::new(2, "c")),
        ]);
        l
    }

    #[test]
    fn first_item_is_active_after_set_rows() {
        let l = with_sep();
        assert_eq!(l.active(), Some(1));
        assert_eq!(l.item_count(), 3);
    }

    #[test]
    fn next_skips_separators_and_loops() {
        let mut l = with_sep();
        l.next();
        assert_eq!(l.active_item().unwrap().id, 1);
        l.next();
        assert_eq!(l.active_item().unwrap().id, 2, "skips the separator");
        l.next();
        assert_eq!(l.active_item().unwrap().id, 0, "loops to the first item");
    }

    #[test]
    fn previous_skips_separators_and_loops() {
        let mut l = with_sep();
        l.previous();
        assert_eq!(l.active_item().unwrap().id, 2, "loops to the last item");
        l.previous();
        assert_eq!(l.active_item().unwrap().id, 1);
        l.previous();
        l.previous();
        assert_eq!(l.active_item().unwrap().id, 2);
    }

    #[test]
    fn pages_clamp_without_looping() {
        let mut l = PickList::default();
        l.set_rows(items(&["a", "b", "c", "d", "e", "f"]));
        l.next_page(4);
        assert_eq!(l.active(), Some(4));
        l.next_page(4);
        assert_eq!(l.active(), Some(5));
        l.next_page(4);
        assert_eq!(l.active(), Some(5), "no loop at the end");
        l.previous_page(4);
        assert_eq!(l.active(), Some(1));
        l.previous_page(4);
        assert_eq!(l.active(), Some(0));
        l.previous_page(4);
        assert_eq!(l.active(), Some(0));
    }

    #[test]
    fn page_landing_on_a_separator_moves_on() {
        let mut l = with_sep();
        l.next_page(2); // from row 1 to row 3 (separator) -> row 4
        assert_eq!(l.active(), Some(4));
        l.previous_page(1); // to row 3 (separator) -> back to row 2
        assert_eq!(l.active(), Some(2));
    }

    #[test]
    fn empty_list_has_no_active_item() {
        let mut l = PickList::default();
        l.next();
        l.previous();
        l.next_page(5);
        l.previous_page(5);
        l.first();
        l.last();
        assert_eq!(l.active(), None);
        assert!(l.active_item().is_none());
    }

    #[test]
    fn keep_active_follows_the_item() {
        let mut l = PickList::default();
        l.set_rows(items(&["a", "b", "c"]));
        l.next(); // b (id 1)
        let mut rows = items(&["a", "b", "c"]);
        rows.insert(0, PickRow::Item(PickItem::new(9, "new")));
        l.set_rows_keep_active(rows);
        assert_eq!(l.active_item().unwrap().id, 1);
        l.set_rows_keep_active(items(&["z"]));
        assert_eq!(l.active_item().unwrap().label, "z", "falls back to the first item");
    }

    #[test]
    fn filter_keeps_order_for_an_empty_query() {
        let items = vec![PickItem::new(0, "zeta"), PickItem::new(1, "alpha")];
        let rows = filter_items(&items, "");
        assert_eq!(rows.iter().map(|r| r.item().unwrap().id).collect::<Vec<_>>(), [0, 1]);
    }

    #[test]
    fn filter_sorts_by_score_and_highlights() {
        let items = vec![
            PickItem::new(0, "open settings"),
            PickItem::new(1, "settings"),
            PickItem::new(2, "unrelated"),
        ];
        let rows = filter_items(&items, "sett");
        let ids: Vec<_> = rows.iter().map(|r| r.item().unwrap().id).collect();
        assert_eq!(ids, [1, 0], "the label prefix match comes first, non-matching items are dropped");
        assert_eq!(rows[0].item().unwrap().label_highlights, vec![Match::new(0, 4)]);
    }

    #[test]
    fn filter_matches_on_description() {
        let items = vec![PickItem::new(0, "main.rs").with_description("src/app"), PickItem::new(1, "lib.rs")];
        let rows = filter_items(&items, "app");
        assert_eq!(rows.len(), 1);
        let it = rows[0].item().unwrap();
        assert!(!it.description_highlights.is_empty());
    }
}
