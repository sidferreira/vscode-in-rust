//! Word wrap: the UI-free line-breaking and projection layer (ADR-0036).
//!
//! upstream: src/vs/editor/common/viewModel/monospaceLineBreaksComputer.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/modelLineProjectionData.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/viewModel/modelLineProjection.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/viewModel/viewModelLines.ts @ 50e1350fc78d
//!
//! `line_breaks` decides where a model line wraps (a port of the monospace computer),
//! `projection_data` holds the result for one line and translates offsets (a port of
//! `ModelLineProjectionData`), and `view_lines` is the projection of a whole buffer: view
//! line count, model <-> view positions, view line content, and incremental updates.

mod line_breaks;
mod projection_data;
mod view_lines;

pub use line_breaks::{BreakParams, LineBreaksComputer, WordBreak, WrappingIndent};
pub use projection_data::{ModelLineProjectionData, OutputPosition, PositionAffinity};
pub use view_lines::{ViewLine, ViewLines, WrapParams};
