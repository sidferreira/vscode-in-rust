//! upstream: src/vs/editor/contrib/suggest/browser/suggestWidget.ts @ 50e1350fc78d
//!
//! The focused row and scroll position of the suggest list (ADR-0084).
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggestWidget.ts (`selectNext`,
//! `selectPrevious`, `selectNextPage`, `selectPreviousPage`, `selectFirst`, `selectLast`) and
//! src/vs/base/browser/ui/list/listWidget.ts (`focusNext(1, true)` wraps; `focusNextPage` goes to the
//! last visible row first and then scrolls a page).

/// The most rows the list shows at once (upstream's widget is 12 rows high).
pub const MAX_ROWS: usize = 12;

/// The item that has focus when a list is shown: the first, unless an item among those with the best
/// score asks to be preselected (upstream `Memory.select`).
pub fn initial_focus(items: &[crate::item::SuggestItem]) -> usize {
    let Some(first) = items.first() else { return 0 };
    for (i, item) in items.iter().enumerate() {
        if item.score.score != first.score.score {
            break;
        }
        if item.source.as_deref().is_some_and(|s| s.preselect) {
            return i;
        }
    }
    0
}

/// Which item has focus and which row is on top.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ListView {
    pub focused: usize,
    pub top: usize,
}

impl ListView {
    /// How many rows show for `len` items.
    pub fn visible_rows(len: usize) -> usize {
        len.min(MAX_ROWS)
    }

    fn max_top(len: usize) -> usize {
        len.saturating_sub(MAX_ROWS)
    }

    /// Focus `index` (clamped) and scroll the least that shows it.
    pub fn focus(&mut self, index: usize, len: usize) {
        if len == 0 {
            *self = Self::default();
            return;
        }
        self.focused = index.min(len - 1);
        self.reveal(len);
    }

    /// Keeps the focused row inside the window and the window inside the list.
    pub fn reveal(&mut self, len: usize) {
        self.top = self.top.min(Self::max_top(len));
        if self.focused < self.top {
            self.top = self.focused;
        } else if self.focused >= self.top + MAX_ROWS {
            self.top = self.focused + 1 - MAX_ROWS;
        }
    }

    /// The list changed (a refilter): the focus stays on a row that exists.
    pub fn clamp(&mut self, len: usize) {
        self.focus(self.focused, len);
    }

    /// Down: the next item, the first after the last.
    pub fn next(&mut self, len: usize) {
        if len > 0 {
            self.focus((self.focused + 1) % len, len);
        }
    }

    /// Up: the previous item, the last before the first.
    pub fn previous(&mut self, len: usize) {
        if len > 0 {
            self.focus((self.focused + len - 1) % len, len);
        }
    }

    pub fn first(&mut self, len: usize) {
        self.focus(0, len);
    }

    pub fn last(&mut self, len: usize) {
        self.focus(len.saturating_sub(1), len);
    }

    /// Page Down: the last visible row, then a page further.
    pub fn next_page(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        let last_visible = (self.top + MAX_ROWS - 1).min(len - 1);
        if self.focused < last_visible {
            self.focused = last_visible;
        } else {
            self.top = (self.top + MAX_ROWS).min(Self::max_top(len));
            self.focused = (self.top + MAX_ROWS - 1).min(len - 1);
        }
        self.reveal(len);
    }

    /// Page Up: the first visible row, then a page back.
    pub fn previous_page(&mut self, len: usize) {
        if len == 0 {
            return;
        }
        if self.focused > self.top {
            self.focused = self.top;
        } else {
            self.top = self.top.saturating_sub(MAX_ROWS);
            self.focused = self.top;
        }
        self.reveal(len);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_and_previous_wrap_around() {
        let mut v = ListView::default();
        v.previous(3);
        assert_eq!(v.focused, 2);
        v.next(3);
        assert_eq!(v.focused, 0);
        v.next(0);
        assert_eq!(v, ListView::default());
    }

    #[test]
    fn moving_past_the_window_scrolls_the_least() {
        let mut v = ListView::default();
        for _ in 0..MAX_ROWS {
            v.next(30);
        }
        assert_eq!((v.focused, v.top), (12, 1));
        v.previous(30);
        v.previous(30);
        assert_eq!((v.focused, v.top), (10, 1));
        v.last(30);
        assert_eq!((v.focused, v.top), (29, 18));
        v.next(30);
        assert_eq!((v.focused, v.top), (0, 0), "wraps to the top");
    }

    #[test]
    fn page_down_goes_to_the_last_visible_row_then_scrolls_a_page() {
        let mut v = ListView::default();
        v.next_page(40);
        assert_eq!((v.focused, v.top), (11, 0));
        v.next_page(40);
        assert_eq!((v.focused, v.top), (23, 12));
        v.next_page(40);
        assert_eq!((v.focused, v.top), (35, 24));
        v.next_page(40);
        assert_eq!((v.focused, v.top), (39, 28), "the last page is the end of the list");
        v.next_page(40);
        assert_eq!(v.focused, 39);
    }

    #[test]
    fn page_up_goes_to_the_first_visible_row_then_back_a_page() {
        let mut v = ListView::default();
        v.last(40);
        v.previous_page(40);
        assert_eq!((v.focused, v.top), (28, 28));
        v.previous_page(40);
        assert_eq!((v.focused, v.top), (16, 16));
        v.first(40);
        v.previous_page(40);
        assert_eq!((v.focused, v.top), (0, 0));
    }

    #[test]
    fn a_shorter_list_keeps_the_focus_inside() {
        let mut v = ListView { focused: 20, top: 9 };
        v.clamp(5);
        assert_eq!((v.focused, v.top), (4, 0));
        v.clamp(0);
        assert_eq!(v, ListView::default());
    }
}
