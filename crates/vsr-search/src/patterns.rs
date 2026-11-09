//! "files to include" / "files to exclude" input handling: splitting the comma separated text,
//! `./folder` search paths, glob expansion, and the file filter built from them.
//!
//! upstream: src/vs/workbench/services/search/common/queryBuilder.ts @ 50e1350fc78d
//!
//! Like upstream, a plain segment such as `src` or `*.rs` is expanded to `**/src/**` and
//! `**/src`; segments starting with `./`, `../` or `/` are search paths (relative to the
//! workspace root, or absolute) whose glob part is applied under that folder.

use std::path::{Component, Path, PathBuf};

use crate::glob::{GlobMatcher, split_glob_aware};

/// `splitGlobPattern`: the comma separated pattern input, trimmed, empty segments dropped.
pub fn split_pattern_input(text: &str) -> Vec<String> {
    split_glob_aware(text, ',')
        .into_iter()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn normalize_slashes(p: &str) -> String {
    p.replace('\\', "/")
}

/// `normalizeGlobPattern`: normalize slashes, remove `./` and trailing slashes.
fn normalize_glob_pattern(p: &str) -> String {
    let p = normalize_slashes(p);
    let p = p.strip_prefix("./").unwrap_or(&p);
    let p = p.trim_end_matches('/');
    if p == "." { String::new() } else { p.to_string() }
}

/// `expandGlobalGlob`: `foo` -> `**/foo/**`, `**/foo`.
fn expand_global_glob(pattern: &str) -> Vec<String> {
    [format!("**/{pattern}/**"), format!("**/{pattern}")]
        .into_iter()
        .map(|p| p.replace("**/**", "**"))
        .collect()
}

fn is_search_path(segment: &str) -> bool {
    segment.starts_with('/')
        || segment == "."
        || segment == ".."
        || segment.starts_with("./")
        || segment.starts_with("../")
}

/// `splitGlobFromPath`.
fn split_glob_from_path(search_path: &str) -> (String, Option<String>) {
    if let Some(glob_idx) = search_path.find(['*', '{', '}', '(', ')', '[', ']', '?'])
        && let Some(slash) = search_path[..glob_idx].rfind('/') {
            let mut path_portion = search_path[..slash].to_string();
            if !path_portion.contains('/') {
                path_portion.push('/');
            }
            return (path_portion, Some(search_path[slash + 1..].to_string()));
        }
    (search_path.to_string(), None)
}

fn lexical_normalize(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for c in p.components() {
        match c {
            Component::ParentDir => {
                if !out.pop() {
                    out.push("..");
                }
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// A folder to search with the globs (relative to it) a file must match.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchPathPattern {
    pub dir: PathBuf,
    pub patterns: Vec<String>,
}

/// Result of [`parse_search_paths`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ParsedPatterns {
    /// Folders from `./x`, `../x` or absolute segments. Empty = search the root.
    pub search_paths: Vec<SearchPathPattern>,
    /// Expanded globs relative to the folder being searched.
    pub patterns: Vec<String>,
}

/// Upstream `QueryBuilder.parseSearchPaths` for a single-folder workspace rooted at `root`.
pub fn parse_search_paths(root: &Path, entries: &[String]) -> ParsedPatterns {
    let mut result = ParsedPatterns::default();
    let mut expanded: Vec<(PathBuf, Option<String>)> = Vec::new();
    for entry in entries {
        let seg = normalize_slashes(entry);
        if !is_search_path(&seg) {
            let p = seg.trim_end_matches('/').to_string();
            let p = if p.starts_with('.') { format!("*{p}") } else { p };
            result.patterns.extend(expand_global_glob(&p));
            continue;
        }
        let (path_portion, glob_portion) = split_glob_from_path(&seg);
        let glob_portion = glob_portion.map(|g| normalize_glob_pattern(&g)).filter(|g| !g.is_empty());
        let (dir, rel_pattern) = if path_portion.starts_with('/') {
            (lexical_normalize(Path::new(&path_portion)), None)
        } else if path_portion.starts_with("../") || path_portion == ".." {
            (lexical_normalize(&root.join(&path_portion)), None)
        } else {
            let p = normalize_glob_pattern(&path_portion);
            (root.to_path_buf(), (!p.is_empty()).then_some(p))
        };
        let pattern = match (rel_pattern, glob_portion) {
            (Some(p), Some(g)) => Some(format!("{p}/{g}")),
            (p, g) => p.or(g),
        };
        expanded.push((dir.clone(), pattern.clone()));
        if let Some(p) = pattern
            && !p.ends_with("**") {
                expanded.push((dir, Some(format!("{p}/**"))));
            }
    }
    for (dir, pattern) in expanded {
        let slot = match result.search_paths.iter_mut().find(|s| s.dir == dir) {
            Some(s) => s,
            None => {
                result.search_paths.push(SearchPathPattern { dir, patterns: Vec::new() });
                result.search_paths.last_mut().expect("just pushed")
            }
        };
        if let Some(p) = pattern
            && !slot.patterns.contains(&p) {
                slot.patterns.push(p);
            }
    }
    result
}

/// `resolveResourcesForSearchIncludes` for a single-folder workspace: `./relative/path`
/// include entries for folders (Find in Folder), glob characters escaped.
pub fn resolve_resources_for_search_includes(root: &Path, resources: &[PathBuf]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for r in resources {
        let Ok(rel) = r.strip_prefix(root) else { continue };
        let rel = rel.to_string_lossy().replace('\\', "/");
        let p = if rel.is_empty() { ".".to_string() } else { format!("./{rel}") };
        let p = crate::glob::escape_glob_pattern(&p);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// Decides which files and folders of one search folder take part in the search.
#[derive(Clone, Debug, Default)]
pub struct FileFilter {
    includes: GlobMatcher,
    folder_includes: GlobMatcher,
    excludes: GlobMatcher,
    exceptions: GlobMatcher,
}

impl FileFilter {
    /// * `includes` / `folder_includes`: a file must match one of each non-empty set.
    /// * `excludes`: globs; `settings_excludes` may contain `!pattern` exceptions (re-includes),
    ///   which also cover everything below the matched folder.
    pub fn new(
        includes: &[String],
        folder_includes: &[String],
        excludes: &[String],
        settings_excludes: &[String],
        ignore_case: bool,
    ) -> Self {
        let mut all_excludes: Vec<String> = excludes.to_vec();
        let mut exceptions: Vec<String> = Vec::new();
        for s in settings_excludes {
            match s.strip_prefix('!') {
                Some(exc) => {
                    exceptions.push(exc.to_string());
                    exceptions.push(format!("{exc}/**"));
                }
                None => all_excludes.push(s.clone()),
            }
        }
        Self {
            includes: GlobMatcher::new(includes, ignore_case),
            folder_includes: GlobMatcher::new(folder_includes, ignore_case),
            excludes: GlobMatcher::new(&all_excludes, ignore_case),
            exceptions: GlobMatcher::new(&exceptions, ignore_case),
        }
    }

    /// Can the walker skip this folder (and everything in it)?
    pub fn prune_dir(&self, rel: &str) -> bool {
        self.exceptions.is_empty() && self.excludes.is_match(rel)
    }

    pub fn accept_file(&self, rel: &str) -> bool {
        if (!self.includes.is_empty() && !self.includes.is_match(rel))
            || (!self.folder_includes.is_empty() && !self.folder_includes.is_match(rel))
        {
            return false;
        }
        // A file is excluded when it, or any folder above it, matches an exclude glob
        // (folders are pruned by the walker unless exceptions exist).
        let mut excluded = self.excludes.is_match(rel);
        if !excluded {
            let mut end = 0;
            while let Some(p) = rel[end..].find('/') {
                end += p;
                if self.excludes.is_match(&rel[..end]) {
                    excluded = true;
                    break;
                }
                end += 1;
            }
        }
        !(excluded && !self.exceptions.is_match(rel))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> PathBuf {
        PathBuf::from("/ws")
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plain_segments_expand_to_any_depth() {
        let p = parse_search_paths(&root(), &s(&["src", "*.rs", ".js"]));
        assert_eq!(
            p.patterns,
            ["**/src/**", "**/src", "**/*.rs/**", "**/*.rs", "**/*.js/**", "**/*.js"]
        );
        assert!(p.search_paths.is_empty());
    }

    #[test]
    fn relative_search_path_becomes_root_relative_patterns() {
        let p = parse_search_paths(&root(), &s(&["./src/**/*.rs"]));
        assert_eq!(p.search_paths.len(), 1);
        assert_eq!(p.search_paths[0].dir, root());
        assert_eq!(p.search_paths[0].patterns, ["src/**/*.rs", "src/**/*.rs/**"]);
    }

    #[test]
    fn parent_and_absolute_search_paths_change_the_folder() {
        let p = parse_search_paths(&root(), &s(&["../other", "/abs/dir"]));
        assert_eq!(p.search_paths[0].dir, PathBuf::from("/other"));
        assert_eq!(p.search_paths[1].dir, PathBuf::from("/abs/dir"));
        assert!(p.search_paths[0].patterns.is_empty());
    }

    #[test]
    fn split_input_respects_braces() {
        assert_eq!(split_pattern_input("*.ts, {a,b}/**,  "), ["*.ts", "{a,b}/**"]);
    }

    #[test]
    fn filter_includes_excludes_and_exceptions() {
        let f = FileFilter::new(&s(&["**/*.rs"]), &[], &s(&["**/gen/**"]), &s(&["**/node_modules", "!node_modules/keep"]), false);
        assert!(f.accept_file("a/b.rs"));
        assert!(!f.accept_file("a/b.txt"));
        assert!(!f.accept_file("a/gen/x.rs"));
        assert!(!f.accept_file("node_modules/x.rs"));
        assert!(f.accept_file("node_modules/keep/x.rs"));
        assert!(!f.prune_dir("gen")); // exceptions disable pruning
    }

    #[test]
    fn prune_when_no_exceptions() {
        let f = FileFilter::new(&[], &[], &[], &s(&["**/node_modules"]), false);
        assert!(f.prune_dir("a/node_modules"));
        assert!(!f.prune_dir("a/src"));
    }
}
