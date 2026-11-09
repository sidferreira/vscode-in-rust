//! Word-wise caret movement (ctrl+left / ctrl+right style).
//!
//! upstream: src/vs/editor/common/cursor/cursorWordOperations.ts @ 50e1350fc78d
//!
//! Simplified. Upstream's `WordCharacterClassifier` is driven by the `wordSeparators`
//! option, has `WordStartFast`/`WordEnd`/accessibility variants, subword and
//! multi-cursor rules. Here a character is whitespace, a separator, or a word
//! character, with upstream's default `USUAL_WORD_SEPARATORS`. Left moves to the
//! start of the previous run of the same class, right to the start of the next word.
//! Moves stop at line ends, except that at a line edge they wrap like a single step.

use vsr_text::Position;

use crate::Editor;

/// upstream `USUAL_WORD_SEPARATORS`.
const USUAL_WORD_SEPARATORS: &str = "`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Class {
    Space,
    Separator,
    Word,
}

fn class_of(ch: char) -> Class {
    if ch.is_whitespace() {
        Class::Space
    } else if USUAL_WORD_SEPARATORS.contains(ch) {
        Class::Separator
    } else {
        Class::Word
    }
}

/// Splits a line into characters, with each character's UTF-16 start offset.
fn units_of(line: &str) -> (Vec<char>, Vec<usize>) {
    let mut chars = Vec::new();
    let mut starts = Vec::new();
    let mut units = 0;
    for ch in line.chars() {
        chars.push(ch);
        starts.push(units);
        units += ch.len_utf16();
    }
    starts.push(units);
    (chars, starts)
}

impl Editor {
    /// Upstream `WordOperations.moveWordLeft` (WordStart), simplified.
    pub fn move_word_left(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            let p = self.selection.range().start;
            self.move_to(p, false, 0);
            return;
        }
        let p = self.selection.active;
        if p.column == 1 {
            let target = self.left_of(p);
            self.move_to(target, extend, 0);
            return;
        }
        let (chars, starts) = units_of(self.buffer.line_content(p.line));
        // Index of the first character at or after the caret: everything before it is left of the caret.
        let mut i = starts.iter().position(|&s| s >= p.column - 1).unwrap_or(chars.len());
        while i > 0 && class_of(chars[i - 1]) == Class::Space {
            i -= 1;
        }
        if i > 0 {
            let class = class_of(chars[i - 1]);
            while i > 0 && class_of(chars[i - 1]) == class {
                i -= 1;
            }
        }
        self.move_to(Position::new(p.line, starts[i] + 1), extend, 0);
    }

    /// Upstream `WordOperations.moveWordRight` (WordStart), simplified.
    pub fn move_word_right(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            let p = self.selection.range().end;
            self.move_to(p, false, 0);
            return;
        }
        let p = self.selection.active;
        if p.column >= self.buffer.line_max_column(p.line) {
            let target = self.right_of(p);
            self.move_to(target, extend, 0);
            return;
        }
        let (chars, starts) = units_of(self.buffer.line_content(p.line));
        let n = chars.len();
        let mut i = starts.iter().position(|&s| s >= p.column - 1).unwrap_or(n);
        if i < n && class_of(chars[i]) != Class::Space {
            let class = class_of(chars[i]);
            while i < n && class_of(chars[i]) == class {
                i += 1;
            }
        }
        while i < n && class_of(chars[i]) == Class::Space {
            i += 1;
        }
        self.move_to(Position::new(p.line, starts[i] + 1), extend, 0);
    }

    /// Upstream `WordOperations.moveWordRight` with `WordNavigationType.WordEnd`: the caret
    /// moves to the end of the next word. This is `cursorWordRight` (Option+Right on macOS).
    pub fn move_word_end_right(&mut self, extend: bool) {
        if !extend && !self.selection.is_empty() {
            let p = self.selection.range().end;
            self.move_to(p, false, 0);
            return;
        }
        let p = self.selection.active;
        if p.column >= self.buffer.line_max_column(p.line) {
            let target = self.right_of(p);
            self.move_to(target, extend, 0);
            return;
        }
        let (chars, starts) = units_of(self.buffer.line_content(p.line));
        let n = chars.len();
        let mut i = starts.iter().position(|&s| s >= p.column - 1).unwrap_or(n);
        while i < n && class_of(chars[i]) == Class::Space {
            i += 1;
        }
        if i < n {
            let class = class_of(chars[i]);
            while i < n && class_of(chars[i]) == class {
                i += 1;
            }
        }
        self.move_to(Position::new(p.line, starts[i] + 1), extend, 0);
    }
}

#[cfg(test)]
mod word_end_tests {
    use vsr_text::{DefaultEol, Position, TextBuffer};

    use crate::{Editor, EditorOptions};

    fn editor(text: &str) -> Editor {
        let b = TextBuffer::new(text, DefaultEol::Lf, true);
        Editor::new(b, EditorOptions { tab_size: 4, insert_spaces: true })
    }

    #[test]
    fn word_end_right_stops_at_word_ends() {
        let mut e = editor("one  two.x\nz");
        let mut stops = Vec::new();
        for _ in 0..6 {
            e.move_word_end_right(false);
            let p = e.selection().active;
            stops.push((p.line, p.column));
        }
        // "one" | "two" | "." | "x" | wraps to line 2 start | "z"
        assert_eq!(stops, [(1, 4), (1, 9), (1, 10), (1, 11), (2, 1), (2, 2)]);
        assert_eq!(e.selection().anchor, Position::new(2, 2));
    }
}
