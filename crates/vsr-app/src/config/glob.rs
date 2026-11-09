//! Glob patterns for `files.exclude`: `*`, `?`, `**`, `[a-z]`, `[!x]` and `{a,b}`, matched against
//! a path relative to the workspace root.
//!
//! upstream: src/vs/base/common/glob.ts @ 50e1350fc78d
//!
//! Not ported: upstream's compiled fast paths and expression caching (we match segment by
//! segment), `**` inside a segment (`a**b` behaves like `a*b`), and relative patterns with a base
//! (`{ base, pattern }`). Sibling clauses (`{ "when": "$(basename).ts" }`) are supported through
//! the `has_sibling` callback of `ExcludeMatcher::is_excluded`.

use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Lit(char),
    Star,
    Any,
    Class { negated: bool, ranges: Vec<(char, char)> },
}

#[derive(Debug, Clone, PartialEq)]
enum Seg {
    /// `**`: zero or more path segments.
    Globstar,
    Pattern(Vec<Tok>),
}

/// One compiled pattern (braces already expanded into alternatives).
#[derive(Debug, Clone)]
pub struct Glob {
    alternatives: Vec<Vec<Seg>>,
}

impl Glob {
    pub fn new(pattern: &str) -> Self {
        let pattern = pattern.trim_start_matches("./").trim_start_matches('/');
        let alternatives = expand_braces(pattern)
            .into_iter()
            .map(|p| p.split('/').filter(|s| !s.is_empty()).map(parse_segment).collect())
            .collect();
        Self { alternatives }
    }

    /// Whether the relative `path` (segments separated by `/`) matches.
    pub fn is_match(&self, path: &str) -> bool {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        self.alternatives.iter().any(|segs| match_segments(segs, &parts))
    }
}

/// `{a,b}` into alternatives, nested braces included. An unmatched brace is a literal.
fn expand_braces(pattern: &str) -> Vec<String> {
    let chars: Vec<char> = pattern.chars().collect();
    let Some(open) = chars.iter().position(|&c| c == '{') else {
        return vec![pattern.to_string()];
    };
    let mut depth = 0;
    let mut close = None;
    let mut commas = Vec::new();
    for (i, &c) in chars.iter().enumerate().skip(open) {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(i);
                    break;
                }
            }
            ',' if depth == 1 => commas.push(i),
            _ => {}
        }
    }
    let Some(close) = close else { return vec![pattern.to_string()] };
    let prefix: String = chars[..open].iter().collect();
    let suffix: String = chars[close + 1..].iter().collect();
    let mut bounds = vec![open];
    bounds.extend(commas);
    bounds.push(close);
    let mut out = Vec::new();
    for w in bounds.windows(2) {
        let option: String = chars[w[0] + 1..w[1]].iter().collect();
        for tail in expand_braces(&format!("{option}{suffix}")) {
            out.extend(expand_braces(&format!("{prefix}{tail}")));
        }
    }
    out
}

fn parse_segment(s: &str) -> Seg {
    if s == "**" {
        return Seg::Globstar;
    }
    let chars: Vec<char> = s.chars().collect();
    let mut toks = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                if toks.last() != Some(&Tok::Star) {
                    toks.push(Tok::Star);
                }
            }
            '?' => toks.push(Tok::Any),
            '[' => {
                if let Some(end) = chars[i + 1..].iter().position(|&c| c == ']').map(|p| p + i + 1).filter(|&e| e > i + 1) {
                    let mut body = &chars[i + 1..end];
                    let negated = matches!(body.first(), Some('!' | '^'));
                    if negated {
                        body = &body[1..];
                    }
                    let mut ranges = Vec::new();
                    let mut j = 0;
                    while j < body.len() {
                        if j + 2 < body.len() && body[j + 1] == '-' {
                            ranges.push((body[j], body[j + 2]));
                            j += 3;
                        } else {
                            ranges.push((body[j], body[j]));
                            j += 1;
                        }
                    }
                    toks.push(Tok::Class { negated, ranges });
                    i = end;
                } else {
                    toks.push(Tok::Lit('['));
                }
            }
            c => toks.push(Tok::Lit(c)),
        }
        i += 1;
    }
    Seg::Pattern(toks)
}

fn match_segments(pattern: &[Seg], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((Seg::Globstar, rest)) => (0..=path.len()).any(|skip| match_segments(rest, &path[skip..])),
        Some((Seg::Pattern(toks), rest)) => match path.split_first() {
            Some((first, tail)) => match_tokens(toks, &first.chars().collect::<Vec<_>>()) && match_segments(rest, tail),
            None => false,
        },
    }
}

fn match_tokens(toks: &[Tok], text: &[char]) -> bool {
    match toks.split_first() {
        None => text.is_empty(),
        Some((Tok::Star, rest)) => (0..=text.len()).any(|n| match_tokens(rest, &text[n..])),
        Some((tok, rest)) => match text.split_first() {
            None => false,
            Some((&c, tail)) => {
                let ok = match tok {
                    Tok::Lit(l) => *l == c,
                    Tok::Any => true,
                    Tok::Class { negated, ranges } => ranges.iter().any(|&(a, b)| a <= c && c <= b) != *negated,
                    Tok::Star => unreachable!("handled above"),
                };
                ok && match_tokens(rest, tail)
            }
        },
    }
}

/// The `files.exclude` map: pattern -> `true`, `false` or `{ "when": "$(basename).ext" }`.
#[derive(Debug, Clone, Default)]
pub struct ExcludeMatcher {
    /// (glob, optional sibling template)
    rules: Vec<(Glob, Option<String>)>,
}

impl ExcludeMatcher {
    /// Anything that is not an object excludes nothing, like an empty map.
    pub fn from_value(value: &Value) -> Self {
        let mut rules = Vec::new();
        if let Value::Object(map) = value {
            for (pattern, rule) in map {
                match rule {
                    Value::Bool(true) => rules.push((Glob::new(pattern), None)),
                    Value::Object(o) => {
                        if let Some(when) = o.get("when").and_then(Value::as_str) {
                            rules.push((Glob::new(pattern), Some(when.to_string())));
                        }
                    }
                    _ => {}
                }
            }
        }
        Self { rules }
    }

    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Whether the entry at `rel_path` (relative to the workspace root, `/` separated) is hidden.
    /// `has_sibling(name)` says whether a sibling with that name exists in the same folder.
    pub fn is_excluded(&self, rel_path: &str, has_sibling: &dyn Fn(&str) -> bool) -> bool {
        self.rules.iter().any(|(glob, when)| {
            glob.is_match(rel_path)
                && match when {
                    None => true,
                    Some(template) => {
                        let base = rel_path.rsplit('/').next().unwrap_or(rel_path);
                        let stem = match base.rfind('.') {
                            Some(dot) if dot > 0 => &base[..dot],
                            _ => base,
                        };
                        has_sibling(&template.replace("$(basename)", stem))
                    }
                }
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn m(pattern: &str, path: &str) -> bool {
        Glob::new(pattern).is_match(path)
    }

    #[test]
    fn star_question_and_classes_stay_inside_a_segment() {
        assert!(m("*.js", "a.js"));
        assert!(!m("*.js", "src/a.js"));
        assert!(m("a?c", "abc"));
        assert!(!m("a?c", "ac"));
        assert!(m("[a-c]x", "bx"));
        assert!(!m("[!a-c]x", "bx"));
        assert!(m("[!a-c]x", "dx"));
    }

    #[test]
    fn globstar_matches_zero_or_more_segments() {
        assert!(m("**/.git", ".git"));
        assert!(m("**/.git", "a/b/.git"));
        assert!(!m("**/.git", "a/.gitignore"));
        assert!(m("src/**", "src"));
        assert!(m("src/**", "src/a/b.rs"));
        assert!(m("**/node_modules/**", "x/node_modules/y/z"));
        assert!(!m("src/**/x.rs", "lib/x.rs"));
        assert!(m("src/**/x.rs", "src/a/b/x.rs"));
    }

    #[test]
    fn plain_names_match_the_whole_relative_path_only() {
        assert!(m("target", "target"));
        assert!(!m("target", "crates/target"));
        assert!(m("./target", "target"));
    }

    #[test]
    fn braces_expand_including_nested_ones() {
        assert!(m("**/*.{js,ts}", "a/b.ts"));
        assert!(m("**/*.{js,ts}", "b.js"));
        assert!(!m("**/*.{js,ts}", "b.rs"));
        assert!(m("{a,b{c,d}}/x", "bd/x"));
        assert!(!m("{a,b{c,d}}/x", "be/x"));
    }

    #[test]
    fn default_files_exclude_hides_vcs_and_os_files() {
        let default = json!({"**/.git": true, "**/.svn": true, "**/.hg": true, "**/.jj": true, "**/.DS_Store": true, "**/Thumbs.db": true});
        let ex = ExcludeMatcher::from_value(&default);
        let none = |_: &str| false;
        assert!(ex.is_excluded(".git", &none));
        assert!(ex.is_excluded("sub/.DS_Store", &none));
        assert!(!ex.is_excluded(".gitignore", &none));
        assert!(!ex.is_excluded("src", &none));
    }

    #[test]
    fn false_entries_and_non_objects_exclude_nothing() {
        let ex = ExcludeMatcher::from_value(&json!({"**/.git": false}));
        assert!(!ex.is_excluded(".git", &|_| false));
        assert!(ExcludeMatcher::from_value(&json!(3)).is_empty());
    }

    #[test]
    fn when_clause_needs_a_matching_sibling() {
        let ex = ExcludeMatcher::from_value(&json!({"**/*.js": {"when": "$(basename).ts"}}));
        assert!(ex.is_excluded("a/app.js", &|s| s == "app.ts"));
        assert!(!ex.is_excluded("a/app.js", &|_| false));
    }
}
