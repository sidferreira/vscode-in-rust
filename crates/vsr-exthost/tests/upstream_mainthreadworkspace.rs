//! upstream: src/vs/workbench/api/test/browser/mainThreadWorkspace.test.ts @ 50e1350fc78d
//!
//! The file-search query of `$startFileSearch`, planned from the same options and configuration as
//! upstream's tests. Upstream asserts on the query its search service receives; here the planned
//! [`FileQuery`] carries the same parts (includes, excludes from the query and from the settings).
//! Upstream stubs the configuration to exactly the entries each test names, so the settings maps
//! start empty.

use serde_json::json;
use vsr_exthost::workspace_search::plan_file_query;
use vsr_search::settings::SearchSettings;

fn empty_settings() -> SearchSettings {
    SearchSettings {
        files_exclude: vec![],
        search_exclude: vec![],
        ..SearchSettings::default()
    }
}

// upstream: MainThreadWorkspace > simple
#[test]
fn simple() {
    let q = plan_file_query(
        &json!({ "maxResults": 10, "includePattern": "foo", "disregardSearchExcludeSettings": true }),
        &empty_settings(),
    );
    assert_eq!(q.includes, vec!["foo".to_string()]);
    assert_eq!(q.max_results, Some(10));
}

// upstream: MainThreadWorkspace > exclude defaults
#[test]
fn exclude_defaults() {
    let mut s = empty_settings();
    s.search_exclude = vec![("searchExclude".into(), true)];
    s.files_exclude = vec![("filesExclude".into(), true)];
    let q = plan_file_query(
        &json!({ "maxResults": 10, "includePattern": "", "disregardSearchExcludeSettings": true }),
        &s,
    );
    assert!(q.includes.is_empty());
    assert_eq!(q.settings_excludes, vec!["filesExclude".to_string()]);
    assert!(q.excludes.is_empty());
}

// upstream: MainThreadWorkspace > disregard excludes
#[test]
fn disregard_excludes() {
    let mut s = empty_settings();
    s.search_exclude = vec![("searchExclude".into(), true)];
    s.files_exclude = vec![("filesExclude".into(), true)];
    let q = plan_file_query(
        &json!({
            "maxResults": 10,
            "includePattern": "",
            "disregardSearchExcludeSettings": true,
            "disregardExcludeSettings": true
        }),
        &s,
    );
    assert!(q.excludes.is_empty());
    assert!(q.settings_excludes.is_empty());
}

// upstream: MainThreadWorkspace > do not disregard anything if disregardExcludeSettings is true
#[test]
fn do_not_disregard_anything_if_disregard_exclude_settings_is_true() {
    let mut s = empty_settings();
    s.search_exclude = vec![("searchExclude".into(), true)];
    s.files_exclude = vec![("filesExclude".into(), true)];
    let q = plan_file_query(
        &json!({
            "maxResults": 10,
            "includePattern": "",
            "disregardExcludeSettings": true,
            "disregardSearchExcludeSettings": false
        }),
        &s,
    );
    assert!(q.excludes.is_empty());
    assert!(q.settings_excludes.is_empty());
}

// upstream: MainThreadWorkspace > exclude string
#[test]
fn exclude_string() {
    let q = plan_file_query(
        &json!({
            "maxResults": 10,
            "includePattern": "",
            "excludePattern": [{ "pattern": "exclude/**" }],
            "disregardSearchExcludeSettings": true
        }),
        &empty_settings(),
    );
    assert!(q.settings_excludes.is_empty());
    assert_eq!(q.excludes, vec!["exclude/**".to_string()]);
}
