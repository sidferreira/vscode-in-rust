//! Visible columns (tabs expanded), indentation normalization and small string helpers.
//!
//! upstream: src/vs/editor/common/core/cursorColumns.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/core/misc/indentation.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/strings.ts @ 50e1350fc78d
//!
//! Simplification: upstream's grapheme iterator is replaced by a per-`char` walk.
//! Combining marks therefore take a visible column each, where upstream merges them.
//! Full-width and emoji ranges are upstream's `isFullWidthCharacter` and
//! `isEmojiImprecise` tables, copied as-is.

/// Length of `s` in UTF-16 code units.
pub(crate) fn utf16_len(s: &str) -> usize {
    // ASCII fast path (`is_ascii` is vectorised even in debug builds): hot on large buffers.
    if s.is_ascii() {
        return s.len();
    }
    s.chars().map(char::len_utf16).sum()
}

/// Upstream `strings.isFullWidthCharacter`.
fn is_full_width(cp: u32) -> bool {
    (0x2E80..=0xD7AF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFF01..=0xFF5E).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp)
}

/// Upstream `strings.isEmojiImprecise`.
fn is_emoji_imprecise(cp: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&cp)
        || matches!(cp, 8986 | 8987 | 9200 | 9203)
        || (9728..=10175).contains(&cp)
        || matches!(cp, 11088 | 11093)
        || (127744..=128591).contains(&cp)
        || (128640..=128764).contains(&cp)
        || (128992..=129008).contains(&cp)
        || (129280..=129535).contains(&cp)
        || (129648..=129782).contains(&cp)
}

/// The visible column after `ch` when the text before it ends at `visible`.
/// Upstream `CursorColumns._nextVisibleColumn`.
pub(crate) fn next_visible(ch: char, visible: usize, tab_size: usize) -> usize {
    if ch == '\t' {
        return next_tab_stop(visible, tab_size);
    }
    let cp = ch as u32;
    if is_full_width(cp) || is_emoji_imprecise(cp) {
        visible + 2
    } else {
        visible + 1
    }
}

/// Upstream `CursorColumns.nextRenderTabStop`.
fn next_tab_stop(visible: usize, tab_size: usize) -> usize {
    visible + tab_size - visible % tab_size
}

/// Upstream `CursorColumns.prevRenderTabStop`: the tab stop at or before `visible - 1`.
pub(crate) fn prev_tab_stop(visible: usize, tab_size: usize) -> usize {
    let v = visible.saturating_sub(1);
    v - v % tab_size
}

/// The visible column of 1-based `column` in `line` (upstream `visibleColumnFromColumn`).
pub(crate) fn visible_column(line: &str, column: usize, tab_size: usize) -> usize {
    let limit = column.saturating_sub(1);
    let mut units = 0;
    let mut visible = 0;
    for ch in line.chars() {
        if units >= limit {
            break;
        }
        visible = next_visible(ch, visible, tab_size);
        units += ch.len_utf16();
    }
    visible
}

/// The 1-based column nearest to `visible` in `line` (upstream `columnFromVisibleColumn`).
/// A visible column past the end gives the line's max column.
pub(crate) fn column_from_visible(line: &str, visible: usize, tab_size: usize) -> usize {
    if visible == 0 {
        return 1;
    }
    let mut before_visible = 0;
    let mut before_column = 1;
    let mut units = 0;
    for ch in line.chars() {
        let after_visible = next_visible(ch, before_visible, tab_size);
        units += ch.len_utf16();
        let after_column = units + 1;
        if after_visible >= visible {
            let before_delta = visible - before_visible;
            let after_delta = after_visible - visible;
            return if after_delta < before_delta { after_column } else { before_column };
        }
        before_visible = after_visible;
        before_column = after_column;
    }
    units + 1
}

/// Whitespace in upstream's sense: space or tab only.
pub(crate) fn is_indent_char(ch: char) -> bool {
    ch == ' ' || ch == '\t'
}

/// The 1-based column of the first character that is not a space or tab, or 0 when
/// the line is empty or all whitespace (upstream `getLineFirstNonWhitespaceColumn`).
pub(crate) fn first_non_whitespace_column(line: &str) -> usize {
    let mut units = 0;
    for ch in line.chars() {
        if !is_indent_char(ch) {
            return units + 1;
        }
        units += ch.len_utf16();
    }
    0
}

/// Rewrites the leading whitespace of `s` the way upstream `normalizeIndentation` does:
/// spaces only when `insert_spaces`, otherwise tabs for whole tab stops and spaces for
/// the rest. The text after the indent is kept.
pub(crate) fn normalize_indentation(s: &str, tab_size: usize, insert_spaces: bool) -> String {
    let indent_end = s.char_indices().find(|&(_, c)| !is_indent_char(c)).map_or(s.len(), |(i, _)| i);
    let (indent, rest) = s.split_at(indent_end);
    let width = indent.chars().fold(0, |v, c| next_visible(c, v, tab_size));
    let mut out = String::new();
    let spaces = if insert_spaces {
        width
    } else {
        for _ in 0..width / tab_size {
            out.push('\t');
        }
        width % tab_size
    };
    for _ in 0..spaces {
        out.push(' ');
    }
    out.push_str(rest);
    out
}
