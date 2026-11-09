//! Regex sources (anchor variants, back references) and the Oniguruma scanner.
//!
//! upstream: src/vs/workbench/services/textMate/common/TMGrammarFactory.ts @ 50e1350fc78d
//! upstream: vscode-textmate src/rule.ts (`RegExpSource`, `RegExpSourceList`) and
//! vscode-oniguruma's `OnigScanner` (microsoft/vscode-textmate, main, 2026-11-09)
//!
//! Matching is vscode-oniguruma's: UTF-8, `ONIG_OPTION_CAPTURE_GROUP`, the default Oniguruma
//! syntax, and a search always runs on the whole line with a start offset (so `\G`, `\A` and
//! look-behinds see the text before the start).

use onig::{Region, Regex, RegexOptions, SearchOptions, Syntax};

use crate::rules::RuleId;

/// Matches nothing, used where an anchor is not allowed.
const NEVER: &str = "\u{ffff}";

#[derive(Debug, Clone)]
pub struct RegExpSource {
    pub source: String,
    pub has_anchor: bool,
    pub has_back_references: bool,
    /// `[A0_G0, A0_G1, A1_G0, A1_G1]`, only when the source has `\A` or `\G`.
    anchor_cache: Option<[String; 4]>,
}

impl RegExpSource {
    pub fn new(source: &str) -> Self {
        // `\z` becomes `$(?!\n)(?<!\n)`; `\A` and `\G` are noted for the anchor variants.
        let chars: Vec<char> = source.chars().collect();
        let mut out = String::with_capacity(source.len());
        let mut has_anchor = false;
        let mut i = 0;
        while i < chars.len() {
            let c = chars[i];
            if c == '\\' && i + 1 < chars.len() {
                match chars[i + 1] {
                    'z' => out.push_str("$(?!\\n)(?<!\\n)"),
                    n => {
                        if n == 'A' || n == 'G' {
                            has_anchor = true;
                        }
                        out.push(c);
                        out.push(n);
                    }
                }
                i += 2;
            } else {
                out.push(c);
                i += 1;
            }
        }
        let has_back_references = has_back_reference(&out);
        let anchor_cache = has_anchor.then(|| build_anchor_cache(&out));
        Self { source: out, has_anchor, has_back_references, anchor_cache }
    }

    pub fn with_source(&self, source: &str) -> Self {
        // Used for the end pattern with its back references resolved: anchors are re-derived.
        Self::new(source)
    }

    /// The source for the given anchor permissions.
    pub fn resolve_anchors(&self, allow_a: bool, allow_g: bool) -> &str {
        match &self.anchor_cache {
            None => &self.source,
            Some(c) => &c[(usize::from(allow_a) << 1) | usize::from(allow_g)],
        }
    }

    /// upstream `resolveBackReferences`: `\1` becomes the escaped text of capture 1.
    pub fn resolve_back_references(&self, line: &str, captures: &[(usize, usize)]) -> String {
        let b = self.source.as_bytes();
        let mut out = String::with_capacity(self.source.len());
        let mut i = 0;
        let mut plain_from = 0;
        while i < b.len() {
            if b[i] == b'\\' && i + 1 < b.len() {
                if b[i + 1].is_ascii_digit() {
                    let mut j = i + 1;
                    while j < b.len() && b[j].is_ascii_digit() {
                        j += 1;
                    }
                    out.push_str(&self.source[plain_from..i]);
                    let idx: usize = self.source[i + 1..j].parse().unwrap_or(usize::MAX);
                    if let Some(&(s, e)) = captures.get(idx) {
                        escape_regexp_characters(&line[s..e], &mut out);
                    }
                    plain_from = j;
                    i = j;
                    continue;
                }
                i += 2;
                continue;
            }
            i += 1;
        }
        out.push_str(&self.source[plain_from..]);
        out
    }
}

/// `/\\(\d+)/`
fn has_back_reference(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = 0;
    while i + 1 < b.len() {
        if b[i] == b'\\' {
            if b[i + 1].is_ascii_digit() {
                return true;
            }
            i += 2;
        } else {
            i += 1;
        }
    }
    false
}

/// upstream `escapeRegExpCharacters`: `-\{}*+?|^$.,[]()#` and whitespace.
fn escape_regexp_characters(value: &str, out: &mut String) {
    for c in value.chars() {
        if matches!(c, '-' | '\\' | '{' | '}' | '*' | '+' | '?' | '|' | '^' | '$' | '.' | ',' | '[' | ']' | '(' | ')' | '#') || c.is_whitespace() {
            out.push('\\');
        }
        out.push(c);
    }
}

fn build_anchor_cache(source: &str) -> [String; 4] {
    let mut a0g0 = String::with_capacity(source.len());
    let mut a0g1 = String::with_capacity(source.len());
    let mut a1g0 = String::with_capacity(source.len());
    let mut a1g1 = String::with_capacity(source.len());
    let chars: Vec<char> = source.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '\\' && i + 1 < chars.len() {
            let n = chars[i + 1];
            match n {
                'A' => {
                    a0g0.push_str(NEVER);
                    a0g1.push_str(NEVER);
                    a1g0.push_str("\\A");
                    a1g1.push_str("\\A");
                }
                'G' => {
                    a0g0.push_str(NEVER);
                    a0g1.push_str("\\G");
                    a1g0.push_str(NEVER);
                    a1g1.push_str("\\G");
                }
                _ => {
                    for s in [&mut a0g0, &mut a0g1, &mut a1g0, &mut a1g1] {
                        s.push('\\');
                        s.push(n);
                    }
                }
            }
            i += 2;
        } else {
            for s in [&mut a0g0, &mut a0g1, &mut a1g0, &mut a1g1] {
                s.push(c);
            }
            i += 1;
        }
    }
    [a0g0, a0g1, a1g0, a1g1]
}

/// A found match: which pattern, and the capture groups as byte ranges (unmatched groups are
/// `(0, 0)`, like vscode-oniguruma's empty captures).
#[derive(Debug, Clone)]
pub struct ScanMatch {
    pub index: usize,
    pub captures: Vec<(usize, usize)>,
}

/// What a regex found the last time it was searched in the current text: valid for any later
/// start at or before the found position (the leftmost match from an earlier start is also the
/// leftmost from a later one), and for "no match" for any later start. Regexes with `\G` depend
/// on the start and are never cached.
#[derive(Clone, Default)]
struct Slot {
    valid: bool,
    from: usize,
    caps: Option<Vec<(usize, usize)>>,
}

/// Search results for one text (a line, or a truncated line for a retokenized capture).
#[derive(Default)]
pub struct ScanCache {
    slots: std::collections::HashMap<u64, Vec<Slot>>,
}

static NEXT_SCANNER_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// An `OnigScanner`: a list of patterns searched together; the leftmost match wins, ties go to
/// the earlier pattern.
pub struct Scanner {
    id: u64,
    regs: Vec<Option<Regex>>,
    uses_g: Vec<bool>,
    pub rule_ids: Vec<RuleId>,
}

impl Scanner {
    pub fn new(sources: &[(String, RuleId)]) -> Self {
        let regs = sources
            .iter()
            .map(|(s, _)| Regex::with_options(s, RegexOptions::REGEX_OPTION_CAPTURE_GROUP, Syntax::oniguruma()).ok())
            .collect();
        Self {
            id: NEXT_SCANNER_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            regs,
            uses_g: sources.iter().map(|(s, _)| s.contains("\\G")).collect(),
            rule_ids: sources.iter().map(|(_, id)| *id).collect(),
        }
    }

    pub fn find_next_match(&self, text: &str, start: usize, cache: &mut ScanCache) -> Option<ScanMatch> {
        let slots = cache.slots.entry(self.id).or_insert_with(|| vec![Slot::default(); self.regs.len()]);
        let mut region = Region::new();
        let mut best: Option<(usize, usize)> = None; // (match start, index)
        for (i, re) in self.regs.iter().enumerate() {
            let Some(re) = re else { continue };
            let slot = &mut slots[i];
            let hit = slot.valid && !self.uses_g[i] && slot.from <= start && slot.caps.as_ref().is_none_or(|c| c[0].0 >= start);
            if !hit {
                let found = re.search_with_options(text, start, text.len(), SearchOptions::SEARCH_OPTION_NONE, Some(&mut region));
                slot.valid = true;
                slot.from = start;
                slot.caps = found.map(|_| (0..region.len()).map(|g| region.pos(g).unwrap_or((0, 0))).collect());
            }
            if let Some(c) = &slot.caps
                && best.is_none_or(|(p, _)| c[0].0 < p)
            {
                best = Some((c[0].0, i));
                if c[0].0 == start {
                    // Nothing beats a match at the start position (ties go to the earlier pattern).
                    break;
                }
            }
        }
        best.map(|(_, index)| ScanMatch { index, captures: slots[index].caps.clone().expect("a found match has captures") })
    }
}
