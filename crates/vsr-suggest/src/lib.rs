//! UI-free completion suggestions (ADR-0080).
//!
//! Upstream: `src/vs/base/common/filters.ts` (fuzzy scoring) and `src/vs/editor/contrib/suggest/browser/`
//! (the completion model). Nothing here knows about gpui or the workbench; `adapter` reads the extension
//! host's completion DTOs (ADR-0082).

pub mod adapter;
pub mod filters;
pub mod item;
pub mod model;
pub mod sort;
pub mod trigger;
pub mod words;
