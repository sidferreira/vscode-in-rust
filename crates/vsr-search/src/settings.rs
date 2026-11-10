//! The user settings that drive a search, with upstream defaults.
//!
//! upstream: src/vs/workbench/contrib/search/browser/search.common.contribution.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/files.contribution.ts @ 50e1350fc78d
//!
//! This is a plain struct on purpose: the configuration service (a gpui global) is built
//! elsewhere. TODO(search-settings): fill this from the `Configuration` global once it lands
//! (see `search_view::SearchSettingsSource` in vsr-app).

/// `files.exclude` / `search.exclude`: glob pattern -> enabled. Order is preserved.
pub type ExcludeMap = Vec<(String, bool)>;

#[derive(Clone, Debug, PartialEq)]
pub struct SearchSettings {
    /// `files.exclude`.
    pub files_exclude: ExcludeMap,
    /// `search.exclude` (inherits `files.exclude`).
    pub search_exclude: ExcludeMap,
    /// `search.useIgnoreFiles`.
    pub use_ignore_files: bool,
    /// `search.useGlobalIgnoreFiles`.
    pub use_global_ignore_files: bool,
    /// `search.useParentIgnoreFiles`.
    pub use_parent_ignore_files: bool,
    /// `search.followSymlinks`.
    pub follow_symlinks: bool,
    /// `search.smartCase`.
    pub smart_case: bool,
    /// `search.maxResults` (`None` = null = unlimited).
    pub max_results: Option<usize>,
    /// `search.searchOnType`.
    pub search_on_type: bool,
    /// `search.searchOnTypeDebouncePeriod` in milliseconds.
    pub search_on_type_debounce_ms: u64,
    /// `search.showLineNumbers`.
    pub show_line_numbers: bool,
    /// `search.collapseResults` (`auto`, `alwaysCollapse`, `alwaysExpand`).
    pub collapse_results: CollapseResults,
    /// `search.ripgrep.maxThreads` (0 = automatic).
    pub threads: usize,
    /// `search.sortOrder`: how the file rows of the results are ordered.
    pub sort_order: ResultSortOrder,
    /// `search.seedOnFocus`: the query takes the editor's selection when the Search view is focused.
    pub seed_on_focus: bool,
    /// `search.seedWithNearestWord`: with no selection, the query takes the word at the caret.
    pub seed_with_nearest_word: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ResultSortOrder {
    /// By folder, then file name.
    #[default]
    Default,
    FileNames,
    /// By the file's extension, then by path.
    Type,
    /// Most recently modified first.
    Modified,
    /// Files with more matches first.
    CountDescending,
    /// Files with fewer matches first.
    CountAscending,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CollapseResults {
    Auto,
    AlwaysCollapse,
    AlwaysExpand,
}

/// `DEFAULT_MAX_SEARCH_RESULTS`.
pub const DEFAULT_MAX_SEARCH_RESULTS: usize = 20000;

impl Default for SearchSettings {
    fn default() -> Self {
        let on = |keys: &[&str]| keys.iter().map(|k| (k.to_string(), true)).collect();
        Self {
            files_exclude: on(&[
                "**/.git",
                "**/.svn",
                "**/.hg",
                "**/.jj",
                "**/.DS_Store",
                "**/Thumbs.db",
            ]),
            search_exclude: on(&["**/node_modules", "**/bower_components", "**/*.code-search"]),
            use_ignore_files: true,
            use_global_ignore_files: false,
            use_parent_ignore_files: false,
            follow_symlinks: true,
            smart_case: false,
            max_results: Some(DEFAULT_MAX_SEARCH_RESULTS),
            search_on_type: true,
            search_on_type_debounce_ms: 300,
            show_line_numbers: false,
            collapse_results: CollapseResults::AlwaysExpand,
            threads: 0,
            sort_order: ResultSortOrder::Default,
            seed_on_focus: false,
            seed_with_nearest_word: false,
        }
    }
}

impl SearchSettings {
    /// The enabled exclude globs of `files.exclude` and (unless `disregard_search_exclude`)
    /// `search.exclude`; later entries override earlier ones with the same key
    /// (upstream `getExcludes`).
    pub fn exclude_globs(&self, disregard_search_exclude: bool) -> Vec<String> {
        let mut merged: ExcludeMap = Vec::new();
        let layers: &[&ExcludeMap] = if disregard_search_exclude {
            &[&self.files_exclude]
        } else {
            &[&self.files_exclude, &self.search_exclude]
        };
        for layer in layers {
            for (k, v) in layer.iter() {
                match merged.iter_mut().find(|(mk, _)| mk == k) {
                    Some(slot) => slot.1 = *v,
                    None => merged.push((k.clone(), *v)),
                }
            }
        }
        merged.into_iter().filter(|(_, on)| *on).map(|(k, _)| k).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_upstream() {
        let s = SearchSettings::default();
        assert_eq!(s.max_results, Some(20000));
        assert!(s.use_ignore_files && !s.use_global_ignore_files && !s.use_parent_ignore_files);
        assert!(s.follow_symlinks && !s.smart_case && s.search_on_type);
        assert_eq!(s.search_on_type_debounce_ms, 300);
        let g = s.exclude_globs(false);
        assert!(g.contains(&"**/node_modules".to_string()) && g.contains(&"**/.git".to_string()));
        assert!(!s.exclude_globs(true).contains(&"**/node_modules".to_string()));
        assert_eq!((s.threads, s.sort_order), (0, ResultSortOrder::Default));
        assert!(!s.seed_on_focus && !s.seed_with_nearest_word);
    }

    #[test]
    fn later_layer_overrides_and_disables() {
        let mut s = SearchSettings::default();
        s.search_exclude.push(("**/.git".into(), false));
        assert!(!s.exclude_globs(false).contains(&"**/.git".to_string()));
    }
}
