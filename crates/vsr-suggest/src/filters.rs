//! upstream: src/vs/base/common/filters.ts @ 50e1350fc78d
//!
//! Fuzzy matching for suggestions: `fuzzyScore`, its graceful variants, `anyScore` and
//! `createMatches`.
//!
//! Upstream source: src/vs/base/common/filters.ts (the fuzzyScore section).
//!
//! Strings are indexed as UTF-16 code units, like the TypeScript original, so scores and match
//! positions agree with upstream for non-BMP text. The `*_low` arguments are the lowercased
//! pattern and word, passed in by the caller as upstream does. When a lowercased string has a
//! different UTF-16 length than its original (a few Unicode case mappings do), the original is
//! lowercased per code unit instead, so the scorer never indexes out of range.

use std::ops::Range;

/// Patterns and words are cut to this many UTF-16 units before scoring (upstream `_maxLen`).
const MAX_LEN: usize = 128;

/// Sentinel for "this cell cannot be reached" (upstream `Number.MIN_SAFE_INTEGER`).
const UNREACHABLE: i64 = -(2 << 52) + 1;

/// Options of `fuzzyScore` (upstream `FuzzyScoreOptions`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FuzzyScoreOptions {
    /// Accept a first pattern character that is not at a word start, or a separator, as a match.
    pub first_match_can_be_weak: bool,
    /// Add a bonus when the pattern matches the whole word.
    pub boost_full_match: bool,
}

impl Default for FuzzyScoreOptions {
    /// Upstream `FuzzyScoreOptions.default`.
    fn default() -> Self {
        Self {
            first_match_can_be_weak: false,
            boost_full_match: true,
        }
    }
}

/// A fuzzy match (upstream `FuzzyScore`, `[score, wordStart, ...matches]`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FuzzyScore {
    pub score: i64,
    /// Where scoring started in the word.
    pub word_start: usize,
    /// Matched positions relative to `word_start`, last pattern character first.
    pub matches: Vec<usize>,
}

impl FuzzyScore {
    /// Upstream `FuzzyScore.Default`: no match, score -100.
    pub fn default_score() -> Self {
        Self {
            score: -100,
            word_start: 0,
            matches: Vec::new(),
        }
    }

    /// Upstream `FuzzyScore.isDefault`.
    pub fn is_default(&self) -> bool {
        self.score == -100 && self.word_start == 0 && self.matches.is_empty()
    }
}

/// The signature of `fuzzyScore` and its graceful variants (upstream `FuzzyScorer`).
pub type FuzzyScorer =
    fn(&str, &str, usize, &str, &str, usize, FuzzyScoreOptions) -> Option<FuzzyScore>;

/// A highlighted range of the word, in UTF-16 units (upstream `IMatch`).
pub type Match = Range<usize>;

fn units(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

/// Lowercased units of `original`, taken from `lowered` when the lengths agree.
fn low_units(original: &[u16], lowered: &str) -> Vec<u16> {
    let low = units(lowered);
    if low.len() == original.len() {
        return low;
    }
    original
        .iter()
        .map(|&u| match char::from_u32(u as u32) {
            Some(c) if c.is_ascii() => c.to_ascii_lowercase() as u16,
            _ => u,
        })
        .collect()
}

/// Upstream `fuzzyScore`: scores `word` against `pattern` from `pattern_start` and `word_start`.
///
/// `pattern_low` and `word_low` are the lowercased strings. Returns `None` when the pattern does
/// not match, or when the match is weak and `first_match_can_be_weak` is off.
pub fn fuzzy_score(
    pattern: &str,
    pattern_low: &str,
    pattern_start: usize,
    word: &str,
    word_low: &str,
    word_start: usize,
    options: FuzzyScoreOptions,
) -> Option<FuzzyScore> {
    let p = units(pattern);
    let pl = low_units(&p, pattern_low);
    let w = units(word);
    let wl = low_units(&w, word_low);
    fuzzy_score_units(&p, &pl, pattern_start, &w, &wl, word_start, options)
}

/// `fuzzyScore` on code units; the algorithm is upstream's, line for line.
fn fuzzy_score_units(
    pattern: &[u16],
    pattern_low: &[u16],
    pattern_start: usize,
    word: &[u16],
    word_low: &[u16],
    word_start: usize,
    options: FuzzyScoreOptions,
) -> Option<FuzzyScore> {
    let pattern_len = pattern.len().min(MAX_LEN);
    let word_len = word.len().min(MAX_LEN);

    if pattern_start >= pattern_len
        || word_start >= word_len
        || (pattern_len - pattern_start) > (word_len - word_start)
    {
        return None;
    }

    // Cheap check first: the pattern characters must occur in order. This also records the
    // earliest word position of each pattern character.
    let mut min_pos = vec![0usize; pattern_len];
    if !is_pattern_in_word_fill(
        pattern_low,
        pattern_start,
        pattern_len,
        word_low,
        word_start,
        word_len,
        Some(&mut min_pos),
    ) {
        return None;
    }
    // The latest word position of each pattern character.
    let mut max_pos = vec![0usize; pattern_len];
    fill_max_word_match_pos(
        pattern_len,
        word_len,
        pattern_start,
        word_start,
        pattern_low,
        word_low,
        &mut max_pos,
    );

    let rows = pattern_len - pattern_start + 2;
    let cols = word_len - word_start + 2;
    let mut tables = Tables::new(rows, cols);
    let mut has_strong_first_match = false;

    // The DP fill. `row` and `column` end where upstream's end (they are read after the loop).
    let mut row: usize = 1;
    let mut column: usize = 1;
    let mut pattern_pos = pattern_start;
    while pattern_pos < pattern_len {
        let min_word_match_pos = min_pos[pattern_pos];
        let max_word_match_pos = max_pos[pattern_pos];
        let next_max_word_match_pos = if pattern_pos + 1 < pattern_len {
            max_pos[pattern_pos + 1]
        } else {
            word_len
        };

        column = min_word_match_pos - word_start + 1;
        let mut word_pos = min_word_match_pos;
        while word_pos < next_max_word_match_pos {
            let mut score = UNREACHABLE;
            let mut can_come_diag = false;

            if word_pos <= max_word_match_pos {
                let new_match_start = tables.diag(row - 1, column - 1) == 0;
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
                    new_match_start,
                    &mut has_strong_first_match,
                );
            }

            let mut diag_score = 0;
            if score != UNREACHABLE {
                can_come_diag = true;
                diag_score = score + tables.table(row - 1, column - 1);
            }

            let can_come_left = word_pos > min_word_match_pos;
            let left_score = if can_come_left {
                tables.table(row, column - 1)
                    + if tables.diag(row, column - 1) > 0 {
                        -5
                    } else {
                        0
                    }
            } else {
                0
            };

            let can_come_left_left =
                word_pos > min_word_match_pos + 1 && tables.diag(row, column - 1) > 0;
            let left_left_score = if can_come_left_left {
                tables.table(row, column - 2)
                    + if tables.diag(row, column - 2) > 0 {
                        -5
                    } else {
                        0
                    }
            } else {
                0
            };

            if can_come_left_left
                && (!can_come_left || left_left_score >= left_score)
                && (!can_come_diag || left_left_score >= diag_score)
            {
                // Prefer skipping two over a diagonal: the match then starts earlier in the word.
                tables.set(row, column, left_left_score, Arrow::LeftLeft, 0);
            } else if can_come_left && (!can_come_diag || left_score >= diag_score) {
                tables.set(row, column, left_score, Arrow::Left, 0);
            } else if can_come_diag {
                let next_diag = tables.diag(row - 1, column - 1) + 1;
                tables.set(row, column, diag_score, Arrow::Diag, next_diag);
            } else {
                // Upstream throws "not possible" here; no match is reported instead.
                return None;
            }

            column += 1;
            word_pos += 1;
        }
        row += 1;
        pattern_pos += 1;
    }

    if !has_strong_first_match && !options.first_match_can_be_weak {
        return None;
    }

    row -= 1;
    column -= 1;

    let mut result_score = tables.table(row, column);
    let mut matches: Vec<usize> = Vec::new();

    let mut backwards_diag_length: i64 = 0;
    let mut max_match_column: i64 = 0;
    let mut column = column as i64;

    while row >= 1 {
        // Find the column where the match goes diagonally up.
        let mut diag_column = column;
        loop {
            let arrow = tables.arrow(row, diag_column);
            if arrow == Arrow::LeftLeft {
                diag_column -= 2;
            } else if arrow == Arrow::Left {
                diag_column -= 1;
            } else {
                break;
            }
            if diag_column < 1 {
                break;
            }
        }

        // Overturn the forwards decision when keeping the backwards diagonal scores better.
        let pattern_char = pattern_low.get(pattern_start + row - 1).copied();
        let word_char = low_at(word_low, word_start as i64 + column - 1);
        if backwards_diag_length > 1
            && pattern_char == word_char
            && !is_upper_case_at_pos(diag_column + word_start as i64 - 1, word, word_low)
            && backwards_diag_length + 1 > tables.diag_signed(row, diag_column)
        {
            diag_column = column;
        }

        if diag_column == column {
            // A contiguous match.
            backwards_diag_length += 1;
        } else {
            backwards_diag_length = 1;
        }

        if max_match_column == 0 {
            // Remember the last matched column.
            max_match_column = diag_column;
        }

        row -= 1;
        column = diag_column - 1;
        matches.push(column.max(0) as usize);
    }

    if word_len - word_start == pattern_len && options.boost_full_match {
        // The word matches the pattern with every character: a total match boost.
        result_score += 2;
    }

    // One penalty for each skipped character in the word.
    let skipped_chars_count = max_match_column - pattern_len as i64;
    result_score -= skipped_chars_count;

    Some(FuzzyScore {
        score: result_score,
        word_start,
        matches,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Arrow {
    None,
    Diag,
    Left,
    LeftLeft,
}

/// The three DP tables of `fuzzyScore`: best score, length of the diagonal run, and the arrow
/// that led to each cell.
struct Tables {
    cols: usize,
    table: Vec<i64>,
    diag: Vec<i64>,
    arrows: Vec<Arrow>,
}

impl Tables {
    fn new(rows: usize, cols: usize) -> Self {
        Self {
            cols,
            table: vec![0; rows * cols],
            diag: vec![0; rows * cols],
            arrows: vec![Arrow::None; rows * cols],
        }
    }

    fn index(&self, row: usize, column: usize) -> usize {
        row * self.cols + column
    }

    fn table(&self, row: usize, column: usize) -> i64 {
        self.table[self.index(row, column)]
    }

    fn diag(&self, row: usize, column: usize) -> i64 {
        self.diag[self.index(row, column)]
    }

    fn diag_signed(&self, row: usize, column: i64) -> i64 {
        if column < 0 {
            0
        } else {
            self.diag(row, column as usize)
        }
    }

    fn arrow(&self, row: usize, column: i64) -> Arrow {
        if column < 0 {
            Arrow::None
        } else {
            self.arrows[self.index(row, column as usize)]
        }
    }

    fn set(&mut self, row: usize, column: usize, score: i64, arrow: Arrow, diag: i64) {
        let i = self.index(row, column);
        self.table[i] = score;
        self.arrows[i] = arrow;
        self.diag[i] = diag;
    }
}

/// Upstream `isPatternInWord` with `fillMinWordPosArr`: checks that the pattern occurs in order,
/// recording the first word position of each pattern character when `min_pos` is given.
fn is_pattern_in_word_fill(
    pattern_low: &[u16],
    mut pattern_pos: usize,
    pattern_len: usize,
    word_low: &[u16],
    mut word_pos: usize,
    word_len: usize,
    mut min_pos: Option<&mut Vec<usize>>,
) -> bool {
    while pattern_pos < pattern_len && word_pos < word_len {
        if pattern_low[pattern_pos] == word_low[word_pos] {
            if let Some(min) = min_pos.as_deref_mut() {
                min[pattern_pos] = word_pos;
            }
            pattern_pos += 1;
        }
        word_pos += 1;
    }
    pattern_pos == pattern_len
}

/// Upstream `_fillInMaxWordMatchPos`: the latest word position of each pattern character, found
/// by matching the pattern backwards from its end.
fn fill_max_word_match_pos(
    pattern_len: usize,
    word_len: usize,
    pattern_start: usize,
    word_start: usize,
    pattern_low: &[u16],
    word_low: &[u16],
    max_pos: &mut [usize],
) {
    let mut pattern_pos = pattern_len as i64 - 1;
    let mut word_pos = word_len as i64 - 1;
    while pattern_pos >= pattern_start as i64 && word_pos >= word_start as i64 {
        if pattern_low[pattern_pos as usize] == word_low[word_pos as usize] {
            max_pos[pattern_pos as usize] = word_pos as usize;
            pattern_pos -= 1;
        }
        word_pos -= 1;
    }
}

/// Upstream `_doScore`: the score of matching `pattern[pattern_pos]` at `word[word_pos]`.
#[allow(clippy::too_many_arguments)]
fn do_score(
    pattern: &[u16],
    pattern_low: &[u16],
    pattern_pos: usize,
    pattern_start: usize,
    word: &[u16],
    word_low: &[u16],
    word_pos: usize,
    word_len: usize,
    word_start: usize,
    new_match_start: bool,
    first_match_strong: &mut bool,
) -> i64 {
    if pattern_low.get(pattern_pos) != word_low.get(word_pos) {
        return UNREACHABLE;
    }

    let wp = word_pos as i64;
    let pattern_char = pattern.get(pattern_pos).copied();
    let word_char = word.get(word_pos).copied();

    let mut score: i64 = 1;
    let mut is_gap_location = false;
    if word_pos == pattern_pos - pattern_start {
        // A common prefix: `foobar <-> foobaz`.
        score = if pattern_char == word_char { 7 } else { 5 };
    } else if is_upper_case_at_pos(wp, word, word_low)
        && (wp == 0 || !is_upper_case_at_pos(wp - 1, word, word_low))
    {
        // Hitting an upper-case letter: `foo <-> forOthers`.
        score = if pattern_char == word_char { 7 } else { 5 };
        is_gap_location = true;
    } else if is_separator_at(word_low, wp) && (wp == 0 || !is_separator_at(word_low, wp - 1)) {
        // Hitting a separator: `. <-> foo.bar`.
        score = 5;
    } else if is_separator_at(word_low, wp - 1) || is_whitespace_at(word_low, wp - 1) {
        // After a separator: `foo <-> bar_foo`.
        score = 5;
        is_gap_location = true;
    }

    if score > 1 && pattern_pos == pattern_start {
        *first_match_strong = true;
    }

    if !is_gap_location {
        is_gap_location = is_upper_case_at_pos(wp, word, word_low)
            || is_separator_at(word_low, wp - 1)
            || is_whitespace_at(word_low, wp - 1);
    }

    if pattern_pos == pattern_start {
        // The first pattern character matches a word character that is not at the word start:
        // a penalty for the gap before this match.
        if word_pos > word_start {
            score -= if is_gap_location { 3 } else { 5 };
        }
    } else if new_match_start {
        // The beginning of a new match, so there is a gap before this location.
        score += if is_gap_location { 2 } else { 0 };
    } else {
        // Part of a contiguous match: a slight bonus unless this is a preferred gap location.
        score += if is_gap_location { 0 } else { 1 };
    }

    if word_pos + 1 == word_len {
        // Gaps are always penalised. Pretend there is a gap after the last character so that a
        // match on the last character gets no unfair advantage.
        score -= if is_gap_location { 3 } else { 5 };
    }

    score
}

/// `value[index]` for a signed index; `None` outside the string (as `undefined` in upstream).
fn low_at(value: &[u16], index: i64) -> Option<u16> {
    if index < 0 {
        None
    } else {
        value.get(index as usize).copied()
    }
}

/// Upstream `isUpperCaseAtPos`: a character differs from its lowercase form. Positions outside
/// the word are not upper case.
fn is_upper_case_at_pos(pos: i64, word: &[u16], word_low: &[u16]) -> bool {
    if pos < 0 {
        return false;
    }
    let pos = pos as usize;
    word.get(pos) != word_low.get(pos)
}

/// Upstream `isSeparatorAtPos`: punctuation that separates words, or an imprecisely detected
/// emoji. Reads the code point at `index`, so a surrogate pair counts as one character.
fn is_separator_at(value: &[u16], index: i64) -> bool {
    if index < 0 || index as usize >= value.len() {
        return false;
    }
    match code_point_at(value, index as usize) {
        0x5F  // _
        | 0x2D // -
        | 0x2E // .
        | 0x20 // space
        | 0x2F // /
        | 0x5C // backslash
        | 0x27 // '
        | 0x22 // "
        | 0x3A // :
        | 0x24 // $
        | 0x3C // <
        | 0x3E // >
        | 0x28 // (
        | 0x29 // )
        | 0x5B // [
        | 0x5D // ]
        | 0x7B // {
        | 0x7D // }
        => true,
        code => is_emoji_imprecise(code),
    }
}

/// Upstream `isWhitespaceAtPos`.
fn is_whitespace_at(value: &[u16], index: i64) -> bool {
    if index < 0 {
        return false;
    }
    matches!(value.get(index as usize), Some(&0x20) | Some(&0x09))
}

/// `codePointAt`: the code point starting at `index`, combining a surrogate pair.
fn code_point_at(value: &[u16], index: usize) -> u32 {
    let high = value[index] as u32;
    if (0xD800..0xDC00).contains(&high)
        && let Some(&low) = value.get(index + 1)
        && (0xDC00..0xE000).contains(&(low as u32))
    {
        return 0x10000 + ((high - 0xD800) << 10) + (low as u32 - 0xDC00);
    }
    high
}

/// Upstream `strings.isEmojiImprecise`: the emoji ranges, not an exact emoji grammar.
pub fn is_emoji_imprecise(x: u32) -> bool {
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

/// Upstream `fuzzyScoreGracefulAggressive`: like `fuzzyScore`, but also tries typo permutations
/// of the pattern and keeps the best alignment.
pub fn fuzzy_score_graceful_aggressive(
    pattern: &str,
    pattern_low: &str,
    pattern_pos: usize,
    word: &str,
    word_low: &str,
    word_pos: usize,
    options: FuzzyScoreOptions,
) -> Option<FuzzyScore> {
    fuzzy_score_with_permutations(
        pattern,
        pattern_low,
        pattern_pos,
        word,
        word_low,
        word_pos,
        true,
        options,
    )
}

/// Upstream `fuzzyScoreGraceful`: tries typo permutations only when the pattern does not match.
pub fn fuzzy_score_graceful(
    pattern: &str,
    pattern_low: &str,
    pattern_pos: usize,
    word: &str,
    word_low: &str,
    word_pos: usize,
    options: FuzzyScoreOptions,
) -> Option<FuzzyScore> {
    fuzzy_score_with_permutations(
        pattern,
        pattern_low,
        pattern_pos,
        word,
        word_low,
        word_pos,
        false,
        options,
    )
}

#[allow(clippy::too_many_arguments)]
fn fuzzy_score_with_permutations(
    pattern: &str,
    pattern_low: &str,
    pattern_pos: usize,
    word: &str,
    word_low: &str,
    word_pos: usize,
    aggressive: bool,
    options: FuzzyScoreOptions,
) -> Option<FuzzyScore> {
    let mut top = fuzzy_score(
        pattern,
        pattern_low,
        pattern_pos,
        word,
        word_low,
        word_pos,
        options,
    );

    if top.is_some() && !aggressive {
        // The original pattern matched and we are not aggressive: keep it.
        return top;
    }

    let pattern_units = units(pattern);
    if pattern_units.len() >= 3 {
        // Try up to seven permutations that swap neighbouring characters, e.g. `cnoso` becomes
        // `conso`, `cnsoo` or `cnoos`.
        let tries = 7.min(pattern_units.len() - 1);
        for moving_pattern_pos in (pattern_pos + 1)..tries {
            let Some(new_pattern) = next_typo_permutation(&pattern_units, moving_pattern_pos)
            else {
                continue;
            };
            let new_pattern_low = new_pattern.to_lowercase();
            if let Some(mut candidate) = fuzzy_score(
                &new_pattern,
                &new_pattern_low,
                pattern_pos,
                word,
                word_low,
                word_pos,
                options,
            ) {
                candidate.score -= 3; // permutation penalty
                let better = match &top {
                    None => true,
                    Some(current) => candidate.score > current.score,
                };
                if better {
                    top = Some(candidate);
                }
            }
        }
    }

    top
}

/// Upstream `nextTypoPermutation`: the pattern with the units at `pattern_pos` and the next one
/// swapped. `None` when the swap is out of range or swaps two equal units.
fn next_typo_permutation(pattern: &[u16], pattern_pos: usize) -> Option<String> {
    if pattern_pos + 1 >= pattern.len() {
        return None;
    }
    let swap1 = pattern[pattern_pos];
    let swap2 = pattern[pattern_pos + 1];
    if swap1 == swap2 {
        return None;
    }
    let mut swapped = pattern.to_vec();
    swapped[pattern_pos] = swap2;
    swapped[pattern_pos + 1] = swap1;
    Some(String::from_utf16_lossy(&swapped))
}

/// Upstream `anyScore`: like `fuzzyScore` with weak first matches allowed, trying the first
/// pattern positions up to 13 in turn. Never fails: without a match it returns score 0 with no
/// highlights.
pub fn any_score(
    pattern: &str,
    pattern_low: &str,
    pattern_pos: usize,
    word: &str,
    word_low: &str,
    word_pos: usize,
) -> FuzzyScore {
    let max = 13.min(units(pattern).len());
    let options = FuzzyScoreOptions {
        first_match_can_be_weak: true,
        boost_full_match: true,
    };
    for pos in pattern_pos..max {
        if let Some(result) =
            fuzzy_score(pattern, pattern_low, pos, word, word_low, word_pos, options)
        {
            return result;
        }
    }
    FuzzyScore {
        score: 0,
        word_start: word_pos,
        matches: Vec::new(),
    }
}

/// Upstream `createMatches`: the highlighted UTF-16 ranges of a score, contiguous positions
/// merged. An absent score highlights nothing.
pub fn create_matches(score: Option<&FuzzyScore>) -> Vec<Match> {
    let Some(score) = score else {
        return Vec::new();
    };
    let mut result: Vec<Match> = Vec::new();
    for &position in score.matches.iter().rev() {
        let pos = position + score.word_start;
        match result.last_mut() {
            Some(last) if last.end == pos => last.end = pos + 1,
            _ => result.push(pos..pos + 1),
        }
    }
    result
}
