use std::path::PathBuf;

use serde_json::json;
use vsr_settings::{UserSettings, default_user_settings_path};

#[test]
fn accepts_comments_and_trailing_commas() {
    let text = r#"
// rscode user settings
{
    /* font */
    "editor.fontSize": 14, // px
    "files.exclude": {
        "**/.git": true,
    },
    "url": "http://example.com/*not a comment*/",
    "list": [1, 2,],
}
"#;
    let settings = UserSettings::parse(text).expect("valid JSONC");
    assert_eq!(settings.get("editor.fontSize"), Some(&json!(14)));
    assert_eq!(settings.get("files.exclude"), Some(&json!({"**/.git": true})));
    assert_eq!(settings.get("url"), Some(&json!("http://example.com/*not a comment*/")));
    assert_eq!(settings.get("list"), Some(&json!([1, 2])));
}

#[test]
fn empty_document_and_non_object_documents() {
    assert!(UserSettings::parse("  \n ").expect("empty is fine").is_empty());
    assert!(UserSettings::parse("[1, 2]").is_err());
    assert!(UserSettings::parse("{ \"a\": }").is_err());
}

#[test]
fn set_reset_and_is_modified() {
    let mut settings = UserSettings::default();
    assert!(!settings.is_modified("editor.fontSize"));
    settings.set("editor.fontSize", json!(16));
    assert!(settings.is_modified("editor.fontSize"));
    assert_eq!(settings.get("editor.fontSize"), Some(&json!(16)));
    settings.reset("editor.fontSize");
    assert!(!settings.is_modified("editor.fontSize"));
    assert_eq!(settings.get("editor.fontSize"), None);
    settings.reset("never.set");
}

#[test]
fn save_and_load_round_trip() {
    let path: PathBuf = std::env::temp_dir().join(format!("vsr-settings-roundtrip-{}.json", std::process::id()));
    let mut settings = UserSettings::default();
    settings.set("editor.fontSize", json!(15));
    settings.set("editor.wordWrap", json!("on"));
    settings.set("files.exclude", json!({"**/tmp": true}));
    settings.save(&path).expect("save");
    let loaded = UserSettings::load(&path).expect("load");
    assert_eq!(loaded, settings);
    std::fs::remove_file(&path).expect("remove our own test file");
}

#[test]
fn missing_file_loads_as_empty() {
    let path = std::env::temp_dir().join(format!("vsr-settings-missing-{}.json", std::process::id()));
    assert!(UserSettings::load(&path).expect("missing is empty").is_empty());
}

#[test]
fn default_path_is_rscode_not_vscode() {
    let path = default_user_settings_path();
    let text = path.to_string_lossy();
    assert!(text.ends_with("rscode/User/settings.json"), "{text}");
    assert!(!text.contains("vscode-rust"));
    if cfg!(target_os = "macos") {
        assert!(text.contains("Library/Application Support/rscode/User/settings.json"), "{text}");
    }
}
