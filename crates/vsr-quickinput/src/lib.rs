//! Quick input logic for rscode (ADR-0028, ADR-0030), free of any UI framework.
//!
//! * [`fuzzy`]: the fuzzy scorer of `vs/base/common/fuzzyScorer.ts` (and the parts of
//!   `filters.ts` it uses), with the same scores, match positions and ordering;
//! * [`filters`]: the word filters of `filters.ts` the Command Palette matches labels with;
//! * [`pick`]: the quick pick model (items, filtering, sorting, active item);
//! * [`open`]: quick open specifics (`path:line:column` suffix, file items, history ordering);
//! * [`commands`]: the picks of the Command Palette (`>`);
//! * [`goto_line`]: Go to Line/Column (`:`) parsing;
//! * [`quick_access`]: which provider handles the typed prefix, and the `?` help list.

pub mod commands;
pub mod filters;
pub mod fuzzy;
pub mod goto_line;
pub mod open;
pub mod pick;
pub mod quick_access;

pub use fuzzy::{
    ItemAccessor, ItemScore, Match, PreparedQuery, PreparedQueryPiece, compare_items_by_fuzzy_score, prepare_query,
    score_fuzzy, score_item_fuzzy,
};
