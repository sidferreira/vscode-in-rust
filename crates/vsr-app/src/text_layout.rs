//! Pure line-layout helpers: tab expansion and the mapping between model columns (1-based,
//! UTF-16, as `vsr-text` uses) and positions in the expanded text that gpui shapes.
//!
//! upstream: src/vs/editor/common/cursor/cursorColumns.ts @ 50e1350fc78d
//!
//! Nothing here touches gpui: the pixel side is injected as a width function (a shaped line's
//! `x_for_index` in the app, a fixed-width stub in tests).

/// Expand tabs to the next tab stop. The shaped text is always this expansion.
pub fn expand_tabs(line: &str, tab_size: usize) -> String {
    if !line.contains('\t') {
        return line.to_string();
    }
    let tab = tab_size.max(1);
    let mut out = String::with_capacity(line.len() + 8);
    let mut visual = 0usize;
    for ch in line.chars() {
        if ch == '\t' {
            let n = tab - visual % tab;
            out.extend(std::iter::repeat_n(' ', n));
            visual += n;
        } else {
            out.push(ch);
            visual += 1;
        }
    }
    out
}

/// Byte offset into `expand_tabs(line)` of the model column `column` (1-based UTF-16; clamped).
pub fn expanded_byte_for_column(line: &str, tab_size: usize, column: usize) -> usize {
    let tab = tab_size.max(1);
    let target = column.saturating_sub(1);
    let (mut utf16, mut visual, mut bytes) = (0usize, 0usize, 0usize);
    for ch in line.chars() {
        if utf16 >= target {
            break;
        }
        utf16 += ch.len_utf16();
        if ch == '\t' {
            let n = tab - visual % tab;
            visual += n;
            bytes += n;
        } else {
            visual += 1;
            bytes += ch.len_utf8();
        }
    }
    bytes
}

/// The model column of the character boundary nearest to byte `byte` of `expand_tabs(line)`.
/// A byte inside a tab's spaces snaps to the closer side of the tab.
pub fn column_for_expanded_byte(line: &str, tab_size: usize, byte: usize) -> usize {
    let tab = tab_size.max(1);
    let (mut utf16, mut visual, mut bytes) = (0usize, 0usize, 0usize);
    for ch in line.chars() {
        let width = if ch == '\t' { tab - visual % tab } else { ch.len_utf8() };
        let step = if ch == '\t' { width } else { ch.len_utf8() };
        if bytes + step > byte {
            // `byte` lies in this character: pick the nearer boundary.
            let into = byte - bytes;
            return if into * 2 >= step { utf16 + ch.len_utf16() + 1 } else { utf16 + 1 };
        }
        bytes += step;
        visual += if ch == '\t' { width } else { 1 };
        utf16 += ch.len_utf16();
    }
    utf16 + 1
}

/// Visual column (tabs expanded, 1-based) of model column `column`: VS Code's status bar "Col".
pub fn visual_column(line: &str, tab_size: usize, column: usize) -> usize {
    let tab = tab_size.max(1);
    let target = column.saturating_sub(1);
    let (mut utf16, mut visual) = (0usize, 0usize);
    for ch in line.chars() {
        if utf16 >= target {
            break;
        }
        utf16 += ch.len_utf16();
        visual += if ch == '\t' { tab - visual % tab } else { 1 };
    }
    visual + 1
}

/// The model column for a click at `x` (pixels from the text origin), given
/// `width_of_prefix(byte)`: the x offset of byte `byte` of the expanded text. Picks the nearest
/// character boundary, so a click in the right half of a glyph lands after it.
pub fn column_for_x(
    line: &str,
    tab_size: usize,
    x: f32,
    width_of_prefix: impl Fn(usize) -> f32,
) -> usize {
    let text = expand_tabs(line, tab_size);
    let mut best = (0usize, f32::INFINITY);
    let mut consider = |byte: usize| {
        let d = (width_of_prefix(byte) - x).abs();
        if d < best.1 {
            best = (byte, d);
        }
    };
    for (i, _) in text.char_indices() {
        consider(i);
    }
    consider(text.len());
    column_for_expanded_byte(line, tab_size, best.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 7.0;
    fn fixed(text: &str) -> impl Fn(usize) -> f32 + '_ {
        move |byte| text[..byte].chars().count() as f32 * W
    }

    #[test]
    fn tabs_expand_to_tab_stops() {
        assert_eq!(expand_tabs("\ta", 4), "    a");
        assert_eq!(expand_tabs("ab\tc", 4), "ab  c");
        assert_eq!(expand_tabs("abcd\tc", 4), "abcd    c");
        assert_eq!(expand_tabs("plain", 4), "plain");
    }

    #[test]
    fn columns_map_through_tabs_and_utf16() {
        // "a\tb": col 1 -> byte 0, col 2 (before tab) -> 1, col 3 (after tab) -> 4.
        assert_eq!(expanded_byte_for_column("a\tb", 4, 1), 0);
        assert_eq!(expanded_byte_for_column("a\tb", 4, 2), 1);
        assert_eq!(expanded_byte_for_column("a\tb", 4, 3), 4);
        // Astral char: 2 UTF-16 units, 4 UTF-8 bytes.
        assert_eq!(expanded_byte_for_column("😀x", 4, 3), 4);
        assert_eq!(column_for_expanded_byte("😀x", 4, 4), 3);
        // Past the end clamps.
        assert_eq!(expanded_byte_for_column("ab", 4, 99), 2);
        assert_eq!(column_for_expanded_byte("ab", 4, 99), 3);
    }

    #[test]
    fn byte_inside_a_tab_snaps_to_nearer_side() {
        // "a\tb" expands to "a   b" (tab covers bytes 1..4).
        assert_eq!(column_for_expanded_byte("a\tb", 4, 1), 2);
        assert_eq!(column_for_expanded_byte("a\tb", 4, 2), 2);
        assert_eq!(column_for_expanded_byte("a\tb", 4, 3), 3);
    }

    #[test]
    fn visual_column_counts_tab_stops() {
        assert_eq!(visual_column("\tx", 4, 2), 5);
        assert_eq!(visual_column("ab\tx", 4, 4), 5);
        assert_eq!(visual_column("abc", 4, 4), 4);
    }

    #[test]
    fn click_x_maps_to_nearest_column_with_fixed_width_font() {
        let line = "hello";
        let t = expand_tabs(line, 4);
        let f = fixed(&t);
        assert_eq!(column_for_x(line, 4, -5.0, &f), 1);
        assert_eq!(column_for_x(line, 4, 0.0, &f), 1);
        assert_eq!(column_for_x(line, 4, 3.0, &f), 1); // left half of 'h'
        assert_eq!(column_for_x(line, 4, 4.0, &f), 2); // right half of 'h'
        assert_eq!(column_for_x(line, 4, 18.0, &f), 4);
        assert_eq!(column_for_x(line, 4, 500.0, &f), 6); // past end
    }

    #[test]
    fn click_x_with_tabs_and_wide_chars() {
        let line = "\tab";
        let t = expand_tabs(line, 4);
        let f = fixed(&t);
        assert_eq!(column_for_x(line, 4, 28.0, &f), 2); // right after the tab
        assert_eq!(column_for_x(line, 4, 10.0, &f), 1); // inside tab, nearer start
        assert_eq!(column_for_x(line, 4, 33.0, &f), 3); // between 'a' and 'b'
        assert_eq!(column_for_x(line, 4, 40.0, &f), 4); // end of line
        let line = "😀b";
        let t = expand_tabs(line, 4);
        let f = fixed(&t);
        assert_eq!(column_for_x(line, 4, 9.0, &f), 3); // after the emoji (2 UTF-16 units)
    }
}
