//! Bracket matching for `editor.matchBrackets`: the bracket next to the caret and its partner, or
//! (`always`) the pair that encloses the caret.
//!
//! There is no tokenizer here, so the scan is a text heuristic: brackets inside a single-line
//! `"..."`, `'...'` or `` `...` `` literal (with backslash escapes) and after a `//` are ignored,
//! and the literal state resets at each line. Multi-line strings and block comments are not
//! understood. Pairs of different kinds are counted separately (`(` with `)` only), except for
//! the enclosing search, which counts every kind.
//!
//! upstream: src/vs/editor/common/model/bracketPairsTextModelPart/bracketPairsImpl.ts @ 50e1350fc78d

use vsr_text::{Position, TextBuffer};

/// Characters a scan may look at before giving up (upstream gives 20 ms; this bounds the work).
const MAX_CHARS: usize = 200_000;

/// The pairs, as (opener, closer).
const PAIRS: [(char, char); 3] = [('(', ')'), ('[', ']'), ('{', '}')];

fn is_opener(c: char) -> bool {
    PAIRS.iter().any(|&(o, _)| o == c)
}

fn is_closer(c: char) -> bool {
    PAIRS.iter().any(|&(_, cl)| cl == c)
}

/// One character of a line: its 1-based UTF-16 column, the character, and whether it is part of
/// a literal or a comment (and so not a bracket for matching).
type Cell = (usize, char, bool);

/// The characters of `text` with their columns and literal/comment flags.
fn line_cells(text: &str) -> Vec<Cell> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut col = 1usize;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    let mut comment = false;
    for (i, &ch) in chars.iter().enumerate() {
        let mut inside = comment || quote.is_some();
        if comment {
            // Everything after `//` is a comment.
        } else if let Some(q) = quote {
            inside = true;
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == q {
                quote = None;
            }
        } else if ch == '/' && chars.get(i + 1) == Some(&'/') {
            comment = true;
            inside = true;
        } else if matches!(ch, '"' | '\'' | '`') {
            quote = Some(ch);
            inside = true;
        }
        out.push((col, ch, inside));
        col += ch.len_utf16();
    }
    out
}

/// Whether the position `p` holds a bracket that counts for matching, and which one.
fn bracket_at(buffer: &TextBuffer, p: Position) -> Option<char> {
    line_cells(buffer.line_content(p.line))
        .into_iter()
        .find(|&(col, ch, inside)| col == p.column && !inside && (is_opener(ch) || is_closer(ch)))
        .map(|(_, ch, _)| ch)
}

/// The position of the partner of the opener at `open_at`, scanning forward.
fn forward_match(buffer: &TextBuffer, open_at: Position, open: char, close: char) -> Option<Position> {
    let mut depth = 0usize;
    let mut budget = MAX_CHARS;
    for line in open_at.line..=buffer.line_count() {
        for (col, ch, inside) in line_cells(buffer.line_content(line)) {
            if (line == open_at.line && col <= open_at.column) || inside {
                continue;
            }
            budget = budget.checked_sub(1)?;
            if ch == open {
                depth += 1;
            } else if ch == close {
                if depth == 0 {
                    return Some(Position::new(line, col));
                }
                depth -= 1;
            }
        }
    }
    None
}

/// The position of the partner of the closer at `close_at`, scanning backward.
fn backward_match(buffer: &TextBuffer, close_at: Position, open: char, close: char) -> Option<Position> {
    let mut depth = 0usize;
    let mut budget = MAX_CHARS;
    for line in (1..=close_at.line).rev() {
        for (col, ch, inside) in line_cells(buffer.line_content(line)).into_iter().rev() {
            if (line == close_at.line && col >= close_at.column) || inside {
                continue;
            }
            budget = budget.checked_sub(1)?;
            if ch == close {
                depth += 1;
            } else if ch == open {
                if depth == 0 {
                    return Some(Position::new(line, col));
                }
                depth -= 1;
            }
        }
    }
    None
}

/// The bracket immediately after the caret, or else before it, and its partner.
fn adjacent(buffer: &TextBuffer, caret: Position) -> Option<(Position, Position)> {
    let before = Position::new(caret.line, caret.column.saturating_sub(1));
    for at in [caret, before] {
        if at.column == 0 {
            continue;
        }
        let Some(ch) = bracket_at(buffer, at) else { continue };
        let partner = if is_opener(ch) {
            let close = PAIRS.iter().find(|p| p.0 == ch)?.1;
            forward_match(buffer, at, ch, close)
        } else {
            let open = PAIRS.iter().find(|p| p.1 == ch)?.0;
            backward_match(buffer, at, open, ch)
        };
        return partner.map(|p| (at, p));
    }
    None
}

/// The innermost pair that encloses the caret, as (opener, closer).
fn enclosing(buffer: &TextBuffer, caret: Position) -> Option<(Position, Position)> {
    let mut depth = 0usize;
    let mut budget = MAX_CHARS;
    let mut opener = None;
    'scan: for line in (1..=caret.line).rev() {
        for (col, ch, inside) in line_cells(buffer.line_content(line)).into_iter().rev() {
            if (line == caret.line && col >= caret.column) || inside {
                continue;
            }
            budget = budget.checked_sub(1)?;
            if is_closer(ch) {
                depth += 1;
            } else if is_opener(ch) {
                if depth == 0 {
                    opener = Some((Position::new(line, col), ch));
                    break 'scan;
                }
                depth -= 1;
            }
        }
    }
    let (at, ch) = opener?;
    let close = PAIRS.iter().find(|p| p.0 == ch)?.1;
    forward_match(buffer, at, ch, close).map(|p| (at, p))
}

/// The two brackets to highlight for a caret: the bracket next to it with its partner. With
/// `enclosing` (`editor.matchBrackets: always`) a caret that is not next to a bracket gets the
/// pair that encloses it. `None` when there is nothing to match.
pub fn matching_brackets(buffer: &TextBuffer, caret: Position, enclosing_too: bool) -> Option<(Position, Position)> {
    adjacent(buffer, caret).or_else(|| enclosing_too.then(|| enclosing(buffer, caret)).flatten())
}

#[cfg(test)]
mod tests {
    use vsr_text::DefaultEol;

    use super::*;

    fn buf(text: &str) -> TextBuffer {
        TextBuffer::new(text, DefaultEol::Lf, true)
    }

    fn at(line: usize, column: usize) -> Position {
        Position::new(line, column)
    }

    #[test]
    fn bracket_after_the_caret_matches_forward() {
        // caret before "(" on "f(a)"
        assert_eq!(matching_brackets(&buf("f(a)"), at(1, 2), false), Some((at(1, 2), at(1, 4))));
    }

    #[test]
    fn bracket_before_the_caret_matches_backward() {
        // caret after ")" on "f(a)"
        assert_eq!(matching_brackets(&buf("f(a)"), at(1, 5), false), Some((at(1, 4), at(1, 2))));
    }

    #[test]
    fn nested_and_multi_line_pairs_are_matched_by_depth() {
        let b = buf("f({\n  [1, (2)],\n})");
        assert_eq!(matching_brackets(&b, at(1, 3), false), Some((at(1, 3), at(3, 1))), "the brace on line 1 closes on line 3");
        assert_eq!(matching_brackets(&b, at(1, 2), false), Some((at(1, 2), at(3, 2))), "the paren closes on line 3");
        assert_eq!(matching_brackets(&b, at(2, 3), false), Some((at(2, 3), at(2, 10))), "[ closes after the inner pair");
        assert_eq!(matching_brackets(&b, at(2, 9), false), Some((at(2, 9), at(2, 7))), "the ) after 2 closes the inner (");
        assert_eq!(matching_brackets(&b, at(2, 8), false), Some((at(2, 7), at(2, 9))), "a caret after ( takes the ( before it");
    }

    #[test]
    fn no_bracket_next_to_the_caret_means_no_match_unless_enclosing() {
        let b = buf("f(abc)");
        assert_eq!(matching_brackets(&b, at(1, 4), false), None, "a is not a bracket");
        assert_eq!(matching_brackets(&b, at(1, 4), true), Some((at(1, 2), at(1, 6))), "always: the enclosing pair");
        assert_eq!(matching_brackets(&buf("abc"), at(1, 2), true), None);
    }

    #[test]
    fn unmatched_brackets_have_no_partner() {
        assert_eq!(matching_brackets(&buf("(a"), at(1, 1), false), None);
        assert_eq!(matching_brackets(&buf("a)"), at(1, 3), false), None);
    }

    #[test]
    fn brackets_inside_literals_and_comments_are_ignored() {
        let b = buf("x = \"(\" + f(1) // )");
        // The "(" inside the string is not a bracket, so the "(" of f matches its ")".
        assert_eq!(matching_brackets(&b, at(1, 12), false), Some((at(1, 12), at(1, 14))));
        // A bracket next to the caret inside a literal does not count.
        assert_eq!(matching_brackets(&b, at(1, 6), false), None);
        // A comment's ")" is not a bracket, and it does not match the "(" before it.
        assert_eq!(matching_brackets(&buf("(a) // )"), at(1, 4), false), Some((at(1, 3), at(1, 1))));
        assert_eq!(matching_brackets(&buf("(a) // )"), at(1, 9), false), None);
    }

    #[test]
    fn escaped_quotes_do_not_end_a_literal() {
        let b = buf(r#"s = "a\"(" ; g()"#);
        assert_eq!(matching_brackets(&b, at(1, 15), false), Some((at(1, 15), at(1, 16))), "the ( after the escaped quote is in the string");
    }

    #[test]
    fn columns_are_utf16() {
        // The emoji is two UTF-16 units, so the "(" is column 3.
        assert_eq!(matching_brackets(&buf("\u{1F600}()"), at(1, 3), false), Some((at(1, 3), at(1, 4))));
    }
}
