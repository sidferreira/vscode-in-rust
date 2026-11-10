//! upstream: src/vs/workbench/api/browser/mainThreadFileSystemEventService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostFileSystemEventService.ts @ 50e1350fc78d
//!
//! File watchers through the real host (ADR-0090): a fixture extension arms a watcher over the
//! workspace; changes made by the forwarded operations and by the app (`ExtensionHost::file_changed`)
//! reach it, and a disposed watcher hears nothing. Needs Node and a Code-OSS build (`RSCODE_CODE_OSS`);
//! otherwise the test prints why and passes.

mod common;

use common::T;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::file_watcher::ChangeKind;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::Launch;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fs-fixture")
}

fn boot(name: &str) -> Option<(ExtensionHost, PathBuf)> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = common::scratch(name);
    let ws = base.join("ws");
    std::fs::create_dir_all(&ws).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), ws.clone())),
        extensions: vec![ExtensionDescription::from_dir(&fixture_dir()).unwrap()],
        default_settings: json!({}),
        user_settings: json!({}),
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    // The forwarded trash delete runs only under a folder the app allows: the workspace.
    host.main_thread
        .file_system
        .allow_destructive_under(&ws)
        .unwrap();
    Some((host, ws))
}

fn command(host: &mut ExtensionHost, id: &str) -> Value {
    host.activate_by_event(&format!("onCommand:{id}"), T)
        .unwrap_or_else(|e| panic!("activate {id}: {e}"));
    match host.execute_contributed_command(id, vec![], T).unwrap() {
        Reply::Json(v) => v,
        other => panic!("{other:?}"),
    }
}

/// The events a watcher recorded since its last collect, sorted for comparison.
fn collect_from(host: &mut ExtensionHost, id: &str) -> Vec<String> {
    let mut events: Vec<String> = command(host, id)
        .as_array()
        .unwrap()
        .iter()
        .map(|e| e.as_str().unwrap().to_string())
        .collect();
    events.sort();
    events
}

fn collect(host: &mut ExtensionHost) -> Vec<String> {
    collect_from(host, "fixture.watchCollect")
}

#[test]
fn watchers_hear_of_forwarded_and_app_changes_and_stop_when_disposed() {
    let Some((mut host, ws)) = boot("workspace-watch") else {
        return;
    };
    assert_eq!(command(&mut host, "fixture.watchArm"), "armed");
    // The RelativePattern watcher is the one form that goes through $watch.
    assert_eq!(host.main_thread.file_watchers.watches.len(), 1);
    // A string-pattern watcher never sends $watch (upstream returns early); the workspace folder
    // delivers its events instead.
    assert_eq!(command(&mut host, "fixture.watchArmString"), "armed");
    assert_eq!(host.main_thread.file_watchers.watches.len(), 1);

    // Forwarded: the rename (deleted + created) and the trash delete (deleted). The native write
    // of move-me.txt is not seen (it runs in the Node host).
    let expected = vec![
        "created moved.txt".to_string(),
        "deleted move-me.txt".to_string(),
        "deleted moved.txt".to_string(),
    ];
    assert_eq!(command(&mut host, "fixture.watchMove"), "moved");
    assert_eq!(collect(&mut host), expected);
    assert_eq!(
        collect_from(&mut host, "fixture.watchCollectString"),
        expected
    );

    // App: a change the app made reaches both watchers.
    std::fs::write(ws.join("a.txt"), "app").unwrap();
    host.file_changed(ChangeKind::Changed, &ws.join("a.txt"));
    assert_eq!(collect(&mut host), vec!["changed a.txt".to_string()]);
    assert_eq!(
        collect_from(&mut host, "fixture.watchCollectString"),
        vec!["changed a.txt".to_string()]
    );

    // Disposed: the relative watcher unwatches, so it hears nothing; the string watcher still does.
    assert_eq!(command(&mut host, "fixture.watchDisarm"), "disarmed");
    assert_eq!(host.main_thread.file_watchers.watches.len(), 0);
    host.file_changed(ChangeKind::Changed, &ws.join("a.txt"));
    assert!(collect(&mut host).is_empty());
    assert_eq!(
        collect_from(&mut host, "fixture.watchCollectString"),
        vec!["changed a.txt".to_string()]
    );
    assert_eq!(command(&mut host, "fixture.watchDisarmString"), "disarmed");
    host.file_changed(ChangeKind::Changed, &ws.join("a.txt"));
    assert!(collect_from(&mut host, "fixture.watchCollectString").is_empty());

    assert!(host.shutdown(Duration::from_secs(5)).is_some());
}
