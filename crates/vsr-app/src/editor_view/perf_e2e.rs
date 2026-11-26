//! Shared helpers for the timing tests of large documents (the 10,000-line tests in `e2e.rs`,
//! `wrap_e2e.rs`, `folding_e2e.rs` and `minimap/e2e.rs`; ADR-0125).
//!
//! Every such test is split in two. The default test runs the workload and its deterministic
//! assertions (line counts, carets, folds, the text) and only prints the timing. The `perf_`
//! sibling is `#[ignore]`d and asserts the wall-clock budget. Budgets are not asserted by default
//! because the default run is parallel and shares the machine: the same workload measured 190 to
//! 370 ms per key under load, against a 100 ms budget that an unloaded debug build meets.

use std::time::Duration;

/// Per-key budget (typing, paging) for a 10,000-line debug build on an unloaded machine.
pub(super) const KEY_BUDGET: Duration = Duration::from_millis(100);

/// Best of `batches` measurements. The minimum keeps the one batch that ran without
/// interference, which is what an upper bound on the cost of one key should use.
pub(super) fn best_of(batches: usize, mut batch: impl FnMut() -> Duration) -> Duration {
    (0..batches).map(|_| batch()).min().unwrap_or(Duration::MAX)
}
