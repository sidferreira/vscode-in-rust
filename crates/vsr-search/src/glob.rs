//! VS Code glob syntax on top of `globset`, plus the glob helpers the query builder needs.
//!
//! upstream: src/vs/base/common/glob.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/node/ripgrepTextSearchEngine.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/common/queryBuilder.ts @ 50e1350fc78d
//!
//! VS Code globs: `*` and `?` never cross `/`, `**` does, `{a,b}` alternates, `[abc]` / `[!abc]`
//! classes. `globset` with `literal_separator(true)` has the same semantics. Brace groups are
//! expanded up front (one level, escape aware) so patterns like `{a/*.*,b/*.*}` work.

use globset::{GlobBuilder, GlobSet, GlobSetBuilder};

/// Split `pattern` at `split_char`, ignoring separators inside `{}` and `[]`
/// (upstream `splitGlobAware`).
pub fn split_glob_aware(pattern: &str, split_char: char) -> Vec<String> {
    if pattern.is_empty() {
        return Vec::new();
    }
    let mut segments = Vec::new();
    let (mut in_braces, mut in_brackets) = (false, false);
    let mut cur = String::new();
    for ch in pattern.chars() {
        match ch {
            c if c == split_char && !in_braces && !in_brackets => {
                segments.push(std::mem::take(&mut cur));
                continue;
            }
            '{' => in_braces = true,
            '}' => in_braces = false,
            '[' => in_brackets = true,
            ']' => in_brackets = false,
            _ => {}
        }
        cur.push(ch);
    }
    if !cur.is_empty() {
        segments.push(cur);
    }
    segments
}

struct BraceSplit {
    fixed_start: Option<String>,
    in_braces: String,
    fixed_end: Option<String>,
}

fn escape_aware_split(pattern: &str) -> BraceSplit {
    let chars: Vec<char> = pattern.chars().collect();
    let mut in_braces = false;
    let mut escaped = false;
    let mut fixed_start = String::new();
    let mut str_in_braces = String::new();
    let none = |s: String| BraceSplit { fixed_start: None, in_braces: s, fixed_end: None };
    for (i, &ch) in chars.iter().enumerate() {
        match ch {
            '\\' => {
                if escaped {
                    let t = if in_braces { &mut str_in_braces } else { &mut fixed_start };
                    t.push_str("\\\\");
                    escaped = false;
                } else {
                    escaped = true;
                }
            }
            '{' => {
                if escaped {
                    let t = if in_braces { &mut str_in_braces } else { &mut fixed_start };
                    t.push(ch);
                    escaped = false;
                } else if in_braces {
                    let rest: String = chars[i + 1..].iter().collect();
                    return none(format!("{fixed_start}{{{str_in_braces}{{{rest}"));
                } else {
                    in_braces = true;
                }
            }
            '}' => {
                if escaped {
                    let t = if in_braces { &mut str_in_braces } else { &mut fixed_start };
                    t.push(ch);
                    escaped = false;
                } else if in_braces {
                    let rest: String = chars[i + 1..].iter().collect();
                    return BraceSplit {
                        fixed_start: Some(fixed_start),
                        in_braces: str_in_braces,
                        fixed_end: Some(rest),
                    };
                } else {
                    fixed_start.push(ch);
                }
            }
            _ => {
                let t = if in_braces { &mut str_in_braces } else { &mut fixed_start };
                if escaped {
                    t.push('\\');
                }
                t.push(ch);
                escaped = false;
            }
        }
    }
    let tail = if in_braces { format!("{{{str_in_braces}") } else { String::new() };
    none(format!("{fixed_start}{tail}"))
}

/// Expand brace groups into plain globs, one level of nesting (upstream
/// `performBraceExpansionForRipgrep`): `eep/{a,b}/test` -> `eep/a/test`, `eep/b/test`.
pub fn perform_brace_expansion(pattern: &str) -> Vec<String> {
    let split = escape_aware_split(pattern);
    let (Some(start), Some(end)) = (split.fixed_start, split.fixed_end) else {
        return vec![split.in_braces];
    };
    let mut arr = split_glob_aware(&split.in_braces, ',');
    if arr.is_empty() {
        arr = vec![String::new()];
    }
    let ends = perform_brace_expansion(&end);
    arr.iter()
        .flat_map(|elem| {
            let s = format!("{start}{elem}");
            ends.iter().map(move |e| format!("{s}{e}")).collect::<Vec<_>>()
        })
        .collect()
}

/// Escape a path so it matches itself as a glob: `?`, `*`, `[`, `]` become `[?]` etc.
pub fn escape_glob_pattern(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for c in path.chars() {
        if matches!(c, '?' | '*' | '[' | ']') {
            out.push('[');
            out.push(c);
            out.push(']');
        } else {
            out.push(c);
        }
    }
    out
}

/// A set of VS Code globs matched against `/`-separated relative paths.
#[derive(Clone, Default)]
pub struct GlobMatcher {
    set: Option<GlobSet>,
    len: usize,
}

impl std::fmt::Debug for GlobMatcher {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "GlobMatcher({} globs)", self.len)
    }
}

impl GlobMatcher {
    /// Invalid globs are skipped.
    pub fn new<S: AsRef<str>>(patterns: &[S], ignore_case: bool) -> Self {
        let mut b = GlobSetBuilder::new();
        let mut len = 0;
        for p in patterns {
            for expanded in perform_brace_expansion(p.as_ref()) {
                if let Ok(g) = GlobBuilder::new(&expanded)
                    .literal_separator(true)
                    .case_insensitive(ignore_case)
                    .backslash_escape(true)
                    .build()
                {
                    b.add(g);
                    len += 1;
                }
            }
        }
        Self { set: if len == 0 { None } else { b.build().ok() }, len }
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn is_match(&self, path: &str) -> bool {
        self.set.as_ref().is_some_and(|s| s.is_match(path))
    }
}

/// `glob.match(pattern, path, { ignoreCase })` for a single pattern.
pub fn glob_match(pattern: &str, path: &str, ignore_case: bool) -> bool {
    GlobMatcher::new(&[pattern], ignore_case).is_match(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_respects_braces_and_brackets() {
        assert_eq!(split_glob_aware("a,{b,c},[d,e],f", ','), ["a", "{b,c}", "[d,e]", "f"]);
        assert!(split_glob_aware("", ',').is_empty());
    }

    #[test]
    fn star_does_not_cross_separators() {
        assert!(glob_match("*.js", "a.js", false));
        assert!(!glob_match("*.js", "dir/a.js", false));
        assert!(glob_match("**/*.js", "dir/sub/a.js", false));
        assert!(glob_match("**/*.js", "a.js", false));
        assert!(glob_match("{a/*.*,b/*.*}", "b/x.y", false));
    }

    #[test]
    fn invalid_globs_are_skipped() {
        let m = GlobMatcher::new(&["[", "*.rs"], false);
        assert!(m.is_match("a.rs"));
    }
}
