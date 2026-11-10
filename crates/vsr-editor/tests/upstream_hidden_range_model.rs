//! The hidden line ranges of collapsed regions, ported from upstream's `hiddenRangeModel.test.ts`.
//! upstream: src/vs/editor/contrib/folding/test/browser/hiddenRangeModel.test.ts @ 50e1350fc78d

use vsr_editor::folding::{FoldingModel, HiddenRangeModel, compute_ranges};
use vsr_text::{DefaultEol, TextBuffer};

fn hidden(h: &HiddenRangeModel) -> Vec<(usize, usize)> {
    h.hidden_ranges().iter().map(|r| (r.start_line_number, r.end_line_number)).collect()
}

fn toggle(model: &mut FoldingModel, hidden_model: &mut HiddenRangeModel, lines: &[u32]) {
    let regions: Vec<_> = lines.iter().map(|&l| model.get_region_at_line(l).unwrap()).collect();
    model.toggle_collapse_state(&regions);
    hidden_model.update(model.regions(), false);
}

// upstream: Hidden Range Model > hasRanges
#[test]
fn has_ranges() {
    let lines = ["/**", " * Comment", " */", "class A {", "  void foo() {", "    if (true) {", "      //hello", "    }", "  }", "}"];
    let text_model = TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true);
    let mut folding_model = FoldingModel::new();
    let mut hidden_range_model = HiddenRangeModel::new();
    assert!(!hidden_range_model.has_ranges());

    let ranges = compute_ranges(&text_model, 4, false, None, 5000);
    folding_model.update(&ranges, None, text_model.line_count(), None);
    hidden_range_model.update(folding_model.regions(), false);

    toggle(&mut folding_model, &mut hidden_range_model, &[1, 6]);
    assert_eq!(hidden(&hidden_range_model), [(2, 3), (7, 7)]);

    assert!(hidden_range_model.has_ranges());
    let expect = |h: &HiddenRangeModel, hidden_lines: &[usize]| {
        for line in 1..=10 {
            assert_eq!(h.is_hidden(line), hidden_lines.contains(&line), "line {line}");
        }
    };
    expect(&hidden_range_model, &[2, 3, 7]);

    toggle(&mut folding_model, &mut hidden_range_model, &[4]);
    assert_eq!(hidden(&hidden_range_model), [(2, 3), (5, 9)]);
    assert!(hidden_range_model.has_ranges());
    expect(&hidden_range_model, &[2, 3, 5, 6, 7, 8, 9]);

    toggle(&mut folding_model, &mut hidden_range_model, &[1, 6, 4]);
    assert_eq!(hidden(&hidden_range_model), []);
    assert!(!hidden_range_model.has_ranges());
    expect(&hidden_range_model, &[]);
}
