//! upstream: src/vs/workbench/services/search/common/queryBuilder.ts @ 50e1350fc78d
//!
//! File-name search for `workspace.findFiles` (`MainThreadWorkspace.$startFileSearch`): the same
//! walker and glob filter as find-in-files ([`crate::engine`], [`crate::patterns::FileFilter`]),
//! listing paths instead of matching text. UI free.

use crate::patterns::FileFilter;
use ignore::WalkBuilder;
use std::path::PathBuf;
use std::sync::Arc;

/// One folder to list.
#[derive(Clone, Debug)]
pub struct FileSearchOptions {
    /// The folder (a workspace root).
    pub root: PathBuf,
    /// Include globs, relative to `root`. Empty = every file.
    pub includes: Vec<String>,
    /// Exclude globs from the query (`!x` exceptions are not used here).
    pub excludes: Vec<String>,
    /// Enabled `files.exclude` / `search.exclude` globs (`!x` entries are exceptions).
    pub settings_excludes: Vec<String>,
    /// Honour `.gitignore`-style ignore files (`search.useIgnoreFiles`).
    pub use_ignore_files: bool,
    pub follow_symlinks: bool,
    pub ignore_case: bool,
    /// `None` = unlimited.
    pub max_results: Option<usize>,
}

/// The files under `options.root` that the filter accepts, sorted; stops after `max_results`
/// (in walk order, then sorted).
pub fn find_files(options: &FileSearchOptions) -> Vec<PathBuf> {
    let filter = Arc::new(FileFilter::new(
        &options.includes,
        &[],
        &options.excludes,
        &options.settings_excludes,
        options.ignore_case,
    ));
    let local = options.use_ignore_files;
    let mut wb = WalkBuilder::new(&options.root);
    wb.hidden(false) // dotfiles are listed, as in upstream's file search
        .require_git(false)
        .follow_links(options.follow_symlinks)
        .git_ignore(local)
        .git_exclude(local)
        .ignore(local)
        .git_global(false)
        .parents(false);
    if local {
        wb.add_custom_ignore_filename(".rgignore");
    }
    let base = options.root.clone();
    let prune = Arc::clone(&filter);
    wb.filter_entry(move |e| {
        if e.depth() == 0 || !e.file_type().is_some_and(|t| t.is_dir()) {
            return true;
        }
        match e.path().strip_prefix(&base) {
            Ok(rel) => !prune.prune_dir(&rel.to_string_lossy()),
            Err(_) => true,
        }
    });

    let mut found = Vec::new();
    for entry in wb.build().flatten() {
        if options.max_results.is_some_and(|max| found.len() >= max) {
            break;
        }
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let Ok(rel) = entry.path().strip_prefix(&options.root) else {
            continue;
        };
        let rel = rel.to_string_lossy().replace('\\', "/");
        if filter.accept_file(&rel) {
            found.push(entry.into_path());
        }
    }
    found.sort();
    found
}

/// Does any file under `root` match? Stops at the first hit.
pub fn any_file(options: &FileSearchOptions) -> bool {
    let mut o = options.clone();
    o.max_results = Some(1);
    !find_files(&o).is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn tree(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("vsr-files-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for f in [
            "a.ts",
            "src/b.ts",
            "src/deep/c.ts",
            "src/readme.md",
            "node_modules/pkg/index.ts",
            "ignored.log",
            ".dot/e.ts",
        ] {
            let p = root.join(f);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, "x").unwrap();
        }
        fs::write(root.join(".gitignore"), "*.log\n").unwrap();
        root
    }

    fn rel(root: &Path, found: &[PathBuf]) -> Vec<String> {
        found
            .iter()
            .map(|p| {
                p.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect()
    }

    fn options(root: &Path) -> FileSearchOptions {
        FileSearchOptions {
            root: root.to_path_buf(),
            includes: vec![],
            excludes: vec![],
            settings_excludes: vec![],
            use_ignore_files: true,
            follow_symlinks: true,
            ignore_case: false,
            max_results: None,
        }
    }

    #[test]
    fn include_globs_select_files_at_any_depth() {
        let root = tree("include");
        let mut o = options(&root);
        o.includes = vec!["**/*.ts".into()];
        let found = rel(&root, &find_files(&o));
        assert!(found.contains(&"a.ts".to_string()));
        assert!(found.contains(&"src/deep/c.ts".to_string()));
        assert!(!found.iter().any(|f| f.ends_with(".md")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn settings_excludes_prune_folders_and_exceptions_keep_them() {
        let root = tree("exclude");
        let mut o = options(&root);
        o.includes = vec!["**/*.ts".into()];
        o.settings_excludes = vec!["**/node_modules".into()];
        assert!(
            !rel(&root, &find_files(&o))
                .iter()
                .any(|f| f.starts_with("node_modules"))
        );
        // `!` re-includes what an exclude removed.
        o.settings_excludes = vec![
            "**/node_modules".into(),
            "!**/node_modules/pkg/index.ts".into(),
        ];
        assert!(rel(&root, &find_files(&o)).contains(&"node_modules/pkg/index.ts".to_string()));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn query_excludes_apply_on_top_of_the_settings() {
        let root = tree("query-exclude");
        let mut o = options(&root);
        o.includes = vec!["**/*.ts".into()];
        o.excludes = vec!["src/**".into()];
        let found = rel(&root, &find_files(&o));
        assert!(found.contains(&"a.ts".to_string()));
        assert!(!found.iter().any(|f| f.starts_with("src/")));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn ignore_files_apply_only_when_asked() {
        let root = tree("ignore");
        let mut o = options(&root);
        o.includes = vec!["*.log".into()];
        assert!(find_files(&o).is_empty(), ".gitignore hides ignored.log");
        o.use_ignore_files = false;
        assert_eq!(rel(&root, &find_files(&o)), vec!["ignored.log".to_string()]);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn max_results_limits_the_listing_and_any_file_stops_early() {
        let root = tree("max");
        let mut o = options(&root);
        o.includes = vec!["**/*.ts".into()];
        o.max_results = Some(2);
        assert_eq!(find_files(&o).len(), 2);
        assert!(any_file(&options(&root)));
        let _ = fs::remove_dir_all(&root);
    }
}
