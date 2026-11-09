//! The word filters the Command Palette matches labels with.
//!
//! upstream: src/vs/base/common/filters.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/normalization.ts @ 50e1350fc78d
//!
//! `matchesContiguousSubString`, `matchesBaseContiguousSubString`, `matchesWords` and `or`.
//! Positions are char indexes. Differences: `tryNormalizeToBase` strips the accents of the
//! precomposed Latin letters of Latin-1 and Latin Extended-A (a table, not full NFD), and
//! `matchesWords` has no Korean alternate-character support.

use std::collections::HashMap;

use crate::fuzzy::Match;

/// `IFilter`.
pub type Filter<'a> = Box<dyn Fn(&str, &str) -> Option<Vec<Match>> + 'a>;

/// `or`: the first filter that matches defines the result.
pub fn or<'a>(filters: Vec<Filter<'a>>) -> impl Fn(&str, &str) -> Option<Vec<Match>> + 'a {
    move |word, target| filters.iter().find_map(|f| f(word, target))
}

fn lower(c: char) -> char {
    let mut it = c.to_lowercase();
    match (it.next(), it.next()) {
        (Some(l), None) => l,
        _ => c,
    }
}

fn find(hay: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }
    (0..=hay.len() - needle.len()).find(|&i| hay[i..i + needle.len()] == *needle)
}

/// `matchesContiguousSubString`: case-insensitive substring.
pub fn matches_contiguous_sub_string(word: &str, word_to_match_against: &str) -> Option<Vec<Match>> {
    let w: Vec<char> = word.chars().map(lower).collect();
    let t: Vec<char> = word_to_match_against.chars().map(lower).collect();
    if w.len() > t.len() {
        return None;
    }
    let i = find(&t, &w)?;
    Some(vec![Match::new(i, i + w.len())])
}

/// `matchesBaseContiguousSubString`: like the above, ignoring accents.
pub fn matches_base_contiguous_sub_string(word: &str, word_to_match_against: &str) -> Option<Vec<Match>> {
    let w: Vec<char> = word.chars().collect();
    let t: Vec<char> = word_to_match_against.chars().collect();
    if w.len() > t.len() {
        return None;
    }
    let w = try_normalize_to_base(word);
    let t = try_normalize_to_base(word_to_match_against);
    let i = find(&t, &w)?;
    Some(vec![Match::new(i, i + w.len())])
}

const ACCENTED: &[(&str, char)] = &[
    ("ÀÁÂÃÄÅĀĂĄ", 'a'),
    ("àáâãäåāăą", 'a'),
    ("ÇĆĈĊČ", 'c'),
    ("çćĉċč", 'c'),
    ("Ďď", 'd'),
    ("ÈÉÊËĒĔĖĘĚ", 'e'),
    ("èéêëēĕėęě", 'e'),
    ("ĜĞĠĢ", 'g'),
    ("ĝğġģ", 'g'),
    ("Ĥĥ", 'h'),
    ("ÌÍÎÏĨĪĬĮİ", 'i'),
    ("ìíîïĩīĭį", 'i'),
    ("Ĵĵ", 'j'),
    ("Ķķ", 'k'),
    ("ĹĻĽĺļľ", 'l'),
    ("ÑŃŅŇ", 'n'),
    ("ñńņň", 'n'),
    ("ÒÓÔÕÖŌŎŐ", 'o'),
    ("òóôõöōŏő", 'o'),
    ("ŔŖŘŕŗř", 'r'),
    ("ŚŜŞŠ", 's'),
    ("śŝşš", 's'),
    ("ŢŤţť", 't'),
    ("ÙÚÛÜŨŪŬŮŰŲ", 'u'),
    ("ùúûüũūŭůűų", 'u'),
    ("Ŵŵ", 'w'),
    ("ÝŶŸ", 'y'),
    ("ýÿŷ", 'y'),
    ("ŹŻŽźżž", 'z'),
];

/// `tryNormalizeToBase`: accents removed, lower case.
pub fn try_normalize_to_base(s: &str) -> Vec<char> {
    s.chars()
        .map(|c| match ACCENTED.iter().find(|(set, _)| set.contains(c)) {
            Some((_, base)) => *base,
            None => lower(c),
        })
        .collect()
}

// ---- matchesWords ---------------------------------------------------------------------------------

fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// `isWordSeparator`.
fn is_word_separator(c: char) -> bool {
    is_whitespace(c) || "()[]{}<>`'\"-/;:,.?!".contains(c)
}

fn characters_match(a: char, b: char) -> bool {
    a == b || (is_word_separator(a) && is_word_separator(b))
}

fn next_word(word: &[char], start: usize) -> usize {
    (start..word.len())
        .find(|&i| is_word_separator(word[i]) || (i > 0 && is_word_separator(word[i - 1])))
        .unwrap_or(word.len())
}

fn join(head: Match, mut tail: Vec<Match>) -> Vec<Match> {
    if tail.is_empty() {
        tail = vec![head];
    } else if head.end == tail[0].start {
        tail[0].start = head.start;
    } else {
        tail.insert(0, head);
    }
    tail
}

type Memo = HashMap<usize, Option<Vec<Match>>>;

/// `matchesWords`: the beginnings of the words of `word` against the beginnings of the words of
/// the target, so `gp` matches `Git: Pull`. With `contiguous`, `pul` matches `Git: Pull` but `gp`
/// does not.
pub fn matches_words(word: &str, target: &str, contiguous: bool) -> Option<Vec<Match>> {
    if target.is_empty() {
        return None;
    }
    let word = try_normalize_to_base(word);
    let target = try_normalize_to_base(target);
    let mut memo = Memo::new();
    let mut target_index = 0;
    while target_index < target.len() {
        let r = words_at(&word, &target, 0, target_index, contiguous, &mut memo);
        if r.is_some() {
            return r;
        }
        target_index = next_word(&target, target_index + 1);
    }
    None
}

fn words_at(word: &[char], target: &[char], wi: usize, ti: usize, contiguous: bool, memo: &mut Memo) -> Option<Vec<Match>> {
    if wi == word.len() {
        return Some(Vec::new());
    }
    if ti == target.len() {
        return None;
    }
    // Separators are one equivalence class, so without memoisation this recursion is exponential.
    let key = wi * (target.len() + 1) + ti;
    if let Some(cached) = memo.get(&key) {
        return cached.clone();
    }
    let computed = words_compute(word, target, wi, ti, contiguous, memo);
    memo.insert(key, computed.clone());
    computed
}

fn words_compute(word: &[char], target: &[char], wi: usize, ti: usize, contiguous: bool, memo: &mut Memo) -> Option<Vec<Match>> {
    if !characters_match(word[wi], target[ti]) {
        return None;
    }
    let mut next_word_index = ti + 1;
    let mut result = words_at(word, target, wi + 1, next_word_index, contiguous, memo);
    if !contiguous {
        while result.is_none() {
            next_word_index = next_word(target, next_word_index);
            if next_word_index >= target.len() {
                break;
            }
            result = words_at(word, target, wi + 1, next_word_index, contiguous, memo);
            next_word_index += 1;
        }
    }
    let result = result?;
    // The characters are only equivalent separators: keep the rest of the match but not this one.
    if word[wi] != target[ti] {
        return Some(result);
    }
    Some(join(Match::new(ti, ti + 1), result))
}
