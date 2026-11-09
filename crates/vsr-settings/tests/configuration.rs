//! Tests for the default + user configuration layering.
//!
//! upstream: src/vs/platform/configuration/test/common/configurationModels.test.ts @ 50e1350fc78d
//! These are our own tests of the same behaviours (layering, change events, affectsConfiguration);
//! no upstream test is ported 1:1.

use serde::Deserialize;
use serde_json::json;
use vsr_settings::{Configuration, SettingsRegistry, UserSettings};

fn config(user: &str) -> Configuration {
    Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap())
}

#[test]
fn defaults_apply_when_the_user_sets_nothing() {
    let c = config("");
    assert_eq!(c.get_f64("editor.fontSize"), Some(12.0));
    assert_eq!(c.get_bool("editor.insertSpaces"), Some(true));
    assert_eq!(c.get_str("workbench.sideBar.location"), Some("left"));
    assert_eq!(c.get_value("no.such.key"), None);
}

#[test]
fn user_values_win_over_defaults() {
    let c = config(r#"{ "editor.fontSize": 16, "workbench.sideBar.location": "right", "editor.insertSpaces": false }"#);
    assert_eq!(c.get_f64("editor.fontSize"), Some(16.0));
    assert_eq!(c.get_str("workbench.sideBar.location"), Some("right"));
    assert_eq!(c.get_bool("editor.insertSpaces"), Some(false));
    // Untouched keys keep their default.
    assert_eq!(c.get_f64("editor.tabSize"), Some(4.0));
}

#[test]
fn unregistered_keys_are_returned_as_written() {
    let c = config(r#"{ "my.custom": [1, 2] }"#);
    assert_eq!(c.get_value("my.custom"), Some(&json!([1, 2])));
}

#[test]
fn wrong_type_falls_back_to_the_default() {
    let c = config(r#"{ "editor.fontSize": "big", "editor.insertSpaces": "yes" }"#);
    assert_eq!(c.get_f64("editor.fontSize"), Some(12.0));
    assert_eq!(c.get_bool("editor.insertSpaces"), Some(true));
    // cursorWidth is an integer setting: 1.5 is not valid.
    assert_eq!(config(r#"{ "editor.cursorWidth": 1.5 }"#).get_f64("editor.cursorWidth"), Some(0.0));
}

#[test]
fn value_not_in_enum_falls_back_to_the_default() {
    let c = config(r#"{ "workbench.sideBar.location": "middle", "editor.lineNumbers": "relative" }"#);
    assert_eq!(c.get_str("workbench.sideBar.location"), Some("left"));
    assert_eq!(c.get_str("editor.lineNumbers"), Some("relative"));
}

#[test]
fn out_of_range_numbers_are_clamped_like_editor_options() {
    let c = config(r#"{ "editor.fontSize": 2, "editor.tabSize": 1000, "editor.cursorWidth": -3 }"#);
    assert_eq!(c.get_f64("editor.fontSize"), Some(6.0));
    assert_eq!(c.get_f64("editor.tabSize"), Some(100.0));
    assert_eq!(c.get_f64("editor.cursorWidth"), Some(0.0));
}

#[derive(Debug, PartialEq, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum CursorStyle {
    Line,
    Block,
    LineThin,
}

#[test]
fn typed_enum_access() {
    let c = config(r#"{ "editor.cursorStyle": "line-thin" }"#);
    assert_eq!(c.get::<CursorStyle>("editor.cursorStyle"), Some(CursorStyle::LineThin));
    assert_eq!(config("").get_enum::<CursorStyle>("editor.cursorStyle"), Some(CursorStyle::Line));
    assert_eq!(config(r#"{ "editor.cursorStyle": "block" }"#).get_enum::<CursorStyle>("editor.cursorStyle"), Some(CursorStyle::Block));
    // A valid enum value this Rust type does not know falls back to the default through get_enum.
    let c = config(r#"{ "editor.cursorStyle": "underline" }"#);
    assert_eq!(c.get::<CursorStyle>("editor.cursorStyle"), None);
    assert_eq!(c.get_enum::<CursorStyle>("editor.cursorStyle"), Some(CursorStyle::Line));
}

#[test]
fn update_user_reports_added_changed_and_removed_keys() {
    let mut c = config(r#"{ "editor.fontSize": 14, "editor.tabSize": 2 }"#);
    let change = c.update_user(UserSettings::parse(r#"{ "editor.fontSize": 16, "files.autoSave": "afterDelay" }"#).unwrap());
    let keys: Vec<&str> = change.affected_keys.iter().map(String::as_str).collect();
    assert_eq!(keys, ["editor.fontSize", "editor.tabSize", "files.autoSave"]);
    assert_eq!(c.get_f64("editor.fontSize"), Some(16.0));
    assert_eq!(c.get_f64("editor.tabSize"), Some(4.0), "removed key goes back to the default");

    // The same document again changes nothing.
    let same = c.update_user(c.user().clone());
    assert!(same.is_empty());
}

#[test]
fn affects_matches_whole_segments_like_affects_configuration() {
    let mut c = config("");
    let change = c.update_user(UserSettings::parse(r#"{ "editor.fontSize": 14 }"#).unwrap());
    assert!(change.affects("editor"));
    assert!(change.affects("editor.fontSize"));
    assert!(!change.affects("editor.font"), "prefix inside a segment does not count");
    assert!(!change.affects("editor.fontSize.x"));
    assert!(!change.affects("files"));
    assert!(!change.affects("edit"));
}

#[test]
fn invalid_to_valid_is_visible_in_get_value() {
    let mut c = config(r#"{ "workbench.sideBar.location": "nope" }"#);
    assert_eq!(c.get_str("workbench.sideBar.location"), Some("left"));
    c.update_user(UserSettings::parse(r#"{ "workbench.sideBar.location": "right" }"#).unwrap());
    assert_eq!(c.get_str("workbench.sideBar.location"), Some("right"));
}
