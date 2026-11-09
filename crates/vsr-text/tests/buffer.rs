//! Behavioural tests for the text buffer. Semantics mirror VS Code:
//! upstream: src/vs/editor/common/model/pieceTreeTextBuffer/pieceTreeTextBufferBuilder.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/core/position.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/core/range.ts @ 50e1350fc78d
//!
//! Lines and columns are 1-based; columns and offsets count UTF-16 code units
//! (that is what the extension API exposes, so we match it at the boundary).

use vsr_text::{DefaultEol, Eol, Position, Range, TextBuffer};

fn buf(s: &str) -> TextBuffer {
    TextBuffer::new(s, DefaultEol::Lf, true)
}

// ---- construction & EOL detection -------------------------------------------

#[test]
fn empty_buffer_has_one_empty_line() {
    let b = buf("");
    assert_eq!(b.line_count(), 1);
    assert_eq!(b.line_content(1), "");
    assert_eq!(b.eol(), Eol::Lf);
}

#[test]
fn default_eol_used_when_no_line_breaks() {
    let b = TextBuffer::new("single line", DefaultEol::CrLf, true);
    assert_eq!(b.eol(), Eol::CrLf);
}

#[test]
fn detects_crlf_when_majority() {
    let b = buf("a\r\nb\r\nc\nd");
    assert_eq!(b.eol(), Eol::CrLf);
    assert_eq!(b.line_count(), 4);
}

#[test]
fn detects_lf_on_tie() {
    // 1 CRLF, 1 LF: CR count (1) is not > half of 2 -> LF
    assert_eq!(buf("a\r\nb\nc").eol(), Eol::Lf);
}

#[test]
fn lone_cr_is_a_line_break() {
    let b = buf("a\rb\rc");
    assert_eq!(b.line_count(), 3);
    assert_eq!(b.line_content(2), "b");
    // lone CR counts towards CR total -> majority CR-based -> CRLF
    assert_eq!(b.eol(), Eol::CrLf);
}

#[test]
fn mixed_line_endings_are_normalized() {
    let b = buf("a\r\nb\nc\n");
    assert_eq!(b.eol(), Eol::Lf);
    assert_eq!(b.value(), "a\nb\nc\n");
}

#[test]
fn no_normalization_when_disabled() {
    let b = TextBuffer::new("a\r\nb\nc", DefaultEol::Lf, false);
    assert_eq!(b.value(), "a\r\nb\nc");
    assert_eq!(b.line_count(), 3);
    assert_eq!(b.line_content(1), "a");
}

// ---- line access ------------------------------------------------------------

#[test]
fn line_content_excludes_eol() {
    let b = buf("first\nsecond\n");
    assert_eq!(b.line_count(), 3);
    assert_eq!(b.line_content(1), "first");
    assert_eq!(b.line_content(2), "second");
    assert_eq!(b.line_content(3), "");
}

#[test]
fn line_length_is_utf16() {
    // "é" is 1 UTF-16 unit, "😀" is 2.
    let b = buf("é😀x");
    assert_eq!(b.line_length(1), 4);
    assert_eq!(b.line_max_column(1), 5);
}

// ---- offset <-> position ----------------------------------------------------

#[test]
fn offset_at_and_position_at_roundtrip() {
    let b = buf("ab\ncde\n\nf");
    assert_eq!(b.offset_at(Position::new(1, 1)), 0);
    assert_eq!(b.offset_at(Position::new(2, 1)), 3);
    assert_eq!(b.offset_at(Position::new(2, 4)), 6);
    assert_eq!(b.offset_at(Position::new(4, 2)), 9);
    for off in 0..=b.len_utf16() {
        assert_eq!(b.offset_at(b.position_at(off)), off, "offset {off}");
    }
    assert_eq!(b.position_at(3), Position::new(2, 1));
    assert_eq!(b.position_at(7), Position::new(3, 1));
}

#[test]
fn positions_are_clamped_like_vscode_validate_position() {
    let b = buf("ab\ncd");
    assert_eq!(b.validate_position(Position::new(0, 0)), Position::new(1, 1));
    assert_eq!(b.validate_position(Position::new(1, 99)), Position::new(1, 3));
    assert_eq!(b.validate_position(Position::new(99, 1)), Position::new(2, 3));
    assert_eq!(b.position_at(1000), Position::new(2, 3));
}

#[test]
fn crlf_counts_two_units_in_offsets() {
    let b = TextBuffer::new("ab\r\ncd", DefaultEol::Lf, true);
    assert_eq!(b.offset_at(Position::new(2, 1)), 4);
    assert_eq!(b.position_at(4), Position::new(2, 1));
}

// ---- ranges & edits ---------------------------------------------------------

#[test]
fn range_normalizes_and_compares() {
    let r = Range::new(Position::new(3, 2), Position::new(1, 5));
    assert_eq!(r.start, Position::new(1, 5));
    assert_eq!(r.end, Position::new(3, 2));
    assert!(!r.is_empty());
    assert!(r.contains(Position::new(2, 100)));
    assert!(!r.contains(Position::new(3, 3)));
    assert!(Range::at(Position::new(1, 1)).is_empty());
}

#[test]
fn value_in_range_spans_lines() {
    let b = buf("hello\nworld\n!");
    let r = Range::new(Position::new(1, 3), Position::new(2, 3));
    assert_eq!(b.value_in_range(r), "llo\nwo");
}

#[test]
fn insert_text() {
    let mut b = buf("hello world");
    b.edit(Range::at(Position::new(1, 6)), ",");
    assert_eq!(b.value(), "hello, world");
}

#[test]
fn replace_across_lines_and_insert_newlines() {
    let mut b = buf("one\ntwo\nthree");
    b.edit(Range::new(Position::new(1, 2), Position::new(3, 3)), "X\nY");
    assert_eq!(b.value(), "oX\nYree");
    assert_eq!(b.line_count(), 2);
    assert_eq!(b.line_content(2), "Yree");
}

#[test]
fn inserted_text_eol_is_normalized_to_buffer_eol() {
    let mut b = TextBuffer::new("a\r\nb", DefaultEol::Lf, true);
    b.edit(Range::at(Position::new(2, 2)), "\nc\r\nd");
    assert_eq!(b.value(), "a\r\nb\r\nc\r\nd");
}

#[test]
fn delete_entire_content() {
    let mut b = buf("x\ny\nz");
    let full = b.full_range();
    assert_eq!(full, Range::new(Position::new(1, 1), Position::new(3, 2)));
    b.edit(full, "");
    assert_eq!(b.value(), "");
    assert_eq!(b.line_count(), 1);
}

#[test]
fn edit_with_utf16_columns() {
    let mut b = buf("😀b");
    // column 3 is after the surrogate pair
    b.edit(Range::new(Position::new(1, 3), Position::new(1, 4)), "c");
    assert_eq!(b.value(), "😀c");
}
