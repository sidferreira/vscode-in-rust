//! UI-free completion suggestions (ADR-0080).
//!
//! Upstream: `src/vs/base/common/filters.ts` (fuzzy scoring) and `src/vs/editor/contrib/suggest/browser/`
//! (the completion model). Nothing here knows about gpui or the workbench.

pub mod filters;
pub mod item;
pub mod model;
pub mod sort;
