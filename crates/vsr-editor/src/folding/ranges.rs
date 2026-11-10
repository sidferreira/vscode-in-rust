//! Folding regions: sorted line ranges with a parent index, collapse state and source, and the
//! merge of a fresh provider result with the regions that were collapsed or created by hand.
//!
//! upstream: src/vs/editor/contrib/folding/browser/foldingRanges.ts @ 50e1350fc78d
//!
//! Lines are 1-based. Upstream packs the parent index into the high bits of two `Uint32Array`s and
//! the three bit fields into `BitField`s; here they are plain vectors (`parents` is computed on
//! first use, like upstream's `ensureParentIndices`). `find_range` and the parent lookups return
//! `None` where upstream returns `-1`.

use std::cell::OnceCell;

/// Upstream `MAX_FOLDING_REGIONS`.
pub const MAX_FOLDING_REGIONS: usize = 0xFFFF;
/// Upstream `MAX_LINE_NUMBER`.
pub const MAX_LINE_NUMBER: u32 = 0xFF_FFFF;

/// Where a region comes from (upstream `FoldSource`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FoldSource {
    /// Computed by the range provider.
    #[default]
    Provider,
    /// Created by "Create Folding Range from Selection".
    UserDefined,
    /// A collapsed range kept from before the last recomputation.
    Recovered,
}

impl FoldSource {
    /// Upstream `foldSourceAbbr`.
    pub fn abbr(self) -> char {
        match self {
            FoldSource::Provider => ' ',
            FoldSource::UserDefined => 'u',
            FoldSource::Recovered => 'r',
        }
    }
}

/// One region as plain data (upstream `FoldRange`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoldRange {
    pub start_line_number: u32,
    pub end_line_number: u32,
    pub kind: Option<String>,
    pub is_collapsed: bool,
    pub source: FoldSource,
}

impl FoldRange {
    pub fn new(start: u32, end: u32) -> Self {
        Self { start_line_number: start, end_line_number: end, kind: None, is_collapsed: false, source: FoldSource::Provider }
    }
}

/// The start lines of the selections, to keep a selection out of a collapsed region
/// (upstream `SelectedLines`).
#[derive(Debug, Clone, Default)]
pub struct SelectedLines(pub Vec<usize>);

impl SelectedLines {
    /// Whether a selection starts on a line in `start..=end`.
    pub fn starts_inside(&self, start: usize, end: usize) -> bool {
        self.0.iter().any(|&l| l >= start && l <= end)
    }
}

#[derive(Debug, Clone)]
pub struct FoldingRegions {
    starts: Vec<u32>,
    ends: Vec<u32>,
    collapsed: Vec<bool>,
    sources: Vec<FoldSource>,
    types: Option<Vec<Option<String>>>,
    parents: OnceCell<Vec<Option<usize>>>,
}

impl Default for FoldingRegions {
    fn default() -> Self {
        Self::empty()
    }
}

impl FoldingRegions {
    pub fn empty() -> Self {
        Self::new(Vec::new(), Vec::new(), None).expect("no regions")
    }

    /// Regions from sorted start and end lines. Fails when the lists differ in length or hold
    /// more than `MAX_FOLDING_REGIONS` regions.
    pub fn new(starts: Vec<u32>, ends: Vec<u32>, types: Option<Vec<Option<String>>>) -> Result<Self, &'static str> {
        if starts.len() != ends.len() || starts.len() > MAX_FOLDING_REGIONS {
            return Err("invalid startIndexes or endIndexes size");
        }
        let n = starts.len();
        Ok(Self { starts, ends, collapsed: vec![false; n], sources: vec![FoldSource::Provider; n], types, parents: OnceCell::new() })
    }

    fn parents(&self) -> &Vec<Option<usize>> {
        self.parents.get_or_init(|| {
            let mut stack: Vec<usize> = Vec::new();
            let mut parents = Vec::with_capacity(self.starts.len());
            for i in 0..self.starts.len() {
                let (s, e) = (self.starts[i], self.ends[i]);
                while let Some(&top) = stack.last() {
                    if self.starts[top] <= s && self.ends[top] >= e {
                        break;
                    }
                    stack.pop();
                }
                parents.push(stack.last().copied());
                stack.push(i);
            }
            parents
        })
    }

    pub fn len(&self) -> usize {
        self.starts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.starts.is_empty()
    }

    pub fn start_line_number(&self, index: usize) -> u32 {
        self.starts[index]
    }

    pub fn end_line_number(&self, index: usize) -> u32 {
        self.ends[index]
    }

    pub fn region_type(&self, index: usize) -> Option<&str> {
        self.types.as_ref().and_then(|t| t[index].as_deref())
    }

    pub fn has_types(&self) -> bool {
        self.types.is_some()
    }

    pub fn is_collapsed(&self, index: usize) -> bool {
        self.collapsed[index]
    }

    pub fn set_collapsed(&mut self, index: usize, state: bool) {
        self.collapsed[index] = state;
    }

    pub fn source(&self, index: usize) -> FoldSource {
        self.sources[index]
    }

    pub fn set_source(&mut self, index: usize, source: FoldSource) {
        self.sources[index] = source;
    }

    /// Collapses or expands every region of `kind`; whether there was one.
    pub fn set_collapsed_all_of_type(&mut self, kind: &str, state: bool) -> bool {
        let Some(types) = &self.types else { return false };
        let mut changed = false;
        for (i, t) in types.iter().enumerate() {
            if t.as_deref() == Some(kind) {
                self.collapsed[i] = state;
                changed = true;
            }
        }
        changed
    }

    pub fn parent_index(&self, index: usize) -> Option<usize> {
        self.parents()[index]
    }

    pub fn contains(&self, index: usize, line: u32) -> bool {
        self.starts[index] <= line && self.ends[index] >= line
    }

    fn find_index(&self, line: u32) -> Option<usize> {
        // The last region that starts at or before `line`.
        self.starts.partition_point(|&s| s <= line).checked_sub(1)
    }

    /// The innermost region that contains `line`.
    pub fn find_range(&self, line: u32) -> Option<usize> {
        let mut index = self.find_index(line)?;
        if self.ends[index] >= line {
            return Some(index);
        }
        while let Some(p) = self.parent_index(index) {
            if self.contains(p, line) {
                return Some(p);
            }
            index = p;
        }
        None
    }

    /// Upstream `toString`: `[ -] 3/5, [u+] 7/9`.
    pub fn describe(&self) -> String {
        (0..self.len())
            .map(|i| format!("[{}{}] {}/{}", self.sources[i].abbr(), if self.collapsed[i] { '+' } else { '-' }, self.starts[i], self.ends[i]))
            .collect::<Vec<_>>()
            .join(", ")
    }

    pub fn to_fold_range(&self, index: usize) -> FoldRange {
        FoldRange {
            start_line_number: self.starts[index],
            end_line_number: self.ends[index],
            kind: self.types.as_ref().and_then(|t| t[index].clone()),
            is_collapsed: self.collapsed[index],
            source: self.sources[index],
        }
    }

    pub fn to_fold_ranges(&self) -> Vec<FoldRange> {
        (0..self.len()).map(|i| self.to_fold_range(i)).collect()
    }

    pub fn from_fold_ranges(ranges: &[FoldRange]) -> Self {
        let starts = ranges.iter().map(|r| r.start_line_number).collect();
        let ends = ranges.iter().map(|r| r.end_line_number).collect();
        let types = ranges
            .iter()
            .any(|r| r.kind.as_deref().is_some_and(|k| !k.is_empty()))
            .then(|| ranges.iter().map(|r| r.kind.clone()).collect());
        let mut regions = Self::new(starts, ends, types).expect("sanitized ranges fit");
        for (i, r) in ranges.iter().enumerate() {
            regions.collapsed[i] = r.is_collapsed;
            regions.sources[i] = r.source;
        }
        regions
    }

    /// Merges two lists, each sorted on start line (upstream `sanitizeAndMerge`).
    ///
    /// `a` holds every region the range provider currently reports, `b` only the previously
    /// collapsed ones and the manual ones. On the same start line the provider's range is taken
    /// (and stays collapsed when it keeps its size, or no selection starts inside it); when
    /// entries overlap, `b` wins and the overlapped entries of `a` are dropped. Invalid entries
    /// (out of range or sequence, overlapping a predecessor without being contained) are dropped.
    pub fn sanitize_and_merge(
        a: &[FoldRange],
        b: &[FoldRange],
        max_line_number: Option<u32>,
        selection: Option<&SelectedLines>,
    ) -> Vec<FoldRange> {
        let max_line = max_line_number.unwrap_or(u32::MAX);
        let (mut ia, mut ib) = (0usize, 0usize);
        let mut stacked: Vec<FoldRange> = Vec::new();
        let mut top: Option<FoldRange> = None;
        let mut prev_line = 0u32;
        let mut result: Vec<FoldRange> = Vec::new();

        while ia < a.len() || ib < b.len() {
            let next_a = a.get(ia);
            let next_b = b.get(ib);
            let mut use_range: Option<FoldRange> = None;
            if let Some(nb) = next_b.filter(|nb| next_a.is_none_or(|na| na.start_line_number >= nb.start_line_number)) {
                if let Some(na) = next_a.filter(|na| na.start_line_number == nb.start_line_number) {
                    if nb.source == FoldSource::UserDefined {
                        // A user defined range (possibly unfolded).
                        use_range = Some(nb.clone());
                    } else {
                        // A previously folded range or a (possibly unfolded) recovered range.
                        let mut r = na.clone();
                        r.is_collapsed = nb.is_collapsed
                            && (na.end_line_number == nb.end_line_number
                                || !selection.is_some_and(|s| s.starts_inside(na.start_line_number as usize + 1, na.end_line_number as usize + 1)));
                        r.source = FoldSource::Provider;
                        use_range = Some(r);
                    }
                    ia += 1;
                } else {
                    let mut r = nb.clone();
                    if nb.is_collapsed && nb.source == FoldSource::Provider {
                        // A previously collapsed range.
                        r.source = FoldSource::Recovered;
                    }
                    use_range = Some(r);
                }
                ib += 1;
            } else if let Some(na) = next_a {
                // `a` is next. The user folded `b` set takes precedence: look ahead in it for a conflict.
                let mut scan = ib;
                loop {
                    match b.get(scan) {
                        Some(p) if p.start_line_number <= na.end_line_number => {
                            if p.source == FoldSource::UserDefined && p.end_line_number > na.end_line_number {
                                break; // a user folded range wins: this `a` is skipped
                            }
                            scan += 1;
                        }
                        _ => {
                            use_range = Some(na.clone());
                            break;
                        }
                    }
                }
                ia += 1;
            }

            if let Some(r) = use_range {
                while top.as_ref().is_some_and(|t| t.end_line_number < r.start_line_number) {
                    top = stacked.pop();
                }
                if r.end_line_number > r.start_line_number
                    && r.start_line_number > prev_line
                    && r.end_line_number <= max_line
                    && top.as_ref().is_none_or(|t| t.end_line_number >= r.end_line_number)
                {
                    prev_line = r.start_line_number;
                    if let Some(t) = top.take() {
                        stacked.push(t);
                    }
                    top = Some(r.clone());
                    result.push(r);
                }
            }
        }
        result
    }
}

/// A snapshot of one region (upstream `FoldingRegion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FoldingRegion {
    pub region_index: usize,
    pub start_line_number: u32,
    pub end_line_number: u32,
    pub parent_index: Option<usize>,
    pub is_collapsed: bool,
}

impl FoldingRegion {
    pub fn new(ranges: &FoldingRegions, index: usize) -> Self {
        Self {
            region_index: index,
            start_line_number: ranges.start_line_number(index),
            end_line_number: ranges.end_line_number(index),
            parent_index: ranges.parent_index(index),
            is_collapsed: ranges.is_collapsed(index),
        }
    }

    /// Whether this region lies inside `start..=end`.
    pub fn contained_by(&self, start: u32, end: u32) -> bool {
        start <= self.start_line_number && end >= self.end_line_number
    }

    pub fn contained_by_region(&self, other: &FoldingRegion) -> bool {
        self.contained_by(other.start_line_number, other.end_line_number)
    }

    pub fn contains_line(&self, line: u32) -> bool {
        self.start_line_number <= line && line <= self.end_line_number
    }

    pub fn hides_line(&self, line: u32) -> bool {
        self.start_line_number < line && line <= self.end_line_number
    }
}
