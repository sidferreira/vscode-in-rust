//! upstream: src/vs/editor/test/common/model/pieceTreeTextBuffer/pieceTreeTextBuffer.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/model/linesTextBuffer/linesTextBufferBuilder.test.ts @ 50e1350fc78d
//!
//! Ports of VS Code's piece tree buffer tests onto `vsr_text::TextBuffer`.
//!
//! Conventions:
//! - Upstream `pieceTable.insert(offset, text)` / `delete(offset, count)` take 0-based
//!   UTF-16 offsets and never normalize line breaks. `buf_insert` / `buf_delete` call
//!   `TextBuffer::insert_at_offset` / `delete_at_offset`, which do the same.
//! - Upstream `createTextBuffer(chunks, normalizeEOL)` is `create_text_buffer`.
//! - `pos_at` is `getPositionAt`. For an offset between the `\r` and `\n` of a CRLF it
//!   returns a column past the line's end, and `offset_at` maps that back.
//! - `assertTreeInvariants` is a no-op: `TextBuffer` has no tree yet.
//! - Each test names its upstream suite and test in a `// upstream:` comment.
//! - Every test string is ASCII, so byte offsets equal UTF-16 offsets in the helpers
//!   that work on `&str` (`split_lines`, `line_starts`, `str_*`).

use vsr_text::{DefaultEol, Position, Range, TextBuffer};

// ---- helpers mirroring the upstream test file -------------------------------

/// `createTextBuffer(chunks, normalizeEOL)`.
fn create_text_buffer(chunks: &[&str], normalize_eol: bool) -> TextBuffer {
    TextBuffer::new(&chunks.concat(), DefaultEol::Lf, normalize_eol)
}

/// `getPositionAt(offset)`. An offset between the `\r` and `\n` of a CRLF gives a column
/// past the line's end, as upstream does.
fn pos_at(b: &TextBuffer, offset: usize) -> Position {
    b.position_at(offset)
}

/// `pieceTable.insert(offset, text)`: raw, no EOL normalization.
fn buf_insert(b: &mut TextBuffer, offset: usize, text: &str) {
    b.insert_at_offset(offset, text);
}

/// `pieceTable.delete(offset, count)`.
fn buf_delete(b: &mut TextBuffer, offset: usize, count: usize) {
    b.delete_at_offset(offset, count);
}

/// `new Range(sl, sc, el, ec)`.
fn rng(sl: usize, sc: usize, el: usize, ec: usize) -> Range {
    Range::new(Position::new(sl, sc), Position::new(el, ec))
}

/// `getLineRawContent(n)`: the line with its terminator (`getLinesRawContent` is `value()`).
fn line_raw(b: &TextBuffer, n: usize) -> String {
    if n < b.line_count() {
        b.value_in_range(rng(n, 1, n + 1, 1))
    } else {
        b.value_in_range(rng(n, 1, n, b.line_max_column(n)))
    }
}

/// `getCharCode`-style access to a raw line: the UTF-16 unit at `index` of `getLineRawContent(line)`.
fn line_char_code(b: &TextBuffer, line: usize, index: usize) -> u16 {
    line_raw(b, line).encode_utf16().nth(index).expect("index in line")
}

/// JS `str.split(/\r\n|\r|\n/)`.
fn split_lines(s: &str) -> Vec<&str> {
    let bytes = s.as_bytes();
    let mut out = Vec::new();
    let (mut start, mut i) = (0, 0);
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                out.push(&s[start..i]);
                i += 2;
                start = i;
            }
            b'\r' | b'\n' => {
                out.push(&s[start..i]);
                i += 1;
                start = i;
            }
            _ => i += 1,
        }
    }
    out.push(&s[start..]);
    out
}

/// Start offsets of every line of `s`: 0, then the end of every line break.
fn line_starts(s: &str) -> Vec<usize> {
    let bytes = s.as_bytes();
    let mut starts = vec![0];
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => {
                i += 2;
                starts.push(i);
            }
            b'\r' | b'\n' => {
                i += 1;
                starts.push(i);
            }
            _ => i += 1,
        }
    }
    starts
}

fn trim_line_feed(text: &str) -> &str {
    text.strip_suffix("\r\n")
        .or_else(|| text.strip_suffix('\n'))
        .or_else(|| text.strip_suffix('\r'))
        .unwrap_or(text)
}

/// `testLinesContent(str, pt)`.
fn test_lines_content(s: &str, b: &TextBuffer) {
    let lines = split_lines(s);
    assert_eq!(b.line_count(), lines.len());
    assert_eq!(b.value(), s);
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        assert_eq!(b.line_content(n), *line);
        let extra = if i == lines.len() - 1 { 1 } else { 2 };
        let end_column = line.encode_utf16().count() + extra;
        let raw = b.value_in_range(rng(n, 1, n, end_column));
        assert_eq!(trim_line_feed(&raw), *line);
    }
}

/// `testLineStarts(str, pt)`: offset <-> position round trip at every line start.
fn test_line_starts(s: &str, b: &TextBuffer) {
    let starts = line_starts(s);
    for (i, &start) in starts.iter().enumerate() {
        assert_eq!(pos_at(b, start), Position::new(i + 1, 1));
        assert_eq!(b.offset_at(Position::new(i + 1, 1)), start);
    }
    for &start in &starts[1..] {
        let pos = pos_at(b, start - 1);
        assert_eq!(b.offset_at(pos), start - 1);
    }
}

/// `assertTreeInvariants(pt)`. Vacuous until the piece tree exists.
fn assert_tree_invariants(_b: &TextBuffer) {}

// ---- ModelBuilder (linesTextBufferBuilder.test.ts) --------------------------
//
// Only the EOL assertion is ported: `mightContainNonBasicASCII` and `mightContainRTL`
// do not exist on `TextBuffer`, and the BOM is not stripped.

mod model_builder {
    use super::*;

    fn test_text_buffer_eol(text: &str, eol: &str) {
        let b = TextBuffer::new(text, DefaultEol::Lf, true);
        let expected = if eol == "\r\n" { vsr_text::Eol::CrLf } else { vsr_text::Eol::Lf };
        assert_eq!(b.eol(), expected);
    }

    // upstream: ModelBuilder > t1
    #[test]
    fn t1_empty() {
        test_text_buffer_eol("", "\n");
    }

    // upstream: ModelBuilder > t2
    #[test]
    fn t2_single_line() {
        test_text_buffer_eol("Hello world", "\n");
    }

    // upstream: ModelBuilder > t3
    #[test]
    fn t3_two_lines() {
        test_text_buffer_eol("Hello world\nHow are you?", "\n");
    }

    // upstream: ModelBuilder > t4
    #[test]
    fn t4_four_lines() {
        test_text_buffer_eol(
            "Hello world\nHow are you?\nIs everything good today?\nDo you enjoy the weather?",
            "\n",
        );
    }

    // upstream: ModelBuilder > carriage return detection (1 \r\n 2 \n)
    #[test]
    fn carriage_return_detection_1_crlf_2_lf() {
        test_text_buffer_eol(
            "Hello world\r\nHow are you?\nIs everything good today?\nDo you enjoy the weather?",
            "\n",
        );
    }

    // upstream: ModelBuilder > carriage return detection (2 \r\n 1 \n)
    #[test]
    fn carriage_return_detection_2_crlf_1_lf() {
        test_text_buffer_eol(
            "Hello world\r\nHow are you?\r\nIs everything good today?\nDo you enjoy the weather?",
            "\r\n",
        );
    }

    // upstream: ModelBuilder > carriage return detection (3 \r\n 0 \n)
    #[test]
    fn carriage_return_detection_3_crlf_0_lf() {
        test_text_buffer_eol(
            "Hello world\r\nHow are you?\r\nIs everything good today?\r\nDo you enjoy the weather?",
            "\r\n",
        );
    }
}

// ---- inserts and deletes -----------------------------------------------------

mod inserts_and_deletes {
    use super::*;

    // upstream: inserts and deletes > basic insert/delete
    #[test]
    fn basic_insert_delete() {
        let mut b = create_text_buffer(&["This is a document with some text."], true);
        buf_insert(&mut b, 34, "This is some more text to insert at offset 34.");
        assert_eq!(
            b.value(),
            "This is a document with some text.This is some more text to insert at offset 34."
        );
        buf_delete(&mut b, 42, 5);
        assert_eq!(
            b.value(),
            "This is a document with some text.This is more text to insert at offset 34."
        );
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > more inserts
    #[test]
    fn more_inserts() {
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "AAA");
        assert_eq!(b.value(), "AAA");
        buf_insert(&mut b, 0, "BBB");
        assert_eq!(b.value(), "BBBAAA");
        buf_insert(&mut b, 6, "CCC");
        assert_eq!(b.value(), "BBBAAACCC");
        buf_insert(&mut b, 5, "DDD");
        assert_eq!(b.value(), "BBBAADDDACCC");
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > more deletes
    #[test]
    fn more_deletes() {
        let mut b = create_text_buffer(&["012345678"], true);
        buf_delete(&mut b, 8, 1);
        assert_eq!(b.value(), "01234567");
        buf_delete(&mut b, 0, 1);
        assert_eq!(b.value(), "1234567");
        buf_delete(&mut b, 5, 1);
        assert_eq!(b.value(), "123457");
        buf_delete(&mut b, 5, 1);
        assert_eq!(b.value(), "12345");
        buf_delete(&mut b, 0, 5);
        assert_eq!(b.value(), "");
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random test 1
    #[test]
    fn random_test_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "ceLPHmFzvCtFeHkCBej ");
        s.insert_str(0, "ceLPHmFzvCtFeHkCBej ");
        assert_eq!(b.value(), s);
        buf_insert(&mut b, 8, "gDCEfNYiBUNkSwtvB K ");
        s.insert_str(8, "gDCEfNYiBUNkSwtvB K ");
        assert_eq!(b.value(), s);
        buf_insert(&mut b, 38, "cyNcHxjNPPoehBJldLS ");
        s.insert_str(38, "cyNcHxjNPPoehBJldLS ");
        assert_eq!(b.value(), s);
        buf_insert(&mut b, 59, "ejMx\nOTgWlbpeDExjOk ");
        s.insert_str(59, "ejMx\nOTgWlbpeDExjOk ");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random test 2
    #[test]
    fn random_test_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "VgPG ");
        s.insert_str(0, "VgPG ");
        buf_insert(&mut b, 2, "DdWF ");
        s.insert_str(2, "DdWF ");
        buf_insert(&mut b, 0, "hUJc ");
        s.insert_str(0, "hUJc ");
        buf_insert(&mut b, 8, "lQEq ");
        s.insert_str(8, "lQEq ");
        buf_insert(&mut b, 10, "Gbtp ");
        s.insert_str(10, "Gbtp ");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random test 3
    #[test]
    fn random_test_3() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "gYSz");
        s.insert_str(0, "gYSz");
        buf_insert(&mut b, 1, "mDQe");
        s.insert_str(1, "mDQe");
        buf_insert(&mut b, 1, "DTMQ");
        s.insert_str(1, "DTMQ");
        buf_insert(&mut b, 2, "GGZB");
        s.insert_str(2, "GGZB");
        buf_insert(&mut b, 12, "wXpq");
        s.insert_str(12, "wXpq");
        assert_eq!(b.value(), s);
    }

    // upstream: inserts and deletes > random delete 1
    #[test]
    fn random_delete_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "vfb");
        s.insert_str(0, "vfb");
        assert_eq!(b.value(), s);
        buf_insert(&mut b, 0, "zRq");
        s.insert_str(0, "zRq");
        assert_eq!(b.value(), s);

        buf_delete(&mut b, 5, 1);
        s.replace_range(5..6, "");
        assert_eq!(b.value(), s);

        buf_insert(&mut b, 1, "UNw");
        s.insert_str(1, "UNw");
        assert_eq!(b.value(), s);

        buf_delete(&mut b, 4, 3);
        s.replace_range(4..7, "");
        assert_eq!(b.value(), s);

        buf_delete(&mut b, 1, 4);
        s.replace_range(1..5, "");
        assert_eq!(b.value(), s);

        buf_delete(&mut b, 0, 1);
        s.replace_range(0..1, "");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random delete 2
    #[test]
    fn random_delete_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "IDT");
        s.insert_str(0, "IDT");
        buf_insert(&mut b, 3, "wwA");
        s.insert_str(3, "wwA");
        buf_insert(&mut b, 3, "Gnr");
        s.insert_str(3, "Gnr");
        buf_delete(&mut b, 6, 3);
        s.replace_range(6..9, "");
        buf_insert(&mut b, 4, "eHp");
        s.insert_str(4, "eHp");
        buf_insert(&mut b, 1, "UAi");
        s.insert_str(1, "UAi");
        buf_insert(&mut b, 2, "FrR");
        s.insert_str(2, "FrR");
        buf_delete(&mut b, 6, 7);
        s.replace_range(6..13, "");
        buf_delete(&mut b, 3, 5);
        s.replace_range(3..8, "");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random delete 3
    #[test]
    fn random_delete_3() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "PqM");
        s.insert_str(0, "PqM");
        buf_delete(&mut b, 1, 2);
        s.replace_range(1..3, "");
        buf_insert(&mut b, 1, "zLc");
        s.insert_str(1, "zLc");
        buf_insert(&mut b, 0, "MEX");
        s.insert_str(0, "MEX");
        buf_insert(&mut b, 0, "jZh");
        s.insert_str(0, "jZh");
        buf_insert(&mut b, 8, "GwQ");
        s.insert_str(8, "GwQ");
        buf_delete(&mut b, 5, 6);
        s.replace_range(5..11, "");
        buf_insert(&mut b, 4, "ktw");
        s.insert_str(4, "ktw");
        buf_insert(&mut b, 5, "GVu");
        s.insert_str(5, "GVu");
        buf_insert(&mut b, 9, "jdm");
        s.insert_str(9, "jdm");
        buf_insert(&mut b, 15, "na\n");
        s.insert_str(15, "na\n");
        buf_delete(&mut b, 5, 8);
        s.replace_range(5..13, "");
        buf_delete(&mut b, 3, 4);
        s.replace_range(3..7, "");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random insert/delete \r bug 1
    // Raw inserts of `\r` need `normalize = false`.
    #[test]
    fn random_insert_delete_r_bug_1() {
        let mut s = String::from("a");
        let mut b = create_text_buffer(&["a"], false);
        buf_delete(&mut b, 0, 1);
        s.replace_range(0..1, "");
        buf_insert(&mut b, 0, "\r\r\n\n");
        s.insert_str(0, "\r\r\n\n");
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_insert(&mut b, 2, "\n\n\ra");
        s.insert_str(2, "\n\n\ra");
        buf_delete(&mut b, 4, 3);
        s.replace_range(4..7, "");
        buf_insert(&mut b, 2, "\na\r\r");
        s.insert_str(2, "\na\r\r");
        buf_insert(&mut b, 6, "\ra\n\n");
        s.insert_str(6, "\ra\n\n");
        buf_insert(&mut b, 0, "aa\n\n");
        s.insert_str(0, "aa\n\n");
        buf_insert(&mut b, 5, "\n\na\r");
        s.insert_str(5, "\n\na\r");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random insert/delete \r bug 2
    #[test]
    fn random_insert_delete_r_bug_2() {
        let mut s = String::from("a");
        let mut b = create_text_buffer(&["a"], false);
        buf_insert(&mut b, 1, "\naa\r");
        s.insert_str(1, "\naa\r");
        buf_delete(&mut b, 0, 4);
        s.replace_range(0..4, "");
        buf_insert(&mut b, 1, "\r\r\na");
        s.insert_str(1, "\r\r\na");
        buf_insert(&mut b, 2, "\n\r\ra");
        s.insert_str(2, "\n\r\ra");
        buf_delete(&mut b, 4, 1);
        s.replace_range(4..5, "");
        buf_insert(&mut b, 8, "\r\n\r\r");
        s.insert_str(8, "\r\n\r\r");
        buf_insert(&mut b, 7, "\n\n\na");
        s.insert_str(7, "\n\n\na");
        buf_insert(&mut b, 13, "a\n\na");
        s.insert_str(13, "a\n\na");
        buf_delete(&mut b, 17, 3);
        s.replace_range(17..20, "");
        buf_insert(&mut b, 2, "a\ra\n");
        s.insert_str(2, "a\ra\n");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random insert/delete \r bug 3
    #[test]
    fn random_insert_delete_r_bug_3() {
        let mut s = String::from("a");
        let mut b = create_text_buffer(&["a"], false);
        buf_insert(&mut b, 0, "\r\na\r");
        s.insert_str(0, "\r\na\r");
        buf_delete(&mut b, 2, 3);
        s.replace_range(2..5, "");
        buf_insert(&mut b, 2, "a\r\n\r");
        s.insert_str(2, "a\r\n\r");
        buf_delete(&mut b, 4, 2);
        s.replace_range(4..6, "");
        buf_insert(&mut b, 4, "a\n\r\n");
        s.insert_str(4, "a\n\r\n");
        buf_insert(&mut b, 1, "aa\n\r");
        s.insert_str(1, "aa\n\r");
        buf_insert(&mut b, 7, "\na\r\n");
        s.insert_str(7, "\na\r\n");
        buf_insert(&mut b, 5, "\n\na\r");
        s.insert_str(5, "\n\na\r");
        buf_insert(&mut b, 10, "\r\r\n\r");
        s.insert_str(10, "\r\r\n\r");
        assert_eq!(b.value(), s);
        buf_delete(&mut b, 21, 3);
        s.replace_range(21..24, "");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random insert/delete \r bug 4s
    #[test]
    fn random_insert_delete_r_bug_4s() {
        let mut s = String::from("a");
        let mut b = create_text_buffer(&["a"], false);
        buf_delete(&mut b, 0, 1);
        s.replace_range(0..1, "");
        buf_insert(&mut b, 0, "\naaa");
        s.insert_str(0, "\naaa");
        buf_insert(&mut b, 2, "\n\naa");
        s.insert_str(2, "\n\naa");
        buf_delete(&mut b, 1, 4);
        s.replace_range(1..5, "");
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_delete(&mut b, 1, 2);
        s.replace_range(1..3, "");
        buf_delete(&mut b, 0, 1);
        s.replace_range(0..1, "");
        buf_insert(&mut b, 0, "a\n\n\r");
        s.insert_str(0, "a\n\n\r");
        buf_insert(&mut b, 2, "aa\r\n");
        s.insert_str(2, "aa\r\n");
        buf_insert(&mut b, 3, "a\naa");
        s.insert_str(3, "a\naa");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }

    // upstream: inserts and deletes > random insert/delete \r bug 5
    #[test]
    fn random_insert_delete_r_bug_5() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\n\n\r");
        s.insert_str(0, "\n\n\n\r");
        buf_insert(&mut b, 1, "\n\n\n\r");
        s.insert_str(1, "\n\n\n\r");
        buf_insert(&mut b, 2, "\n\r\r\r");
        s.insert_str(2, "\n\r\r\r");
        buf_insert(&mut b, 8, "\n\r\n\r");
        s.insert_str(8, "\n\r\n\r");
        buf_delete(&mut b, 5, 2);
        s.replace_range(5..7, "");
        buf_insert(&mut b, 4, "\n\r\r\r");
        s.insert_str(4, "\n\r\r\r");
        buf_insert(&mut b, 8, "\n\n\n\r");
        s.insert_str(8, "\n\n\n\r");
        buf_delete(&mut b, 0, 7);
        s.replace_range(0..7, "");
        buf_insert(&mut b, 1, "\r\n\r\r");
        s.insert_str(1, "\r\n\r\r");
        buf_insert(&mut b, 15, "\n\r\r\r");
        s.insert_str(15, "\n\r\r\r");
        assert_eq!(b.value(), s);
        assert_tree_invariants(&b);
    }
}

// ---- prefix sum for line feed ----------------------------------------------

mod prefix_sum_for_line_feed {
    use super::*;

    // upstream: prefix sum for line feed > basic
    #[test]
    fn basic() {
        let b = create_text_buffer(&["1\n2\n3\n4"], true);
        assert_eq!(b.line_count(), 4);
        assert_eq!(b.position_at(0), Position::new(1, 1));
        assert_eq!(b.position_at(1), Position::new(1, 2));
        assert_eq!(b.position_at(2), Position::new(2, 1));
        assert_eq!(b.position_at(3), Position::new(2, 2));
        assert_eq!(b.position_at(4), Position::new(3, 1));
        assert_eq!(b.position_at(5), Position::new(3, 2));
        assert_eq!(b.position_at(6), Position::new(4, 1));

        assert_eq!(b.offset_at(Position::new(1, 1)), 0);
        assert_eq!(b.offset_at(Position::new(1, 2)), 1);
        assert_eq!(b.offset_at(Position::new(2, 1)), 2);
        assert_eq!(b.offset_at(Position::new(2, 2)), 3);
        assert_eq!(b.offset_at(Position::new(3, 1)), 4);
        assert_eq!(b.offset_at(Position::new(3, 2)), 5);
        assert_eq!(b.offset_at(Position::new(4, 1)), 6);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > append
    #[test]
    fn append() {
        let mut b = create_text_buffer(&["a\nb\nc\nde"], true);
        buf_insert(&mut b, 8, "fh\ni\njk");
        assert_eq!(b.line_count(), 6);
        assert_eq!(b.position_at(9), Position::new(4, 4));
        assert_eq!(b.offset_at(Position::new(1, 1)), 0);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > insert
    #[test]
    fn insert() {
        let mut b = create_text_buffer(&["a\nb\nc\nde"], true);
        buf_insert(&mut b, 7, "fh\ni\njk");
        assert_eq!(b.line_count(), 6);
        assert_eq!(b.position_at(6), Position::new(4, 1));
        assert_eq!(b.position_at(7), Position::new(4, 2));
        assert_eq!(b.position_at(8), Position::new(4, 3));
        assert_eq!(b.position_at(9), Position::new(4, 4));
        assert_eq!(b.position_at(12), Position::new(6, 1));
        assert_eq!(b.position_at(13), Position::new(6, 2));
        assert_eq!(b.position_at(14), Position::new(6, 3));

        assert_eq!(b.offset_at(Position::new(4, 1)), 6);
        assert_eq!(b.offset_at(Position::new(4, 2)), 7);
        assert_eq!(b.offset_at(Position::new(4, 3)), 8);
        assert_eq!(b.offset_at(Position::new(4, 4)), 9);
        assert_eq!(b.offset_at(Position::new(6, 1)), 12);
        assert_eq!(b.offset_at(Position::new(6, 2)), 13);
        assert_eq!(b.offset_at(Position::new(6, 3)), 14);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > delete
    #[test]
    fn delete() {
        let mut b = create_text_buffer(&["a\nb\nc\ndefh\ni\njk"], true);
        buf_delete(&mut b, 7, 2);

        assert_eq!(b.value(), "a\nb\nc\ndh\ni\njk");
        assert_eq!(b.line_count(), 6);
        assert_eq!(b.position_at(6), Position::new(4, 1));
        assert_eq!(b.position_at(7), Position::new(4, 2));
        assert_eq!(b.position_at(8), Position::new(4, 3));
        assert_eq!(b.position_at(9), Position::new(5, 1));
        assert_eq!(b.position_at(11), Position::new(6, 1));
        assert_eq!(b.position_at(12), Position::new(6, 2));
        assert_eq!(b.position_at(13), Position::new(6, 3));

        assert_eq!(b.offset_at(Position::new(4, 1)), 6);
        assert_eq!(b.offset_at(Position::new(4, 2)), 7);
        assert_eq!(b.offset_at(Position::new(4, 3)), 8);
        assert_eq!(b.offset_at(Position::new(5, 1)), 9);
        assert_eq!(b.offset_at(Position::new(6, 1)), 11);
        assert_eq!(b.offset_at(Position::new(6, 2)), 12);
        assert_eq!(b.offset_at(Position::new(6, 3)), 13);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > add+delete 1
    #[test]
    fn add_delete_1() {
        let mut b = create_text_buffer(&["a\nb\nc\nde"], true);
        buf_insert(&mut b, 8, "fh\ni\njk");
        buf_delete(&mut b, 7, 2);

        assert_eq!(b.value(), "a\nb\nc\ndh\ni\njk");
        assert_eq!(b.line_count(), 6);
        assert_eq!(b.position_at(6), Position::new(4, 1));
        assert_eq!(b.position_at(7), Position::new(4, 2));
        assert_eq!(b.position_at(8), Position::new(4, 3));
        assert_eq!(b.position_at(9), Position::new(5, 1));
        assert_eq!(b.position_at(11), Position::new(6, 1));
        assert_eq!(b.position_at(12), Position::new(6, 2));
        assert_eq!(b.position_at(13), Position::new(6, 3));

        assert_eq!(b.offset_at(Position::new(4, 1)), 6);
        assert_eq!(b.offset_at(Position::new(4, 2)), 7);
        assert_eq!(b.offset_at(Position::new(4, 3)), 8);
        assert_eq!(b.offset_at(Position::new(5, 1)), 9);
        assert_eq!(b.offset_at(Position::new(6, 1)), 11);
        assert_eq!(b.offset_at(Position::new(6, 2)), 12);
        assert_eq!(b.offset_at(Position::new(6, 3)), 13);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > insert random bug 1: prefixSumComputer.removeValues(start, cnt) cnt is 1 based.
    #[test]
    fn insert_random_bug_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, " ZX \n Z\nZ\n YZ\nY\nZXX ");
        s.insert_str(0, " ZX \n Z\nZ\n YZ\nY\nZXX ");
        buf_insert(&mut b, 14, "X ZZ\nYZZYZXXY Y XY\n ");
        s.insert_str(14, "X ZZ\nYZZYZXXY Y XY\n ");
        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > insert random bug 2: prefixSumComputer initialize does not do deep copy of UInt32Array.
    #[test]
    fn insert_random_bug_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "ZYZ\nYY XY\nX \nZ Y \nZ ");
        s.insert_str(0, "ZYZ\nYY XY\nX \nZ Y \nZ ");
        buf_insert(&mut b, 3, "XXY \n\nY Y YYY  ZYXY ");
        s.insert_str(3, "XXY \n\nY Y YYY  ZYXY ");
        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > delete random bug 1: I forgot to update the lineFeedCnt when deletion is on one single piece.
    #[test]
    fn delete_random_bug_1() {
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "ba\na\nca\nba\ncbab\ncaa ");
        buf_insert(&mut b, 13, "cca\naabb\ncac\nccc\nab ");
        buf_delete(&mut b, 5, 8);
        buf_delete(&mut b, 30, 2);
        buf_insert(&mut b, 24, "cbbacccbac\nbaaab\n\nc ");
        buf_delete(&mut b, 29, 3);
        buf_delete(&mut b, 23, 9);
        buf_delete(&mut b, 21, 5);
        buf_delete(&mut b, 30, 3);
        buf_insert(&mut b, 3, "cb\nac\nc\n\nacc\nbb\nb\nc ");
        buf_delete(&mut b, 19, 5);
        buf_insert(&mut b, 18, "\nbb\n\nacbc\ncbb\nc\nbb\n ");
        buf_insert(&mut b, 65, "cbccbac\nbc\n\nccabba\n ");
        buf_insert(&mut b, 77, "a\ncacb\n\nac\n\n\n\n\nabab ");
        buf_delete(&mut b, 30, 9);
        buf_insert(&mut b, 45, "b\n\nc\nba\n\nbbbba\n\naa\n ");
        buf_insert(&mut b, 82, "ab\nbb\ncabacab\ncbc\na ");
        buf_delete(&mut b, 123, 9);
        buf_delete(&mut b, 71, 2);
        buf_insert(&mut b, 33, "acaa\nacb\n\naa\n\nc\n\n\n\n ");

        let s = b.value();
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > delete random bug rb tree 1
    #[test]
    fn delete_random_bug_rb_tree_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "YXXZ\n\nYY\n");
        s.insert_str(0, "YXXZ\n\nYY\n");
        buf_delete(&mut b, 0, 5);
        s.replace_range(0..5, "");
        buf_insert(&mut b, 0, "ZXYY\nX\nZ\n");
        s.insert_str(0, "ZXYY\nX\nZ\n");
        buf_insert(&mut b, 10, "\nXY\nYXYXY");
        s.insert_str(10, "\nXY\nYXYXY");
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > delete random bug rb tree 2
    #[test]
    fn delete_random_bug_rb_tree_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "YXXZ\n\nYY\n");
        s.insert_str(0, "YXXZ\n\nYY\n");
        buf_insert(&mut b, 0, "ZXYY\nX\nZ\n");
        s.insert_str(0, "ZXYY\nX\nZ\n");
        buf_insert(&mut b, 10, "\nXY\nYXYXY");
        s.insert_str(10, "\nXY\nYXYXY");
        buf_insert(&mut b, 8, "YZXY\nZ\nYX");
        s.insert_str(8, "YZXY\nZ\nYX");
        buf_insert(&mut b, 12, "XX\nXXYXYZ");
        s.insert_str(12, "XX\nXXYXYZ");
        buf_delete(&mut b, 0, 4);
        s.replace_range(0..4, "");

        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: prefix sum for line feed > delete random bug rb tree 3
    #[test]
    fn delete_random_bug_rb_tree_3() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "YXXZ\n\nYY\n");
        s.insert_str(0, "YXXZ\n\nYY\n");
        buf_delete(&mut b, 7, 2);
        s.replace_range(7..9, "");
        buf_delete(&mut b, 6, 1);
        s.replace_range(6..7, "");
        buf_delete(&mut b, 0, 5);
        s.replace_range(0..5, "");
        buf_insert(&mut b, 0, "ZXYY\nX\nZ\n");
        s.insert_str(0, "ZXYY\nX\nZ\n");
        buf_insert(&mut b, 10, "\nXY\nYXYXY");
        s.insert_str(10, "\nXY\nYXYXY");
        buf_insert(&mut b, 8, "YZXY\nZ\nYX");
        s.insert_str(8, "YZXY\nZ\nYX");
        buf_insert(&mut b, 12, "XX\nXXYXYZ");
        s.insert_str(12, "XX\nXXYXYZ");
        buf_delete(&mut b, 0, 4);
        s.replace_range(0..4, "");
        buf_delete(&mut b, 30, 3);
        s.replace_range(30..33, "");

        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- offset 2 position -----------------------------------------------------

mod offset_2_position {
    use super::*;

    // upstream: offset 2 position > random tests bug 1
    #[test]
    fn random_tests_bug_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "huuyYzUfKOENwGgZLqn ");
        s.insert_str(0, "huuyYzUfKOENwGgZLqn ");
        buf_delete(&mut b, 18, 2);
        s.replace_range(18..20, "");
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_delete(&mut b, 12, 4);
        s.replace_range(12..16, "");
        buf_insert(&mut b, 3, "hMbnVEdTSdhLlPevXKF ");
        s.insert_str(3, "hMbnVEdTSdhLlPevXKF ");
        buf_delete(&mut b, 22, 8);
        s.replace_range(22..30, "");
        buf_insert(&mut b, 4, "S umSnYrqOmOAV\nEbZJ ");
        s.insert_str(4, "S umSnYrqOmOAV\nEbZJ ");

        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- get text in range -----------------------------------------------------

mod get_text_in_range {
    use super::*;

    // upstream: get text in range > getContentInRange
    #[test]
    fn get_content_in_range() {
        let mut b = create_text_buffer(&["a\nb\nc\nde"], true);
        buf_insert(&mut b, 8, "fh\ni\njk");
        buf_delete(&mut b, 7, 2);
        // 'a\nb\nc\ndh\ni\njk'

        assert_eq!(b.value_in_range(rng(1, 1, 1, 3)), "a\n");
        assert_eq!(b.value_in_range(rng(2, 1, 2, 3)), "b\n");
        assert_eq!(b.value_in_range(rng(3, 1, 3, 3)), "c\n");
        assert_eq!(b.value_in_range(rng(4, 1, 4, 4)), "dh\n");
        assert_eq!(b.value_in_range(rng(5, 1, 5, 3)), "i\n");
        assert_eq!(b.value_in_range(rng(6, 1, 6, 3)), "jk");
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > random test value in range
    #[test]
    fn random_test_value_in_range() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);

        buf_insert(&mut b, 0, "ZXXY");
        s.insert_str(0, "ZXXY");
        buf_insert(&mut b, 1, "XZZY");
        s.insert_str(1, "XZZY");
        buf_insert(&mut b, 5, "\nX\n\n");
        s.insert_str(5, "\nX\n\n");
        buf_insert(&mut b, 3, "\nXX\n");
        s.insert_str(3, "\nXX\n");
        buf_insert(&mut b, 12, "YYYX");
        s.insert_str(12, "YYYX");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > random test value in range exception
    #[test]
    fn random_test_value_in_range_exception() {
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "XZ\nZ");
        buf_delete(&mut b, 0, 3);
        buf_delete(&mut b, 0, 1);
        buf_insert(&mut b, 0, "ZYX\n");
        buf_delete(&mut b, 0, 4);

        assert_eq!(b.value_in_range(rng(1, 1, 1, 1)), "");
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > random tests bug 1
    #[test]
    fn random_tests_bug_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "huuyYzUfKOENwGgZLqn ");
        s.insert_str(0, "huuyYzUfKOENwGgZLqn ");
        buf_delete(&mut b, 18, 2);
        s.replace_range(18..20, "");
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_delete(&mut b, 12, 4);
        s.replace_range(12..16, "");
        buf_insert(&mut b, 3, "hMbnVEdTSdhLlPevXKF ");
        s.insert_str(3, "hMbnVEdTSdhLlPevXKF ");
        buf_delete(&mut b, 22, 8);
        s.replace_range(22..30, "");
        buf_insert(&mut b, 4, "S umSnYrqOmOAV\nEbZJ ");
        s.insert_str(4, "S umSnYrqOmOAV\nEbZJ ");
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > random tests bug 2
    #[test]
    fn random_tests_bug_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "xfouRDZwdAHjVXJAMV\n ");
        s.insert_str(0, "xfouRDZwdAHjVXJAMV\n ");
        buf_insert(&mut b, 16, "dBGndxpFZBEAIKykYYx ");
        s.insert_str(16, "dBGndxpFZBEAIKykYYx ");
        buf_delete(&mut b, 7, 6);
        s.replace_range(7..13, "");
        buf_delete(&mut b, 9, 7);
        s.replace_range(9..16, "");
        buf_delete(&mut b, 17, 6);
        s.replace_range(17..23, "");
        buf_delete(&mut b, 0, 4);
        s.replace_range(0..4, "");
        buf_insert(&mut b, 9, "qvEFXCNvVkWgvykahYt ");
        s.insert_str(9, "qvEFXCNvVkWgvykahYt ");
        buf_delete(&mut b, 4, 6);
        s.replace_range(4..10, "");
        buf_insert(&mut b, 11, "OcSChUYT\nzPEBOpsGmR ");
        s.insert_str(11, "OcSChUYT\nzPEBOpsGmR ");
        buf_insert(&mut b, 15, "KJCozaXTvkE\nxnqAeTz ");
        s.insert_str(15, "KJCozaXTvkE\nxnqAeTz ");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > get line content
    #[test]
    fn get_line_content() {
        let mut b = create_text_buffer(&["1"], true);
        assert_eq!(line_raw(&b, 1), "1");
        buf_insert(&mut b, 1, "2");
        assert_eq!(line_raw(&b, 1), "12");
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > get line content basic
    #[test]
    fn get_line_content_basic() {
        let b = create_text_buffer(&["1\n2\n3\n4"], true);
        assert_eq!(line_raw(&b, 1), "1\n");
        assert_eq!(line_raw(&b, 2), "2\n");
        assert_eq!(line_raw(&b, 3), "3\n");
        assert_eq!(line_raw(&b, 4), "4");
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > get line content after inserts/deletes
    #[test]
    fn get_line_content_after_inserts_deletes() {
        let mut b = create_text_buffer(&["a\nb\nc\nde"], true);
        buf_insert(&mut b, 8, "fh\ni\njk");
        buf_delete(&mut b, 7, 2);
        // 'a\nb\nc\ndh\ni\njk'

        assert_eq!(line_raw(&b, 1), "a\n");
        assert_eq!(line_raw(&b, 2), "b\n");
        assert_eq!(line_raw(&b, 3), "c\n");
        assert_eq!(line_raw(&b, 4), "dh\n");
        assert_eq!(line_raw(&b, 5), "i\n");
        assert_eq!(line_raw(&b, 6), "jk");
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > random 1
    #[test]
    fn random_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "J eNnDzQpnlWyjmUu\ny ");
        s.insert_str(0, "J eNnDzQpnlWyjmUu\ny ");
        buf_insert(&mut b, 0, "QPEeRAQmRwlJqtZSWhQ ");
        s.insert_str(0, "QPEeRAQmRwlJqtZSWhQ ");
        buf_delete(&mut b, 5, 1);
        s.replace_range(5..6, "");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: get text in range > random 2
    #[test]
    fn random_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], true);
        buf_insert(&mut b, 0, "DZoQ tglPCRHMltejRI ");
        s.insert_str(0, "DZoQ tglPCRHMltejRI ");
        buf_insert(&mut b, 10, "JRXiyYqJ qqdcmbfkKX ");
        s.insert_str(10, "JRXiyYqJ qqdcmbfkKX ");
        buf_delete(&mut b, 16, 3);
        s.replace_range(16..19, "");
        buf_delete(&mut b, 25, 1);
        s.replace_range(25..26, "");
        buf_insert(&mut b, 18, "vH\nNlvfqQJPm\nSFkhMc ");
        s.insert_str(18, "vH\nNlvfqQJPm\nSFkhMc ");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- CRLF ------------------------------------------------------------------
//
// `delete CR in CRLF 1` and `delete CR in CRLF 2` are not ported yet. They delete up to
// an offset between `\r` and `\n`, which `delete_at_offset` can express.

mod crlf {
    use super::*;

    // upstream: CRLF > random bug 1
    #[test]
    fn random_bug_1() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\n\r\r");
        s.insert_str(0, "\n\n\r\r");
        buf_insert(&mut b, 1, "\r\n\r\n");
        s.insert_str(1, "\r\n\r\n");
        buf_delete(&mut b, 5, 3);
        s.replace_range(5..8, "");
        buf_delete(&mut b, 2, 3);
        s.replace_range(2..5, "");

        assert_eq!(b.line_count(), split_lines(&s).len());
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 2
    #[test]
    fn random_bug_2() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\r\n\r");
        s.insert_str(0, "\n\r\n\r");
        buf_insert(&mut b, 2, "\n\r\r\r");
        s.insert_str(2, "\n\r\r\r");
        buf_delete(&mut b, 4, 1);
        s.replace_range(4..5, "");

        assert_eq!(b.line_count(), split_lines(&s).len());
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 3
    #[test]
    fn random_bug_3() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\n\n\r");
        s.insert_str(0, "\n\n\n\r");
        buf_delete(&mut b, 2, 2);
        s.replace_range(2..4, "");
        buf_delete(&mut b, 0, 2);
        s.replace_range(0..2, "");
        buf_insert(&mut b, 0, "\r\r\r\r");
        s.insert_str(0, "\r\r\r\r");
        buf_insert(&mut b, 2, "\r\n\r\r");
        s.insert_str(2, "\r\n\r\r");
        buf_insert(&mut b, 3, "\r\r\r\n");
        s.insert_str(3, "\r\r\r\n");

        assert_eq!(b.line_count(), split_lines(&s).len());
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 4
    #[test]
    fn random_bug_4() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\n\n\n");
        s.insert_str(0, "\n\n\n\n");
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_insert(&mut b, 1, "\r\r\r\r");
        s.insert_str(1, "\r\r\r\r");
        buf_insert(&mut b, 6, "\r\n\n\r");
        s.insert_str(6, "\r\n\n\r");
        buf_delete(&mut b, 5, 3);
        s.replace_range(5..8, "");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 5
    #[test]
    fn random_bug_5() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\n\n\n");
        s.insert_str(0, "\n\n\n\n");
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_insert(&mut b, 0, "\n\r\r\n");
        s.insert_str(0, "\n\r\r\n");
        buf_insert(&mut b, 4, "\n\r\r\n");
        s.insert_str(4, "\n\r\r\n");
        buf_delete(&mut b, 4, 3);
        s.replace_range(4..7, "");
        buf_insert(&mut b, 5, "\r\r\n\r");
        s.insert_str(5, "\r\r\n\r");
        buf_insert(&mut b, 12, "\n\n\n\r");
        s.insert_str(12, "\n\n\n\r");
        buf_insert(&mut b, 5, "\r\r\r\n");
        s.insert_str(5, "\r\r\r\n");
        buf_insert(&mut b, 20, "\n\n\r\n");
        s.insert_str(20, "\n\n\r\n");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 6
    #[test]
    fn random_bug_6() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\r\r\n");
        s.insert_str(0, "\n\r\r\n");
        buf_insert(&mut b, 4, "\r\n\n\r");
        s.insert_str(4, "\r\n\n\r");
        buf_insert(&mut b, 3, "\r\n\n\n");
        s.insert_str(3, "\r\n\n\n");
        buf_delete(&mut b, 4, 8);
        s.replace_range(4..12, "");
        buf_insert(&mut b, 4, "\r\n\n\r");
        s.insert_str(4, "\r\n\n\r");
        buf_insert(&mut b, 0, "\r\n\n\r");
        s.insert_str(0, "\r\n\n\r");
        buf_delete(&mut b, 4, 0);
        buf_delete(&mut b, 8, 4);
        s.replace_range(8..12, "");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 7
    #[test]
    fn random_bug_7() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\r\r\n\n");
        s.insert_str(0, "\r\r\n\n");
        buf_insert(&mut b, 4, "\r\n\n\r");
        s.insert_str(4, "\r\n\n\r");
        buf_insert(&mut b, 7, "\n\r\r\r");
        s.insert_str(7, "\n\r\r\r");
        buf_insert(&mut b, 11, "\n\n\r\n");
        s.insert_str(11, "\n\n\r\n");
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 8
    #[test]
    fn random_bug_8() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\r\n\n\r");
        s.insert_str(0, "\r\n\n\r");
        buf_delete(&mut b, 1, 0);
        buf_insert(&mut b, 3, "\n\n\n\r");
        s.insert_str(3, "\n\n\n\r");
        buf_insert(&mut b, 7, "\n\n\r\n");
        s.insert_str(7, "\n\n\r\n");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 10
    #[test]
    fn random_bug_10() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "qneW");
        s.insert_str(0, "qneW");
        buf_insert(&mut b, 0, "YhIl");
        s.insert_str(0, "YhIl");
        buf_insert(&mut b, 0, "qdsm");
        s.insert_str(0, "qdsm");
        buf_delete(&mut b, 7, 0);
        buf_insert(&mut b, 12, "iiPv");
        s.insert_str(12, "iiPv");
        buf_insert(&mut b, 9, "V\rSA");
        s.insert_str(9, "V\rSA");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: CRLF > random bug 9
    #[test]
    fn random_bug_9() {
        let mut s = String::new();
        let mut b = create_text_buffer(&[""], false);
        buf_insert(&mut b, 0, "\n\n\n\n");
        s.insert_str(0, "\n\n\n\n");
        buf_insert(&mut b, 3, "\n\r\n\r");
        s.insert_str(3, "\n\r\n\r");
        buf_insert(&mut b, 2, "\n\r\n\n");
        s.insert_str(2, "\n\r\n\n");
        buf_insert(&mut b, 0, "\n\n\r\r");
        s.insert_str(0, "\n\n\r\r");
        buf_insert(&mut b, 3, "\r\r\r\r");
        s.insert_str(3, "\r\r\r\r");
        buf_insert(&mut b, 3, "\n\n\r\r");
        s.insert_str(3, "\n\n\r\r");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- centralized lineStarts with CRLF --------------------------------------
//
// `delete CR in CRLF 1` and `delete CR in CRLF 2` are not ported yet (see the CRLF module).

mod centralized_line_starts_with_crlf {
    use super::*;

    // upstream: centralized lineStarts with CRLF > random bug 1
    #[test]
    fn random_bug_1() {
        let mut s = String::from("\n\n\r\r");
        let mut b = create_text_buffer(&["\n\n\r\r"], false);
        buf_insert(&mut b, 1, "\r\n\r\n");
        s.insert_str(1, "\r\n\r\n");
        buf_delete(&mut b, 5, 3);
        s.replace_range(5..8, "");
        buf_delete(&mut b, 2, 3);
        s.replace_range(2..5, "");

        assert_eq!(b.line_count(), split_lines(&s).len());
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 2
    #[test]
    fn random_bug_2() {
        let mut s = String::from("\n\r\n\r");
        let mut b = create_text_buffer(&["\n\r\n\r"], false);
        buf_insert(&mut b, 2, "\n\r\r\r");
        s.insert_str(2, "\n\r\r\r");
        buf_delete(&mut b, 4, 1);
        s.replace_range(4..5, "");

        assert_eq!(b.line_count(), split_lines(&s).len());
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 3
    #[test]
    fn random_bug_3() {
        let mut s = String::from("\n\n\n\r");
        let mut b = create_text_buffer(&["\n\n\n\r"], false);
        buf_delete(&mut b, 2, 2);
        s.replace_range(2..4, "");
        buf_delete(&mut b, 0, 2);
        s.replace_range(0..2, "");
        buf_insert(&mut b, 0, "\r\r\r\r");
        s.insert_str(0, "\r\r\r\r");
        buf_insert(&mut b, 2, "\r\n\r\r");
        s.insert_str(2, "\r\n\r\r");
        buf_insert(&mut b, 3, "\r\r\r\n");
        s.insert_str(3, "\r\r\r\n");

        assert_eq!(b.line_count(), split_lines(&s).len());
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 4
    #[test]
    fn random_bug_4() {
        let mut s = String::from("\n\n\n\n");
        let mut b = create_text_buffer(&["\n\n\n\n"], false);
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_insert(&mut b, 1, "\r\r\r\r");
        s.insert_str(1, "\r\r\r\r");
        buf_insert(&mut b, 6, "\r\n\n\r");
        s.insert_str(6, "\r\n\n\r");
        buf_delete(&mut b, 5, 3);
        s.replace_range(5..8, "");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 5
    #[test]
    fn random_bug_5() {
        let mut s = String::from("\n\n\n\n");
        let mut b = create_text_buffer(&["\n\n\n\n"], false);
        buf_delete(&mut b, 3, 1);
        s.replace_range(3..4, "");
        buf_insert(&mut b, 0, "\n\r\r\n");
        s.insert_str(0, "\n\r\r\n");
        buf_insert(&mut b, 4, "\n\r\r\n");
        s.insert_str(4, "\n\r\r\n");
        buf_delete(&mut b, 4, 3);
        s.replace_range(4..7, "");
        buf_insert(&mut b, 5, "\r\r\n\r");
        s.insert_str(5, "\r\r\n\r");
        buf_insert(&mut b, 12, "\n\n\n\r");
        s.insert_str(12, "\n\n\n\r");
        buf_insert(&mut b, 5, "\r\r\r\n");
        s.insert_str(5, "\r\r\r\n");
        buf_insert(&mut b, 20, "\n\n\r\n");
        s.insert_str(20, "\n\n\r\n");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 6
    #[test]
    fn random_bug_6() {
        let mut s = String::from("\n\r\r\n");
        let mut b = create_text_buffer(&["\n\r\r\n"], false);
        buf_insert(&mut b, 4, "\r\n\n\r");
        s.insert_str(4, "\r\n\n\r");
        buf_insert(&mut b, 3, "\r\n\n\n");
        s.insert_str(3, "\r\n\n\n");
        buf_delete(&mut b, 4, 8);
        s.replace_range(4..12, "");
        buf_insert(&mut b, 4, "\r\n\n\r");
        s.insert_str(4, "\r\n\n\r");
        buf_insert(&mut b, 0, "\r\n\n\r");
        s.insert_str(0, "\r\n\n\r");
        buf_delete(&mut b, 4, 0);
        buf_delete(&mut b, 8, 4);
        s.replace_range(8..12, "");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 7
    #[test]
    fn random_bug_7() {
        let mut s = String::from("\r\n\n\r");
        let mut b = create_text_buffer(&["\r\n\n\r"], false);
        buf_delete(&mut b, 1, 0);
        buf_insert(&mut b, 3, "\n\n\n\r");
        s.insert_str(3, "\n\n\n\r");
        buf_insert(&mut b, 7, "\n\n\r\n");
        s.insert_str(7, "\n\n\r\n");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 8
    #[test]
    fn random_bug_8() {
        let mut s = String::from("\r\r\n\n");
        let mut b = create_text_buffer(&["\r\r\n\n"], false);
        buf_insert(&mut b, 4, "\r\n\n\r");
        s.insert_str(4, "\r\n\n\r");
        buf_insert(&mut b, 7, "\n\r\r\r");
        s.insert_str(7, "\n\r\r\r");
        buf_insert(&mut b, 11, "\n\n\r\n");
        s.insert_str(11, "\n\n\r\n");
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 9
    #[test]
    fn random_bug_9() {
        let mut s = String::from("qneW");
        let mut b = create_text_buffer(&["qneW"], false);
        buf_insert(&mut b, 0, "YhIl");
        s.insert_str(0, "YhIl");
        buf_insert(&mut b, 0, "qdsm");
        s.insert_str(0, "qdsm");
        buf_delete(&mut b, 7, 0);
        buf_insert(&mut b, 12, "iiPv");
        s.insert_str(12, "iiPv");
        buf_insert(&mut b, 9, "V\rSA");
        s.insert_str(9, "V\rSA");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random bug 10
    #[test]
    fn random_bug_10() {
        let mut s = String::from("\n\n\n\n");
        let mut b = create_text_buffer(&["\n\n\n\n"], false);
        buf_insert(&mut b, 3, "\n\r\n\r");
        s.insert_str(3, "\n\r\n\r");
        buf_insert(&mut b, 2, "\n\r\n\n");
        s.insert_str(2, "\n\r\n\n");
        buf_insert(&mut b, 0, "\n\n\r\r");
        s.insert_str(0, "\n\n\r\r");
        buf_insert(&mut b, 3, "\r\r\r\r");
        s.insert_str(3, "\r\r\r\r");
        buf_insert(&mut b, 3, "\n\n\r\r");
        s.insert_str(3, "\n\n\r\r");

        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random chunk bug 1
    #[test]
    fn random_chunk_bug_1() {
        let mut b = create_text_buffer(&["\n\r\r\n\n\n\r\n\r"], false);
        let mut s = String::from("\n\r\r\n\n\n\r\n\r");
        buf_delete(&mut b, 0, 2);
        s.replace_range(0..2, "");
        buf_insert(&mut b, 1, "\r\r\n\n");
        s.insert_str(1, "\r\r\n\n");
        buf_insert(&mut b, 7, "\r\r\r\r");
        s.insert_str(7, "\r\r\r\r");

        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random chunk bug 2
    #[test]
    fn random_chunk_bug_2() {
        let mut b = create_text_buffer(&["\n\r\n\n\n\r\n\r\n\r\r\n\n\n\r\r\n\r\n"], false);
        let mut s = String::from("\n\r\n\n\n\r\n\r\n\r\r\n\n\n\r\r\n\r\n");
        buf_insert(&mut b, 16, "\r\n\r\r");
        s.insert_str(16, "\r\n\r\r");
        buf_insert(&mut b, 13, "\n\n\r\r");
        s.insert_str(13, "\n\n\r\r");
        buf_insert(&mut b, 19, "\n\n\r\n");
        s.insert_str(19, "\n\n\r\n");
        buf_delete(&mut b, 5, 0);
        buf_delete(&mut b, 11, 2);
        s.replace_range(11..13, "");

        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random chunk bug 3
    #[test]
    fn random_chunk_bug_3() {
        let mut b = create_text_buffer(&["\r\n\n\n\n\n\n\r\n"], false);
        let mut s = String::from("\r\n\n\n\n\n\n\r\n");
        buf_insert(&mut b, 4, "\n\n\r\n\r\r\n\n\r");
        s.insert_str(4, "\n\n\r\n\r\r\n\n\r");
        buf_delete(&mut b, 4, 4);
        s.replace_range(4..8, "");
        buf_insert(&mut b, 11, "\r\n\r\n\n\r\r\n\n");
        s.insert_str(11, "\r\n\r\n\n\r\r\n\n");
        buf_delete(&mut b, 1, 2);
        s.replace_range(1..3, "");

        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: centralized lineStarts with CRLF > random chunk bug 4
    #[test]
    fn random_chunk_bug_4() {
        let mut b = create_text_buffer(&["\n\r\n\r"], false);
        let mut s = String::from("\n\r\n\r");
        buf_insert(&mut b, 4, "\n\n\r\n");
        s.insert_str(4, "\n\n\r\n");
        buf_insert(&mut b, 3, "\r\n\n\n");
        s.insert_str(3, "\r\n\n\n");

        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- random is unsupervised ------------------------------------------------
//
// Only the fixed-sequence test is ported. `random insert delete`, `random chunks` and
// `random chunks 2` use Math.random and are skipped.

mod random_is_unsupervised {
    use super::*;

    // upstream: random is unsupervised > splitting large change buffer
    #[test]
    fn splitting_large_change_buffer() {
        let mut b = create_text_buffer(&[""], false);
        let mut s = String::new();

        buf_insert(&mut b, 0, "WUZ\nXVZY\n");
        s.insert_str(0, "WUZ\nXVZY\n");
        buf_insert(&mut b, 8, "\r\r\nZXUWVW");
        s.insert_str(8, "\r\r\nZXUWVW");
        buf_delete(&mut b, 10, 7);
        s.replace_range(10..17, "");
        buf_delete(&mut b, 10, 1);
        s.replace_range(10..11, "");
        buf_insert(&mut b, 4, "VX\r\r\nWZVZ");
        s.insert_str(4, "VX\r\r\nWZVZ");
        buf_delete(&mut b, 11, 3);
        s.replace_range(11..14, "");
        buf_delete(&mut b, 12, 4);
        s.replace_range(12..16, "");
        buf_delete(&mut b, 8, 0);
        buf_delete(&mut b, 10, 2);
        s.replace_range(10..12, "");
        buf_insert(&mut b, 0, "VZXXZYZX\r");
        s.insert_str(0, "VZXXZYZX\r");

        assert_eq!(b.value(), s);
        test_line_starts(&s, &b);
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- buffer api ------------------------------------------------------------

mod buffer_api {
    use super::*;

    // upstream: buffer api > getLineCharCode - issue #45735
    #[test]
    fn get_line_char_code_issue_45735() {
        let b = create_text_buffer(&["LINE1\nline2"], true);
        assert_eq!(line_char_code(&b, 1, 0), u16::from(b'L'));
        assert_eq!(line_char_code(&b, 1, 1), u16::from(b'I'));
        assert_eq!(line_char_code(&b, 1, 2), u16::from(b'N'));
        assert_eq!(line_char_code(&b, 1, 3), u16::from(b'E'));
        assert_eq!(line_char_code(&b, 1, 4), u16::from(b'1'));
        assert_eq!(line_char_code(&b, 1, 5), u16::from(b'\n'));
        assert_eq!(line_char_code(&b, 2, 0), u16::from(b'l'));
        assert_eq!(line_char_code(&b, 2, 1), u16::from(b'i'));
        assert_eq!(line_char_code(&b, 2, 2), u16::from(b'n'));
        assert_eq!(line_char_code(&b, 2, 3), u16::from(b'e'));
        assert_eq!(line_char_code(&b, 2, 4), u16::from(b'2'));
    }

    // upstream: buffer api > getLineCharCode - issue #47733
    #[test]
    fn get_line_char_code_issue_47733() {
        let b = create_text_buffer(&["", "LINE1\n", "line2"], true);
        assert_eq!(line_char_code(&b, 1, 0), u16::from(b'L'));
        assert_eq!(line_char_code(&b, 1, 1), u16::from(b'I'));
        assert_eq!(line_char_code(&b, 1, 2), u16::from(b'N'));
        assert_eq!(line_char_code(&b, 1, 3), u16::from(b'E'));
        assert_eq!(line_char_code(&b, 1, 4), u16::from(b'1'));
        assert_eq!(line_char_code(&b, 1, 5), u16::from(b'\n'));
        assert_eq!(line_char_code(&b, 2, 0), u16::from(b'l'));
        assert_eq!(line_char_code(&b, 2, 1), u16::from(b'i'));
        assert_eq!(line_char_code(&b, 2, 2), u16::from(b'n'));
        assert_eq!(line_char_code(&b, 2, 3), u16::from(b'e'));
        assert_eq!(line_char_code(&b, 2, 4), u16::from(b'2'));
    }
}

// ---- search offset cache ---------------------------------------------------

mod search_offset_cache {
    use super::*;

    // upstream: search offset cache > render white space exception
    #[test]
    fn render_white_space_exception() {
        let mut b = create_text_buffer(&["class Name{\n\t\n\t\t\tget() {\n\n\t\t\t}\n\t\t}"], true);
        let mut s = String::from("class Name{\n\t\n\t\t\tget() {\n\n\t\t\t}\n\t\t}");
        buf_insert(&mut b, 12, "s");
        s.insert(12, 's');
        buf_insert(&mut b, 13, "e");
        s.insert(13, 'e');
        buf_insert(&mut b, 14, "t");
        s.insert(14, 't');
        buf_insert(&mut b, 15, "()");
        s.insert_str(15, "()");
        buf_delete(&mut b, 16, 1);
        s.replace_range(16..17, "");
        buf_insert(&mut b, 17, "()");
        s.insert_str(17, "()");
        buf_delete(&mut b, 18, 1);
        s.replace_range(18..19, "");
        buf_insert(&mut b, 18, "}");
        s.insert(18, '}');
        buf_insert(&mut b, 12, "\n");
        s.insert(12, '\n');
        buf_delete(&mut b, 12, 1);
        s.replace_range(12..13, "");
        buf_delete(&mut b, 18, 1);
        s.replace_range(18..19, "");
        buf_insert(&mut b, 18, "}");
        s.insert(18, '}');
        buf_delete(&mut b, 17, 2);
        s.replace_range(17..19, "");
        buf_delete(&mut b, 16, 1);
        s.replace_range(16..17, "");
        buf_insert(&mut b, 16, ")");
        s.insert(16, ')');
        buf_delete(&mut b, 15, 2);
        s.replace_range(15..17, "");
        assert_eq!(b.value(), s);
    }

    // upstream: search offset cache > Line breaks replacement is not necessary when EOL is normalized
    #[test]
    fn line_breaks_replacement_is_not_necessary_when_eol_is_normalized() {
        let mut b = create_text_buffer(&["abc"], true);
        let mut s = String::from("abc");
        buf_insert(&mut b, 3, "def\nabc");
        s.push_str("def\nabc");
        test_line_starts(&s, &b);
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: search offset cache > Line breaks replacement is not necessary when EOL is normalized 2
    #[test]
    fn line_breaks_replacement_is_not_necessary_when_eol_is_normalized_2() {
        let mut b = create_text_buffer(&["abc\n"], true);
        let mut s = String::from("abc\n");
        buf_insert(&mut b, 4, "def\nabc");
        s.push_str("def\nabc");
        test_line_starts(&s, &b);
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: search offset cache > Line breaks replacement is not necessary when EOL is normalized 3
    #[test]
    fn line_breaks_replacement_is_not_necessary_when_eol_is_normalized_3() {
        let mut b = create_text_buffer(&["abc\n"], true);
        let mut s = String::from("abc\n");
        buf_insert(&mut b, 2, "def\nabc");
        s.replace_range(2..2, "def\nabc");
        test_line_starts(&s, &b);
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }

    // upstream: search offset cache > Line breaks replacement is not necessary when EOL is normalized 4
    #[test]
    fn line_breaks_replacement_is_not_necessary_when_eol_is_normalized_4() {
        let mut b = create_text_buffer(&["abc\n"], true);
        let mut s = String::from("abc\n");
        buf_insert(&mut b, 3, "def\nabc");
        s.replace_range(3..3, "def\nabc");
        test_line_starts(&s, &b);
        test_lines_content(&s, &b);
        assert_tree_invariants(&b);
    }
}

// ---- Skipped upstream tests ------------------------------------------------
//
// Not ported. Each entry gives the reason; the first applicable reason is the one listed.
//
// Needs features TextBuffer does not have yet:
//   - snapshot (5 tests: `bug #45564, piece tree pieces should be immutable`,
//     `immutable snapshot 1` .. `3`): `createSnapshot` and TextModel `applyEdits`.
//   - chunk based search (3 tests: `#45892. For some cases, the buffer is empty ...`,
//     `#45770. FindInNode should not cross node boundary.`, `search searching from the
//     middle`): `findMatchesLineByLine`.
//   - buffer api `getNearestChunk`: piece-boundary introspection; there is no piece tree.
//   - buffer api `equal`, `equal with more chunks`, `equal 2, empty buffer`,
//     `equal 3, empty buffer`: no `equal` on TextBuffer.
//   - ModelBuilder `BOM handling`, `RTL handling 2`, `RTL handling 3`,
//     `ASCII handling 1`, `ASCII handling 2`: no BOM stripping and no
//     `mightContainRTL` / `mightContainNonBasicASCII` flags.
//   - random is unsupervised > `random insert delete`, `random chunks`,
//     `random chunks 2`: `Math.random` fuzzers, not fixed tests.
//   - CRLF > `delete CR in CRLF 1`, `delete CR in CRLF 2`, and the same two in
//     centralized lineStarts with CRLF: not ported yet, but `delete_at_offset` can
//     express them (they delete up to an offset between `\r` and `\n`).
//
