//! Folding regions from indentation, and from region markers (`#region` ... `#endregion`).
//!
//! upstream: src/vs/editor/contrib/folding/browser/indentRangeProvider.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/model/utils.ts @ 50e1350fc78d
//!
//! `compute_ranges` is upstream `computeRanges`: one pass from the last line to the first that
//! keeps a stack of the indentation levels still open. `RangesCollector` keeps the cap on the
//! number of regions: over the limit, the regions of the deepest indentation are dropped first.

use regex::Regex;
use vsr_text::TextBuffer;

use super::ranges::{FoldingRegions, MAX_LINE_NUMBER};

/// Upstream `MAX_FOLDING_REGIONS_FOR_INDENT_DEFAULT`.
pub const MAX_FOLDING_REGIONS_FOR_INDENT_DEFAULT: usize = 5000;

/// The region markers of a language (upstream `FoldingMarkers`): a line that matches `start`
/// opens a region and one that matches `end` closes it.
#[derive(Debug, Clone)]
pub enum FoldingMarkers {
    /// Same flags on both: `(start)|(?:end)` is one scan per line, and group 1 tells which side matched.
    Combined(Regex),
    /// Different flags: two scans.
    Separate { start: Regex, end: Regex },
}

impl FoldingMarkers {
    /// The markers for the two regular expressions (JavaScript syntax), `None` when one does
    /// not compile.
    pub fn new(start: &str, end: &str) -> Option<Self> {
        Self::with_flags(start, false, end, false)
    }

    /// Like `new`, with each expression's `i` (ignore case) flag. Upstream builds the combined
    /// pattern only when both have the same flags.
    pub fn with_flags(start: &str, start_ignore_case: bool, end: &str, end_ignore_case: bool) -> Option<Self> {
        let flag = |i: bool| if i { "(?i)" } else { "" };
        if start_ignore_case == end_ignore_case {
            return Regex::new(&format!("{}({start})|(?:{end})", flag(start_ignore_case))).ok().map(Self::Combined);
        }
        let start = Regex::new(&format!("{}{start}", flag(start_ignore_case))).ok()?;
        let end = Regex::new(&format!("{}{end}", flag(end_ignore_case))).ok()?;
        Some(Self::Separate { start, end })
    }

    /// Whether `line` starts a region (`Some(true)`), ends one (`Some(false)`), or neither.
    pub fn classify(&self, line: &str) -> Option<bool> {
        match self {
            Self::Combined(p) => p.captures(line).map(|c| c.get(1).is_some()),
            Self::Separate { start, end } => {
                if start.is_match(line) {
                    Some(true)
                } else {
                    end.is_match(line).then_some(false)
                }
            }
        }
    }
}

/// Upstream `computeIndentLevel`: the indentation of `line` in columns, `None` for a line that
/// is empty or only whitespace.
pub fn compute_indent_level(line: &str, tab_size: usize) -> Option<usize> {
    let tab = tab_size.max(1);
    let mut indent = 0usize;
    for b in line.bytes() {
        match b {
            b' ' => indent += 1,
            b'\t' => indent = indent - indent % tab + tab,
            _ => return Some(indent),
        }
    }
    None
}

/// Collects regions found from the last line to the first (upstream `RangesCollector`).
#[derive(Debug)]
pub struct RangesCollector {
    starts: Vec<u32>,
    ends: Vec<u32>,
    indent_occurrences: Vec<usize>,
    limit: usize,
    exceeded: Option<(usize, usize)>,
}

impl RangesCollector {
    pub fn new(limit: usize) -> Self {
        Self { starts: Vec::new(), ends: Vec::new(), indent_occurrences: Vec::new(), limit, exceeded: None }
    }

    pub fn insert_first(&mut self, start: u32, end: u32, indent: usize) {
        if start > MAX_LINE_NUMBER || end > MAX_LINE_NUMBER {
            return;
        }
        self.starts.push(start);
        self.ends.push(end);
        if indent < 1000 {
            if self.indent_occurrences.len() <= indent {
                self.indent_occurrences.resize(indent + 1, 0);
            }
            self.indent_occurrences[indent] += 1;
        }
    }

    /// `(regions found, limit)` when the limit cut some, `None` while within it (what upstream
    /// reports through `FoldingLimitReporter.update`).
    pub fn exceeded(&self) -> Option<(usize, usize)> {
        self.exceeded
    }

    /// The regions in document order. Over the limit, those of the smallest indentation stay.
    pub fn to_indent_ranges(&mut self, buf: &TextBuffer, tab_size: usize) -> FoldingRegions {
        let n = self.starts.len();
        let limit = self.limit;
        if n <= limit {
            self.exceeded = None;
            let starts: Vec<u32> = self.starts.iter().rev().copied().collect();
            let ends: Vec<u32> = self.ends.iter().rev().copied().collect();
            return FoldingRegions::new(starts, ends, None).expect("within the limit");
        }
        self.exceeded = Some((n, limit));
        let mut entries = 0usize;
        let mut max_indent = self.indent_occurrences.len();
        for (i, &occ) in self.indent_occurrences.iter().enumerate() {
            if occ != 0 {
                if occ + entries > limit {
                    max_indent = i;
                    break;
                }
                entries += occ;
            }
        }
        let (mut starts, mut ends) = (Vec::with_capacity(limit), Vec::with_capacity(limit));
        for i in (0..n).rev() {
            let start = self.starts[i];
            let indent = compute_indent_level(buf.line_content(start as usize), tab_size);
            // Upstream compares with -1 for a blank line, which is below any `maxIndent`.
            let keep = match indent {
                None => true,
                Some(ind) if ind < max_indent => true,
                Some(ind) if ind == max_indent => {
                    entries += 1;
                    entries - 1 < limit
                }
                _ => false,
            };
            if keep && starts.len() < limit {
                starts.push(start);
                ends.push(self.ends[i]);
            }
        }
        FoldingRegions::new(starts, ends, None).expect("within the limit")
    }
}

struct Previous {
    /// The indentation, or -2 for a marker region.
    indent: i64,
    /// The end line of the region above.
    end_above: i64,
    /// The start line of the region (marker regions only).
    line: i64,
}

/// Upstream `computeRanges`: the folding regions of `buf` by indentation and, with `markers`, by
/// region markers. `off_side` languages attach blank lines to the block above.
pub fn compute_ranges(buf: &TextBuffer, tab_size: usize, off_side: bool, markers: Option<&FoldingMarkers>, limit: usize) -> FoldingRegions {
    compute_ranges_with_report(buf, tab_size, off_side, markers, limit).0
}

/// Like `compute_ranges`, also returning how many regions were found when the limit cut some.
pub fn compute_ranges_with_report(
    buf: &TextBuffer,
    tab_size: usize,
    off_side: bool,
    markers: Option<&FoldingMarkers>,
    limit: usize,
) -> (FoldingRegions, Option<(usize, usize)>) {
    let mut result = RangesCollector::new(limit);
    let line_count = buf.line_count() as i64;
    let mut previous_regions: Vec<Previous> = vec![Previous { indent: -1, end_above: line_count + 1, line: line_count + 1 }];

    let mut line = line_count;
    while line > 0 {
        let content = buf.line_content(line as usize);
        let indent = compute_indent_level(content, tab_size);
        let this_line = line;
        line -= 1;
        let Some(indent) = indent else {
            if off_side {
                // For offSide languages, empty lines belong to the previous block. The next block is
                // already written to the results, so this only affects the end of the block before.
                if let Some(p) = previous_regions.last_mut() {
                    p.end_above = this_line;
                }
            }
            continue;
        };
        let indent = indent as i64;
        let line = this_line;
        let mut previous = previous_regions.len() - 1;
        let mut is_start = false;
        let mut is_end = false;
        if let Some(kind) = markers.and_then(|m| m.classify(content)) {
            is_start = kind;
            is_end = !kind;
        }
        if is_start || is_end {
            if is_start {
                // Discard all regions until the folding pattern.
                let mut i = previous_regions.len() - 1;
                while i > 0 && previous_regions[i].indent != -2 {
                    i -= 1;
                }
                if i > 0 {
                    previous_regions.truncate(i + 1);
                    previous = i;
                    // A new folding range from the pattern, including the end line.
                    result.insert_first(line as u32, previous_regions[previous].line as u32, indent as usize);
                    let p = &mut previous_regions[previous];
                    p.line = line;
                    p.indent = indent;
                    p.end_above = line;
                    continue;
                }
                // No end marker found: the line is a regular line.
            } else {
                // An end pattern match.
                previous_regions.push(Previous { indent: -2, end_above: line, line });
                continue;
            }
        }
        if previous_regions[previous].indent > indent {
            // Discard all regions with a larger indent.
            loop {
                previous_regions.pop();
                previous = previous_regions.len() - 1;
                if previous_regions[previous].indent <= indent {
                    break;
                }
            }
            // A new folding range; it needs at least size 1.
            let end_line = previous_regions[previous].end_above - 1;
            if end_line - line >= 1 {
                result.insert_first(line as u32, end_line as u32, indent as usize);
            }
        }
        if previous_regions[previous].indent == indent {
            previous_regions[previous].end_above = line;
        } else {
            // A new region with a bigger indent.
            previous_regions.push(Previous { indent, end_above: line, line });
        }
    }
    let regions = result.to_indent_ranges(buf, tab_size);
    (regions, result.exceeded())
}
