//! upstream: src/vs/workbench/api/common/extHostExtensionService.ts @ 50e1350fc78d
//!
//! Shared by the real-host tests: boot a host with the fixture extension, or skip.
#![allow(dead_code)]

use std::path::PathBuf;
use std::time::Duration;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_rpc::handshake::Launch;

pub const T: Duration = Duration::from_secs(20);
pub const FIXTURE_ID: &str = "rscode-test.errorlens-fixture";

pub fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/errorlens-fixture")
}

/// A scratch folder under `target/` (cleaned by `cargo clean`, not by the tests).
pub fn scratch(name: &str) -> PathBuf {
    let dir =
        PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `None` (after printing why) when Node or the Code-OSS build is missing.
pub fn boot(
    name: &str,
    env: &[(&str, &str)],
    user_settings: serde_json::Value,
) -> Option<ExtensionHost> {
    boot_with(name, env, serde_json::json!({}), user_settings)
}

/// Pump the host's events until `done` holds (20 s at most).
pub fn wait_until(host: &mut ExtensionHost, done: impl Fn(&ExtensionHost) -> bool) {
    let deadline = std::time::Instant::now() + T;
    while !done(host) {
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for the host"
        );
        host.pump(Duration::from_millis(20));
    }
}

pub fn boot_with(
    name: &str,
    env: &[(&str, &str)],
    default_settings: serde_json::Value,
    user_settings: serde_json::Value,
) -> Option<ExtensionHost> {
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
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace)),
        extensions: vec![ExtensionDescription::from_dir(&fixture_dir()).unwrap()],
        default_settings,
        user_settings,
        env: env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    Some(host)
}
