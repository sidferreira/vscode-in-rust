//! Text buffer and position/range primitives.
//!
//! upstream: src/vs/editor/common/core/position.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/core/range.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/model/pieceTreeTextBuffer/pieceTreeTextBufferBuilder.ts @ 50e1350fc78d
//!
//! The storage is a simple line vector for now. The public API matches VS Code
//! semantics (1-based lines/columns, UTF-16 columns and offsets) so a faithful
//! piece-tree port can replace the internals later without touching callers.
//! See DECISIONS.md, ADR-0005.

mod position;
mod range;

pub use position::Position;
pub use range::Range;

/// The line terminator a buffer uses when it writes new lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Eol {
    Lf,
    CrLf,
}

impl Eol {
    pub fn as_str(self) -> &'static str {
        match self {
            Eol::Lf => "\n",
            Eol::CrLf => "\r\n",
        }
    }
}

/// Mirrors `DefaultEndOfLine`: used when the content has no line breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultEol {
    Lf,
    CrLf,
}

#[derive(Debug, Clone)]
struct Line {
    text: String,
    /// The terminator after this line ("" for the last line).
    eol: &'static str,
}

#[derive(Debug, Clone)]
pub struct TextBuffer {
    lines: Vec<Line>,
    eol: Eol,
    normalize_eol: bool,
}

impl TextBuffer {
    pub fn new(text: &str, default_eol: DefaultEol, normalize_eol: bool) -> Self {
        let lines = split_lines(text);
        let (mut cr, mut lf, mut crlf) = (0usize, 0usize, 0usize);
        for l in &lines {
            match l.eol {
                "\r" => cr += 1,
                "\n" => lf += 1,
                "\r\n" => crlf += 1,
                _ => {}
            }
        }
        let total = cr + lf + crlf;
        let eol = if total == 0 {
            match default_eol {
                DefaultEol::Lf => Eol::Lf,
                DefaultEol::CrLf => Eol::CrLf,
            }
        } else if (cr + crlf) * 2 > total {
            Eol::CrLf
        } else {
            Eol::Lf
        };
        let mut buf = TextBuffer { lines, eol, normalize_eol };
        if normalize_eol {
            buf.normalize_all();
        }
        buf
    }

    fn normalize_all(&mut self) {
        let eol = self.eol.as_str();
        let last = self.lines.len() - 1;
        for (i, l) in self.lines.iter_mut().enumerate() {
            if i != last {
                l.eol = eol;
            }
        }
    }

    pub fn eol(&self) -> Eol {
        self.eol
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Content of a 1-based line, without its terminator.
    pub fn line_content(&self, line: usize) -> &str {
        &self.lines[line - 1].text
    }

    /// Length of a line in UTF-16 code units.
    pub fn line_length(&self, line: usize) -> usize {
        utf16_len(&self.lines[line - 1].text)
    }

    pub fn line_max_column(&self, line: usize) -> usize {
        self.line_length(line) + 1
    }

    /// The whole content, with the stored line terminators.
    pub fn value(&self) -> String {
        let mut s = String::new();
        for l in &self.lines {
            s.push_str(&l.text);
            s.push_str(l.eol);
        }
        s
    }

    /// Total length in UTF-16 code units.
    pub fn len_utf16(&self) -> usize {
        self.lines.iter().map(|l| utf16_len(&l.text) + l.eol.len()).sum()
    }

    pub fn full_range(&self) -> Range {
        let last = self.line_count();
        Range::new(Position::new(1, 1), Position::new(last, self.line_max_column(last)))
    }

    /// Clamp a position into the buffer, like `TextModel.validatePosition`.
    /// A column inside a surrogate pair snaps to before the pair.
    pub fn validate_position(&self, p: Position) -> Position {
        if p.line < 1 {
            return Position::new(1, 1);
        }
        if p.line > self.line_count() {
            let last = self.line_count();
            return Position::new(last, self.line_max_column(last));
        }
        let max = self.line_max_column(p.line);
        let column = p.column.clamp(1, max);
        let (_, snapped) = utf16_col_to_byte(&self.lines[p.line - 1].text, column);
        Position::new(p.line, snapped)
    }

    pub fn offset_at(&self, p: Position) -> usize {
        let p = self.validate_position(p);
        let before: usize = self.lines[..p.line - 1]
            .iter()
            .map(|l| utf16_len(&l.text) + l.eol.len())
            .sum();
        before + p.column - 1
    }

    pub fn position_at(&self, offset: usize) -> Position {
        let mut remaining = offset;
        for (i, l) in self.lines.iter().enumerate() {
            let len = utf16_len(&l.text);
            let line = i + 1;
            if remaining <= len || i == self.lines.len() - 1 {
                return self.validate_position(Position::new(line, remaining.min(len) + 1));
            }
            let with_eol = len + l.eol.len();
            if remaining < with_eol {
                // offset points inside a CRLF: VS Code puts it at end of line
                return Position::new(line, len + 1);
            }
            remaining -= with_eol;
        }
        unreachable!("buffer always has at least one line")
    }

    pub fn value_in_range(&self, r: Range) -> String {
        let start = self.validate_position(r.start);
        let end = self.validate_position(r.end);
        let (sb, _) = utf16_col_to_byte(&self.lines[start.line - 1].text, start.column);
        let (eb, _) = utf16_col_to_byte(&self.lines[end.line - 1].text, end.column);
        if start.line == end.line {
            return self.lines[start.line - 1].text[sb..eb].to_string();
        }
        let mut s = String::new();
        let first = &self.lines[start.line - 1];
        s.push_str(&first.text[sb..]);
        s.push_str(first.eol);
        for l in &self.lines[start.line..end.line - 1] {
            s.push_str(&l.text);
            s.push_str(l.eol);
        }
        s.push_str(&self.lines[end.line - 1].text[..eb]);
        s
    }

    /// Replace `range` with `text`.
    pub fn edit(&mut self, range: Range, text: &str) {
        let start = self.validate_position(range.start);
        let end = self.validate_position(range.end);
        let (sb, _) = utf16_col_to_byte(&self.lines[start.line - 1].text, start.column);
        let (eb, _) = utf16_col_to_byte(&self.lines[end.line - 1].text, end.column);

        let prefix = &self.lines[start.line - 1].text[..sb];
        let end_line = &self.lines[end.line - 1];
        let suffix = &end_line.text[eb..];
        let tail_eol = end_line.eol;

        let mut new_lines = split_lines(text);
        if self.normalize_eol {
            let eol = self.eol.as_str();
            for l in new_lines.iter_mut() {
                if !l.eol.is_empty() {
                    l.eol = eol;
                }
            }
        }
        new_lines[0].text.insert_str(0, prefix);
        let last = new_lines.last_mut().expect("split_lines returns >= 1 line");
        last.text.push_str(suffix);
        last.eol = tail_eol;

        self.lines.splice(start.line - 1..end.line, new_lines);
    }
}

fn split_lines(text: &str) -> Vec<Line> {
    let mut lines = Vec::new();
    let bytes = text.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let eol: &'static str = match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => "\r\n",
            b'\r' => "\r",
            b'\n' => "\n",
            _ => {
                i += 1;
                continue;
            }
        };
        lines.push(Line { text: text[start..i].to_string(), eol });
        i += eol.len();
        start = i;
    }
    lines.push(Line { text: text[start..].to_string(), eol: "" });
    lines
}

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Map a 1-based UTF-16 column to a byte index. Returns the byte index and the
/// (possibly snapped) column actually used.
fn utf16_col_to_byte(s: &str, column: usize) -> (usize, usize) {
    let target = column.saturating_sub(1);
    let mut units = 0;
    for (byte, ch) in s.char_indices() {
        if units + ch.len_utf16() > target {
            return (byte, units + 1);
        }
        units += ch.len_utf16();
    }
    (s.len(), units + 1)
}
