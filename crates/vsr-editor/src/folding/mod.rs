//! Code folding: the UI-free regions, model and hidden lines (ADR-0042), and their integration
//! in the `Editor` (`editor_folding`).
//!
//! upstream: src/vs/editor/contrib/folding/browser/foldingRanges.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/folding/browser/foldingModel.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/folding/browser/indentRangeProvider.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/folding/browser/hiddenRangeModel.ts @ 50e1350fc78d
//!
//! `ranges` holds the regions and their merge, `indent` computes them from indentation and region
//! markers, `model` is the collapse state and its operations, `hidden` turns collapsed regions
//! into hidden line ranges. Range providers from language servers (`syntaxRangeProvider.ts`) do
//! not exist yet, so the strategy `auto` is the indentation one.

mod hidden;
mod indent;
mod model;
mod ranges;

pub use hidden::{HiddenRangeModel, LineRange};
pub use indent::{
    FoldingMarkers, MAX_FOLDING_REGIONS_FOR_INDENT_DEFAULT, RangesCollector, compute_indent_level, compute_ranges,
    compute_ranges_with_report,
};
pub use model::{
    FoldingModel, LineMap, get_next_fold_line, get_parent_fold_line, get_previous_fold_line, set_collapse_state_at_level,
    set_collapse_state_for_matching_lines, set_collapse_state_for_rest, set_collapse_state_for_type, set_collapse_state_levels_down,
    set_collapse_state_levels_up, set_collapse_state_up, toggle_collapse_state,
};
pub use ranges::{
    FoldRange, FoldSource, FoldingRegion, FoldingRegions, MAX_FOLDING_REGIONS, MAX_LINE_NUMBER, SelectedLines,
};
