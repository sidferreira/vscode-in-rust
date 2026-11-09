//! The text query and how it becomes a regex: literal escaping, smart case, whole word,
//! multiline detection and the CRLF-aware newline fix-ups.
//!
//! upstream: src/vs/workbench/services/search/common/queryBuilder.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/node/ripgrepTextSearchEngine.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/model/textModelSearch.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/strings.ts @ 50e1350fc78d
//!
//! Upstream builds a ripgrep command line (`getRgArgs`); we build the same regex source and feed
//! it to `grep-regex` directly, so the flag mapping is: `--ignore-case`/`--case-sensitive` ->
//! `case_insensitive`, `--crlf` -> `crlf(true)` + CRLF line terminator, `--multiline` ->
//! `Searcher::multi_line`, `--fixed-strings` -> escaped literal, `--word-regexp`-like handling ->
//! `createRegExp`'s `\b` rule (see [`regex_source`]).

use regex::Regex;

/// What the user typed in the search box, plus the three toggles (upstream `IPatternInfo`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TextSearchQuery {
    pub pattern: String,
    pub is_regex: bool,
    pub is_case_sensitive: bool,
    pub is_word_match: bool,
    pub is_multiline: bool,
}

impl TextSearchQuery {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self { pattern: pattern.into(), ..Self::default() }
    }

    /// Effective case sensitivity: the toggle, or smart case (`search.smartCase`): a pattern
    /// with an uppercase character (ignoring escapes like `\S` in regex mode) is case sensitive.
    pub fn effective_case_sensitive(&self, smart_case: bool) -> bool {
        if smart_case && contains_uppercase_character(&self.pattern, self.is_regex) {
            return true;
        }
        self.is_case_sensitive
    }

    /// Whether the search must run in multiline mode (upstream `QueryBuilder.isMultiline`).
    pub fn effective_multiline(&self) -> bool {
        self.is_multiline
            || (self.is_regex && is_multiline_regex_source(&self.pattern))
            || self.pattern.contains('\n')
    }
}

/// `strings.containsUppercaseCharacter`.
pub fn contains_uppercase_character(target: &str, ignore_escaped_chars: bool) -> bool {
    if target.is_empty() {
        return false;
    }
    if ignore_escaped_chars {
        let mut stripped = String::with_capacity(target.len());
        let mut it = target.chars();
        while let Some(c) = it.next() {
            if c == '\\' {
                it.next();
            } else {
                stripped.push(c);
            }
        }
        return stripped.to_lowercase() != stripped;
    }
    target.to_lowercase() != target
}

/// `isMultilineRegexSource`: does the regex source obviously need to span lines?
pub fn is_multiline_regex_source(search: &str) -> bool {
    let chars: Vec<char> = search.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '\n' => return true,
            '\\' => {
                i += 1;
                if i >= chars.len() {
                    break;
                }
                if matches!(chars[i], 'n' | 'r' | 'W') {
                    return true;
                }
            }
            _ => {}
        }
        i += 1;
    }
    false
}

/// `strings.escapeRegExpCharacters`.
pub fn escape_regexp_characters(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for c in value.chars() {
        if matches!(c, '\\' | '{' | '}' | '*' | '+' | '?' | '|' | '^' | '$' | '.' | '[' | ']' | '(' | ')') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// `\u1234` and `\u{1234}` to `\x{1234}` (upstream `unicodeEscapesToPCRE2`).
pub fn unicode_escapes_to_pcre2(pattern: &str) -> String {
    let plain = Regex::new(r"(?i)((?:[^\\]|^)(?:\\\\)*)\\u([a-z0-9]{4})").expect("static regex");
    let braced = Regex::new(r"(?i)((?:[^\\]|^)(?:\\\\)*)\\u\{([a-z0-9]{4})\}").expect("static regex");
    let mut pattern = pattern.to_string();
    for re in [&plain, &braced] {
        while re.is_match(&pattern) {
            pattern = re.replace_all(&pattern, "${1}\\x{${2}}").into_owned();
        }
    }
    pattern
}

/// `fixNewline`: a literal newline in the pattern matches `\r?\n`.
pub fn fix_newline(pattern: &str) -> String {
    pattern.replace('\n', "\\r?\\n")
}

fn quantifier_at(chars: &[char], i: usize) -> bool {
    match chars.get(i) {
        Some('+' | '*' | '?') => true,
        Some('{') => chars.get(i + 1).is_some_and(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// Is the pattern well formed enough for the scanner (balanced groups, closed classes)?
fn is_balanced(chars: &[char]) -> bool {
    let (mut depth, mut i) = (0i32, 0);
    let mut in_class = false;
    while i < chars.len() {
        match chars[i] {
            '\\' => i += 1,
            '[' if !in_class => in_class = true,
            ']' if in_class => in_class = false,
            '(' if !in_class => depth += 1,
            ')' if !in_class => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            _ => {}
        }
        i += 1;
    }
    depth == 0 && !in_class
}

/// `fixRegexNewline`: make `\n` in a regex also match `\r\n` (upstream rewrites the regexpp AST;
/// this is a scanner over the same cases). Returns the input unchanged if it does not parse.
pub fn fix_regex_newline(pattern: &str) -> String {
    let chars: Vec<char> = pattern.chars().collect();
    if !is_balanced(&chars) {
        return pattern.to_string();
    }
    let n = chars.len();
    let mut out = String::with_capacity(pattern.len() + 8);
    let mut groups: Vec<bool> = Vec::new(); // is-lookbehind per open group
    let mut i = 0;
    while i < n {
        let c = chars[i];
        let in_lookbehind = groups.iter().any(|&b| b);
        match c {
            '\\' if i + 1 < n => {
                if chars[i + 1] == 'n' && !in_lookbehind {
                    if quantifier_at(&chars, i + 2) {
                        out.push_str("(?:\\r?\\n)");
                    } else {
                        out.push_str("\\r?\\n");
                    }
                } else {
                    out.push(c);
                    out.push(chars[i + 1]);
                }
                i += 2;
            }
            '(' => {
                let rest: String = chars[i + 1..n.min(i + 4)].iter().collect();
                groups.push(rest.starts_with("?<=") || rest.starts_with("?<!"));
                out.push(c);
                i += 1;
            }
            ')' => {
                groups.pop();
                out.push(c);
                i += 1;
            }
            '[' => {
                // Collect the class tokens up to the closing ']'.
                let mut j = i + 1;
                let negate = chars.get(j) == Some(&'^');
                if negate {
                    j += 1;
                }
                let mut tokens: Vec<String> = Vec::new();
                while j < n && chars[j] != ']' {
                    if chars[j] == '\\' && j + 1 < n {
                        tokens.push(chars[j..j + 2].iter().collect());
                        j += 2;
                    } else {
                        tokens.push(chars[j].to_string());
                        j += 1;
                    }
                }
                let raw: String = chars[i..=j].iter().collect();
                // A `\n` that is a range endpoint is left alone (upstream only rewrites plain ones).
                let mut endpoint = vec![false; tokens.len()];
                for k in 1..tokens.len().saturating_sub(1) {
                    if tokens[k] == "-" {
                        endpoint[k - 1] = true;
                        endpoint[k + 1] = true;
                    }
                }
                let removable: Vec<bool> =
                    tokens.iter().zip(&endpoint).map(|(t, &e)| t == "\\n" && !e).collect();
                if in_lookbehind || !removable.iter().any(|&b| b) {
                    out.push_str(&raw);
                } else {
                    let other: String = tokens
                        .iter()
                        .zip(&removable)
                        .filter(|(_, r)| !**r)
                        .map(|(t, _)| t.as_str())
                        .collect();
                    let quantified = quantifier_at(&chars, j + 1);
                    if negate {
                        if quantified {
                            if other.is_empty() {
                                out.push('.');
                            } else {
                                out.push_str(&format!("[^{other}]"));
                            }
                        } else {
                            out.push_str("(?!\\r?\\n");
                            if !other.is_empty() {
                                out.push_str(&format!("|[{other}]"));
                            }
                            out.push(')');
                        }
                    } else if other.is_empty() {
                        out.push_str("\\r?\\n");
                    } else {
                        out.push_str(&format!("(?:[{other}]|\\r?\\n)"));
                    }
                }
                i = j + 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

/// The regex source handed to the matcher for `query` (the pattern part of upstream `getRgArgs`).
pub fn regex_source(query: &TextSearchQuery) -> String {
    let multiline = query.effective_multiline();
    let mut pattern = query.pattern.clone();
    let mut is_regex = query.is_regex;
    if is_regex {
        // queryBuilder.getContentPattern: real line breaks in a regex become `\n`.
        pattern = pattern.replace("\r\n", "\\n").replace('\n', "\\n");
    }
    if multiline && !is_regex {
        pattern = escape_regexp_characters(&pattern);
        is_regex = true;
    }
    if is_regex {
        pattern = unicode_escapes_to_pcre2(&pattern);
    }
    if query.is_word_match {
        // createRegExp: add `\b` only next to a word character.
        if !is_regex {
            pattern = escape_regexp_characters(&pattern);
        }
        let is_word = |c: char| c.is_alphanumeric() || c == '_';
        if pattern.chars().next().is_some_and(is_word) {
            pattern.insert_str(0, "\\b");
        }
        if pattern.chars().last().is_some_and(is_word) {
            pattern.push_str("\\b");
        }
        // (a newline in a whole-word literal still has to match CRLF files)
        return if multiline { fix_newline(&fix_regex_newline(&pattern)) } else { pattern };
    }
    if is_regex {
        return fix_newline(&fix_regex_newline(&pattern));
    }
    escape_regexp_characters(&pattern)
}
