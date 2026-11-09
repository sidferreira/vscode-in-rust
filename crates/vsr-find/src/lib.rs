//! The UI-free editor Find and Replace model (`vs/editor/contrib/find`, ADR-0027): search a text
//! buffer, navigate matches ("3 of 12"), replace one / all through `vsr-editor` edits.
//!
//! upstream: src/vs/editor/contrib/find/browser/findModel.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findState.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findDecorations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/replacePattern.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/replaceAllCommand.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findController.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/model/textModelSearch.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/search.ts @ 50e1350fc78d
//!
//! * [`search`]: compile a search string (query semantics shared with the Search view through
//!   `vsr_search::query`) and collect matches with capture groups;
//! * [`replace_pattern`]: `$1`, `\u`, Preserve Case and friends;
//! * [`model`]: the model ([`FindModel`]) and the seeding / scope helpers of `FindController`.
//!
//! Depends on `vsr-text` and `vsr-editor` only for the data it works on; no UI.

pub mod model;
pub mod replace_pattern;
pub mod search;

pub use model::{FindModel, FindState, MATCHES_LIMIT, MatchInfo, SeedMode, scope_from_selection, seed_search_string};
pub use replace_pattern::{ReplacePattern, ReplacePiece, build_replace_string_with_case_preserved, parse_replace_string};
pub use search::{CompileError, FindMatch, SearchParams, compile};
