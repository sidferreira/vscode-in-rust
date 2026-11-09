//! Quick input logic for rscode (ADR-0028), free of any UI framework.
//!
//! * [`fuzzy`]: the fuzzy scorer of `vs/base/common/fuzzyScorer.ts` (and the parts of
//!   `filters.ts` it uses), with the same scores, match positions and ordering;
//! * [`pick`]: the quick pick model (items, filtering, sorting, active item);
//! * [`open`]: quick open specifics (`path:line:column` suffix, file items, history ordering).

pub mod fuzzy;
pub mod open;
pub mod pick;

pub use fuzzy::{
    ItemAccessor, ItemScore, Match, PreparedQuery, PreparedQueryPiece, compare_items_by_fuzzy_score, prepare_query,
    score_fuzzy, score_item_fuzzy,
};
