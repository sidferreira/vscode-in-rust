//! The model line <-> view line mapping, ported from upstream's `modelLineProjection.test.ts`.
//! upstream: src/vs/editor/test/browser/viewModel/modelLineProjection.test.ts @ 50e1350fc78d
//!
//! Only `SplitLine` is ported. The other tests of the file need hidden areas (folding), indent
//! guides over view lines, token rendering (`getViewLinesData`) or injected text, which rscode
//! does not have.

use vsr_editor::Position;
use vsr_editor::wrap::{ModelLineProjectionData, PositionAffinity, ViewLines};
use vsr_text::{DefaultEol, TextBuffer};

/// Upstream `createSplitLine(splitLengths, breakingOffsetsVisibleColumn, wrappedTextIndentLength)`.
fn split_line(split_lengths: &[usize], visible_columns: &[f64], indent: usize) -> ViewLines {
    let mut offsets = Vec::new();
    let mut total = 0;
    for l in split_lengths {
        total += l;
        offsets.push(total);
    }
    ViewLines::from_parts(vec![(total, Some(ModelLineProjectionData::new(offsets, visible_columns.to_vec(), indent)))])
}

fn model(text: &str) -> TextBuffer {
    TextBuffer::new(text, DefaultEol::Lf, true)
}

// upstream: Editor ViewModel - SplitLinesCollection > SplitLine
#[test]
fn split_line_maps_columns() {
    let model1 = model("My First LineMy Second LineAnd another one");
    let line1 = split_line(&[13, 14, 15], &[13.0, (13 + 14) as f64, (13 + 14 + 15) as f64], 0);

    assert_eq!(line1.view_line_count(), 3);
    assert_eq!(line1.view_line_content(&model1, 0), "My First Line");
    assert_eq!(line1.view_line_content(&model1, 1), "My Second Line");
    assert_eq!(line1.view_line_content(&model1, 2), "And another one");
    assert_eq!(line1.view_line_length(0) + 1, 14);
    assert_eq!(line1.view_line_length(1) + 1, 15);
    assert_eq!(line1.view_line_length(2) + 1, 16);
    let continues: Vec<bool> = (0..3).map(|i| !line1.view_line(i).is_last).collect();
    assert_eq!(continues, [true, true, false]);
    for col in 1..=14 {
        assert_eq!(line1.model_position_of(0, col).0.column, col, "getInputColumnOfOutputPosition(0, {col})");
    }
    for col in 1..=15 {
        assert_eq!(line1.model_position_of(1, col).0.column, 13 + col, "getInputColumnOfOutputPosition(1, {col})");
    }
    for col in 1..=16 {
        assert_eq!(line1.model_position_of(2, col).0.column, 13 + 14 + col, "getInputColumnOfOutputPosition(2, {col})");
    }
    let at = |col: usize| line1.view_position_of(Position::new(1, col), PositionAffinity::None);
    for col in 1..=13 {
        assert_eq!(at(col), (0, col), "getOutputPositionOfInputPosition({col})");
    }
    for col in 1 + 13..=14 + 13 {
        assert_eq!(at(col), (1, col - 13), "getOutputPositionOfInputPosition({col})");
    }
    for col in 1 + 13 + 14..=15 + 14 + 13 {
        assert_eq!(at(col), (2, col - 13 - 14), "getOutputPositionOfInputPosition({col})");
    }

    let line1 = split_line(&[13, 14, 15], &[13.0, (13 + 14) as f64, (13 + 14 + 15) as f64], 4);

    assert_eq!(line1.view_line_count(), 3);
    assert_eq!(line1.view_line_content(&model1, 0), "My First Line");
    assert_eq!(line1.view_line_content(&model1, 1), "    My Second Line");
    assert_eq!(line1.view_line_content(&model1, 2), "    And another one");
    assert_eq!(line1.view_line_length(0) + 1, 14);
    assert_eq!(line1.view_line_length(1) + 1, 19);
    assert_eq!(line1.view_line_length(2) + 1, 20);

    let mut mapping: Vec<Vec<usize>> = Vec::new();
    for line_index in 0..line1.view_line_count() {
        mapping.push((1..=line1.view_line_length(line_index) + 1).map(|col| line1.model_position_of(line_index, col).0.column).collect());
    }
    assert_eq!(
        mapping,
        vec![
            vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14],
            vec![14, 14, 14, 14, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28],
            vec![28, 28, 28, 28, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43],
        ]
    );

    for col in 1..=13 {
        assert_eq!(at4(&line1, col), (0, col), "6.getOutputPositionOfInputPosition({col})");
    }
    for col in 1 + 13..=14 + 13 {
        assert_eq!(at4(&line1, col), (1, 4 + col - 13), "7.getOutputPositionOfInputPosition({col})");
    }
    for col in 1 + 13 + 14..=15 + 14 + 13 {
        assert_eq!(at4(&line1, col), (2, 4 + col - 13 - 14), "8.getOutputPositionOfInputPosition({col})");
    }
}

fn at4(lines: &ViewLines, col: usize) -> (usize, usize) {
    lines.view_position_of(Position::new(1, col), PositionAffinity::None)
}
