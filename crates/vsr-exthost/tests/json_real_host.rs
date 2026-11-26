//! upstream: extensions/json-language-features/client/src/node/jsonClientMain.ts @ 50e1350fc78d
//!
//! Code-OSS's own `json-language-features` in the real host, end to end (ADR-0101, ADR-0102):
//! it activates on `onLanguage:json`, its server (`server/out/node/jsonServerMain`) runs as a child
//! process, a syntax error in an open JSON editor becomes a marker in `MainThreadDiagnostics`, and
//! a `$schema` next to the file drives completion. The server reports diagnostics by *pull*
//! (`textDocument/diagnostic`) and the client only pulls for documents that are visible, so the
//! document must have an editor. Skips (and says why) without `RSCODE_CODE_OSS` or the built client.

mod common;

use common::{T, scratch, wait_until};
use serde_json::json;
use std::path::PathBuf;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_exthost::language_features::dto::Position;
use vsr_exthost::language_features::query::CompletionTrigger;
use vsr_rpc::handshake::Launch;

fn json_extension_dir() -> Option<PathBuf> {
    let root = std::env::var_os("RSCODE_CODE_OSS")?;
    let dir = PathBuf::from(root).join("extensions/json-language-features");
    dir.join("client/out/node/jsonClientMain.js").exists().then_some(dir)
}

/// A started host with the JSON extension, and the workspace folder; `None` (printing why) to skip.
fn boot(name: &str) -> Option<(ExtensionHost, PathBuf)> {
    boot_with(name, json!({}))
}

fn boot_with(name: &str, user_settings: serde_json::Value) -> Option<(ExtensionHost, PathBuf)> {
    let Some(dir) = json_extension_dir() else {
        eprintln!("SKIP {name}: RSCODE_CODE_OSS not set or json-language-features not built");
        return None;
    };
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = scratch(name);
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        socket_dir: Some(vsr_test_scratch::socket_dir!()),
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace.clone())),
        extensions: vec![ExtensionDescription::from_dir(&dir).unwrap()],
        default_settings: json!({}),
        user_settings,
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T).unwrap_or_else(|e| panic!("start: {e}"));
    Some((host, workspace))
}

fn open(host: &mut ExtensionHost, file: &std::path::Path, text: &str) {
    host.open_document(file, text, "json", T).unwrap();
    host.show_editor(file, T).unwrap();
    host.activate_by_event("onLanguage:json", T).unwrap();
}

#[test]
fn json_language_features_activates_and_registers_its_providers() {
    let Some((mut host, ws)) = boot("json-activate") else {
        return;
    };
    open(&mut host, &ws.join("a.json"), "{}\n");
    assert!(
        host.main_thread.extensions.activation_errors.is_empty(),
        "{:?}",
        host.main_thread.extensions.activation_errors
    );
    assert!(
        !host.main_thread.language_features.registrations.is_empty(),
        "json-language-features registered no providers"
    );
}

#[test]
fn a_json_syntax_error_becomes_a_marker() {
    let Some((mut host, ws)) = boot("json-marker") else {
        return;
    };
    let file = ws.join("a.json");
    open(&mut host, &file, "{ \"name\" \"x\" }\n");
    let key = format!("file://{}", file.display());
    wait_until(&mut host, |h| !h.main_thread.diagnostics.markers_for(&key).is_empty());
    let markers = host.main_thread.diagnostics.markers_for(&key);
    assert_eq!(markers[0]["message"], "Colon expected");
    assert_eq!(markers[0]["severity"], 8, "an error");
    assert_eq!(markers[0]["source"], "json");
    assert_eq!(markers[0]["startLineNumber"], 1);
}

#[test]
fn a_schema_next_to_the_file_drives_completion() {
    let Some((mut host, ws)) = boot("json-schema") else {
        return;
    };
    std::fs::write(
        ws.join("schema.json"),
        r#"{"type":"object","properties":{"color":{"type":"string","enum":["red","green"]}}}"#,
    )
    .unwrap();
    let file = ws.join("a.json");
    open(&mut host, &file, "{ \"$schema\": \"./schema.json\", \"color\": \"\" }\n");
    // Inside the empty string: 1-based column 41 is between the quotes.
    let col = "{ \"$schema\": \"./schema.json\", \"color\": \"".len() as u32 + 1;
    let deadline = std::time::Instant::now() + T;
    loop {
        let got = host
            .provide_completion_items(&file, Position::new(1, col), CompletionTrigger::Invoke, T)
            .unwrap();
        let labels: Vec<String> = got
            .results
            .iter()
            .flat_map(|(_, l)| l.items.iter().map(|i| i.label.clone()))
            .collect();
        if labels.iter().any(|l| l.contains("red")) && labels.iter().any(|l| l.contains("green")) {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "no schema completions: {labels:?} {:?}",
            got.errors
        );
        host.pump(std::time::Duration::from_millis(200));
    }
}

#[test]
fn json_validate_enable_false_silences_the_markers() {
    let Some((mut host, ws)) = boot_with(
        "json-validate-off",
        json!({ "json": { "validate": { "enable": false } } }),
    ) else {
        return;
    };
    let file = ws.join("a.json");
    open(&mut host, &file, "{ \"name\" \"x\" }\n");
    // The markers arrive within a second or two when validation is on (the test above); give
    // the server the same time and expect none.
    let until = std::time::Instant::now() + std::time::Duration::from_secs(6);
    while std::time::Instant::now() < until {
        host.pump(std::time::Duration::from_millis(100));
    }
    let key = format!("file://{}", file.display());
    assert!(host.main_thread.diagnostics.markers_for(&key).is_empty());
}

#[test]
fn json_schemas_setting_associates_a_schema_with_matching_files() {
    let Some((mut host, ws)) = boot_with(
        "json-schemas-setting",
        json!({ "json": { "schemas": [{
            "fileMatch": ["*.conf.json"],
            "schema": { "type": "object", "properties": { "mode": { "enum": ["fast", "slow"] } } }
        }] } }),
    ) else {
        return;
    };
    let file = ws.join("a.conf.json");
    open(&mut host, &file, "{ \"mode\": \"\" }\n");
    let col = "{ \"mode\": \"".len() as u32 + 1;
    let deadline = std::time::Instant::now() + T;
    loop {
        let got = host
            .provide_completion_items(&file, Position::new(1, col), CompletionTrigger::Invoke, T)
            .unwrap();
        let labels: Vec<String> = got
            .results
            .iter()
            .flat_map(|(_, l)| l.items.iter().map(|i| i.label.clone()))
            .collect();
        if labels.iter().any(|l| l.contains("fast")) && labels.iter().any(|l| l.contains("slow")) {
            return;
        }
        assert!(std::time::Instant::now() < deadline, "{labels:?}");
        host.pump(std::time::Duration::from_millis(200));
    }
}

/// The text of the range formatting edits (the JSON client registers a range provider only), polled for `secs` when none are expected.
fn format_json(settings: serde_json::Value, expect: bool) -> Option<Vec<String>> {
    use vsr_exthost::language_features::dto::{FormattingOptions, Range};
    let (mut host, ws) = boot_with("json-format", settings)?;
    let file = ws.join("a.json");
    open(&mut host, &file, "{\"a\":1,\"b\":[1,2]}\n");
    let deadline = std::time::Instant::now()
        + if expect { T } else { std::time::Duration::from_secs(4) };
    loop {
        let got = host
            .provide_range_formatting_edits(
                &file,
                Range { start_line: 1, start_column: 1, end_line: 2, end_column: 1 },
                &FormattingOptions {
                    tab_size: 2,
                    insert_spaces: true,
                },
                T,
            )
            .unwrap();
        let texts: Vec<String> = got.flatten().into_iter().map(|e| e.text).collect();
        if !texts.is_empty() || std::time::Instant::now() >= deadline {
            return Some(texts);
        }
        host.pump(std::time::Duration::from_millis(200));
    }
}

#[test]
fn json_format_enable_false_registers_no_formatter() {
    let Some(on) = format_json(json!({}), true) else {
        return;
    };
    assert!(!on.is_empty(), "formatter on by default");
    let off = format_json(json!({ "json": { "format": { "enable": false } } }), false).unwrap();
    assert!(off.is_empty(), "{off:?}");
}

#[test]
fn json_format_keep_lines_keeps_a_single_line_document_on_one_line() {
    let Some(default) = format_json(json!({}), true) else {
        return;
    };
    let keep = format_json(json!({ "json": { "format": { "keepLines": true } } }), true).unwrap();
    assert!(default.join("").contains('\n'), "{default:?}");
    assert!(!keep.join("").contains('\n'), "{keep:?}");
}
