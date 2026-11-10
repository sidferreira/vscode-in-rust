//! upstream: src/vs/workbench/api/common/extHostWorkspace.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadWorkspace.ts @ 50e1350fc78d
//!
//! `workspace.findFiles` end to end: the fixture extension calls it inside a fresh workspace and the
//! listing comes back through `$startFileSearch`. Needs Node and a Code-OSS build (`RSCODE_CODE_OSS`);
//! otherwise each test prints why and passes.

mod common;

use common::T;
use serde_json::{Value, json};
use std::path::PathBuf;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::Launch;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fs-fixture")
}

/// Boot the fs fixture on a fresh workspace with `user_settings`, and fill the workspace.
fn boot(name: &str, user_settings: Value) -> Option<(ExtensionHost, PathBuf)> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = common::scratch(name);
    let ws = base.join("ws");
    for f in [
        "a.txt",
        "hidden.txt",
        "sub/b.txt",
        "sub/skip/c.txt",
        "sub/d.md",
    ] {
        let p = ws.join(f);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "x").unwrap();
    }
    let mut host = ExtensionHost::boot(&BootParams {
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), ws.clone())),
        extensions: vec![ExtensionDescription::from_dir(&fixture_dir()).unwrap()],
        default_settings: json!({}),
        user_settings,
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    Some((host, ws))
}

fn find_files(host: &mut ExtensionHost) -> Value {
    host.activate_by_event("onCommand:fixture.findFiles", T)
        .unwrap_or_else(|e| panic!("activate: {e}"));
    match host
        .execute_contributed_command("fixture.findFiles", vec![], T)
        .unwrap()
    {
        Reply::Json(v) => v,
        other => panic!("{other:?}"),
    }
}

#[test]
fn find_files_lists_the_workspace_through_the_real_host() {
    // The user excludes hidden.txt through files.exclude, which the listing must honour.
    let Some((mut host, _ws)) = boot(
        "workspace-find",
        json!({ "files": { "exclude": { "**/hidden.txt": true } } }),
    ) else {
        return;
    };
    let r = find_files(&mut host);
    assert_eq!(
        r["all"],
        json!(["a.txt", "sub/b.txt", "sub/skip/c.txt"]),
        "{r}"
    );
    assert_eq!(r["excluded"], json!(["a.txt", "sub/b.txt"]), "{r}");
    assert_eq!(r["limited"], 2, "{r}");
    assert_eq!(r["relative"], json!(["sub/b.txt"]), "{r}");
    assert!(host.shutdown(std::time::Duration::from_secs(5)).is_some());
}
