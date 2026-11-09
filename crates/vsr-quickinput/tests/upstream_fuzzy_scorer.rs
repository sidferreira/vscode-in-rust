//! upstream: src/vs/base/test/common/fuzzyScorer.test.ts @ 50e1350fc78d
//!
//! Ports of VS Code's fuzzy scorer tests onto `vsr_quickinput::fuzzy`.
//!
//! Conventions:
//! - `URI.file(path)` becomes [`Res::file`]: the path as a `/`-style string, with a leading `/`
//!   added when missing (as `URI.file` does). `URI.from({ scheme: vscodeRemote, path })` becomes
//!   [`Res::remote`]: the path as given. On macOS `fsPath` equals `path` for both, so one field.
//! - `isWindows` is false and `sep` is `/`: branches `isWindows ? a : b` take `b`.
//! - `posix` / `win32` `normalize`, `basename` and `dirname` are small helpers. The slash accessor
//!   uses the path as is; the backslash accessor turns `/` into `\` (`win32.normalize`).
//! - Positions and matches are char indexes (identical to UTF-16 offsets for the BMP).
//! - `IItemScore` is [`ItemScore`]; `labelMatch` / `descriptionMatch` are `Option<Vec<Match>>`.
//!   Upstream `x.labelMatch![i].start` is `span(lm(&x), i)`; `x.labelMatch?.length` is `len_opt`.
//! - Upstream `Object.create(null)` caches are [`FuzzyScorerCache`] (a `HashMap`); `Object.keys(cache)`
//!   is `cache.len()`.
//! - Sorts with `[a, b, c].sort((x, y) => compare(x, y))` are `sort`, a stable `Vec::sort_by` on
//!   the sign of the comparison, as in JS.
//! - Not ported: `scoreItem - cache key matches the hash of the scoring inputs`. It asserts the
//!   JS `hash()` string used as the key in `Object.keys(cache)`. Our cache is a `HashMap` keyed
//!   by the scoring inputs (`CacheKey`), so there is no hash key to compare.
//! - `ensureNoDisposablesAreLeakedInTestSuite` has no counterpart and is not ported.

#![allow(clippy::vec_init_then_push)] // mirrors the upstream tests

use vsr_quickinput::fuzzy as fz;
use vsr_quickinput::fuzzy::{
    FuzzyScore, FuzzyScore2, FuzzyScorerCache, ItemAccessor, ItemScore, Match, PreparedQuery,
    PreparedQueryPiece, prepare_query,
};

// ---- helpers mirroring the upstream test file -------------------------------------------------

/// A resource: the `fsPath` of a `URI` (`/`-style, macOS). Also stands in for a remote URI, whose
/// `fsPath` is its path as given.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Res {
    path: String,
}

impl Res {
    /// `URI.file(path)`: a leading `/` is added when missing.
    fn file(path: &str) -> Self {
        let path = if path.starts_with('/') { path.to_string() } else { format!("/{path}") };
        Self { path }
    }

    /// `URI.from({ scheme: Schemas.vscodeRemote, path })`: the path as given.
    fn remote(path: &str) -> Self {
        Self { path: path.to_string() }
    }
}

/// `basename` (posix).
fn basename(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    trimmed.rsplit('/').next().unwrap_or("").to_string()
}

/// `dirname` (posix).
fn dirname(path: &str) -> String {
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        None => ".".to_string(),
        Some(0) => "/".to_string(),
        Some(i) => trimmed[..i].to_string(),
    }
}

/// `ResourceAccessor.getItemLabel` / `getItemDescription` / `getItemPath`.
fn label_of(r: &Res) -> String {
    basename(&r.path)
}

fn desc_of(r: &Res) -> String {
    dirname(&r.path)
}

fn path_of(r: &Res) -> String {
    r.path.clone()
}

/// `ResourceAccessor`: label = basename, description = dirname, path = the whole path.
struct ResourceAccessor;

impl ItemAccessor<Res> for ResourceAccessor {
    fn label(&self, item: &Res) -> Option<String> {
        Some(label_of(item))
    }
    fn description(&self, item: &Res) -> Option<String> {
        Some(desc_of(item))
    }
    fn path(&self, item: &Res) -> Option<String> {
        Some(path_of(item))
    }
}

/// `ResourceWithSlashAccessor`: `posix.normalize`, which leaves these normalized paths as they are.
struct ResourceWithSlashAccessor;

impl ItemAccessor<Res> for ResourceWithSlashAccessor {
    fn label(&self, item: &Res) -> Option<String> {
        Some(label_of(item))
    }
    fn description(&self, item: &Res) -> Option<String> {
        Some(dirname(&item.path))
    }
    fn path(&self, item: &Res) -> Option<String> {
        Some(item.path.clone())
    }
}

/// `ResourceWithBackslashAccessor`: `win32.normalize`, which turns `/` into `\`.
struct ResourceWithBackslashAccessor;

impl ItemAccessor<Res> for ResourceWithBackslashAccessor {
    fn label(&self, item: &Res) -> Option<String> {
        Some(label_of(item))
    }
    fn description(&self, item: &Res) -> Option<String> {
        Some(dirname(&item.path).replace('/', "\\"))
    }
    fn path(&self, item: &Res) -> Option<String> {
        Some(item.path.replace('/', "\\"))
    }
}

/// `NullAccessor`: every accessor returns `undefined`.
struct NullAccessor;

impl ItemAccessor<Res> for NullAccessor {
    fn label(&self, _item: &Res) -> Option<String> {
        None
    }
    fn description(&self, _item: &Res) -> Option<String> {
        None
    }
    fn path(&self, _item: &Res) -> Option<String> {
        None
    }
}

/// `resourceAccessorWithDescription(description)`.
struct ResourceAccessorWithDescription(Option<String>);

impl ItemAccessor<Res> for ResourceAccessorWithDescription {
    fn label(&self, item: &Res) -> Option<String> {
        Some(label_of(item))
    }
    fn description(&self, _item: &Res) -> Option<String> {
        self.0.clone()
    }
    fn path(&self, item: &Res) -> Option<String> {
        Some(path_of(item))
    }
}

/// `_doScore(target, query, allowNonContiguousMatches?)`.
fn do_score(target: &str, query: &str, allow: Option<bool>) -> FuzzyScore {
    let pq = prepare_query(query);
    let allow = allow.unwrap_or(!pq.expect_contiguous_match);
    fz::score_fuzzy(target, &pq.normalized, &pq.normalized_lowercase, allow)
}

/// `_doScore2(target, query, matchOffset = 0)`.
fn do_score2(target: &str, query: &str, match_offset: usize) -> FuzzyScore2 {
    fz::score_fuzzy2(target, &prepare_query(query), 0, match_offset)
}

/// `scoreItem(item, query, allow, accessor)` with a fresh cache.
fn score_one(item: &Res, query: &str, allow: bool, accessor: &dyn ItemAccessor<Res>) -> ItemScore {
    score_one_cached(item, query, allow, accessor, &mut FuzzyScorerCache::default())
}

/// `scoreItem(item, query, allow, accessor, cache)`.
fn score_one_cached(
    item: &Res,
    query: &str,
    allow: bool,
    accessor: &dyn ItemAccessor<Res>,
    cache: &mut FuzzyScorerCache,
) -> ItemScore {
    fz::score_item_fuzzy(Some(item), &prepare_query(query), allow, accessor, cache)
}

/// `compareItemsByScore(a, b, query, true, ResourceAccessor)`.
fn compare_by_score(a: &Res, b: &Res, query: &str) -> i32 {
    fz::compare_items_by_fuzzy_score(a, b, &prepare_query(query), true, &ResourceAccessor, &mut FuzzyScorerCache::default())
}

/// `[...items].sort((r1, r2) => compareItemsByScore(r1, r2, query, true, ResourceAccessor))`.
fn sort(items: &[&Res], query: &str) -> Vec<Res> {
    let mut sorted: Vec<Res> = items.iter().map(|r| (*r).clone()).collect();
    sorted.sort_by(|a, b| compare_by_score(a, b, query).cmp(&0));
    sorted
}

/// `ItemScore.labelMatch!`.
fn lm(s: &ItemScore) -> &[Match] {
    s.label_match.as_deref().expect("labelMatch")
}

/// `ItemScore.descriptionMatch!`.
fn dm(s: &ItemScore) -> &[Match] {
    s.description_match.as_deref().expect("descriptionMatch")
}

/// `matches[i]` as `(start, end)`.
fn span(matches: &[Match], i: usize) -> (usize, usize) {
    (matches[i].start, matches[i].end)
}

/// `matches?.length`.
fn len_opt(matches: &Option<Vec<Match>>) -> Option<usize> {
    matches.as_ref().map(Vec::len)
}

/// `query.values!` of a prepared query.
fn values(q: &PreparedQuery) -> &[PreparedQueryPiece] {
    q.values.as_deref().expect("values")
}

// ---- tests ------------------------------------------------------------------------------------

// upstream: Fuzzy Scorer > score (fuzzy)
#[test]
fn score_fuzzy() {
    let target = "HelLo-World";

    let mut scores: Vec<FuzzyScore> = Vec::new();
    scores.push(do_score(target, "HelLo-World", Some(true))); // direct case match
    scores.push(do_score(target, "hello-world", Some(true))); // direct mix-case match
    scores.push(do_score(target, "HW", Some(true))); // direct case prefix (multiple)
    scores.push(do_score(target, "hw", Some(true))); // direct mix-case prefix (multiple)
    scores.push(do_score(target, "H", Some(true))); // direct case prefix
    scores.push(do_score(target, "h", Some(true))); // direct mix-case prefix
    scores.push(do_score(target, "W", Some(true))); // direct case word prefix
    scores.push(do_score(target, "Ld", Some(true))); // in-string case match (multiple)
    scores.push(do_score(target, "ld", Some(true))); // in-string mix-case match (consecutive, avoids scattered hit)
    scores.push(do_score(target, "w", Some(true))); // direct mix-case word prefix
    scores.push(do_score(target, "L", Some(true))); // in-string case match
    scores.push(do_score(target, "l", Some(true))); // in-string mix-case match
    scores.push(do_score(target, "4", Some(true))); // no match

    // Assert scoring order
    let mut sorted_scores = scores.clone();
    sorted_scores.sort_by_key(|b| std::cmp::Reverse(b.0));
    assert_eq!(scores, sorted_scores);
}

// upstream: Fuzzy Scorer > score (non fuzzy)
#[test]
fn score_non_fuzzy() {
    let target = "HelLo-World";

    assert!(do_score(target, "HelLo-World", Some(false)).0 > 0);
    assert_eq!(do_score(target, "HelLo-World", Some(false)).1.len(), "HelLo-World".len());

    assert!(do_score(target, "hello-world", Some(false)).0 > 0);
    assert_eq!(do_score(target, "HW", Some(false)).0, 0);
    assert!(do_score(target, "h", Some(false)).0 > 0);
    assert!(do_score(target, "ello", Some(false)).0 > 0);
    assert!(do_score(target, "ld", Some(false)).0 > 0);
    assert_eq!(do_score(target, "eo", Some(false)).0, 0);
}

// upstream: Fuzzy Scorer > scoreItem - matches are proper
#[test]
fn score_item_matches_are_proper() {
    let mut cache = FuzzyScorerCache::default();
    let res = fz::score_item_fuzzy::<Res>(None, &prepare_query("something"), true, &ResourceAccessor, &mut cache);
    assert_eq!(res.score, 0);

    let resource = Res::file("/xyz/some/path/someFile123.txt");

    let res = score_one(&resource, "something", true, &NullAccessor);
    assert_eq!(res.score, 0);

    // Path Identity
    let identity_res = score_one(&resource, &path_of(&resource), true, &ResourceAccessor);
    assert!(identity_res.score != 0);
    assert_eq!(len_opt(&identity_res.description_match), Some(1));
    assert_eq!(len_opt(&identity_res.label_match), Some(1));
    assert_eq!(span(dm(&identity_res), 0).0, 0);
    assert_eq!(span(dm(&identity_res), 0).1, desc_of(&resource).len());
    assert_eq!(span(lm(&identity_res), 0).0, 0);
    assert_eq!(span(lm(&identity_res), 0).1, label_of(&resource).len());

    // Basename Prefix
    let basename_prefix_res = score_one(&resource, "som", true, &ResourceAccessor);
    assert!(basename_prefix_res.score != 0);
    assert!(basename_prefix_res.description_match.is_none());
    assert_eq!(len_opt(&basename_prefix_res.label_match), Some(1));
    assert_eq!(span(lm(&basename_prefix_res), 0), (0, "som".len()));

    // Basename Camelcase
    let basename_camelcase_res = score_one(&resource, "sF", true, &ResourceAccessor);
    assert!(basename_camelcase_res.score != 0);
    assert!(basename_camelcase_res.description_match.is_none());
    assert_eq!(len_opt(&basename_camelcase_res.label_match), Some(2));
    assert_eq!(span(lm(&basename_camelcase_res), 0), (0, 1));
    assert_eq!(span(lm(&basename_camelcase_res), 1), (4, 5));

    // Basename Match
    let basename_res = score_one(&resource, "of", true, &ResourceAccessor);
    assert!(basename_res.score != 0);
    assert!(basename_res.description_match.is_none());
    assert_eq!(len_opt(&basename_res.label_match), Some(2));
    assert_eq!(span(lm(&basename_res), 0), (1, 2));
    assert_eq!(span(lm(&basename_res), 1), (4, 5));

    // Path Match
    let path_res = score_one(&resource, "xyz123", true, &ResourceAccessor);
    assert!(path_res.score != 0);
    assert!(path_res.description_match.is_some());
    assert!(path_res.label_match.is_some());
    assert_eq!(len_opt(&path_res.label_match), Some(1));
    assert_eq!(span(lm(&path_res), 0), (8, 11));
    assert_eq!(len_opt(&path_res.description_match), Some(1));
    assert_eq!(span(dm(&path_res), 0), (1, 4));

    // Ellipsis Match
    let ellipsis_res = score_one(&resource, "…me/path/someFile123.txt", true, &ResourceAccessor);
    assert!(ellipsis_res.score != 0);
    assert!(path_res.description_match.is_some());
    assert!(path_res.label_match.is_some());
    assert_eq!(len_opt(&path_res.label_match), Some(1));
    assert_eq!(span(lm(&path_res), 0), (8, 11));
    assert_eq!(len_opt(&path_res.description_match), Some(1));
    assert_eq!(span(dm(&path_res), 0), (1, 4));

    // No Match
    let no_res = score_one(&resource, "987", true, &ResourceAccessor);
    assert_eq!(no_res.score, 0);
    assert!(no_res.label_match.is_none());
    assert!(no_res.description_match.is_none());

    // No Exact Match
    let no_exact_res = score_one(&resource, "\"sF\"", true, &ResourceAccessor);
    assert_eq!(no_exact_res.score, 0);
    assert!(no_exact_res.label_match.is_none());
    assert!(no_exact_res.description_match.is_none());
    assert_eq!(no_res.score, no_exact_res.score);

    // Verify Scores
    assert!(identity_res.score > basename_prefix_res.score);
    assert!(basename_prefix_res.score > basename_res.score);
    assert!(basename_res.score > path_res.score);
    assert!(path_res.score > no_res.score);
}

// upstream: Fuzzy Scorer > scoreItem - multiple
#[test]
fn score_item_multiple() {
    let resource = Res::file("/xyz/some/path/someFile123.txt");

    let res1 = score_one(&resource, "xyz some", true, &ResourceAccessor);
    assert!(res1.score != 0);
    assert_eq!(len_opt(&res1.label_match), Some(1));
    assert_eq!(span(lm(&res1), 0), (0, 4));
    assert_eq!(len_opt(&res1.description_match), Some(1));
    assert_eq!(span(dm(&res1), 0), (1, 4));

    let res2 = score_one(&resource, "some xyz", true, &ResourceAccessor);
    assert!(res2.score != 0);
    assert_eq!(res1.score, res2.score);
    assert_eq!(len_opt(&res2.label_match), Some(1));
    assert_eq!(span(lm(&res2), 0), (0, 4));
    assert_eq!(len_opt(&res2.description_match), Some(1));
    assert_eq!(span(dm(&res2), 0), (1, 4));

    let res3 = score_one(&resource, "some xyz file file123", true, &ResourceAccessor);
    assert!(res3.score != 0);
    assert!(res3.score > res2.score);
    assert_eq!(len_opt(&res3.label_match), Some(1));
    assert_eq!(span(lm(&res3), 0), (0, 11));
    assert_eq!(len_opt(&res3.description_match), Some(1));
    assert_eq!(span(dm(&res3), 0), (1, 4));

    let res4 = score_one(&resource, "path z y", true, &ResourceAccessor);
    assert!(res4.score != 0);
    assert!(res4.score < res2.score);
    assert_eq!(len_opt(&res4.label_match), Some(0));
    assert_eq!(len_opt(&res4.description_match), Some(2));
    assert_eq!(span(dm(&res4), 0), (2, 4));
    assert_eq!(span(dm(&res4), 1), (10, 14));
}

// upstream: Fuzzy Scorer > scoreItem - multiple with cache yields different results
#[test]
fn score_item_multiple_with_cache_yields_different_results() {
    let resource = Res::file("/xyz/some/path/someFile123.txt");
    let mut cache = FuzzyScorerCache::default();
    let res1 = score_one_cached(&resource, "xyz sm", true, &ResourceAccessor, &mut cache);
    assert!(res1.score != 0);

    // from the cache's perspective this should be a totally different query
    let res2 = score_one_cached(&resource, "xyz \"sm\"", true, &ResourceAccessor, &mut cache);
    assert_eq!(res2.score, 0);
}

// Not ported: `scoreItem - cache key matches the hash of the scoring inputs` (see the file header).

// upstream: Fuzzy Scorer > scoreItem - cache keys separate items, queries and match modes
#[test]
fn score_item_cache_keys_separate_items_queries_and_match_modes() {
    let mut cache = FuzzyScorerCache::default();

    // same label, different description
    score_one_cached(&Res::file("/xyz/some/path/someFile123.txt"), "some", true, &ResourceAccessor, &mut cache);
    score_one_cached(&Res::file("/xyz/other/path/someFile123.txt"), "some", true, &ResourceAccessor, &mut cache);

    // same item, different queries (incl. multi piece and quoted/contiguous)
    let resource = Res::file("/xyz/some/path/someFile123.txt");
    score_one_cached(&resource, "somefile", true, &ResourceAccessor, &mut cache);
    score_one_cached(&resource, "xyz sm", true, &ResourceAccessor, &mut cache);
    score_one_cached(&resource, "xyz \"sm\"", true, &ResourceAccessor, &mut cache);
    score_one_cached(&resource, "sm xyz", true, &ResourceAccessor, &mut cache);

    // same item and query, different match mode
    score_one_cached(&resource, "somefile", false, &ResourceAccessor, &mut cache);

    // an item without description
    let no_desc = Res::file("/someFile123.txt");
    score_one_cached(&no_desc, "some", true, &ResourceAccessorWithDescription(None), &mut cache);
    score_one_cached(&no_desc, "some", true, &ResourceAccessorWithDescription(Some(String::new())), &mut cache);

    assert_eq!(cache.len(), 9, "each distinct scoring input needs its own cache entry");
}

// upstream: Fuzzy Scorer > scoreItem - repeated scoring hits the cache and preserves the result
#[test]
fn score_item_repeated_scoring_hits_the_cache_and_preserves_the_result() {
    let resource = Res::file("/xyz/some/path/someFile123.txt");

    for query in ["somefile", "xyz sm", "xyz \"sm\"", "ünïcödé"] {
        let mut cache = FuzzyScorerCache::default();
        let cold = score_one_cached(&resource, query, true, &ResourceAccessor, &mut cache);
        let warm = score_one_cached(&resource, query, true, &ResourceAccessor, &mut cache);

        assert_eq!(cache.len(), 1);
        assert_eq!(warm, cold, "warm scoring should return the cached result for {query:?}");
    }
}

// upstream: Fuzzy Scorer > scoreItem - mutating a prepared query is reflected in the cache key
#[test]
fn score_item_mutating_a_prepared_query_is_reflected_in_the_cache_key() {
    let resource = Res::file("/xyz/some/path/someFile123.txt");
    let mut cache = FuzzyScorerCache::default();

    let mut query = prepare_query("xyz sm");
    let before = fz::score_item_fuzzy(Some(&resource), &query, true, &ResourceAccessor, &mut cache);
    assert!(before.score != 0);

    // callers own the prepared query object, so a mutated query must not reuse the previous entry
    query.values.as_mut().expect("values")[1].expect_contiguous_match = true;
    let after = fz::score_item_fuzzy(Some(&resource), &query, true, &ResourceAccessor, &mut cache);

    assert_eq!(cache.len(), 2);
    assert_eq!(
        after,
        fz::score_item_fuzzy(Some(&resource), &prepare_query("xyz \"sm\""), true, &ResourceAccessor, &mut FuzzyScorerCache::default())
    );
}

// upstream: Fuzzy Scorer > scoreItem - invalid input
#[test]
fn score_item_invalid_input() {
    let mut cache = FuzzyScorerCache::default();
    // `null` item or an empty query (upstream `null!`) score 0
    let res = fz::score_item_fuzzy::<Res>(None, &prepare_query(""), true, &ResourceAccessor, &mut cache);
    assert_eq!(res.score, 0);

    let res = fz::score_item_fuzzy::<Res>(None, &prepare_query("null"), true, &ResourceAccessor, &mut cache);
    assert_eq!(res.score, 0);
}

// upstream: Fuzzy Scorer > scoreItem - optimize for file paths
#[test]
fn score_item_optimize_for_file_paths() {
    let resource = Res::file("/xyz/others/spath/some/xsp/file123.txt");

    // xsp is more relevant to the end of the file path even though it matches
    // fuzzy also in the beginning. we verify the more relevant match at the
    // end gets returned.
    let path_res = score_one(&resource, "xspfile123", true, &ResourceAccessor);
    assert!(path_res.score != 0);
    assert!(path_res.description_match.is_some());
    assert!(path_res.label_match.is_some());
    assert_eq!(len_opt(&path_res.label_match), Some(1));
    assert_eq!(span(lm(&path_res), 0), (0, 7));
    assert_eq!(len_opt(&path_res.description_match), Some(1));
    assert_eq!(span(dm(&path_res), 0), (23, 26));
}

// upstream: Fuzzy Scorer > scoreItem - avoid match scattering (bug #36119)
#[test]
fn score_item_avoid_match_scattering_bug_36119() {
    let resource = Res::file("projects/ui/cula/ats/target.mk");

    let path_res = score_one(&resource, "tcltarget.mk", true, &ResourceAccessor);
    assert!(path_res.score != 0);
    assert!(path_res.description_match.is_some());
    assert!(path_res.label_match.is_some());
    assert_eq!(len_opt(&path_res.label_match), Some(1));
    assert_eq!(span(lm(&path_res), 0), (0, 9));
}

// upstream: Fuzzy Scorer > scoreItem - prefers more compact matches
#[test]
fn score_item_prefers_more_compact_matches() {
    let resource = Res::file("/1a111d1/11a1d1/something.txt");

    // expect "ad" to be matched towards the end of the file because the
    // match is more compact
    let res = score_one(&resource, "ad", true, &ResourceAccessor);
    assert!(res.score != 0);
    assert!(res.description_match.is_some());
    assert_eq!(lm(&res).len(), 0);
    assert_eq!(len_opt(&res.description_match), Some(2));
    assert_eq!(span(dm(&res), 0), (11, 12));
    assert_eq!(span(dm(&res), 1), (13, 14));
}

// upstream: Fuzzy Scorer > scoreItem - proper target offset
#[test]
fn score_item_proper_target_offset() {
    let resource = Res::file("etem");

    let res = score_one(&resource, "teem", true, &ResourceAccessor);
    assert_eq!(res.score, 0);
}

// upstream: Fuzzy Scorer > scoreItem - proper target offset #2
#[test]
fn score_item_proper_target_offset_2() {
    let resource = Res::file("ede");

    let res = score_one(&resource, "de", true, &ResourceAccessor);

    assert_eq!(len_opt(&res.label_match), Some(1));
    assert_eq!(span(lm(&res), 0), (1, 3));
}

// upstream: Fuzzy Scorer > scoreItem - proper target offset #3
#[test]
fn score_item_proper_target_offset_3() {
    let resource = Res::file("/src/vs/editor/browser/viewParts/lineNumbers/flipped-cursor-2x.svg");

    let res = score_one(&resource, "debug", true, &ResourceAccessor);

    assert_eq!(len_opt(&res.description_match), Some(3));
    assert_eq!(span(dm(&res), 0), (9, 10));
    assert_eq!(span(dm(&res), 1), (36, 37));
    assert_eq!(span(dm(&res), 2), (40, 41));

    assert_eq!(len_opt(&res.label_match), Some(2));
    assert_eq!(span(lm(&res), 0), (9, 10));
    assert_eq!(span(lm(&res), 1), (20, 21));
}

// upstream: Fuzzy Scorer > scoreItem - no match unless query contained in sequence
#[test]
fn score_item_no_match_unless_query_contained_in_sequence() {
    let resource = Res::file("abcde");

    let res = score_one(&resource, "edcda", true, &ResourceAccessor);
    assert_eq!(res.score, 0);
}

// upstream: Fuzzy Scorer > scoreItem - match if using slash or backslash (local, remote resource)
#[test]
fn score_item_match_if_using_slash_or_backslash_local_remote_resource() {
    let local_resource = Res::file("abcde/super/duper");
    let remote_resource = Res::remote("abcde/super/duper");

    for resource in [&local_resource, &remote_resource] {
        let mut res = score_one(resource, "abcde\\super\\duper", true, &ResourceAccessor);
        assert!(res.score != 0);

        res = score_one(resource, "abcde\\super\\duper", true, &ResourceWithSlashAccessor);
        assert!(res.score != 0);

        res = score_one(resource, "abcde\\super\\duper", true, &ResourceWithBackslashAccessor);
        assert!(res.score != 0);

        res = score_one(resource, "abcde/super/duper", true, &ResourceAccessor);
        assert!(res.score != 0);

        res = score_one(resource, "abcde/super/duper", true, &ResourceWithSlashAccessor);
        assert!(res.score != 0);

        res = score_one(resource, "abcde/super/duper", true, &ResourceWithBackslashAccessor);
        assert!(res.score != 0);
    }
}

// upstream: Fuzzy Scorer > scoreItem - ensure upper case bonus only applies on non-consecutive matches (bug #134723)
#[test]
fn score_item_ensure_upper_case_bonus_only_applies_on_non_consecutive_matches_bug_134723() {
    let resource_with_upper = Res::file("ASDFasdfasdf");
    let resource_all_lower = Res::file("asdfasdfasdf");

    assert!(
        score_one(&resource_all_lower, "asdf", true, &ResourceAccessor).score
            > score_one(&resource_with_upper, "asdf", true, &ResourceAccessor).score
    );
}

// upstream: Fuzzy Scorer > compareItemsByScore - identity
#[test]
fn compare_items_by_score_identity() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileB.txt");
    let resource_c = Res::file("/unrelated/some/path/other/fileC.txt");

    // Full resource A path
    let mut query = path_of(&resource_a);

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], &query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], &query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    // Full resource B path
    query = path_of(&resource_b);

    res = sort(&[&resource_a, &resource_b, &resource_c], &query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], &query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - basename prefix
#[test]
fn compare_files_by_score_basename_prefix() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileB.txt");
    let resource_c = Res::file("/unrelated/some/path/other/fileC.txt");

    // Full resource A basename
    let mut query = label_of(&resource_a);

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], &query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], &query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    // Full resource B basename
    query = label_of(&resource_b);

    res = sort(&[&resource_a, &resource_b, &resource_c], &query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], &query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - basename camelcase
#[test]
fn compare_files_by_score_basename_camelcase() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileB.txt");
    let resource_c = Res::file("/unrelated/some/path/other/fileC.txt");

    // resource A camelcase
    let mut query = "fA";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    // resource B camelcase
    query = "fB";

    res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - basename scores
#[test]
fn compare_files_by_score_basename_scores() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileB.txt");
    let resource_c = Res::file("/unrelated/some/path/other/fileC.txt");

    // Resource A part of basename
    let mut query = "fileA";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    // Resource B part of basename
    query = "fileB";

    res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - path scores
#[test]
fn compare_files_by_score_path_scores() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileB.txt");
    let resource_c = Res::file("/unrelated/some/path/other/fileC.txt");

    // Resource A part of path
    let mut query = "pathfileA";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    // Resource B part of path
    query = "pathfileB";

    res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter basenames
#[test]
fn compare_files_by_score_prefer_shorter_basenames() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileBLonger.txt");
    let resource_c = Res::file("/unrelated/the/path/other/fileC.txt");

    // Resource A part of path
    let query = "somepath";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter basenames (match on basename)
#[test]
fn compare_files_by_score_prefer_shorter_basenames_match_on_basename() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileBLonger.txt");
    let resource_c = Res::file("/unrelated/the/path/other/fileC.txt");

    // Resource A part of path
    let query = "file";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_c);
    assert_eq!(res[2], resource_b);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_c);
    assert_eq!(res[2], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter paths
#[test]
fn compare_files_by_score_prefer_shorter_paths() {
    let resource_a = Res::file("/some/path/fileA.txt");
    let resource_b = Res::file("/some/path/other/fileB.txt");
    let resource_c = Res::file("/unrelated/some/path/other/fileC.txt");

    // Resource A part of path
    let query = "somepath";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter paths (bug #17443)
#[test]
fn compare_files_by_score_prefer_shorter_paths_bug_17443() {
    let resource_a = Res::file("config/test/t1.js");
    let resource_b = Res::file("config/test.js");
    let resource_c = Res::file("config/test/t2.js");

    let query = "co/te";

    let res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
    assert_eq!(res[2], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer matches in label over description if scores are otherwise equal
#[test]
fn compare_files_by_score_prefer_matches_in_label_over_description_if_scores_are_otherwise_equal() {
    let resource_a = Res::file("parts/quick/arrow-left-dark.svg");
    let resource_b = Res::file("parts/quickopen/quickopen.ts");

    let query = "partsquick";

    let res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer camel case matches
#[test]
fn compare_files_by_score_prefer_camel_case_matches() {
    let resource_a = Res::file("config/test/NullPointerException.java");
    let resource_b = Res::file("config/test/nopointerexception.java");

    for query in ["npe", "NPE"] {
        let mut res = sort(&[&resource_a, &resource_b], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);

        res = sort(&[&resource_b, &resource_a], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);
    }
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer more compact camel case matches
#[test]
fn compare_files_by_score_prefer_more_compact_camel_case_matches() {
    let resource_a = Res::file("config/test/openthisAnythingHandler.js");
    let resource_b = Res::file("config/test/openthisisnotsorelevantforthequeryAnyHand.js");

    let query = "AH";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer more compact matches (label)
#[test]
fn compare_files_by_score_prefer_more_compact_matches_label() {
    let resource_a = Res::file("config/test/examasdaple.js");
    let resource_b = Res::file("config/test/exampleasdaasd.ts");

    let query = "xp";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer more compact matches (path)
#[test]
fn compare_files_by_score_prefer_more_compact_matches_path() {
    let resource_a = Res::file("config/test/examasdaple/file.js");
    let resource_b = Res::file("config/test/exampleasdaasd/file.ts");

    let query = "xp";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer more compact matches (label and path)
#[test]
fn compare_files_by_score_prefer_more_compact_matches_label_and_path() {
    let resource_a = Res::file("config/example/thisfile.ts");
    let resource_b = Res::file("config/24234243244/example/file.js");

    let query = "exfile";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #34210)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_34210() {
    let resource_a = Res::file("node_modules1/bundle/lib/model/modules/ot1/index.js");
    let resource_b = Res::file("node_modules1/bundle/lib/model/modules/un1/index.js");
    let resource_c = Res::file("node_modules1/bundle/lib/model/modules/modu1/index.js");
    let resource_d = Res::file("node_modules1/bundle/lib/model/modules/oddl1/index.js");

    // isWindows is false: the non-Windows query
    let mut query = "modu1/index.js";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c, &resource_d], query);
    assert_eq!(res[0], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a, &resource_d], query);
    assert_eq!(res[0], resource_c);

    query = "un1/index.js";

    res = sort(&[&resource_a, &resource_b, &resource_c, &resource_d], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_c, &resource_b, &resource_a, &resource_d], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #21019 1.)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_21019_1() {
    let resource_a = Res::file("app/containers/Services/NetworkData/ServiceDetails/ServiceLoad/index.js");
    let resource_b = Res::file("app/containers/Services/NetworkData/ServiceDetails/ServiceDistribution/index.js");
    let resource_c = Res::file("app/containers/Services/NetworkData/ServiceDetailTabs/ServiceTabs/StatVideo/index.js");

    let query = "StatVideoindex";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #21019 2.)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_21019_2() {
    let resource_a = Res::file("src/build-helper/store/redux.ts");
    let resource_b = Res::file("src/repository/store/redux.ts");

    let query = "reproreduxts";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #26649)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_26649() {
    let resource_a = Res::file("photobook/src/components/AddPagesButton/index.js");
    let resource_b = Res::file("photobook/src/components/ApprovalPageHeader/index.js");
    let resource_c = Res::file("photobook/src/canvasComponents/BookPage/index.js");

    let query = "bookpageIndex";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_c);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_c);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #33247)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_33247() {
    let resource_a = Res::file("ui/src/utils/constants.js");
    let resource_b = Res::file("ui/src/ui/Icons/index.js");

    // isWindows is false: the non-Windows query
    let query = "ui/icons";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #33247 comment)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_33247_comment() {
    let resource_a = Res::file("ui/src/components/IDInput/index.js");
    let resource_b = Res::file("ui/src/ui/Input/index.js");

    // isWindows is false: the non-Windows query
    let query = "ui/input/index";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #36166)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_36166() {
    let resource_a = Res::file("django/contrib/sites/locale/ga/LC_MESSAGES/django.mo");
    let resource_b = Res::file("django/core/signals.py");

    let query = "djancosig";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #32918)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_32918() {
    let resource_a = Res::file("adsys/protected/config.php");
    let resource_b = Res::file("adsys/protected/framework/smarty/sysplugins/smarty_internal_config.php");
    let resource_c = Res::file("duowanVideo/wap/protected/config.php");

    let query = "protectedconfig.php";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_c);
    assert_eq!(res[2], resource_b);

    res = sort(&[&resource_c, &resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_c);
    assert_eq!(res[2], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #14879)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_14879() {
    let resource_a = Res::file("pkg/search/gradient/testdata/constraint_attrMatchString.yml");
    let resource_b = Res::file("cmd/gradient/main.go");

    let query = "gradientmain";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #14727 1)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_14727_1() {
    let resource_a = Res::file("alpha-beta-cappa.txt");
    let resource_b = Res::file("abc.txt");

    let query = "abc";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #14727 2)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_14727_2() {
    let resource_a = Res::file("xerxes-yak-zubba/index.js");
    let resource_b = Res::file("xyz/index.js");

    let query = "xyz";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #18381)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_18381() {
    let resource_a = Res::file("AssymblyInfo.cs");
    let resource_b = Res::file("IAsynchronousTask.java");

    let query = "async";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #35572)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_35572() {
    let resource_a = Res::file("static/app/source/angluar/-admin/-organization/-settings/layout/layout.js");
    let resource_b = Res::file("static/app/source/angular/-admin/-project/-settings/_settings/settings.js");

    let query = "partisettings";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #36810)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_36810() {
    let resource_a = Res::file("Trilby.TrilbyTV.Web.Portal/Views/Systems/Index.cshtml");
    let resource_b = Res::file("Trilby.TrilbyTV.Web.Portal/Areas/Admins/Views/Tips/Index.cshtml");

    let query = "tipsindex.cshtml";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter hit (bug #20546)
#[test]
fn compare_files_by_score_prefer_shorter_hit_bug_20546() {
    let resource_a = Res::file("editor/core/components/tests/list-view-spec.js");
    let resource_b = Res::file("editor/core/components/list-view.js");

    let query = "listview";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - avoid match scattering (bug #12095)
#[test]
fn compare_files_by_score_avoid_match_scattering_bug_12095() {
    let resource_a = Res::file("src/vs/workbench/contrib/files/common/explorerViewModel.ts");
    let resource_b = Res::file("src/vs/workbench/contrib/files/browser/views/explorerView.ts");
    let resource_c = Res::file("src/vs/workbench/contrib/files/browser/views/explorerViewer.ts");

    let query = "filesexplorerview.ts";

    let mut res = sort(&[&resource_a, &resource_b, &resource_c], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_a, &resource_c, &resource_b], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer case match (bug #96122)
#[test]
fn compare_files_by_score_prefer_case_match_bug_96122() {
    let resource_a = Res::file("lists.php");
    let resource_b = Res::file("lib/Lists.php");

    let query = "Lists.php";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter match (bug #103052) - foo bar
#[test]
fn compare_files_by_score_prefer_shorter_match_bug_103052_foo_bar() {
    let resource_a = Res::file("app/emails/foo.bar.js");
    let resource_b = Res::file("app/emails/other-footer.other-bar.js");

    for query in ["foo bar", "foobar"] {
        let mut res = sort(&[&resource_a, &resource_b], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);

        res = sort(&[&resource_b, &resource_a], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);
    }
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter match (bug #103052) - payment model
#[test]
fn compare_files_by_score_prefer_shorter_match_bug_103052_payment_model() {
    let resource_a = Res::file("app/components/payment/payment.model.js");
    let resource_b = Res::file("app/components/online-payments-history/online-payments-history.model.js");

    for query in ["payment model", "paymentmodel"] {
        let mut res = sort(&[&resource_a, &resource_b], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);

        res = sort(&[&resource_b, &resource_a], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);
    }
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer shorter match (bug #103052) - color
#[test]
fn compare_files_by_score_prefer_shorter_match_bug_103052_color() {
    let resource_a = Res::file("app/constants/color.js");
    let resource_b = Res::file("app/components/model/input/pick-avatar-color.js");

    for query in ["color js", "colorjs"] {
        let mut res = sort(&[&resource_a, &resource_b], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);

        res = sort(&[&resource_b, &resource_a], query);
        assert_eq!(res[0], resource_a);
        assert_eq!(res[1], resource_b);
    }
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer strict case prefix
#[test]
fn compare_files_by_score_prefer_strict_case_prefix() {
    let resource_a = Res::file("app/constants/color.js");
    let resource_b = Res::file("app/components/model/input/Color.js");

    let mut query = "Color";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    query = "color";

    res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - prefer prefix (bug #103052)
#[test]
fn compare_files_by_score_prefer_prefix_bug_103052() {
    let resource_a = Res::file("test/smoke/src/main.ts");
    let resource_b = Res::file("src/vs/editor/common/services/semantikTokensProviderStyling.ts");

    let query = "smoke main.ts";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_a);
    assert_eq!(res[1], resource_b);
}

// upstream: Fuzzy Scorer > compareFilesByScore - boost better prefix match if multiple queries are used
#[test]
fn compare_files_by_score_boost_better_prefix_match_if_multiple_queries_are_used() {
    let resource_a = Res::file("src/vs/workbench/services/host/browser/browserHostService.ts");
    let resource_b = Res::file("src/vs/workbench/browser/workbench.ts");

    for query in ["workbench.ts browser", "browser workbench.ts", "browser workbench", "workbench browser"] {
        let mut res = sort(&[&resource_a, &resource_b], query);
        assert_eq!(res[0], resource_b);
        assert_eq!(res[1], resource_a);

        res = sort(&[&resource_b, &resource_a], query);
        assert_eq!(res[0], resource_b);
        assert_eq!(res[1], resource_a);
    }
}

// upstream: Fuzzy Scorer > compareFilesByScore - boost shorter prefix match if multiple queries are used
#[test]
fn compare_files_by_score_boost_shorter_prefix_match_if_multiple_queries_are_used() {
    let resource_a = Res::file("src/vs/workbench/node/actions/windowActions.ts");
    let resource_b = Res::file("src/vs/workbench/electron-node/window.ts");

    for query in ["window node", "window.ts node"] {
        let mut res = sort(&[&resource_a, &resource_b], query);
        assert_eq!(res[0], resource_b);
        assert_eq!(res[1], resource_a);

        res = sort(&[&resource_b, &resource_a], query);
        assert_eq!(res[0], resource_b);
        assert_eq!(res[1], resource_a);
    }
}

// upstream: Fuzzy Scorer > compareFilesByScore - skip preference on label match when using path sep
#[test]
fn compare_files_by_score_skip_preference_on_label_match_when_using_path_sep() {
    let resource_a = Res::file("djangosite/ufrela/def.py");
    let resource_b = Res::file("djangosite/urls/default.py");

    let query = "url/def";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - boost shorter prefix match if multiple queries are used (#99171)
#[test]
fn compare_files_by_score_boost_shorter_prefix_match_if_multiple_queries_are_used_99171() {
    let resource_a = Res::file("mesh_editor_lifetime_job.h");
    let resource_b = Res::file("lifetime_job.h");

    let query = "m life, life m";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > compareFilesByScore - boost consecutive matches in the beginning over end
#[test]
fn compare_files_by_score_boost_consecutive_matches_in_the_beginning_over_end() {
    let resource_a = Res::file("src/vs/server/node/extensionHostStatusService.ts");
    let resource_b = Res::file("src/vs/workbench/browser/parts/notifications/notificationsStatus.ts");

    let query = "notStatus";

    let mut res = sort(&[&resource_a, &resource_b], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);

    res = sort(&[&resource_b, &resource_a], query);
    assert_eq!(res[0], resource_b);
    assert_eq!(res[1], resource_a);
}

// upstream: Fuzzy Scorer > prepareQuery
#[test]
fn prepare_query_test() {
    assert_eq!(prepare_query(" f*a ").normalized, "fa");
    assert_eq!(prepare_query(" f…a ").normalized, "fa");
    assert_eq!(prepare_query("main#").normalized, "main");
    assert_eq!(prepare_query("main#").original, "main#");
    assert_eq!(prepare_query("foo*").normalized, "foo");
    assert_eq!(prepare_query("foo*").original, "foo*");
    assert_eq!(prepare_query("model Tester.ts").original, "model Tester.ts");
    assert_eq!(prepare_query("model Tester.ts").original_lowercase, "model Tester.ts".to_lowercase());
    assert_eq!(prepare_query("model Tester.ts").normalized, "modelTester.ts");
    assert!(!prepare_query("model Tester.ts").expect_contiguous_match); // doesn't have quotes in it
    assert_eq!(prepare_query("Model Tester.ts").normalized_lowercase, "modeltester.ts");
    assert!(!prepare_query("ModelTester.ts").contains_path_separator);
    assert!(prepare_query("Model/Tester.ts").contains_path_separator);
    assert!(prepare_query("\"hello\"").expect_contiguous_match);
    assert_eq!(prepare_query("\"hello\"").normalized, "hello");

    // with spaces
    let mut query = prepare_query("He*llo World");
    assert_eq!(query.original, "He*llo World");
    assert_eq!(query.normalized, "HelloWorld");
    assert_eq!(query.normalized_lowercase, "HelloWorld".to_lowercase());
    assert_eq!(query.values.as_ref().map(Vec::len), Some(2));
    assert_eq!(values(&query)[0].original, "He*llo");
    assert_eq!(values(&query)[0].normalized, "Hello");
    assert_eq!(values(&query)[0].normalized_lowercase, "Hello".to_lowercase());
    assert_eq!(values(&query)[1].original, "World");
    assert_eq!(values(&query)[1].normalized, "World");
    assert_eq!(values(&query)[1].normalized_lowercase, "World".to_lowercase());

    let restored_query = fz::pieces_to_query(values(&query));
    assert_eq!(restored_query.original, query.original);
    assert_eq!(restored_query.values.as_ref().map(Vec::len), query.values.as_ref().map(Vec::len));
    assert_eq!(restored_query.contains_path_separator, query.contains_path_separator);

    // with spaces that are empty
    query = prepare_query(" Hello   World  \t");
    assert_eq!(query.original, " Hello   World  \t");
    assert_eq!(query.original_lowercase, " Hello   World  \t".to_lowercase());
    assert_eq!(query.normalized, "HelloWorld");
    assert_eq!(query.normalized_lowercase, "HelloWorld".to_lowercase());
    assert_eq!(query.values.as_ref().map(Vec::len), Some(2));
    assert_eq!(values(&query)[0].original, "Hello");
    assert_eq!(values(&query)[0].original_lowercase, "Hello".to_lowercase());
    assert_eq!(values(&query)[0].normalized, "Hello");
    assert_eq!(values(&query)[0].normalized_lowercase, "Hello".to_lowercase());
    assert_eq!(values(&query)[1].original, "World");
    assert_eq!(values(&query)[1].original_lowercase, "World".to_lowercase());
    assert_eq!(values(&query)[1].normalized, "World");
    assert_eq!(values(&query)[1].normalized_lowercase, "World".to_lowercase());

    // Path related (isWindows is false)
    assert_eq!(prepare_query("/some/path").path_normalized, "/some/path");
    assert_eq!(prepare_query("/some/path").normalized, "/some/path");
    assert!(prepare_query("/some/path").contains_path_separator);
    assert_eq!(prepare_query("\\some\\path").path_normalized, "/some/path");
    assert_eq!(prepare_query("\\some\\path").normalized, "/some/path");
    assert!(prepare_query("\\some\\path").contains_path_separator);
}

// upstream: Fuzzy Scorer > fuzzyScore2 (matching)
#[test]
fn fuzzy_score2_matching() {
    let target = "HelLo-World";

    for offset in [0usize, 3] {
        let target_with_offset = if offset == 0 { target.to_string() } else { format!("123{target}") };
        let (score, matches) = do_score2(&target_with_offset, "HelLo-World", offset);

        assert!(score.unwrap_or(0) != 0);
        assert_eq!(matches.len(), 1);
        assert_eq!(span(&matches, 0), (offset, target.len() + offset));

        let (score, matches) = do_score2(&target_with_offset, "HW", offset);

        assert!(score.unwrap_or(0) != 0);
        assert_eq!(matches.len(), 2);
        assert_eq!(span(&matches, 0), (offset, 1 + offset));
        assert_eq!(span(&matches, 1), (6 + offset, 7 + offset));
    }
}

// upstream: Fuzzy Scorer > fuzzyScore2 (multiple queries)
#[test]
fn fuzzy_score2_multiple_queries() {
    let target = "HelLo-World";

    let (first_single_score, first_single_matches) = do_score2(target, "HelLo", 0);
    let (second_single_score, second_single_matches) = do_score2(target, "World", 0);
    let first_and_second_single_matches: Vec<Match> =
        first_single_matches.iter().chain(second_single_matches.iter()).copied().collect();

    let (mut multi_score, mut multi_matches) = do_score2(target, "HelLo World", 0);

    // assertScore: the combined score is at least the sum, and the matches are the single matches in order
    let assert_score = |multi_score: Option<i32>, multi_matches: &[Match]| {
        assert!(
            multi_score.unwrap_or(0) >= first_single_score.unwrap_or(0) + second_single_score.unwrap_or(0),
            "multi score {multi_score:?} below the sum of single scores"
        );
        for (i, multi_match) in multi_matches.iter().enumerate() {
            let single = first_and_second_single_matches.get(i).expect("a single match for each multi match");
            assert_eq!((multi_match.start, multi_match.end), (single.start, single.end));
        }
    };
    // assertNoScore
    let assert_no_score = |multi_score: Option<i32>, multi_matches: &[Match]| {
        assert_eq!(multi_score, None);
        assert_eq!(multi_matches.len(), 0);
    };

    assert_score(multi_score, &multi_matches);

    (multi_score, multi_matches) = do_score2(target, "World HelLo", 0);
    assert_score(multi_score, &multi_matches);

    (multi_score, multi_matches) = do_score2(target, "World HelLo World", 0);
    assert_score(multi_score, &multi_matches);

    (multi_score, multi_matches) = do_score2(target, "World HelLo Nothing", 0);
    assert_no_score(multi_score, &multi_matches);

    (multi_score, multi_matches) = do_score2(target, "More Nothing", 0);
    assert_no_score(multi_score, &multi_matches);
}

// upstream: Fuzzy Scorer > fuzzyScore2 (#95716)
#[test]
fn fuzzy_score2_95716() {
    // Only the score and match presence are asserted: the emoji is outside the BMP.
    let target = "# ❌ Wow";

    let score = do_score2(target, "❌", 0);
    assert!(score.0.is_some());
    assert!(!score.1.is_empty());
}

// upstream: Fuzzy Scorer > Using quotes should expect contiguous matches match
#[test]
fn using_quotes_should_expect_contiguous_matches_match() {
    // missing the "i" in the query
    assert_eq!(do_score("contiguous", "\"contguous\"", None).0, 0);

    let score = do_score("contiguous", "\"contiguous\"", None);
    assert!(score.0 > 0);
}

// upstream: Fuzzy Scorer > Using quotes should highlight contiguous indexes
#[test]
fn using_quotes_should_highlight_contiguous_indexes() {
    let score = do_score("2021-7-26.md", "\"26\"", None);
    assert_eq!(score.0, 14);

    // The indexes of the 2 and 6 of "26"
    assert_eq!(score.1[0], 7);
    assert_eq!(score.1[1], 8);
}

// upstream: Fuzzy Scorer > Workspace symbol search with special characters (#, *)
#[test]
fn workspace_symbol_search_with_special_characters() {
    // Simulates the scenario from the issue where rust-analyzer uses # and * as query modifiers
    // The original query (with special chars) should reach the language server
    // but normalized query (without special chars) should be used for fuzzy matching

    // Test #: User types "main#", language server returns "main" symbol
    let mut query = prepare_query("main#");
    assert_eq!(query.original, "main#"); // Sent to language server
    assert_eq!(query.normalized, "main"); // Used for fuzzy matching
    let (mut score, mut matches) = do_score2("main", "main#", 0);
    assert!(score.is_some_and(|s| s > 0), "Should match \"main\" symbol when query is \"main#\"");
    assert!(!matches.is_empty());

    // Test *: User types "foo*", language server returns "foo" symbol
    query = prepare_query("foo*");
    assert_eq!(query.original, "foo*"); // Sent to language server
    assert_eq!(query.normalized, "foo"); // Used for fuzzy matching
    (score, matches) = do_score2("foo", "foo*", 0);
    assert!(score.is_some_and(|s| s > 0), "Should match \"foo\" symbol when query is \"foo*\"");
    assert!(!matches.is_empty());

    // Test both: User types "MyClass#*", should match "MyClass"
    query = prepare_query("MyClass#*");
    assert_eq!(query.original, "MyClass#*");
    assert_eq!(query.normalized, "MyClass");
    (score, matches) = do_score2("MyClass", "MyClass#*", 0);
    assert!(score.is_some_and(|s| s > 0), "Should match \"MyClass\" symbol when query is \"MyClass#*\"");
    assert!(!matches.is_empty());

    // Test fuzzy matching still works: User types "MC#", should match "MyClass"
    query = prepare_query("MC#");
    assert_eq!(query.original, "MC#");
    assert_eq!(query.normalized, "MC");
    (score, matches) = do_score2("MyClass", "MC#", 0);
    assert!(score.is_some_and(|s| s > 0), "Should fuzzy match \"MyClass\" symbol when query is \"MC#\"");
    assert!(!matches.is_empty());

    // Make sure leading # or # in the middle are not removed.
    query = prepare_query("#SpecialFunction");
    assert_eq!(query.original, "#SpecialFunction");
    assert_eq!(query.normalized, "#SpecialFunction");
    (score, matches) = do_score2("#SpecialFunction", "#SpecialFunction", 0);
    assert!(score.is_some_and(|s| s > 0), "Should match \"#SpecialFunction\" symbol when query is \"#SpecialFunction\"");
    assert!(!matches.is_empty());

    // Make sure standalone # is not removed
    query = prepare_query("#");
    assert_eq!(query.original, "#");
    assert_eq!(query.normalized, "#", "Standalone # should not be removed");
    (score, matches) = do_score2("#", "#", 0);
    assert!(score.is_some_and(|s| s > 0), "Should match \"#\" symbol when query is \"#\"");
    assert!(!matches.is_empty());
}
