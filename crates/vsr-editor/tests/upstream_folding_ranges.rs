//! Folding regions and the merge of region lists, ported from upstream's `foldingRanges.test.ts`.
//! All 7 tests are ported.
//! upstream: src/vs/editor/contrib/folding/test/browser/foldingRanges.test.ts @ 50e1350fc78d

use vsr_editor::folding::{
    FoldRange, FoldSource, FoldingMarkers, FoldingRegions, MAX_FOLDING_REGIONS, RangesCollector, compute_ranges,
};
use vsr_text::{DefaultEol, TextBuffer};

fn markers() -> FoldingMarkers {
    FoldingMarkers::new(r"^\s*#region\b", r"^\s*#endregion\b").unwrap()
}

fn buffer(lines: &[String]) -> TextBuffer {
    TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true)
}

fn fold_range(from: u32, to: u32, collapsed: bool, source: FoldSource, kind: Option<&str>) -> FoldRange {
    FoldRange { start_line_number: from, end_line_number: to, kind: kind.map(str::to_string), is_collapsed: collapsed, source }
}

// upstream: FoldingRanges > test max folding regions
#[test]
fn test_max_folding_regions() {
    let mut lines: Vec<String> = Vec::new();
    let n_regions = MAX_FOLDING_REGIONS;
    let mut collector = RangesCollector::new(MAX_FOLDING_REGIONS);
    for _ in 0..n_regions {
        let start = lines.len() as u32;
        lines.push("#region".into());
        let end = lines.len() as u32;
        lines.push("#endregion".into());
        collector.insert_first(start, end, 0);
    }
    let model = buffer(&lines);
    let actual = collector.to_indent_ranges(&model, 4);
    assert_eq!(actual.len(), n_regions, "len");
}

// upstream: FoldingRanges > findRange
#[test]
fn find_range() {
    let lines: Vec<String> = [
        "#region",
        "#endregion",
        "class A {",
        "  void foo() {",
        "    if (true) {",
        "        return;",
        "    }",
        "",
        "    if (true) {",
        "      return;",
        "    }",
        "  }",
        "}",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let actual = compute_ranges(&buffer(&lines), 4, false, Some(&markers()), 5000);
    // r0 = (1, 2), r1 = (3, 12), r2 = (4, 11), r3 = (5, 6), r4 = (9, 10)
    let expected = [Some(0), Some(0), Some(1), Some(2), Some(3), Some(3), Some(2), Some(2), Some(4), Some(4), Some(2), Some(1), None];
    for (i, want) in expected.iter().enumerate() {
        assert_eq!(actual.find_range(i as u32 + 1), *want, "{}", i + 1);
    }
}

// upstream: FoldingRanges > setCollapsed
#[test]
fn set_collapsed() {
    let n_regions = 500;
    let mut lines: Vec<String> = Vec::new();
    lines.extend((0..n_regions).map(|_| "#region".to_string()));
    lines.extend((0..n_regions).map(|_| "#endregion".to_string()));
    let mut actual = compute_ranges(&buffer(&lines), 4, false, Some(&markers()), 5000);
    assert_eq!(actual.len(), n_regions, "len");
    for i in 0..n_regions {
        actual.set_collapsed(i, i % 3 == 0);
    }
    for i in 0..n_regions {
        assert_eq!(actual.is_collapsed(i), i % 3 == 0, "line{i}");
    }
}

fn assert_equal_ranges(a: &FoldRange, b: &FoldRange, msg: &str) {
    assert_eq!(a.start_line_number, b.start_line_number, "{msg} start");
    assert_eq!(a.end_line_number, b.end_line_number, "{msg} end");
    assert_eq!(a.kind, b.kind, "{msg} type");
    assert_eq!(a.is_collapsed, b.is_collapsed, "{msg} collapsed");
    assert_eq!(a.source, b.source, "{msg} source");
}

use FoldSource::{Provider, Recovered, UserDefined};

// upstream: FoldingRanges > sanitizeAndMerge1
#[test]
fn sanitize_and_merge_1() {
    let set1 = [
        fold_range(0, 100, false, Provider, None),         // invalid, should be removed
        fold_range(1, 100, false, Provider, Some("A")),    // valid
        fold_range(1, 100, false, Provider, Some("Z")),    // invalid, duplicate start
        fold_range(10, 10, false, Provider, None),         // invalid, should be removed
        fold_range(20, 80, false, Provider, Some("C1")),   // valid inside 'B'
        fold_range(22, 80, true, Provider, Some("D1")),    // valid inside 'C1'
        fold_range(90, 101, false, Provider, None),        // invalid, should be removed
    ];
    let set2 = [
        fold_range(20, 80, true, Provider, None),          // should merge with C1
        fold_range(18, 80, true, Provider, None),          // invalid, out of order
        fold_range(21, 81, true, Provider, Some("Z")),     // invalid, overlapping
        fold_range(22, 80, true, Provider, Some("D2")),    // should merge with D1
    ];
    let result = FoldingRegions::sanitize_and_merge(&set1, &set2, Some(100), None);
    assert_eq!(result.len(), 3, "result length1");
    assert_equal_ranges(&result[0], &fold_range(1, 100, false, Provider, Some("A")), "A1");
    assert_equal_ranges(&result[1], &fold_range(20, 80, true, Provider, Some("C1")), "C1");
    assert_equal_ranges(&result[2], &fold_range(22, 80, true, Provider, Some("D1")), "D1");
}

// upstream: FoldingRanges > sanitizeAndMerge2
#[test]
fn sanitize_and_merge_2() {
    let set1 = [
        fold_range(1, 100, false, Provider, Some("a1")), // valid
        fold_range(2, 100, false, Provider, Some("a2")), // valid
        fold_range(3, 19, false, Provider, Some("a3")),  // valid
        fold_range(20, 71, false, Provider, Some("a4")), // overlaps b3
        fold_range(21, 29, false, Provider, Some("a5")), // valid
        fold_range(81, 91, false, Provider, Some("a6")), // overlaps b4
    ];
    let set2 = [
        fold_range(30, 39, true, Provider, Some("b1")),    // valid, will be recovered
        fold_range(40, 49, true, UserDefined, Some("b2")), // valid
        fold_range(50, 100, true, UserDefined, Some("b3")), // overlaps a4
        fold_range(80, 90, true, UserDefined, Some("b4")), // overlaps a6
        fold_range(92, 100, true, UserDefined, Some("b5")), // valid
    ];
    let result = FoldingRegions::sanitize_and_merge(&set1, &set2, Some(100), None);
    assert_eq!(result.len(), 9, "result length1");
    assert_equal_ranges(&result[0], &fold_range(1, 100, false, Provider, Some("a1")), "P1");
    assert_equal_ranges(&result[1], &fold_range(2, 100, false, Provider, Some("a2")), "P2");
    assert_equal_ranges(&result[2], &fold_range(3, 19, false, Provider, Some("a3")), "P3");
    assert_equal_ranges(&result[3], &fold_range(21, 29, false, Provider, Some("a5")), "P4");
    assert_equal_ranges(&result[4], &fold_range(30, 39, true, Recovered, Some("b1")), "P5");
    assert_equal_ranges(&result[5], &fold_range(40, 49, true, UserDefined, Some("b2")), "P6");
    assert_equal_ranges(&result[6], &fold_range(50, 100, true, UserDefined, Some("b3")), "P7");
    assert_equal_ranges(&result[7], &fold_range(80, 90, true, UserDefined, Some("b4")), "P8");
    assert_equal_ranges(&result[8], &fold_range(92, 100, true, UserDefined, Some("b5")), "P9");
}

// upstream: FoldingRanges > sanitizeAndMerge3
#[test]
fn sanitize_and_merge_3() {
    let set1 = [
        fold_range(1, 100, false, Provider, Some("a1")), // valid
        fold_range(10, 29, false, Provider, Some("a2")), // matches manual hidden
        fold_range(35, 39, true, Recovered, Some("a3")), // valid
    ];
    let set2 = [
        fold_range(10, 29, true, Recovered, Some("b1")), // matches a
        fold_range(20, 28, true, Provider, Some("b2")),  // should remain
        fold_range(30, 39, true, Recovered, Some("b3")), // should remain
    ];
    let result = FoldingRegions::sanitize_and_merge(&set1, &set2, Some(100), None);
    assert_eq!(result.len(), 5, "result length3");
    assert_equal_ranges(&result[0], &fold_range(1, 100, false, Provider, Some("a1")), "R1");
    assert_equal_ranges(&result[1], &fold_range(10, 29, true, Provider, Some("a2")), "R2");
    assert_equal_ranges(&result[2], &fold_range(20, 28, true, Recovered, Some("b2")), "R3");
    assert_equal_ranges(&result[3], &fold_range(30, 39, true, Recovered, Some("b3")), "R3");
    assert_equal_ranges(&result[4], &fold_range(35, 39, true, Recovered, Some("a3")), "R4");
}

// upstream: FoldingRanges > sanitizeAndMerge4
#[test]
fn sanitize_and_merge_4() {
    let set1 = [fold_range(1, 100, false, Provider, Some("a1"))];
    let set2 = [
        fold_range(20, 28, true, Provider, Some("b1")), // hidden
        fold_range(30, 38, true, Provider, Some("b2")), // hidden
    ];
    let result = FoldingRegions::sanitize_and_merge(&set1, &set2, Some(100), None);
    assert_eq!(result.len(), 3, "result length4");
    assert_equal_ranges(&result[0], &fold_range(1, 100, false, Provider, Some("a1")), "R1");
    assert_equal_ranges(&result[1], &fold_range(20, 28, true, Recovered, Some("b1")), "R2");
    assert_equal_ranges(&result[2], &fold_range(30, 38, true, Recovered, Some("b2")), "R3");
}
