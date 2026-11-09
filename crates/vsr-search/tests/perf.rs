//! Performance sanity check (not a unit test: needs the upstream clone, prints timings).
//!
//! `UPSTREAM_CLONE=/path/to/vscode-original cargo test -p vsr-search --release --test perf -- --ignored --nocapture`

use std::path::PathBuf;
use vsr_search::{SearchOptions, TextSearchQuery, search_collect};

#[test]
#[ignore = "needs UPSTREAM_CLONE; run with --release --ignored --nocapture"]
fn search_the_upstream_src_tree() {
    let Ok(clone) = std::env::var("UPSTREAM_CLONE") else { return };
    let root = PathBuf::from(clone).join("src");
    for (label, mut q) in [
        ("literal `function`", TextSearchQuery::new("function")),
        ("literal `registerSingleton` (rare)", TextSearchQuery::new("registerSingleton")),
        ("regex `\\bclass\\s+\\w+Service\\b`", TextSearchQuery::new("\\bclass\\s+\\w+Service\\b")),
    ] {
        q.is_regex = label.contains("regex");
        let mut o = SearchOptions::new(&root);
        o.max_results = None;
        let (files, done) = search_collect(&q, &o).unwrap();
        let n: usize = files.iter().map(|f| f.result_count()).sum();
        o.exclude_globs_from_settings.clear();
        let (_, d2) = search_collect(&q, &o).unwrap();
        println!("  (without settings excludes: {:?})", d2.duration);
        println!("{label}: {n} results in {} files, {} files searched, {:?}", files.len(), done.files_searched, done.duration);
    }
}
