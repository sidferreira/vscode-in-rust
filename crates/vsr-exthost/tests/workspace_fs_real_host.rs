//! upstream: src/vs/workbench/api/common/extHostFileSystemConsumer.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadFileSystem.ts @ 50e1350fc78d
//!
//! `vscode.workspace.fs` on the file scheme, end to end: a fixture extension (plain JS,
//! `fixtures/fs-fixture`) runs every operation inside a fresh workspace, and the report says what
//! each one returned or threw. Needs Node and a Code-OSS build (`RSCODE_CODE_OSS`); otherwise the
//! test prints why and passes.

mod common;

use common::T;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::Launch;

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fs-fixture")
}

/// A host with the fs fixture and a fresh workspace `ws`; `outside` is a sibling folder.
fn boot(name: &str) -> Option<(ExtensionHost, PathBuf, PathBuf)> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = common::scratch(name);
    let workspace = base.join("ws");
    let outside = base.join("outside");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace.clone())),
        extensions: vec![ExtensionDescription::from_dir(&fixture_dir()).unwrap()],
        default_settings: json!({}),
        user_settings: json!({}),
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    // The app allows destructive operations only under the folders it owns; the test's own
    // workspace is such a folder, `outside` is not.
    host.main_thread
        .file_system
        .allow_destructive_under(&workspace)
        .unwrap();
    Some((host, workspace, outside))
}

fn run_fixture(host: &mut ExtensionHost, ws: &Path, outside: &Path) -> Value {
    host.activate_by_event("onCommand:fixture.fsRun", T)
        .unwrap_or_else(|e| panic!("activate: {e}"));
    match host
        .execute_contributed_command(
            "fixture.fsRun",
            vec![
                json!(ws.display().to_string()),
                json!(outside.display().to_string()),
            ],
            T,
        )
        .unwrap()
    {
        Reply::Json(v) => v,
        other => panic!("{other:?}"),
    }
}

/// `report[step]` is `{ ok: false, code }` for a step that threw.
fn code(report: &Value, step: &str) -> String {
    report[step]["code"]
        .as_str()
        .unwrap_or_else(|| panic!("{step} did not throw: {report}"))
        .to_string()
}

fn ok(report: &Value, step: &str) -> Value {
    assert_eq!(report[step]["ok"], true, "{step}: {report}");
    report[step]["value"].clone()
}

#[test]
fn workspace_fs_operations_round_trip_through_the_real_host() {
    let Some((mut host, ws, outside)) = boot("workspace-fs") else {
        return;
    };
    std::fs::write(outside.join("keep.txt"), "keep").unwrap();
    let r = run_fixture(&mut host, &ws, &outside);

    // Reads and writes, with the file type and size from stat.
    ok(&r, "writeNew");
    assert_eq!(ok(&r, "readFile"), "hello");
    assert_eq!(ok(&r, "stat"), json!({ "type": 1, "size": 5 }));
    assert_eq!(code(&r, "statMissing"), "FileNotFound");
    assert!(
        ok(&r, "readDirectory")
            .to_string()
            .contains(r#"["a.txt",1]"#)
    );
    ok(&r, "createDirectory");
    assert_eq!(ok(&r, "statFolder"), 2);
    assert_eq!(code(&r, "readFileOfFolder"), "FileIsADirectory");
    assert_eq!(code(&r, "readDirOfFile"), "FileNotADirectory");
    // Writing creates the missing parents (upstream's mkdirp); writing over a folder is refused.
    ok(&r, "writeCreatesParents");
    assert_eq!(code(&r, "writeOverFolder"), "FileIsADirectory");
    // Rename and copy honour `overwrite`.
    assert_eq!(code(&r, "renameNoOverwrite"), "FileExists");
    ok(&r, "rename");
    // `c.txt` (the moved "hello") replaces `d.txt`.
    assert_eq!(ok(&r, "renameOverwrite"), "hello");
    ok(&r, "copyFolder");
    ok(&r, "copyFileOnce");
    assert_eq!(code(&r, "copyFileAgain"), "FileExists");
    // Deletes: a missing path, a non-empty folder without `recursive`, then the allowed ones.
    assert_eq!(code(&r, "deleteMissing"), "FileNotFound");
    assert!(r["deleteNonEmptyFolder"]["ok"] == false, "{r}");
    ok(&r, "deleteRecursive");
    ok(&r, "deleteTrashInside");
    assert!(!ws.join("sub").exists() && !ws.join("d.txt").exists());
    assert!(ws.join("sub2/x/b.txt").is_file(), "copy kept the tree");

    // Outside the allowed folders: a rename that would overwrite the folder, and a trash, are
    // refused and nothing is touched.
    ok(&r, "writeForOutside");
    assert_eq!(code(&r, "overwriteOutsideFolder"), "NoPermissions");
    assert_eq!(ok(&r, "outsideReadAfter"), "keep");
    assert_eq!(code(&r, "outsideTrash"), "Unavailable");
    assert!(outside.join("keep.txt").is_file());

    // Another scheme has no provider.
    assert_eq!(code(&r, "otherScheme"), "Unavailable");

    assert!(host.shutdown(std::time::Duration::from_secs(5)).is_some());
}
