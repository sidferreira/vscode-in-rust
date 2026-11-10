//! upstream: src/vs/workbench/api/browser/mainThreadWorkspace.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/common/queryBuilder.ts @ 50e1350fc78d
//!
//! `MainThreadWorkspace` for `workspace.findFiles` (`$startFileSearch`) and `workspace.fs`-less
//! existence checks (`$checkExists`, the `workspaceContains:` activation), ADR-0089. The query is
//! planned as upstream's query builder does (excludes from `files.exclude` and `search.exclude`
//! unless the call disregards them), then listed by `vsr-search` over the workspace folders.
//! Text search (`$startTextSearch`) is not served: the host falls back to its own answer.

use crate::file_system::file_path;
use crate::main_thread::arg;
use serde_json::Value;
use std::path::PathBuf;
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::file_uri;
use vsr_rpc::rpc::{Call, HandlerError};
use vsr_search::files::{FileSearchOptions, any_file, find_files};
use vsr_search::settings::{ExcludeMap, SearchSettings};

/// The search settings that matter for listing files, from the default and user layers (nested
/// JSON, as the configuration service keeps them). Upstream's defaults are the starting point.
pub fn settings_from(defaults: &Value, user: &Value) -> SearchSettings {
    let mut s = SearchSettings::default();
    for layer in [defaults, user] {
        merge_excludes(&mut s.files_exclude, layer.pointer("/files/exclude"));
        merge_excludes(&mut s.search_exclude, layer.pointer("/search/exclude"));
        if let Some(b) = layer
            .pointer("/search/useIgnoreFiles")
            .and_then(Value::as_bool)
        {
            s.use_ignore_files = b;
        }
        if let Some(b) = layer
            .pointer("/search/followSymlinks")
            .and_then(Value::as_bool)
        {
            s.follow_symlinks = b;
        }
    }
    s
}

/// `files.exclude` / `search.exclude`: entries whose value is a boolean (the object form with a
/// `when` clause is not used here). Later layers override earlier ones with the same key.
fn merge_excludes(map: &mut ExcludeMap, value: Option<&Value>) {
    let Some(obj) = value.and_then(Value::as_object) else {
        return;
    };
    for (glob, on) in obj {
        let Some(on) = on.as_bool() else { continue };
        match map.iter_mut().find(|(k, _)| k == glob) {
            Some(slot) => slot.1 = on,
            None => map.push((glob.clone(), on)),
        }
    }
}

/// A planned file query: what `plan_file_query` decided from the call's options and settings.
#[derive(Debug, Clone, PartialEq)]
pub struct FileQuery {
    pub includes: Vec<String>,
    pub excludes: Vec<String>,
    pub settings_excludes: Vec<String>,
    pub use_ignore_files: bool,
    pub follow_symlinks: bool,
    pub ignore_case: bool,
    pub max_results: Option<usize>,
}

/// The query for one `$startFileSearch` options object (`IFileQueryBuilderOptions`).
///
/// * the include is `includePattern` (or `filePattern`, the findFiles2 form), if any;
/// * `excludePattern` entries are `{ pattern }` objects or strings, applied on top of the settings;
/// * `disregardExcludeSettings` drops both `files.exclude` and `search.exclude`;
///   `disregardSearchExcludeSettings` drops `search.exclude` only;
/// * `disregardIgnoreFiles` (the legacy `findFiles` always sends it) turns ignore files off, else
///   `search.useIgnoreFiles` decides; `ignoreSymlinks` overrides `search.followSymlinks`.
pub fn plan_file_query(options: &Value, settings: &SearchSettings) -> FileQuery {
    let flag = |k: &str| options.get(k).and_then(Value::as_bool).unwrap_or(false);
    let text = |k: &str| {
        options
            .get(k)
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };
    let include = text("includePattern").or_else(|| text("filePattern"));
    let excludes = options
        .get("excludePattern")
        .and_then(Value::as_array)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|e| match e {
                    Value::String(s) => Some(s.clone()),
                    other => other
                        .get("pattern")
                        .and_then(Value::as_str)
                        .map(str::to_string),
                })
                .collect()
        })
        .unwrap_or_default();
    let settings_excludes = if flag("disregardExcludeSettings") {
        Vec::new()
    } else {
        settings.exclude_globs(flag("disregardSearchExcludeSettings"))
    };
    let disregard_ignore = options.get("disregardIgnoreFiles").and_then(Value::as_bool);
    let use_ignore_files = match disregard_ignore {
        Some(d) => !d,
        None => settings.use_ignore_files,
    };
    let follow_symlinks = match options.get("ignoreSymlinks").and_then(Value::as_bool) {
        Some(ignore) => !ignore,
        None => settings.follow_symlinks,
    };
    FileQuery {
        includes: include.into_iter().collect(),
        excludes,
        settings_excludes,
        use_ignore_files,
        follow_symlinks,
        ignore_case: flag("ignoreGlobCase"),
        max_results: options
            .get("maxResults")
            .and_then(Value::as_u64)
            .map(|n| n as usize),
    }
}

/// The workspace folders and the search settings the query runs with.
#[derive(Debug, Default)]
pub struct WorkspaceService {
    pub folders: Vec<PathBuf>,
    pub settings: SearchSettings,
}

impl WorkspaceService {
    /// The service for a workspace with `folders` and the given default and user layers.
    pub fn new(folders: Vec<PathBuf>, defaults: &Value, user: &Value) -> Self {
        Self {
            folders,
            settings: settings_from(defaults, user),
        }
    }

    /// Settings changed (`$acceptConfigurationChanged`): the layers as the host gets them.
    pub fn set_layers(&mut self, defaults: &Value, user: &Value) {
        self.settings = settings_from(defaults, user);
    }

    pub fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        Some(match call.method.as_str() {
            "$startFileSearch" => self.start_file_search(call).map(Reply::Json),
            "$checkExists" => self.check_exists(call).map(Reply::Json),
            _ => return None,
        })
    }

    /// `$startFileSearch(includeFolder | null, options, token)`: the matching files as URIs.
    fn start_file_search(&self, call: &Call) -> Result<Value, HandlerError> {
        let roots = match arg(call, 0) {
            Value::Null => self.folders.clone(),
            folder => vec![file_path(folder)?],
        };
        let options = arg(call, 1);
        let query = plan_file_query(options, &self.settings);
        let mut remaining = query.max_results;
        let mut out = Vec::new();
        for root in roots {
            if remaining == Some(0) {
                break;
            }
            let found = find_files(&self.options_for(root, &query, remaining));
            if let Some(r) = remaining.as_mut() {
                *r = r.saturating_sub(found.len());
            }
            out.extend(found.iter().map(|p| file_uri(p)));
        }
        Ok(Value::Array(out))
    }

    /// `$checkExists(folders, includes, token)`: does any file under `folders` match `includes`?
    fn check_exists(&self, call: &Call) -> Result<Value, HandlerError> {
        let folders: Vec<PathBuf> = match arg(call, 0).as_array() {
            Some(list) => list.iter().map(file_path).collect::<Result<_, _>>()?,
            None => self.folders.clone(),
        };
        let includes: Vec<String> = arg(call, 1)
            .as_array()
            .map(|l| {
                l.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        let query = FileQuery {
            includes,
            excludes: Vec::new(),
            settings_excludes: self.settings.exclude_globs(false),
            use_ignore_files: self.settings.use_ignore_files,
            follow_symlinks: self.settings.follow_symlinks,
            ignore_case: false,
            max_results: Some(1),
        };
        let hit = folders
            .into_iter()
            .any(|root| any_file(&self.options_for(root, &query, Some(1))));
        Ok(Value::Bool(hit))
    }

    fn options_for(
        &self,
        root: PathBuf,
        query: &FileQuery,
        max_results: Option<usize>,
    ) -> FileSearchOptions {
        FileSearchOptions {
            root,
            includes: query.includes.clone(),
            excludes: query.excludes.clone(),
            settings_excludes: query.settings_excludes.clone(),
            use_ignore_files: query.use_ignore_files,
            follow_symlinks: query.follow_symlinks,
            ignore_case: query.ignore_case,
            max_results,
        }
    }
}
