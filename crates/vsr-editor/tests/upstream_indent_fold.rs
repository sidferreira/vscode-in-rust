//! The cap on the number of indentation regions, ported from upstream's `indentFold.test.ts`.
//! upstream: src/vs/editor/contrib/folding/test/browser/indentFold.test.ts @ 50e1350fc78d

use vsr_editor::folding::compute_ranges_with_report;
use vsr_text::{DefaultEol, TextBuffer};

// upstream: Indentation Folding > Limit by indent
#[test]
fn limit_by_indent() {
    let lines = [
        "A",
        "  A",
        "  A",
        "    A",
        "      A",
        "    A",
        "      A",
        "      A",
        "         A",
        "      A",
        "         A",
        "  A",
        "              A",
        "                 A",
        "A",
        "  A",
    ];
    let (r1, r2, r3, r4, r5, r6, r7, r8, r9) = ((1, 14), (3, 11), (4, 5), (6, 11), (8, 9), (10, 11), (12, 14), (13, 14), (15, 16));
    let model = TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true);

    let assert_limit = |max: usize, expected: &[(u32, u32)], message: &str| {
        let (regions, reported) = compute_ranges_with_report(&model, 4, true, None, max);
        assert!(regions.len() <= max, "max {message}");
        let actual: Vec<(u32, u32)> = (0..regions.len()).map(|i| (regions.start_line_number(i), regions.end_line_number(i))).collect();
        assert_eq!(actual, expected, "{message}");
        assert_eq!(reported.map(|(_, limit)| limit), if 9 <= max { None } else { Some(max) }, "limited");
    };

    assert_limit(1000, &[r1, r2, r3, r4, r5, r6, r7, r8, r9], "1000");
    assert_limit(9, &[r1, r2, r3, r4, r5, r6, r7, r8, r9], "9");
    assert_limit(8, &[r1, r2, r3, r4, r5, r6, r7, r9], "8");
    assert_limit(7, &[r1, r2, r3, r4, r5, r7, r9], "7");
    assert_limit(6, &[r1, r2, r3, r4, r7, r9], "6");
    assert_limit(5, &[r1, r2, r3, r7, r9], "5");
    assert_limit(4, &[r1, r2, r7, r9], "4");
    assert_limit(3, &[r1, r2, r9], "3");
    assert_limit(2, &[r1, r9], "2");
    assert_limit(1, &[r1], "1");
    assert_limit(0, &[], "0");
}
