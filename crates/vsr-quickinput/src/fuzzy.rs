//! The fuzzy scorer: scores items (label, description, path) against a query and reports the
//! characters to highlight.
//!
//! upstream: src/vs/base/common/fuzzyScorer.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/filters.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/comparers.ts @ 50e1350fc78d
//!
//! A port, not a re-design: scores, match positions and the ordering of
//! [`compare_items_by_fuzzy_score`] are those of VS Code, and its unit tests are ported in
//! `tests/upstream_fuzzy_scorer.rs`. Differences:
//!
//! * Strings are handled as `char`s, so match positions are char indexes where upstream has UTF-16
//!   code unit offsets (identical for the BMP). Lowercasing maps each char to one lowercase char
//!   (chars whose lowercase is longer, such as `İ`, stay as they are), so lengths are preserved.
//! * The platform is macOS: `sep` is `/`, path identity is case-insensitive, backslashes in a query
//!   are turned into `/`.
//! * `compareFileNames` uses an `Intl.Collator` (numeric, base sensitivity); here a numeric,
//!   case-insensitive natural comparison stands in (no accent folding, no ICU punctuation order).
//! * The score cache is a `HashMap` keyed by the scoring inputs instead of a number hash.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::ops::Deref;

const SEP: char = '/';

/// `IMatch`: a range of characters, end exclusive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Match {
    pub start: usize,
    pub end: usize,
}

impl Match {
    pub const fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }
}

fn lower_char(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

fn lower_chars(s: &[char]) -> Vec<char> {
    s.iter().map(|&c| lower_char(c)).collect()
}

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn is_upper_ascii(c: char) -> bool {
    c.is_ascii_uppercase()
}

// ---- filters.ts subset ------------------------------------------------------------------------

/// `matchesPrefix`: case-insensitive prefix match; `None` when `word` is not a prefix of
/// `word_to_match_against`.
pub fn matches_prefix(word: &str, word_to_match_against: &str) -> Option<Vec<Match>> {
    let w = chars(word);
    let t = chars(word_to_match_against);
    if t.is_empty() || t.len() < w.len() {
        return None;
    }
    if !w.iter().zip(&t).all(|(&a, &b)| lower_char(a) == lower_char(b)) {
        return None;
    }
    Some(if w.is_empty() { Vec::new() } else { vec![Match::new(0, w.len())] })
}

const MAX_LEN: usize = 128;

fn is_emoji_imprecise(x: u32) -> bool {
    (0x1F1E6..=0x1F1FF).contains(&x)
        || x == 8986
        || x == 8987
        || x == 9200
        || x == 9203
        || (9728..=10175).contains(&x)
        || x == 11088
        || x == 11093
        || (127744..=128591).contains(&x)
        || (128640..=128764).contains(&x)
        || (128992..=129008).contains(&x)
        || (129280..=129535).contains(&x)
        || (129648..=129782).contains(&x)
}

fn is_separator_at(value: &[char], index: isize) -> bool {
    if index < 0 || index as usize >= value.len() {
        return false;
    }
    let c = value[index as usize];
    matches!(c, '_' | '-' | '.' | ' ' | '/' | '\\' | '\'' | '"' | ':' | '$' | '<' | '>' | '(' | ')' | '[' | ']' | '{' | '}')
        || is_emoji_imprecise(c as u32)
}

fn is_whitespace_at(value: &[char], index: isize) -> bool {
    if index < 0 || index as usize >= value.len() {
        return false;
    }
    matches!(value[index as usize], ' ' | '\t')
}

fn is_upper_case_at(pos: usize, word: &[char], word_low: &[char]) -> bool {
    word[pos] != word_low[pos]
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrow {
    None,
    Diag,
    Left,
    LeftLeft,
}

/// `FuzzyScoreOptions`.
#[derive(Clone, Copy, Debug)]
pub struct FuzzyScoreOptions {
    pub first_match_can_be_weak: bool,
    pub boost_full_match: bool,
}

impl Default for FuzzyScoreOptions {
    fn default() -> Self {
        Self { first_match_can_be_weak: false, boost_full_match: true }
    }
}

/// `FuzzyScore` of filters.ts: the score, the offset matching started at, and the matched
/// positions relative to it (in the order upstream stores them: last match first).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FilterScore {
    pub score: i32,
    pub word_start: usize,
    pub positions: Vec<usize>,
}

/// `createMatches(score)` of filters.ts.
pub fn create_filter_matches(score: Option<&FilterScore>) -> Vec<Match> {
    let mut res: Vec<Match> = Vec::new();
    let Some(s) = score else { return res };
    for &p in s.positions.iter().rev() {
        let pos = p + s.word_start;
        match res.last_mut() {
            Some(last) if last.end == pos => last.end = pos + 1,
            _ => res.push(Match::new(pos, pos + 1)),
        }
    }
    res
}

/// `fuzzyScore` of filters.ts. Strings are given as char slices.
#[allow(clippy::too_many_arguments)]
pub fn fuzzy_score(
    pattern: &[char],
    pattern_low: &[char],
    pattern_start: usize,
    word: &[char],
    word_low: &[char],
    word_start: usize,
    options: FuzzyScoreOptions,
) -> Option<FilterScore> {
    let pattern_len = pattern.len().min(MAX_LEN);
    let word_len = word.len().min(MAX_LEN);
    if pattern_start >= pattern_len || word_start >= word_len || (pattern_len - pattern_start) > (word_len - word_start) {
        return None;
    }

    // Do the pattern characters occur (in order) in the word at all?
    let mut min_pos = vec![0usize; pattern_len + 1];
    {
        let (mut pp, mut wp) = (pattern_start, word_start);
        while pp < pattern_len && wp < word_len {
            if pattern_low[pp] == word_low[wp] {
                min_pos[pp] = wp;
                pp += 1;
            }
            wp += 1;
        }
        if pp != pattern_len {
            return None;
        }
    }
    // Max matching word position for each pattern position.
    let mut max_pos = vec![0usize; pattern_len + 1];
    {
        let mut pp = pattern_len as isize - 1;
        let mut wp = word_len as isize - 1;
        while pp >= pattern_start as isize && wp >= word_start as isize {
            if pattern_low[pp as usize] == word_low[wp as usize] {
                max_pos[pp as usize] = wp as usize;
                pp -= 1;
            }
            wp -= 1;
        }
    }

    let stride = word_len + 1;
    let idx = |r: usize, c: usize| r * stride + c;
    let rows = pattern_len + 1;
    let mut diag = vec![0i32; rows * stride];
    let mut table = vec![0i32; rows * stride];
    let mut arrows = vec![Arrow::None; rows * stride];

    let mut row = 1usize;
    let mut column = 1usize;
    let mut has_strong_first_match = false;
    let mut pattern_pos = pattern_start;

    while pattern_pos < pattern_len {
        let min_word_match_pos = min_pos[pattern_pos];
        let max_word_match_pos = max_pos[pattern_pos];
        let next_max_word_match_pos = if pattern_pos + 1 < pattern_len { max_pos[pattern_pos + 1] } else { word_len };

        column = min_word_match_pos - word_start + 1;
        let mut word_pos = min_word_match_pos;
        while word_pos < next_max_word_match_pos {
            let mut score = NO_SCORE_I32;
            let mut can_come_diag = false;
            if word_pos <= max_word_match_pos {
                score = do_score(
                    pattern,
                    pattern_low,
                    pattern_pos,
                    pattern_start,
                    word,
                    word_low,
                    word_pos,
                    word_len,
                    word_start,
                    diag[idx(row - 1, column - 1)] == 0,
                    &mut has_strong_first_match,
                );
            }
            let mut diag_score = 0;
            if score != NO_SCORE_I32 {
                can_come_diag = true;
                diag_score = score + table[idx(row - 1, column - 1)];
            }
            let can_come_left = word_pos > min_word_match_pos;
            let left_score = if can_come_left {
                table[idx(row, column - 1)] + if diag[idx(row, column - 1)] > 0 { -5 } else { 0 }
            } else {
                0
            };
            let can_come_left_left = word_pos > min_word_match_pos + 1 && diag[idx(row, column - 1)] > 0;
            let left_left_score = if can_come_left_left {
                table[idx(row, column - 2)] + if diag[idx(row, column - 2)] > 0 { -5 } else { 0 }
            } else {
                0
            };

            if can_come_left_left && (!can_come_left || left_left_score >= left_score) && (!can_come_diag || left_left_score >= diag_score) {
                table[idx(row, column)] = left_left_score;
                arrows[idx(row, column)] = Arrow::LeftLeft;
                diag[idx(row, column)] = 0;
            } else if can_come_left && (!can_come_diag || left_score >= diag_score) {
                table[idx(row, column)] = left_score;
                arrows[idx(row, column)] = Arrow::Left;
                diag[idx(row, column)] = 0;
            } else if can_come_diag {
                table[idx(row, column)] = diag_score;
                arrows[idx(row, column)] = Arrow::Diag;
                diag[idx(row, column)] = diag[idx(row - 1, column - 1)] + 1;
            } else {
                unreachable!("not possible");
            }
            word_pos += 1;
            column += 1;
        }
        row += 1;
        pattern_pos += 1;
    }

    if !has_strong_first_match && !options.first_match_can_be_weak {
        return None;
    }

    row -= 1;
    column -= 1;

    let mut result = FilterScore { score: table[idx(row, column)], word_start, positions: Vec::new() };
    let mut backwards_diag_length = 0usize;
    let mut max_match_column = 0usize;

    while row >= 1 {
        // Find the column where we go diagonally up.
        let mut diag_column = column;
        loop {
            match arrows[idx(row, diag_column)] {
                Arrow::LeftLeft => diag_column -= 2,
                Arrow::Left => diag_column -= 1,
                _ => break,
            }
            if diag_column < 1 {
                break;
            }
        }

        // Overturn the "forwards" decision if keeping the "backwards" diagonal gives a better match.
        if backwards_diag_length > 1
            && pattern_low[pattern_start + row - 1] == word_low[word_start + column - 1]
            && !is_upper_case_at(diag_column + word_start - 1, word, word_low)
            && backwards_diag_length as i32 + 1 > diag[idx(row, diag_column)]
        {
            diag_column = column;
        }

        if diag_column == column {
            backwards_diag_length += 1;
        } else {
            backwards_diag_length = 1;
        }
        if max_match_column == 0 {
            max_match_column = diag_column;
        }

        row -= 1;
        column = diag_column - 1;
        result.positions.push(column);
    }

    if word_len - word_start == pattern_len && options.boost_full_match {
        result.score += 2;
    }
    // 1 penalty for each skipped character in the word.
    result.score -= max_match_column as i32 - pattern_len as i32;
    Some(result)
}

const NO_SCORE_I32: i32 = i32::MIN;

#[allow(clippy::too_many_arguments)]
fn do_score(
    pattern: &[char],
    pattern_low: &[char],
    pattern_pos: usize,
    pattern_start: usize,
    word: &[char],
    word_low: &[char],
    word_pos: usize,
    word_len: usize,
    word_start: usize,
    new_match_start: bool,
    out_first_match_strong: &mut bool,
) -> i32 {
    if pattern_low[pattern_pos] != word_low[word_pos] {
        return NO_SCORE_I32;
    }
    let wp = word_pos as isize;
    let mut score = 1;
    let mut is_gap_location = false;
    if word_pos == pattern_pos - pattern_start {
        // common prefix: `foobar <-> foobaz`
        score = if pattern[pattern_pos] == word[word_pos] { 7 } else { 5 };
    } else if is_upper_case_at(word_pos, word, word_low) && (word_pos == 0 || !is_upper_case_at(word_pos - 1, word, word_low)) {
        // hitting upper-case: `foo <-> forOthers`
        score = if pattern[pattern_pos] == word[word_pos] { 7 } else { 5 };
        is_gap_location = true;
    } else if is_separator_at(word_low, wp) && (word_pos == 0 || !is_separator_at(word_low, wp - 1)) {
        // hitting a separator: `. <-> foo.bar`
        score = 5;
    } else if is_separator_at(word_low, wp - 1) || is_whitespace_at(word_low, wp - 1) {
        // post separator: `foo <-> bar_foo`
        score = 5;
        is_gap_location = true;
    }

    if score > 1 && pattern_pos == pattern_start {
        *out_first_match_strong = true;
    }
    if !is_gap_location {
        is_gap_location = is_upper_case_at(word_pos, word, word_low) || is_separator_at(word_low, wp - 1) || is_whitespace_at(word_low, wp - 1);
    }

    if pattern_pos == pattern_start {
        if word_pos > word_start {
            score -= if is_gap_location { 3 } else { 5 };
        }
    } else if new_match_start {
        score += if is_gap_location { 2 } else { 0 };
    } else {
        score += if is_gap_location { 0 } else { 1 };
    }

    if word_pos + 1 == word_len {
        score -= if is_gap_location { 3 } else { 5 };
    }
    score
}

// ---- scoreFuzzy -------------------------------------------------------------------------------

/// `FuzzyScore` of fuzzyScorer.ts: the score (0 = no match) and the matched char positions.
pub type FuzzyScore = (i32, Vec<usize>);

/// `scoreFuzzy`: score `target` against `query`. `query_lower` is `query` lowercased.
pub fn score_fuzzy(target: &str, query: &str, query_lower: &str, allow_non_contiguous_matches: bool) -> FuzzyScore {
    if target.is_empty() || query.is_empty() {
        return (0, Vec::new());
    }
    let t = chars(target);
    let q = chars(query);
    if t.len() < q.len() {
        return (0, Vec::new());
    }
    let t_lower = lower_chars(&t);
    let q_lower = chars(query_lower);
    do_score_fuzzy(&q, &q_lower, &t, &t_lower, allow_non_contiguous_matches)
}

fn do_score_fuzzy(query: &[char], query_lower: &[char], target: &[char], target_lower: &[char], allow_non_contiguous: bool) -> FuzzyScore {
    let (ql, tl) = (query.len(), target.len());
    let mut scores = vec![0i32; ql * tl];
    let mut matches = vec![0usize; ql * tl];

    for qi in 0..ql {
        let q_off = qi * tl;
        let q_prev_off = q_off.wrapping_sub(tl);
        let qi_gt_null = qi > 0;
        for ti in 0..tl {
            let ti_gt_null = ti > 0;
            let current = q_off + ti;
            let left = current.wrapping_sub(1);
            let diag = q_prev_off.wrapping_add(ti).wrapping_sub(1);

            let left_score = if ti_gt_null { scores[left] } else { 0 };
            let diag_score = if qi_gt_null && ti_gt_null { scores[diag] } else { 0 };
            let seq_len = if qi_gt_null && ti_gt_null { matches[diag] } else { 0 };

            let score = if diag_score == 0 && qi_gt_null {
                0
            } else {
                compute_char_score(query[qi], query_lower[qi], target, target_lower, ti, seq_len)
            };

            let is_valid = score != 0 && diag_score + score >= left_score;
            if is_valid
                && (allow_non_contiguous
                    || qi_gt_null
                    || (ti + ql <= tl && target_lower[ti..ti + ql] == *query_lower))
            {
                matches[current] = seq_len + 1;
                scores[current] = diag_score + score;
            } else {
                matches[current] = 0;
                scores[current] = left_score;
            }
        }
    }

    // Restore positions (from the bottom right of the matrix).
    let mut positions = Vec::new();
    let mut qi = ql as isize - 1;
    let mut ti = tl as isize - 1;
    while qi >= 0 && ti >= 0 {
        let current = qi as usize * tl + ti as usize;
        if matches[current] == 0 {
            ti -= 1;
        } else {
            positions.push(ti as usize);
            qi -= 1;
            ti -= 1;
        }
    }
    positions.reverse();
    (scores[ql * tl - 1], positions)
}

fn compute_char_score(q: char, q_lower: char, target: &[char], target_lower: &[char], ti: usize, seq_len: usize) -> i32 {
    if !consider_as_equal(q_lower, target_lower[ti]) {
        return 0;
    }
    let mut score = 1; // character match bonus
    // Consecutive match bonus: up to 3 get the full bonus (6), the rest half of it (3).
    if seq_len > 0 {
        score += (seq_len.min(3) * 6 + seq_len.saturating_sub(3) * 3) as i32;
    }
    if q == target[ti] {
        score += 1; // same case
    }
    if ti == 0 {
        score += 8; // start of word
    } else {
        let sep = score_separator_at(target[ti - 1]);
        if sep != 0 {
            score += sep;
        } else if is_upper_ascii(target[ti]) && seq_len == 0 {
            // camel case, only outside of a contiguous sequence (NPE => NullPointerException, HTTP => HTTP: no boost)
            score += 2;
        }
    }
    score
}

fn consider_as_equal(a: char, b: char) -> bool {
    if a == b {
        return true;
    }
    if a == '/' || a == '\\' {
        return b == '/' || b == '\\';
    }
    false
}

fn score_separator_at(c: char) -> i32 {
    match c {
        '/' | '\\' => 5, // prefer path separators...
        '_' | '-' | '.' | ' ' | '\'' | '"' | ':' => 4, // ...over other separators
        _ => 0,
    }
}

// ---- scoreFuzzy2 ------------------------------------------------------------------------------

/// `FuzzyScore2`: the score (`None` = no match) and the matches.
pub type FuzzyScore2 = (Option<i32>, Vec<Match>);

fn create_matches(offsets: &[usize]) -> Vec<Match> {
    let mut ret: Vec<Match> = Vec::new();
    for &pos in offsets {
        match ret.last_mut() {
            Some(last) if last.end == pos => last.end += 1,
            _ => ret.push(Match::new(pos, pos + 1)),
        }
    }
    ret
}

fn matches_overlap(a: Match, b: Match) -> bool {
    !(a.end < b.start || b.end < a.start)
}

fn normalize_matches(mut matches: Vec<Match>) -> Vec<Match> {
    matches.sort_by_key(|m| m.start);
    let mut out: Vec<Match> = Vec::new();
    for m in matches {
        match out.last_mut() {
            Some(cur) if matches_overlap(*cur, m) => {
                cur.start = cur.start.min(m.start);
                cur.end = cur.end.max(m.end);
            }
            _ => out.push(m),
        }
    }
    out
}

/// `scoreFuzzy2`: the alternate scorer (used for symbols), on a prepared query (all pieces must match).
pub fn score_fuzzy2(target: &str, query: &PreparedQuery, pattern_start: usize, word_start: usize) -> FuzzyScore2 {
    if let Some(values) = query.values.as_ref().filter(|v| v.len() > 1) {
        let mut total = 0;
        let mut all = Vec::new();
        for piece in values {
            let (score, matches) = score_fuzzy2_piece(target, piece, pattern_start, word_start);
            let Some(score) = score else { return (None, Vec::new()) };
            total += score;
            all.extend(matches);
        }
        return (Some(total), normalize_matches(all));
    }
    score_fuzzy2_piece(target, &query.piece, pattern_start, word_start)
}

/// `scoreFuzzy2` on a single query piece.
pub fn score_fuzzy2_piece(target: &str, query: &PreparedQueryPiece, pattern_start: usize, word_start: usize) -> FuzzyScore2 {
    let word = chars(target);
    let word_low = lower_chars(&word);
    let pattern = chars(&query.normalized);
    let pattern_low = chars(&query.normalized_lowercase);
    let opts = FuzzyScoreOptions { first_match_can_be_weak: true, boost_full_match: true };
    match fuzzy_score(&pattern, &pattern_low, pattern_start, &word, &word_low, word_start, opts) {
        Some(score) => (Some(score.score), create_filter_matches(Some(&score))),
        None => (None, Vec::new()),
    }
}

// ---- item scorer ------------------------------------------------------------------------------

/// `IItemScore`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemScore {
    pub score: i32,
    /// Matches within the label.
    pub label_match: Option<Vec<Match>>,
    /// Matches within the description.
    pub description_match: Option<Vec<Match>>,
}

/// `IItemAccessor`.
pub trait ItemAccessor<T> {
    fn label(&self, item: &T) -> Option<String>;
    fn description(&self, item: &T) -> Option<String>;
    /// If the item is a file, its path.
    fn path(&self, item: &T) -> Option<String>;
}

pub const PATH_IDENTITY_SCORE: i32 = 1 << 18;
pub const LABEL_PREFIX_SCORE_THRESHOLD: i32 = 1 << 17;
pub const LABEL_SCORE_THRESHOLD: i32 = 1 << 16;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CacheKey {
    normalized: String,
    pieces: Vec<(String, bool)>,
    label: String,
    description: Option<String>,
    allow_non_contiguous_matches: bool,
}

/// `FuzzyScorerCache`.
pub type FuzzyScorerCache = HashMap<CacheKey, ItemScore>;

fn non_empty(s: Option<String>) -> Option<String> {
    s.filter(|s| !s.is_empty())
}

/// `scoreItemFuzzy`. An absent item or an empty query/label scores 0.
pub fn score_item_fuzzy<T>(
    item: Option<&T>,
    query: &PreparedQuery,
    allow_non_contiguous_matches: bool,
    accessor: &dyn ItemAccessor<T>,
    cache: &mut FuzzyScorerCache,
) -> ItemScore {
    let Some(item) = item else { return ItemScore::default() };
    if query.normalized.is_empty() {
        return ItemScore::default();
    }
    let Some(label) = non_empty(accessor.label(item)) else { return ItemScore::default() };
    let description = accessor.description(item);

    let pieces: Vec<(String, bool)> = match &query.values {
        Some(v) => v.iter().map(|p| (p.normalized.clone(), p.expect_contiguous_match)).collect(),
        None => vec![(query.normalized.clone(), query.expect_contiguous_match)],
    };
    let key = CacheKey {
        normalized: query.normalized.clone(),
        pieces,
        label: label.clone(),
        description: description.clone(),
        allow_non_contiguous_matches,
    };
    if let Some(cached) = cache.get(&key) {
        return cached.clone();
    }
    let score = do_score_item_fuzzy(&label, description.as_deref(), non_empty(accessor.path(item)).as_deref(), query, allow_non_contiguous_matches);
    cache.insert(key, score.clone());
    score
}

/// `scoreItemFuzzy` without the cache (for scoring a whole list once per query).
pub fn score_item_fuzzy_uncached<T>(item: Option<&T>, query: &PreparedQuery, allow_non_contiguous_matches: bool, accessor: &dyn ItemAccessor<T>) -> ItemScore {
    let Some(item) = item else { return ItemScore::default() };
    if query.normalized.is_empty() {
        return ItemScore::default();
    }
    let Some(label) = non_empty(accessor.label(item)) else { return ItemScore::default() };
    let description = accessor.description(item);
    do_score_item_fuzzy(&label, description.as_deref(), non_empty(accessor.path(item)).as_deref(), query, allow_non_contiguous_matches)
}

fn equals_ignore_case(a: &str, b: &str) -> bool {
    let (a, b) = (chars(a), chars(b));
    a.len() == b.len() && lower_chars(&a) == lower_chars(&b)
}

fn do_score_item_fuzzy(label: &str, description: Option<&str>, path: Option<&str>, query: &PreparedQuery, allow: bool) -> ItemScore {
    let prefer_label_matches = path.is_none() || !query.contains_path_separator;

    // Identity matches on the full path rank highest.
    if let Some(path) = path {
        let identical = if cfg!(target_os = "linux") { query.path_normalized == path } else { equals_ignore_case(&query.path_normalized, path) };
        if identical {
            return ItemScore {
                score: PATH_IDENTITY_SCORE,
                label_match: Some(vec![Match::new(0, label.chars().count())]),
                description_match: description.filter(|d| !d.is_empty()).map(|d| vec![Match::new(0, d.chars().count())]),
            };
        }
    }

    if let Some(values) = query.values.as_ref().filter(|v| v.len() > 1) {
        let mut total = 0;
        let mut label_matches = Vec::new();
        let mut description_matches = Vec::new();
        for piece in values {
            let single = do_score_item_fuzzy_single(label, description, path, piece, prefer_label_matches, allow);
            if single.score == 0 {
                return ItemScore::default(); // all pieces must match
            }
            total += single.score;
            label_matches.extend(single.label_match.unwrap_or_default());
            description_matches.extend(single.description_match.unwrap_or_default());
        }
        return ItemScore {
            score: total,
            label_match: Some(normalize_matches(label_matches)),
            description_match: Some(normalize_matches(description_matches)),
        };
    }
    do_score_item_fuzzy_single(label, description, path, &query.piece, prefer_label_matches, allow)
}

fn do_score_item_fuzzy_single(
    label: &str,
    description: Option<&str>,
    path: Option<&str>,
    query: &PreparedQueryPiece,
    prefer_label_matches: bool,
    allow: bool,
) -> ItemScore {
    let description = description.filter(|d| !d.is_empty());
    let allow = allow && !query.expect_contiguous_match;

    // Prefer label matches if told so or there is no description.
    if prefer_label_matches || description.is_none() {
        let (label_score, label_positions) = score_fuzzy(label, &query.normalized, &query.normalized_lowercase, allow);
        if label_score != 0 {
            // A prefix match on the label gets a much higher base score, so typing a file name wins
            // over results that have it somewhere in the middle; shorter labels win among prefixes.
            let label_prefix_match = matches_prefix(&query.normalized, label);
            let base = if label_prefix_match.is_some() {
                let boost = ((query.normalized.chars().count() as f64 / label.chars().count() as f64) * 100.0).round() as i32;
                LABEL_PREFIX_SCORE_THRESHOLD + boost
            } else {
                LABEL_SCORE_THRESHOLD
            };
            return ItemScore {
                score: base + label_score,
                label_match: Some(label_prefix_match.unwrap_or_else(|| create_matches(&label_positions))),
                description_match: None,
            };
        }
    }

    // Score description + label together.
    if let Some(description) = description {
        let mut prefix = description.to_string();
        if path.is_some() {
            prefix.push(SEP); // assume this is a file path
        }
        let prefix_len = prefix.chars().count();
        let both = format!("{prefix}{label}");
        let (score, positions) = score_fuzzy(&both, &query.normalized, &query.normalized_lowercase, allow);
        if score != 0 {
            let mut label_match = Vec::new();
            let mut description_match = Vec::new();
            for h in create_matches(&positions) {
                if h.start < prefix_len && h.end > prefix_len {
                    // The match overlaps label and description: split it.
                    label_match.push(Match::new(0, h.end - prefix_len));
                    description_match.push(Match::new(h.start, prefix_len));
                } else if h.start >= prefix_len {
                    label_match.push(Match::new(h.start - prefix_len, h.end - prefix_len));
                } else {
                    description_match.push(h);
                }
            }
            return ItemScore { score, label_match: Some(label_match), description_match: Some(description_match) };
        }
    }
    ItemScore::default()
}

// ---- comparers --------------------------------------------------------------------------------

fn ordering_to_i32(o: Ordering) -> i32 {
    match o {
        Ordering::Less => -1,
        Ordering::Equal => 0,
        Ordering::Greater => 1,
    }
}

/// `compareItemsByFuzzyScore`: negative when `a` ranks before `b`.
pub fn compare_items_by_fuzzy_score<T>(
    a: &T,
    b: &T,
    query: &PreparedQuery,
    allow_non_contiguous_matches: bool,
    accessor: &dyn ItemAccessor<T>,
    cache: &mut FuzzyScorerCache,
) -> i32 {
    let score_a = score_item_fuzzy(Some(a), query, allow_non_contiguous_matches, accessor, cache);
    let score_b = score_item_fuzzy(Some(b), query, allow_non_contiguous_matches, accessor, cache);
    compare_scored(a, b, &score_a, &score_b, query, accessor)
}

/// The comparison of two items whose scores are already known (lets callers sort without rescoring).
pub fn compare_scored<T>(a: &T, b: &T, item_a: &ItemScore, item_b: &ItemScore, query: &PreparedQuery, accessor: &dyn ItemAccessor<T>) -> i32 {
    let (sa, sb) = (item_a.score, item_b.score);

    // 1.) identity matches have the highest score
    if (sa == PATH_IDENTITY_SCORE || sb == PATH_IDENTITY_SCORE) && sa != sb {
        return if sa == PATH_IDENTITY_SCORE { -1 } else { 1 };
    }

    // 2.) matches on the label rank above label+description matches
    if sa > LABEL_SCORE_THRESHOLD || sb > LABEL_SCORE_THRESHOLD {
        if sa != sb {
            return if sa > sb { -1 } else { 1 };
        }
        // prefer more compact matches in the label (unless prefix matches: longer prefixes win)
        if sa < LABEL_PREFIX_SCORE_THRESHOLD && sb < LABEL_PREFIX_SCORE_THRESHOLD {
            let c = compare_by_match_length(item_a.label_match.as_deref(), item_b.label_match.as_deref());
            if c != 0 {
                return c;
            }
        }
        // prefer shorter labels
        let la = accessor.label(a).map_or(0, |l| l.chars().count());
        let lb = accessor.label(b).map_or(0, |l| l.chars().count());
        if la != lb {
            return la as i32 - lb as i32;
        }
    }

    // 3.) compare by score in label+description
    if sa != sb {
        return if sa > sb { -1 } else { 1 };
    }

    // 4.) identical scores: prefer matches in the label
    let has_a = item_a.label_match.as_ref().is_some_and(|m| !m.is_empty());
    let has_b = item_b.label_match.as_ref().is_some_and(|m| !m.is_empty());
    if has_a && !has_b {
        return -1;
    } else if has_b && !has_a {
        return 1;
    }

    // 5.) identical scores: prefer more compact matches (label and description)
    let da = label_and_description_match_distance(a, item_a, accessor);
    let db = label_and_description_match_distance(b, item_b, accessor);
    if da != 0 && db != 0 && da != db {
        return if db > da { -1 } else { 1 };
    }

    // 6.) fall back
    fallback_compare(a, b, query, accessor)
}

fn label_and_description_match_distance<T>(item: &T, score: &ItemScore, accessor: &dyn ItemAccessor<T>) -> i64 {
    let mut match_start: i64 = -1;
    let mut match_end: i64 = -1;
    let desc = score.description_match.as_deref().filter(|m| !m.is_empty());
    let label = score.label_match.as_deref().filter(|m| !m.is_empty());
    if let Some(d) = desc {
        match_start = d[0].start as i64;
    } else if let Some(l) = label {
        match_start = l[0].start as i64;
    }
    if let Some(l) = label {
        match_end = l[l.len() - 1].end as i64;
        if desc.is_some()
            && let Some(d) = accessor.description(item).filter(|d| !d.is_empty()) {
                match_end += d.chars().count() as i64;
            }
    } else if let Some(d) = desc {
        match_end = d[d.len() - 1].end as i64;
    }
    match_end - match_start
}

fn compare_by_match_length(a: Option<&[Match]>, b: Option<&[Match]>) -> i32 {
    let a = a.filter(|m| !m.is_empty());
    let b = b.filter(|m| !m.is_empty());
    match (a, b) {
        (None, None) => 0,
        (Some(_), None) => -1,
        (None, Some(_)) => 1,
        (Some(a), Some(b)) => {
            let la = a[a.len() - 1].end - a[0].start;
            let lb = b[b.len() - 1].end - b[0].start;
            if la == lb {
                0
            } else if lb < la {
                1
            } else {
                -1
            }
        }
    }
}

fn fallback_compare<T>(a: &T, b: &T, query: &PreparedQuery, accessor: &dyn ItemAccessor<T>) -> i32 {
    let label_a = accessor.label(a).unwrap_or_default();
    let label_b = accessor.label(b).unwrap_or_default();
    let desc_a = accessor.description(a).filter(|d| !d.is_empty());
    let desc_b = accessor.description(b).filter(|d| !d.is_empty());

    let len_a = label_a.chars().count() + desc_a.as_ref().map_or(0, |d| d.chars().count());
    let len_b = label_b.chars().count() + desc_b.as_ref().map_or(0, |d| d.chars().count());
    if len_a != len_b {
        return len_a as i32 - len_b as i32;
    }

    let path_a = non_empty(accessor.path(a));
    let path_b = non_empty(accessor.path(b));
    if let (Some(pa), Some(pb)) = (&path_a, &path_b) {
        let (la, lb) = (pa.chars().count(), pb.chars().count());
        if la != lb {
            return la as i32 - lb as i32;
        }
    }

    if label_a != label_b {
        return compare_anything(&label_a, &label_b, &query.normalized);
    }
    if let (Some(da), Some(db)) = (&desc_a, &desc_b)
        && da != db {
            return compare_anything(da, db, &query.normalized);
        }
    if let (Some(pa), Some(pb)) = (&path_a, &path_b)
        && pa != pb {
            return compare_anything(pa, pb, &query.normalized);
        }
    0
}

/// Natural comparison (digit runs by value), ignoring case: stands in for the numeric,
/// base-sensitivity `Intl.Collator` of comparers.ts.
pub fn collate(a: &str, b: &str) -> Ordering {
    let (a, b) = (lower_chars(&chars(a)), lower_chars(&chars(b)));
    let (mut i, mut j) = (0, 0);
    while i < a.len() && j < b.len() {
        if a[i].is_ascii_digit() && b[j].is_ascii_digit() {
            let si = i;
            while i < a.len() && a[i].is_ascii_digit() {
                i += 1;
            }
            let sj = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            let na: String = a[si..i].iter().collect::<String>().trim_start_matches('0').to_string();
            let nb: String = b[sj..j].iter().collect::<String>().trim_start_matches('0').to_string();
            let c = na.len().cmp(&nb.len()).then_with(|| na.cmp(&nb));
            if c != Ordering::Equal {
                return c;
            }
        } else {
            if a[i] != b[j] {
                return a[i].cmp(&b[j]);
            }
            i += 1;
            j += 1;
        }
    }
    (a.len() - i).cmp(&(b.len() - j))
}

/// `compareFileNames`.
pub fn compare_file_names(one: &str, other: &str) -> i32 {
    let r = collate(one, other);
    // numeric collation makes `foo1` and `foo01` equal: disambiguate
    if r == Ordering::Equal && one != other {
        return if one < other { -1 } else { 1 };
    }
    ordering_to_i32(r)
}

/// `compareByPrefix`.
pub fn compare_by_prefix(one: &str, other: &str, look_for: &str) -> i32 {
    let a = one.to_lowercase();
    let b = other.to_lowercase();
    let (pa, pb) = (a.starts_with(look_for), b.starts_with(look_for));
    if pa != pb {
        return if pa { -1 } else { 1 };
    } else if pa && pb {
        match a.chars().count().cmp(&b.chars().count()) {
            Ordering::Less => return -1,
            Ordering::Greater => return 1,
            Ordering::Equal => {}
        }
    }
    0
}

/// `compareAnything`.
pub fn compare_anything(one: &str, other: &str, look_for: &str) -> i32 {
    let a = one.to_lowercase();
    let b = other.to_lowercase();
    let prefix = compare_by_prefix(one, other, look_for);
    if prefix != 0 {
        return prefix;
    }
    let (sa, sb) = (a.ends_with(look_for), b.ends_with(look_for));
    if sa != sb {
        return if sa { -1 } else { 1 };
    }
    let r = compare_file_names(&a, &b);
    if r != 0 {
        return r;
    }
    ordering_to_i32(a.cmp(&b))
}

// ---- query normalizer -------------------------------------------------------------------------

/// `IPreparedQueryPiece`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedQueryPiece {
    /// The query as typed.
    pub original: String,
    pub original_lowercase: String,
    /// The original with `\` turned into `/`.
    pub path_normalized: String,
    /// The path normalized query without whitespace, wildcards, quotes, ellipsis and a trailing `#`.
    pub normalized: String,
    pub normalized_lowercase: String,
    /// The query is wrapped in quotes: it must be a substring (no fuzzy matching).
    pub expect_contiguous_match: bool,
}

/// `IPreparedQuery`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedQuery {
    pub piece: PreparedQueryPiece,
    /// The query split by spaces into pieces (only when there are at least two non-empty words).
    pub values: Option<Vec<PreparedQueryPiece>>,
    pub contains_path_separator: bool,
}

impl Deref for PreparedQuery {
    type Target = PreparedQueryPiece;
    fn deref(&self) -> &PreparedQueryPiece {
        &self.piece
    }
}

fn query_expects_exact_match(q: &str) -> bool {
    q.starts_with('"') && q.ends_with('"')
}

struct Normalized {
    path_normalized: String,
    normalized: String,
    normalized_lowercase: String,
}

fn normalize_query(original: &str) -> Normalized {
    let path_normalized = original.replace('\\', "/");
    // Remove quotes, wildcards, whitespace and ellipsis, then a trailing `#` (when it is not the only char).
    let mut normalized: String = path_normalized.chars().filter(|&c| !(c == '*' || c == '\u{2026}' || c == '"' || c.is_whitespace())).collect();
    if normalized.chars().count() > 1 && normalized.ends_with('#') {
        normalized.pop();
    }
    let normalized_lowercase = normalized.to_lowercase();
    Normalized { path_normalized, normalized, normalized_lowercase }
}

/// `prepareQuery`.
pub fn prepare_query(original: &str) -> PreparedQuery {
    let n = normalize_query(original);
    let contains_path_separator = n.path_normalized.contains(SEP);
    let mut values: Option<Vec<PreparedQueryPiece>> = None;
    let split: Vec<&str> = original.split(' ').collect();
    if split.len() > 1 {
        for piece in split {
            let pn = normalize_query(piece);
            if !pn.normalized.is_empty() {
                values.get_or_insert_with(Vec::new).push(PreparedQueryPiece {
                    original: piece.to_string(),
                    original_lowercase: piece.to_lowercase(),
                    path_normalized: pn.path_normalized,
                    normalized: pn.normalized,
                    normalized_lowercase: pn.normalized_lowercase,
                    expect_contiguous_match: query_expects_exact_match(piece),
                });
            }
        }
    }
    PreparedQuery {
        piece: PreparedQueryPiece {
            original: original.to_string(),
            original_lowercase: original.to_lowercase(),
            path_normalized: n.path_normalized,
            normalized: n.normalized,
            normalized_lowercase: n.normalized_lowercase,
            expect_contiguous_match: query_expects_exact_match(original),
        },
        values,
        contains_path_separator,
    }
}

/// `pieceToQuery` for one piece.
pub fn piece_to_query(piece: &PreparedQueryPiece) -> PreparedQuery {
    prepare_query(&piece.original)
}

/// `pieceToQuery` for several pieces.
pub fn pieces_to_query(pieces: &[PreparedQueryPiece]) -> PreparedQuery {
    prepare_query(&pieces.iter().map(|p| p.original.as_str()).collect::<Vec<_>>().join(" "))
}

/// Convert char-index matches to byte ranges of `text` (for rendering).
pub fn match_byte_ranges(text: &str, matches: &[Match]) -> Vec<std::ops::Range<usize>> {
    let offsets: Vec<usize> = text.char_indices().map(|(i, _)| i).chain(std::iter::once(text.len())).collect();
    matches
        .iter()
        .filter_map(|m| {
            let (s, e) = (*offsets.get(m.start)?, *offsets.get(m.end.min(offsets.len() - 1))?);
            (s < e).then_some(s..e)
        })
        .collect()
}
