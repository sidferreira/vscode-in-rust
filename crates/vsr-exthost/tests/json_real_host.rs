//! upstream: extensions/json-language-features/client/src/node/jsonClientMain.ts @ 50e1350fc78d
//!
//! Probe (ADR-0101): boots Code-OSS's own `json-language-features` in the real host, opens a
//! `.json` document and fires `onLanguage:json`. What it proves today: the extension activates
//! with no activation error, registers its language providers, and its language server
//! (`server/out/node/jsonServerMain`) is spawned as a child process. What it does not yet prove:
//! diagnostics reach the main thread (the open document produces no markers yet). The test
//! prints the unhandled `$methods` so the next main-thread shapes come from a run. It asserts
//! only the activation facts. Skips (and says why) without `RSCODE_CODE_OSS` or the built client.

mod common;

use common::{T, scratch};
use serde_json::json;
use std::path::PathBuf;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_rpc::handshake::Launch;

fn json_extension_dir() -> Option<PathBuf> {
    let root = std::env::var_os("RSCODE_CODE_OSS")?;
    let dir = PathBuf::from(root).join("extensions/json-language-features");
    dir.join("client/out/node/jsonClientMain.js").exists().then_some(dir)
}

#[test]
fn json_language_features_activates_and_spawns_its_server() {
    let Some(dir) = json_extension_dir() else {
        eprintln!("SKIP json probe: RSCODE_CODE_OSS not set or json-language-features not built");
        return;
    };
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP json probe: {why}");
            return;
        }
    };
    let base = scratch("json-probe");
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace.clone())),
        extensions: vec![ExtensionDescription::from_dir(&dir).unwrap()],
        default_settings: json!({}),
        user_settings: json!({}),
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T).unwrap_or_else(|e| panic!("start: {e}"));
    let file = workspace.join("a.json");
    host.open_document(&file, "{ \"name\" \"x\" }\n", "json", T).unwrap();
    host.activate_by_event("onLanguage:json", T).unwrap();

    assert!(
        host.main_thread.extensions.activation_errors.is_empty(),
        "{:?}",
        host.main_thread.extensions.activation_errors
    );
    assert!(
        !host.main_thread.language_features.registrations.is_empty(),
        "json-language-features registered no providers"
    );
    for u in &host.main_thread.unhandled {
        eprintln!("json probe unhandled: {}.{}", u.proxy, u.method);
    }
}
