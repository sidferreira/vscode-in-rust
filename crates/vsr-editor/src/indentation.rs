//! `editor.detectIndentation`: guess tabs or spaces, and the indent size, from the text.
//!
//! upstream: src/vs/editor/common/model/indentationGuesser.ts @ 50e1350fc78d
//!
//! Looks at the first 10,000 lines. Lines are compared with the previous line that had content:
//! the difference of their indentation votes for an indent size (2 to 8, even sizes preferred); the
//! lines indented with tabs and the lines indented with spaces vote for tabs or spaces. A size is only
//! guessed when spaces win.

use vsr_text::TextBuffer;

/// upstream `IGuessedIndentation`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GuessedIndentation {
    /// The indent size when `insert_spaces` is set (the default tab size otherwise).
    pub tab_size: usize,
    pub insert_spaces: bool,
}

#[derive(Default)]
struct SpacesDiffResult {
    spaces_diff: usize,
    looks_like_alignment: bool,
}

/// The difference in spaces between the indentation of two lines (upstream `spacesDiff`).
fn spaces_diff(a: &[u16], a_length: usize, b: &[u16], b_length: usize, result: &mut SpacesDiffResult) {
    result.spaces_diff = 0;
    result.looks_like_alignment = false;
    // This can go both ways: `\t` against `\t    ` counts 1 tab and 4 spaces.
    let mut i = 0;
    while i < a_length && i < b_length && a[i] == b[i] {
        i += 1;
    }
    let count = |s: &[u16], from: usize, to: usize| {
        let mut spaces = 0usize;
        let mut tabs = 0usize;
        for &c in &s[from..to] {
            if c == b' ' as u16 {
                spaces += 1;
            } else {
                tabs += 1;
            }
        }
        (spaces, tabs)
    };
    let (a_spaces, a_tabs) = count(a, i, a_length);
    let (b_spaces, b_tabs) = count(b, i, b_length);
    if a_spaces > 0 && a_tabs > 0 {
        return;
    }
    if b_spaces > 0 && b_tabs > 0 {
        return;
    }
    let tabs_diff = a_tabs.abs_diff(b_tabs);
    let spaces_d = a_spaces.abs_diff(b_spaces);
    if tabs_diff == 0 {
        // The difference might be alignment (`const a = b + c,` then `      d = b - c;`): not a hint.
        result.spaces_diff = spaces_d;
        if spaces_d > 0
            && b_spaces >= 1
            && b_spaces - 1 < a.len()
            && b_spaces < b.len()
            && b[b_spaces] != b' ' as u16
            && a[b_spaces - 1] == b' ' as u16
            && a.last() == Some(&(b',' as u16))
        {
            result.looks_like_alignment = true;
        }
        return;
    }
    if spaces_d % tabs_diff == 0 {
        result.spaces_diff = spaces_d / tabs_diff;
    }
}

/// upstream `guessIndentation`.
pub fn guess_indentation(source: &TextBuffer, default_tab_size: usize, default_insert_spaces: bool) -> GuessedIndentation {
    // Look at most at the first 10k lines.
    let lines_count = source.line_count().min(10_000);
    let mut lines_indented_with_tabs = 0usize;
    let mut lines_indented_with_spaces = 0usize;
    let mut previous_line: Vec<u16> = Vec::new();
    let mut previous_indentation = 0usize;
    const ALLOWED_TAB_SIZE_GUESSES: [usize; 7] = [2, 4, 6, 8, 3, 5, 7]; // prefer even guesses, limit to [2, 8]
    const MAX_ALLOWED_TAB_SIZE_GUESS: usize = 8;
    let mut spaces_diff_count = [0usize; 9];
    let mut tmp = SpacesDiffResult::default();
    for line_number in 1..=lines_count {
        let current: Vec<u16> = source.line_content(line_number).encode_utf16().collect();
        let mut has_content = false;
        let mut indentation = 0usize;
        let mut spaces = 0usize;
        let mut tabs = 0usize;
        for (j, &c) in current.iter().enumerate() {
            if c == b'\t' as u16 {
                tabs += 1;
            } else if c == b' ' as u16 {
                spaces += 1;
            } else {
                // Hit a non-whitespace character on this line.
                has_content = true;
                indentation = j;
                break;
            }
        }
        // Ignore empty or only whitespace lines.
        if !has_content {
            continue;
        }
        if tabs > 0 {
            lines_indented_with_tabs += 1;
        } else if spaces > 1 {
            lines_indented_with_spaces += 1;
        }
        spaces_diff(&previous_line, previous_indentation, &current, indentation, &mut tmp);
        if tmp.looks_like_alignment {
            // With spaces by default and a diff of the tab size, it may be valid indentation:
            //   - item1
            //     - item2
            // otherwise skip the line:
            //   const a = 1,
            //         b = 2;
            if !(default_insert_spaces && default_tab_size == tmp.spaces_diff) {
                continue;
            }
        }
        if tmp.spaces_diff <= MAX_ALLOWED_TAB_SIZE_GUESS {
            spaces_diff_count[tmp.spaces_diff] += 1;
        }
        previous_line = current;
        previous_indentation = indentation;
    }

    let mut insert_spaces = default_insert_spaces;
    if lines_indented_with_tabs != lines_indented_with_spaces {
        insert_spaces = lines_indented_with_tabs < lines_indented_with_spaces;
    }

    let mut tab_size = default_tab_size;
    // Guess the tab size only when inserting spaces.
    if insert_spaces {
        let mut score = 0;
        for possible in ALLOWED_TAB_SIZE_GUESSES {
            let s = spaces_diff_count[possible];
            if s > score {
                score = s;
                tab_size = possible;
            }
        }
        // A tab size of 2 wins over 4 only with at least 2/3 of the occurrences of 4 (YAML files with
        // some 4-space diffs from deeper nesting), while 4 stays when it is clearly predominant.
        if tab_size == 4 && spaces_diff_count[4] > 0 && spaces_diff_count[2] > 0 && spaces_diff_count[2] as f64 >= spaces_diff_count[4] as f64 * 2.0 / 3.0 {
            tab_size = 2;
        }
    }
    GuessedIndentation { insert_spaces, tab_size }
}
