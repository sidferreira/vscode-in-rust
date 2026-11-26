//! The change log of `TextBuffer::edit` (ADR-0120): each edit is reported as the UTF-16 offset it
//! started at, the units it removed and the units it inserted, in order.

use vsr_text::{DefaultEol, Position, Range, TextBuffer, TextChange};

fn buffer(text: &str) -> TextBuffer {
    TextBuffer::new(text, DefaultEol::Lf, false)
}

fn at(line: usize, column: usize) -> Position {
    Position::new(line, column)
}

#[test]
fn nothing_is_recorded_until_tracking_starts() {
    let mut b = buffer("abc");
    b.edit(Range::new(at(1, 1), at(1, 2)), "x");
    assert!(b.take_changes().is_empty());
}

#[test]
fn a_replacement_reports_its_offset_removed_and_inserted_units() {
    let mut b = buffer("hello world");
    b.track_changes(true);
    b.edit(Range::new(at(1, 7), at(1, 12)), "rust");
    assert_eq!(b.take_changes(), vec![TextChange { offset: 6, removed: 5, inserted: 4 }]);
    assert_eq!(b.value(), "hello rust");
}

#[test]
fn an_insertion_and_a_deletion_are_changes_too() {
    let mut b = buffer("ab");
    b.track_changes(true);
    b.edit(Range::new(at(1, 2), at(1, 2)), "XYZ");
    b.edit(Range::new(at(1, 1), at(1, 3)), "");
    assert_eq!(
        b.take_changes(),
        vec![
            TextChange { offset: 1, removed: 0, inserted: 3 },
            TextChange { offset: 0, removed: 2, inserted: 0 },
        ]
    );
    assert_eq!(b.value(), "YZb");
}

#[test]
fn a_multi_line_edit_counts_the_line_breaks() {
    let mut b = buffer("one\ntwo\nthree");
    b.track_changes(true);
    // Remove "two\n" (offsets 4..8) and insert "a\nb" in its place.
    b.edit(Range::new(at(2, 1), at(3, 1)), "a\nb\n");
    assert_eq!(b.take_changes(), vec![TextChange { offset: 4, removed: 4, inserted: 4 }]);
    assert_eq!(b.value(), "one\na\nb\nthree");
}

#[test]
fn taking_the_changes_empties_the_log_and_tracking_continues() {
    let mut b = buffer("x");
    b.track_changes(true);
    b.edit(Range::new(at(1, 2), at(1, 2)), "y");
    assert_eq!(b.take_changes().len(), 1);
    assert!(b.take_changes().is_empty());
    b.edit(Range::new(at(1, 1), at(1, 1)), "z");
    assert_eq!(b.take_changes(), vec![TextChange { offset: 0, removed: 0, inserted: 1 }]);
}

#[test]
fn stopping_tracking_drops_what_was_recorded() {
    let mut b = buffer("x");
    b.track_changes(true);
    b.edit(Range::new(at(1, 2), at(1, 2)), "y");
    b.track_changes(false);
    assert!(b.take_changes().is_empty());
    b.edit(Range::new(at(1, 1), at(1, 1)), "z");
    assert!(b.take_changes().is_empty());
}

#[test]
fn offsets_count_utf16_units_not_chars() {
    // U+1F600 is two UTF-16 units.
    let mut b = buffer("\u{1F600}a");
    b.track_changes(true);
    b.edit(Range::new(at(1, 3), at(1, 4)), "b");
    assert_eq!(b.take_changes(), vec![TextChange { offset: 2, removed: 1, inserted: 1 }]);
}

#[test]
fn raw_offset_inserts_and_deletes_are_recorded_too() {
    let mut b = buffer("a\u{1F600}bc");
    b.track_changes(true);
    b.insert_at_offset(3, "XY");
    b.delete_at_offset(0, 1);
    // An offset inside the surrogate pair snaps to before it; the change reports the snapped offset.
    b.insert_at_offset(1, "!");
    b.delete_at_offset(5, 0);
    assert_eq!(
        b.take_changes(),
        vec![
            TextChange { offset: 3, removed: 0, inserted: 2 },
            TextChange { offset: 0, removed: 1, inserted: 0 },
            TextChange { offset: 0, removed: 0, inserted: 1 },
        ]
    );
}
