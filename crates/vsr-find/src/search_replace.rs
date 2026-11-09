//! The replace string of the Search view: what one match is replaced with.
//!
//! upstream: src/vs/workbench/services/search/common/replace.ts @ 50e1350fc78d
//!
//! Search and Find share the primitives (`build_replace_string_with_case_preserved`) but upstream
//! keeps two classes, and so do we: `ReplacePattern` here re-runs the search regex on the text of
//! one match (`getReplaceString`) to get its capture groups, parses the replace string as a
//! string (not as pieces), applies Preserve Case to the whole template (so `newrege$1` becomes
//! `Newrege$1` before `$1` is filled in) and only then expands `$1` / `$&`, the way
//! `String.prototype.replace` does.
//!
//! Differences: Rust `regex` (no look-around, so the upstream tests that use it are ported with
//! look-ahead-free equivalents); `$` inside text inserted by a case operation (`\U$1`) is escaped
//! instead of being re-read as a parameter (upstream re-reads it, which is a bug); `` $` `` and
//! `$'` refer to the matched text the pattern was given.

use regex::{Captures, Regex, RegexBuilder};
use vsr_search::TextSearchQuery;
use vsr_search::query::regex_source;

use crate::replace_pattern::build_replace_string_with_case_preserved;
use crate::search::CompileError;

#[derive(Clone, Debug)]
pub struct SearchReplacePattern {
    replace_pattern: String,
    has_parameters: bool,
    regex: Regex,
}

impl SearchReplacePattern {
    /// `new ReplacePattern(replaceString, searchPatternInfo)`: the search regex comes from the
    /// query (smart case folded in by `smart_case`); only a regex search reads `$1`, `\n` and
    /// friends in the replace string.
    pub fn new(replace_string: &str, query: &TextSearchQuery, smart_case: bool) -> Result<Self, CompileError> {
        let source = regex_source(query);
        let regex = RegexBuilder::new(&source)
            .case_insensitive(!query.effective_case_sensitive(smart_case))
            .multi_line(true)
            .build()
            .map_err(|e| CompileError(e.to_string()))?;
        Ok(Self::with_regex(replace_string, query.is_regex, regex))
    }

    /// `new ReplacePattern(replaceString, parseParameters, regEx)`.
    pub fn with_regex(replace_string: &str, parse_parameters: bool, regex: Regex) -> Self {
        let (replace_pattern, has_parameters) =
            if parse_parameters { parse_replace_string(replace_string) } else { (replace_string.to_string(), false) };
        Self { replace_pattern, has_parameters, regex }
    }

    pub fn has_parameters(&self) -> bool {
        self.has_parameters
    }

    /// The parsed replace string (`\n` already a newline, `$0` spelled `$&`).
    pub fn pattern(&self) -> &str {
        &self.replace_pattern
    }

    /// The replacement for the first match in `text` (normally the text of exactly one match);
    /// `None` when the regex does not match it.
    pub fn get_replace_string(&self, text: &str, preserve_case: bool) -> Option<String> {
        let caps = self.regex.captures(text)?;
        let matches: Vec<String> = caps.iter().map(|g| g.map_or_else(String::new, |m| m.as_str().to_string())).collect();
        let template = self.build_replace_string(&matches, preserve_case);
        if !self.has_parameters {
            return Some(template);
        }
        let template = replace_with_case_operations(&template, &matches);
        Some(expand(&template, &caps, text))
    }

    fn build_replace_string(&self, matches: &[String], preserve_case: bool) -> String {
        if preserve_case {
            build_replace_string_with_case_preserved(Some(matches), &self.replace_pattern)
        } else {
            self.replace_pattern.clone()
        }
    }
}

/// `parseReplaceString`: `\n` LF, `\t` TAB, `\\` a backslash, `$0` becomes `$&`; everything else
/// is untouched. Also says whether the string has `$n`-style parameters.
fn parse_replace_string(replace_string: &str) -> (String, bool) {
    let chars: Vec<char> = replace_string.chars().collect();
    let len = chars.len();
    if len == 0 {
        return (String::new(), false);
    }
    let digit = |c: char, from: char| (from..='9').contains(&c);
    let mut has_parameters = false;
    let (mut substr_from, mut result) = (0usize, String::new());
    let take = |a: usize, b: usize| -> String { chars[a..b.max(a)].iter().collect() };
    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch == '\\' {
            i += 1;
            if i >= len {
                break;
            }
            let replace_with = match chars[i] {
                '\\' => Some('\\'),
                'n' => Some('\n'),
                't' => Some('\t'),
                _ => None,
            };
            if let Some(r) = replace_with {
                result.push_str(&take(substr_from, i - 1));
                result.push(r);
                substr_from = i + 1;
            }
        }
        // Upstream tests `chCode` (the character before the advance) again, so a backslash never
        // reaches this branch.
        if ch == '$' {
            i += 1;
            if i >= len {
                break;
            }
            let next = chars[i];
            let mut replace_with: Option<&str> = None;
            match next {
                '0' => {
                    replace_with = Some("$&");
                    has_parameters = true;
                }
                '`' | '\'' => has_parameters = true,
                _ if digit(next, '1') => {
                    if i == len - 1 {
                        has_parameters = true;
                    } else {
                        i += 1;
                        if !digit(chars[i], '0') {
                            has_parameters = true;
                            i -= 1;
                        } else if i == len - 1 {
                            has_parameters = true;
                        } else {
                            i += 1;
                            if !digit(chars[i], '0') {
                                has_parameters = true;
                                i -= 1;
                            }
                        }
                    }
                }
                _ => {}
            }
            if let Some(r) = replace_with {
                result.push_str(&take(substr_from, i - 1));
                result.push_str(r);
                substr_from = i + 1;
            }
        }
        i += 1;
    }
    if substr_from == 0 {
        return (replace_string.to_string(), has_parameters);
    }
    result.push_str(&take(substr_from, len));
    (result, has_parameters)
}

/// `replaceWithCaseOperations`, up to the point where upstream hands the string to `replace`:
/// `\u \U \l \L` before a `$N` are applied to group N and the `$N` is replaced by the result.
fn replace_with_case_operations(template: &str, matches: &[String]) -> String {
    let chars: Vec<char> = template.chars().collect();
    let is_op = |c: char| matches!(c, 'u' | 'U' | 'l' | 'L');
    let mut out = String::new();
    let mut i = 0;
    let mut prefix_start = 0;
    while i < chars.len() {
        // The shortest prefix followed by (`\X`)* `$digits`.
        let mut j = i;
        while j + 1 < chars.len() && chars[j] == '\\' && is_op(chars[j + 1]) {
            j += 2;
        }
        if j < chars.len() && chars[j] == '$' && chars.get(j + 1).is_some_and(char::is_ascii_digit) {
            let mut k = j + 1;
            while k < chars.len() && chars[k].is_ascii_digit() {
                k += 1;
            }
            let ops: Vec<char> = chars[i..j].iter().filter(|c| **c != '\\').copied().collect();
            let digits: String = chars[j + 1..k].iter().collect();
            let group = digits.parse::<usize>().ok().and_then(|n| matches.get(n)).filter(|g| !g.is_empty());
            let prefix: String = chars[prefix_start..i].iter().collect();
            out.push_str(&prefix);
            match (ops.is_empty(), group) {
                (false, Some(replacement)) => out.push_str(&apply_ops(&ops, replacement)),
                _ => out.extend(&chars[i..k]),
            }
            i = k;
            prefix_start = k;
        } else {
            i += 1;
        }
    }
    out.extend(&chars[prefix_start..]);
    out
}

fn apply_ops(ops: &[char], replacement: &str) -> String {
    let rep: Vec<char> = replacement.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < ops.len() {
        match ops[i] {
            'U' => {
                out.extend(rep[i.min(rep.len())..].iter().flat_map(|c| c.to_uppercase()));
                i = rep.len();
            }
            'L' => {
                out.extend(rep[i.min(rep.len())..].iter().flat_map(|c| c.to_lowercase()));
                i = rep.len();
            }
            'u' => match rep.get(i) {
                Some(c) => out.extend(c.to_uppercase()),
                None => break,
            },
            'l' => match rep.get(i) {
                Some(c) => out.extend(c.to_lowercase()),
                None => break,
            },
            _ => {}
        }
        i += 1;
    }
    if i < rep.len() {
        out.extend(&rep[i..]);
    }
    // The result goes through `$` expansion next: keep a literal `$` literal.
    out.replace('$', "$$")
}

/// JavaScript's replacement patterns for one match: `$$`, `$&`, `` $` ``, `$'`, `$n`, `$nn`.
fn expand(template: &str, caps: &Captures<'_>, text: &str) -> String {
    let m0 = caps.get(0).expect("group 0");
    let groups = caps.len() - 1;
    let group = |n: usize| caps.get(n).map_or("", |m| m.as_str());
    let chars: Vec<char> = template.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] != '$' || i + 1 >= chars.len() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        let next = chars[i + 1];
        match next {
            '$' => {
                out.push('$');
                i += 2;
            }
            '&' => {
                out.push_str(m0.as_str());
                i += 2;
            }
            '`' => {
                out.push_str(&text[..m0.start()]);
                i += 2;
            }
            '\'' => {
                out.push_str(&text[m0.end()..]);
                i += 2;
            }
            d if d.is_ascii_digit() => {
                let d1 = d.to_digit(10).unwrap_or(0) as usize;
                let d2 = chars.get(i + 2).and_then(|c| c.to_digit(10)).map(|d| d as usize);
                if let Some(d2) = d2.filter(|d2| (1..=groups).contains(&(d1 * 10 + d2))) {
                    out.push_str(group(d1 * 10 + d2));
                    i += 3;
                } else if (1..=groups).contains(&d1) {
                    out.push_str(group(d1));
                    i += 2;
                } else {
                    out.push('$');
                    i += 1;
                }
            }
            _ => {
                out.push('$');
                i += 1;
            }
        }
    }
    out
}
