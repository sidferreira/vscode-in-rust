//! The Find and Replace input history: the entries kept for `editor.find.history` and
//! `editor.find.replaceHistory`, and the Up / Down walk through them (upstream
//! `base/common/history.ts` `HistoryNavigator`, used by the find widget's `HistoryInputBox`).
//!
//! Entries are newest first and unique. Walking up from the newest entry keeps the text that was
//! being typed (the draft) so that walking back down restores it. Editing a recalled entry starts the
//! walk again from the edited text.
//!
//! upstream: src/vs/base/common/history.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/platform/history/browser/contextScopedHistoryWidget.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

/// How many entries are kept (upstream `HistoryNavigator` limit of the find widget: 100).
pub const MAX_HISTORY: usize = 100;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SearchHistory {
    /// Newest first, no empty entries, no duplicates.
    entries: Vec<String>,
    /// The entry shown by the walk, `None` when the walk is not active.
    position: Option<usize>,
    /// The text that was in the box when the walk started.
    draft: String,
}

impl SearchHistory {
    /// The history read from storage (newest first); invalid entries are dropped as `add` would.
    pub fn from_entries(entries: Vec<String>) -> Self {
        let mut h = Self::default();
        for e in entries.iter().rev() {
            h.add(e);
        }
        h
    }

    /// The entries, newest first.
    pub fn entries(&self) -> &[String] {
        &self.entries
    }

    /// Records a submitted text: it becomes the newest entry (an older copy is removed). Empty text is
    /// not recorded. Ends any walk.
    pub fn add(&mut self, text: &str) {
        self.position = None;
        if text.is_empty() {
            return;
        }
        self.entries.retain(|e| e != text);
        self.entries.insert(0, text.to_string());
        self.entries.truncate(MAX_HISTORY);
    }

    /// Up: the next older entry. `current` is the text in the box: if it is not the entry the walk
    /// showed, the walk starts again from it. `None` when there is no history.
    pub fn previous(&mut self, current: &str) -> Option<&str> {
        if self.entries.is_empty() {
            return None;
        }
        let next = match self.position {
            Some(p) if self.entries[p] == current => (p + 1).min(self.entries.len() - 1),
            _ => {
                self.draft = current.to_string();
                0
            }
        };
        self.position = Some(next);
        Some(&self.entries[next])
    }

    /// Down: the next newer entry, or the draft after the newest. `None` when no walk is active or the
    /// text was edited since the walk showed it.
    pub fn next(&mut self, current: &str) -> Option<&str> {
        let p = self.position?;
        if self.entries[p] != current {
            self.position = None;
            return None;
        }
        if p == 0 {
            self.position = None;
            Some(self.draft.as_str())
        } else {
            self.position = Some(p - 1);
            Some(&self.entries[p - 1])
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_keeps_newest_first_unique_and_non_empty() {
        let mut h = SearchHistory::default();
        h.add("a");
        h.add("");
        h.add("b");
        h.add("a");
        assert_eq!(h.entries(), ["a", "b"]);
    }

    #[test]
    fn the_history_is_capped() {
        let mut h = SearchHistory::default();
        for i in 0..MAX_HISTORY + 5 {
            h.add(&format!("q{i}"));
        }
        assert_eq!(h.entries().len(), MAX_HISTORY);
        assert_eq!(h.entries()[0], format!("q{}", MAX_HISTORY + 4));
    }

    #[test]
    fn up_walks_back_and_down_restores_the_draft() {
        let mut h = SearchHistory::from_entries(vec!["one".into(), "two".into(), "three".into()]);
        assert_eq!(h.previous("typing"), Some("one"));
        assert_eq!(h.previous("one"), Some("two"));
        assert_eq!(h.previous("two"), Some("three"));
        assert_eq!(h.previous("three"), Some("three"), "the oldest stays");
        assert_eq!(h.next("three"), Some("two"));
        assert_eq!(h.next("two"), Some("one"));
        assert_eq!(h.next("one"), Some("typing"), "down past the newest gives the draft");
        assert_eq!(h.next("typing"), None, "and the walk is over");
    }

    #[test]
    fn editing_a_recalled_entry_starts_the_walk_again() {
        let mut h = SearchHistory::from_entries(vec!["one".into(), "two".into()]);
        assert_eq!(h.previous(""), Some("one"));
        assert_eq!(h.previous("one edited"), Some("one"), "the edited text is the new draft");
        assert_eq!(h.next("one"), Some("one edited"));
        assert_eq!(h.next("one edited"), None, "not a walk entry any more");
    }

    #[test]
    fn empty_history_recalls_nothing() {
        let mut h = SearchHistory::default();
        assert_eq!(h.previous("x"), None);
        assert_eq!(h.next("x"), None);
    }

    #[test]
    fn storage_entries_are_cleaned_like_add() {
        let h = SearchHistory::from_entries(vec!["a".into(), "".into(), "a".into(), "b".into()]);
        assert_eq!(h.entries(), ["a", "b"]);
    }
}
