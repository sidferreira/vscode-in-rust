//! Go to Line/Column (`:`): parsing what is typed after the prefix.
//!
//! upstream: src/vs/editor/contrib/quickAccess/browser/gotoLineQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/quickAccess/test/browser/gotoLineQuickAccess.test.ts @ 50e1350fc78d
//!
//! [`parse_position`] is `AbstractGotoLineQuickAccessProvider.parsePosition`: `line`, `line:col`
//! (also `,` and `#`), negative values counting from the end, and `:offset` (a character
//! position, 1-based unless zero-based offsets are on). Columns are visible columns (tabs
//! expanded). Positions are 1-based `(line, column)` with UTF-16 columns, like the editor model.
//!
//! The model is a trait so the editor's text buffer and test models can both be used.

/// What `parsePosition` reads from the text model (`ITextModel`).
pub trait GotoLineModel {
    fn line_count(&self) -> usize;
    /// Content of a 1-based line, without its terminator.
    fn line_content(&self, line: usize) -> String;
    /// Total length in UTF-16 code units (`getValueLength`).
    fn value_length(&self) -> usize;
    /// `getPositionAt`: `(line, column)` of an offset.
    fn position_at(&self, offset: usize) -> (usize, usize);
    fn tab_size(&self) -> usize;
}

/// The result of parsing (`Partial<IPosition> & { inOffsetMode?, label }`).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParsedPosition {
    pub line: Option<usize>,
    pub column: Option<usize>,
    pub in_offset_mode: bool,
    /// The text of the single pick: a hint or what Enter does.
    pub label: String,
}

/// The pick shown when no text editor is active.
pub const NO_EDITOR_LABEL: &str = "Open a text editor first to go to a line or an offset.";

fn next_visible(ch: char, visible: usize, tab_size: usize) -> usize {
    if ch == '\t' {
        return visible + tab_size - visible % tab_size;
    }
    let cp = ch as u32;
    let wide = (0x2E80..=0xD7AF).contains(&cp)
        || (0xF900..=0xFAFF).contains(&cp)
        || (0xFF01..=0xFF5E).contains(&cp)
        || (0xFFE0..=0xFFE6).contains(&cp);
    visible + if wide { 2 } else { 1 }
}

/// `CursorColumns.visibleColumnFromColumn` (0-based result).
fn visible_column_from_column(line: &str, column: usize, tab_size: usize) -> usize {
    let limit = column.saturating_sub(1);
    let (mut units, mut visible) = (0, 0);
    for ch in line.chars() {
        if units >= limit {
            break;
        }
        visible = next_visible(ch, visible, tab_size);
        units += ch.len_utf16();
    }
    visible
}

/// `CursorColumns.columnFromVisibleColumn` (1-based result).
fn column_from_visible_column(line: &str, visible: usize, tab_size: usize) -> usize {
    if visible == 0 {
        return 1;
    }
    let (mut before_visible, mut before_column, mut units) = (0, 1, 0);
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

fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// JS `parseInt(s, 10)`: optional sign and leading digits, anything after them ignored.
fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim_start();
    let (neg, rest) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    let n: i64 = digits.parse().ok()?;
    Some(if neg { -n } else { n })
}

/// `parsePosition(editor, value)`. `value` is the typed text without the first `:` (the quick
/// access prefix): `5:2` for line 5 column 2 (typed `:5:2`), `:100` for offset 100 (typed `::100`).
/// `zero_based_offset` is the stored "Toggle Zero-Based Offset" button.
pub fn parse_position(model: Option<&dyn GotoLineModel>, value: &str, zero_based_offset: bool) -> ParsedPosition {
    let Some(model) = model else {
        return ParsedPosition { label: NO_EDITOR_LABEL.into(), ..Default::default() };
    };

    if let Some(rest) = value.strip_prefix(':') {
        // `::<offset>`: a character offset in the model.
        let max_offset = model.value_length() as i64;
        let Some(mut offset) = parse_int(rest) else {
            return ParsedPosition {
                in_offset_mode: true,
                label: if zero_based_offset {
                    format!("Type a character position to go to (from 0 to {}).", max_offset - 1)
                } else {
                    format!("Type a character position to go to (from 1 to {max_offset}).")
                },
                ..Default::default()
            };
        };
        let reverse = offset < 0;
        if !zero_based_offset {
            offset -= offset.signum();
        }
        if reverse {
            offset += max_offset;
        }
        let (line, column) = model.position_at(offset.clamp(0, max_offset) as usize);
        let visible = visible_column_from_column(&model.line_content(line), column, model.tab_size()) + 1;
        return ParsedPosition {
            line: Some(line),
            column: Some(column),
            in_offset_mode: true,
            label: format!("Press 'Enter' to go to line {line} at column {visible}."),
        };
    }

    // `line,col`, `line:col`, `line#col`
    let parts: Vec<&str> = value.split([',', ':', '#']).collect();
    let max_line = model.line_count() as i64;
    let Some(mut line) = parse_int(parts[0].trim()) else {
        return ParsedPosition { label: format!("Type a line number to go to (from 1 to {max_line})."), ..Default::default() };
    };
    // Negative lines count from the end; clip to the valid range.
    if line < 0 {
        line += max_line + 1;
    }
    let line = line.clamp(1, max_line) as usize;

    // The column is a visible column.
    let tab_size = model.tab_size();
    let content = model.line_content(line);
    let max_column = visible_column_from_column(&content, utf16_len(&content) + 1, tab_size) as i64 + 1;

    let column = parts.get(1).and_then(|p| parse_int(p.trim()));
    let Some(mut column) = column else {
        return ParsedPosition {
            line: Some(line),
            column: Some(1),
            label: if parts.len() < 2 {
                format!("Press 'Enter' to go to line {line} or enter colon : to add a column number.")
            } else {
                format!("Press 'Enter' to go to line {line} or enter a column number (from 1 to {max_column}).")
            },
            ..Default::default()
        };
    };
    if column < 0 {
        column += max_column;
    }
    let column = column.clamp(1, max_column) as usize;
    let real_column = column_from_visible_column(&content, column - 1, tab_size);
    ParsedPosition {
        line: Some(line),
        column: Some(real_column),
        in_offset_mode: false,
        label: format!("Press 'Enter' to go to line {line} at column {column}."),
    }
}

/// A model over a list of lines (`\n` between them), for tests and simple callers.
pub struct LinesModel {
    pub lines: Vec<String>,
    pub tab_size: usize,
}

impl LinesModel {
    pub fn new(lines: &[&str], tab_size: usize) -> Self {
        Self { lines: lines.iter().map(|l| l.to_string()).collect(), tab_size }
    }
}

impl GotoLineModel for LinesModel {
    fn line_count(&self) -> usize {
        self.lines.len()
    }

    fn line_content(&self, line: usize) -> String {
        self.lines[line - 1].clone()
    }

    fn value_length(&self) -> usize {
        self.lines.iter().map(|l| utf16_len(l)).sum::<usize>() + self.lines.len() - 1
    }

    fn position_at(&self, offset: usize) -> (usize, usize) {
        let mut remaining = offset;
        for (i, l) in self.lines.iter().enumerate() {
            let len = utf16_len(l);
            if remaining <= len || i == self.lines.len() - 1 {
                return (i + 1, remaining.min(len) + 1);
            }
            remaining -= len + 1;
        }
        unreachable!("at least one line")
    }

    fn tab_size(&self) -> usize {
        self.tab_size
    }
}
