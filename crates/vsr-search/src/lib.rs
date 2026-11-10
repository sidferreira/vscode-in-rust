//! Find-in-files engine for rscode (ADR-0023), mirroring `vs/workbench/services/search`.
//!
//! UI free. The pieces:
//! * [`query`]: the query text and toggles, smart case, literal escaping, the regex source;
//! * [`glob`] / [`patterns`]: VS Code glob syntax, "files to include/exclude" parsing and the filter;
//! * [`preview`]: result previews with elision around long lines;
//! * [`settings`]: the settings that drive a search, with upstream defaults;
//! * [`engine`]: the streaming search (`ignore` walker + `grep-searcher`);
//! * [`files`]: the file-name listing behind `workspace.findFiles` (same walker and filter).

pub mod engine;
pub mod files;
pub mod glob;
pub mod patterns;
pub mod preview;
pub mod query;
pub mod settings;

pub use engine::{
    FileMatch, SearchComplete, SearchError, SearchOptions, TextMatch, build_matcher, search, search_collect,
};
pub use patterns::split_pattern_input;
pub use preview::{PreviewOptions, SearchRange, TextSearchMatch};
pub use query::TextSearchQuery;
pub use settings::{CollapseResults, ResultSortOrder, SearchSettings};
