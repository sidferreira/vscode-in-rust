//! Section headers for the minimap, ported from upstream's `findSectionHeaders.test.ts`
//! (all 19 tests).
//! upstream: src/vs/editor/test/common/services/findSectionHeaders.test.ts @ 50e1350fc78d

use vsr_editor::section_headers::{FindSectionHeaderOptions, SectionHeader, find_section_headers};

const BANNER: &str = r"^// =+\n^// (?<label>[^\n]+?)\n^// =+$";

fn opts(regex: &str) -> FindSectionHeaderOptions<'_> {
    FindSectionHeaderOptions {
        region_start: None,
        find_region_section_headers: false,
        find_mark_section_headers: true,
        mark_section_header_regex: regex,
        block_comment_start_token: None,
        block_comment_end_token: None,
    }
}

fn find(lines: &[&str], o: &FindSectionHeaderOptions) -> Vec<SectionHeader> {
    find_section_headers(lines, o)
}

fn texts(h: &[SectionHeader]) -> Vec<&str> {
    h.iter().map(|h| h.text.as_str()).collect()
}

// upstream: FindSectionHeaders > finds simple section headers
#[test]
fn finds_simple_section_headers() {
    let lines = ["regular line", "MARK: My Section", "another line", "MARK: Another Section", "last line"];
    let h = find(&lines, &opts(r"MARK:\s*(?<label>.*)$"));
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("My Section", 2, 2));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Another Section", 4, 4));
}

// upstream: FindSectionHeaders > finds section headers with separators
#[test]
fn finds_section_headers_with_separators() {
    let lines = ["regular line", "MARK: -My Section", "another line", "MARK: - Another Section", "last line"];
    let h = find(&lines, &opts(r"MARK:\s*(?<separator>-?)\s*(?<label>.*)$"));
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].text.as_str(), h[0].has_separator_line), ("My Section", true));
    assert_eq!((h[1].text.as_str(), h[1].has_separator_line), ("Another Section", true));
}

// upstream: FindSectionHeaders > finds multi-line section headers with separators
#[test]
fn finds_multi_line_section_headers_with_separators() {
    let lines = [
        "regular line",
        "// ==========",
        "// My Section",
        "// ==========",
        "code...",
        "// ==========",
        "// Another Section",
        "// ==========",
        "more code...",
    ];
    let h = find(&lines, &opts(BANNER));
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("My Section", 2, 4));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Another Section", 6, 8));
}

// upstream: FindSectionHeaders > handles overlapping multi-line section headers correctly
#[test]
fn handles_overlapping_multi_line_section_headers_correctly() {
    let lines = ["// ==========", "// Section 1", "// ==========", "// ==========", "// Section 2", "// =========="];
    let h = find(&lines, &opts(BANNER));
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("Section 1", 1, 3));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Section 2", 4, 6));
}

// upstream: FindSectionHeaders > section headers must be in comments when specified
#[test]
fn section_headers_must_be_in_comments_when_specified() {
    let lines = ["// ==========", "// Section 1", "// ==========", "==========", "Section 2", "=========="];
    let h = find(&lines, &opts(r"^(?://\s)?=+\n^(?://\s)?(?<label>[^\n]+?)\n^(?://\s)?=+$"));
    assert!(h[0].should_be_in_comments);
}

fn chunk_lines(boundary: &[(usize, &str)]) -> Vec<String> {
    let mut lines: Vec<String> = (0..150).map(|i| format!("line {i}")).collect();
    for &(i, s) in boundary {
        lines[i] = s.to_string();
    }
    lines
}

// upstream: FindSectionHeaders > handles section headers at chunk boundaries
#[test]
fn handles_section_headers_at_chunk_boundaries() {
    let lines = chunk_lines(&[
        (97, "// =========="),
        (98, "// Section 1"),
        (99, "// =========="),
        (100, "// =========="),
        (101, "// Section 2"),
        (102, "// =========="),
    ]);
    let h = find_section_headers(&lines[..], &opts(BANNER));
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("Section 1", 98, 100));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Section 2", 101, 103));
}

// upstream: FindSectionHeaders > handles empty regex gracefully without infinite loop
#[test]
fn handles_empty_regex_gracefully_without_infinite_loop() {
    assert_eq!(find(&["line 1", "line 2", "line 3"], &opts("")).len(), 0);
}

// upstream: FindSectionHeaders > handles whitespace-only regex gracefully without infinite loop
#[test]
fn handles_whitespace_only_regex_gracefully_without_infinite_loop() {
    assert_eq!(find(&["line 1", "line 2", "line 3"], &opts("   ")).len(), 0);
}

// upstream: FindSectionHeaders > correctly advances past matches without infinite loop
#[test]
fn correctly_advances_past_matches_without_infinite_loop() {
    let lines = [
        "// ==========",
        "// Section 1",
        "// ==========",
        "some code",
        "// ==========",
        "// Section 2",
        "// ==========",
        "more code",
        "// ==========",
        "// Section 3",
        "// ==========",
    ];
    let h = find(&lines, &opts(BANNER));
    assert_eq!(texts(&h), ["Section 1", "Section 2", "Section 3"]);
}

// upstream: FindSectionHeaders > handles consecutive section headers correctly
#[test]
fn handles_consecutive_section_headers_correctly() {
    let lines = ["// ==========", "// Section 1", "// ==========", "// ==========", "// Section 2", "// =========="];
    assert_eq!(texts(&find(&lines, &opts(BANNER))), ["Section 1", "Section 2"]);
}

// upstream: FindSectionHeaders > handles nested separators correctly
#[test]
fn handles_nested_separators_correctly() {
    let lines = ["// ==============", "// Major Section", "// ==============", "", "// ----------", "// Subsection", "// ----------"];
    let h = find(&lines, &opts(r"^// [-=]+\n^// (?<label>[^\n]+?)\n^// [-=]+$"));
    assert_eq!(texts(&h), ["Major Section", "Subsection"]);
}

// upstream: FindSectionHeaders > handles section headers at chunk boundaries correctly
#[test]
fn handles_section_headers_at_chunk_boundaries_correctly() {
    let mut lines: Vec<String> = (0..97).map(|i| format!("line {i}")).collect();
    for s in ["// ==========", "// Section 1", "// ==========", "// ==========", "// Section 2", "// =========="] {
        lines.push(s.to_string());
    }
    lines.extend((103..150).map(|i| format!("line {i}")));
    let h = find_section_headers(&lines[..], &opts(BANNER));
    assert_eq!(h.len(), 2, "both section headers across the chunk boundary");
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("Section 1", 98, 100));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Section 2", 101, 103));
}

// upstream: FindSectionHeaders > handles overlapping section headers without duplicates
#[test]
fn handles_overlapping_section_headers_without_duplicates() {
    let lines = [
        "// ==========",
        "// Section 1",
        "// ==========",
        "// Section 2",
        "// ==========",
        "// ==========",
        "// Section 3",
        "// ===========",
    ];
    let h = find(&lines, &opts(BANNER));
    assert_eq!(h.len(), 2);
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("Section 1", 1, 3));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Section 3", 6, 8));
}

// upstream: FindSectionHeaders > handles partially overlapping multiline section headers correctly
#[test]
fn handles_partially_overlapping_multiline_section_headers_correctly() {
    let lines = [
        "// ================",
        "// Major Section 1",
        "// ================",
        "// --------",
        "// Subsection 1.1",
        "// --------",
        "// ================",
        "// Major Section 2",
        "// ================",
    ];
    let h = find(&lines, &opts(r"^// [-=]+\n^// (?<label>[^\n]+?)\n^// [-=]+$"));
    assert_eq!(h.len(), 3);
    assert_eq!((h[0].text.as_str(), h[0].start_line_number, h[0].end_line_number), ("Major Section 1", 1, 3));
    assert_eq!((h[1].text.as_str(), h[1].start_line_number, h[1].end_line_number), ("Subsection 1.1", 4, 6));
    assert_eq!((h[2].text.as_str(), h[2].start_line_number, h[2].end_line_number), ("Major Section 2", 7, 9));
}

fn summary(h: &[SectionHeader]) -> Vec<(usize, String, bool)> {
    h.iter().map(|h| (h.start_line_number, h.text.clone(), h.has_separator_line)).collect()
}

fn row(line: usize, text: &str, sep: bool) -> (usize, String, bool) {
    (line, text.to_string(), sep)
}

// upstream: FindSectionHeaders > strips the block comment end token from mark section header labels
#[test]
fn strips_the_block_comment_end_token_from_mark_section_header_labels() {
    let lines = [
        "<!-- MARK: Html Section -->",
        "/* MARK: - C Section */",
        "<!-- MARK: Spaced Section   -->   ",
        "<!-- MARK: -->",
        "<!-- MARK: - -->",
        "// MARK: Line Comment Section */ trailing",
        "// MARK: Regex /foo.*/",
        "/* closed */ // MARK: After Closed Comment */",
    ];
    let mut o = opts(r"\bMARK:\s*(?<separator>\-?)\s*(?<label>.*)$");
    o.block_comment_start_token = Some("<!--");
    o.block_comment_end_token = Some("-->");
    let html = summary(&find(&lines, &o));
    o.block_comment_start_token = Some("/*");
    o.block_comment_end_token = Some("*/");
    let c = summary(&find(&lines, &o));
    assert_eq!(
        html,
        [
            row(1, "Html Section", false),
            row(2, "C Section */", true),
            row(3, "Spaced Section", false),
            row(5, "", true),
            row(6, "Line Comment Section */ trailing", false),
            row(7, "Regex /foo.*/", false),
            row(8, "After Closed Comment */", false),
        ]
    );
    assert_eq!(
        c,
        [
            row(1, "Html Section -->", false),
            row(2, "C Section", true),
            row(3, "Spaced Section   -->   ", false),
            row(4, "->", true),
            row(5, "-->", true),
            row(6, "Line Comment Section */ trailing", false),
            row(7, "Regex /foo.*/", false),
            row(8, "After Closed Comment */", false),
        ]
    );
}

// upstream: FindSectionHeaders > keeps the separator when the block comment end token is split across the separator and the label
#[test]
fn keeps_the_separator_when_the_block_comment_end_token_is_split_across_the_separator_and_the_label() {
    let lines = ["<!-- MARK: - -->", "<!-- MARK: -->"];
    let mut o = opts(r"\bMARK:\s*(?<separator>[- ]*)\s*(?<label>.*)$");
    o.block_comment_start_token = Some("<!--");
    o.block_comment_end_token = Some("-->");
    assert_eq!(summary(&find(&lines, &o)), [row(1, "", true)]);
}

// upstream: FindSectionHeaders > determines the block comment context from the captured text
#[test]
fn determines_the_block_comment_context_from_the_captured_text() {
    let lines = ["<!-- MARK: Section -->", "<!-- MARK: - -> -->"];
    let mut o = opts(r"^<!--\s*MARK:\s*(?<label>.*)$");
    o.block_comment_start_token = Some("<!--");
    o.block_comment_end_token = Some("-->");
    let pairs = |h: &[SectionHeader]| h.iter().map(|h| (h.text.clone(), h.has_separator_line)).collect::<Vec<_>>();
    assert_eq!(pairs(&find(&lines, &o)), [("Section".to_string(), false), ("- ->".to_string(), false)]);
    o.mark_section_header_regex = r"\bMARK:\s*(?<separator>-?)\s*(?<label>.*?)\s*-->$";
    assert_eq!(pairs(&find(&lines, &o)), [("Section".to_string(), false), ("->".to_string(), true)]);
}

// upstream: FindSectionHeaders > supports identical block comment start and end tokens
#[test]
fn supports_identical_block_comment_start_and_end_tokens() {
    let lines = ["\"\"\" MARK: Doc Section \"\"\"", "\"\"\"closed\"\"\" # MARK: Quote \"\"\""];
    let mut o = opts(r"\bMARK:\s*(?<separator>\-?)\s*(?<label>.*)$");
    o.block_comment_start_token = Some("\"\"\"");
    o.block_comment_end_token = Some("\"\"\"");
    assert_eq!(texts(&find(&lines, &o)), ["Doc Section", "Quote \"\"\""]);
}

// upstream: FindSectionHeaders > keeps mark section header labels without block comment tokens unchanged
#[test]
fn keeps_mark_section_header_labels_without_block_comment_tokens_unchanged() {
    let lines = ["<!-- MARK: Html Section -->"];
    let o = opts(r"\bMARK:\s*(?<separator>\-?)\s*(?<label>.*)$");
    let mut custom = opts(r"\bMARK:\s*(?<label>\w+ \w+)");
    custom.block_comment_start_token = Some("<!--");
    custom.block_comment_end_token = Some("-->");
    assert_eq!(texts(&find(&lines, &o)), ["Html Section -->"]);
    assert_eq!(texts(&find(&lines, &custom)), ["Html Section"]);
}
