//! upstream: src/vs/workbench/api/common/extHostExtensionService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadExtensionService.ts @ 50e1350fc78d
//!
//! Loading an extension into upstream's real extension host (ADR-0064): the fixture extension is
//! described, activated by event, and its command runs. Needs Node and a Code-OSS build
//! (`RSCODE_CODE_OSS`); skips with a printed reason otherwise. The Node stand-in of `vsr-rpc`
//! cannot load extensions, so there is no stand-in variant of these.

mod common;
use common::*;
use serde_json::json;
use std::time::Duration;
use vsr_exthost::description::ExtensionDescription;
use vsr_rpc::codec::Reply;

#[test]
fn description_is_what_the_host_expects() {
    // No host needed: the shape of IExtensionDescription.
    let d = ExtensionDescription::from_dir(&fixture_dir()).unwrap();
    assert_eq!(d.id, FIXTURE_ID);
    assert_eq!(
        d.json["identifier"],
        json!({ "value": FIXTURE_ID, "_lower": FIXTURE_ID })
    );
    assert_eq!(d.json["targetPlatform"], "undefined");
    assert_eq!(d.json["extensionLocation"]["scheme"], "file");
    assert_eq!(d.activation_events, ["onCommand:fixture.hello"]);
    assert_eq!(
        d.configuration_defaults()
            .iter()
            .map(|(k, _)| k.as_str())
            .collect::<Vec<_>>(),
        ["fixture.enabled", "fixture.message", "fixture.statusPrefix"]
    );
}

#[test]
fn an_extension_is_activated_by_event_and_its_command_runs() {
    let Some(mut host) = boot("exthost-activate", &[], json!({})) else {
        return;
    };
    // Not activated yet: its only activation event is the command.
    assert!(!host.main_thread.extensions.is_activated(FIXTURE_ID));
    host.activate_by_event("onCommand:fixture.hello", T)
        .unwrap();
    let ext = &host.main_thread.extensions;
    assert!(ext.is_activated(FIXTURE_ID), "{ext:?}");
    assert_eq!(ext.will_activate, [FIXTURE_ID]);
    assert!(
        ext.activation_errors.is_empty(),
        "{:?}",
        ext.activation_errors
    );
    assert_eq!(
        ext.activated[0].reason["activationEvent"],
        "onCommand:fixture.hello"
    );

    // Round trip into the extension: its command reads configuration, calls another command
    // through the API and returns what it saw.
    let reply = host
        .execute_contributed_command("fixture.report", vec![], T)
        .unwrap();
    let Reply::Json(seen) = reply else {
        panic!("{reply:?}")
    };
    assert_eq!(seen["viaExecute"], "hello world");
    assert_eq!(seen["apisCreated"], true);
    assert_eq!(
        seen["enabled"], true,
        "the contributed default reaches getConfiguration"
    );
    assert_eq!(seen["message"], "from the default");
    let reply = host
        .execute_contributed_command("fixture.hello", vec![json!("rust")], T)
        .unwrap();
    assert_eq!(reply, Reply::Json(json!("hello rust")));

    // A wrong extension location makes `require('vscode')` fall back with this warning.
    assert!(
        !host
            .main_thread
            .console
            .iter()
            .any(|m| m.to_string().contains("Could not identify extension")),
        "{:?}",
        host.main_thread.console
    );
    let exit = host.shutdown(Duration::from_secs(5)).unwrap();
    assert!(exit.exited_cleanly, "{}", exit.stderr);
}

#[test]
fn an_activation_error_is_surfaced() {
    let Some(mut host) = boot("exthost-fail", &[("FIXTURE_FAIL", "1")], json!({})) else {
        return;
    };
    // The host answers `$activate` with `true` even when `activate()` throws; the error comes
    // through MainThreadExtensionService.$onExtensionActivationError.
    host.activate(FIXTURE_ID, T).unwrap();
    let errors = &host.main_thread.extensions.activation_errors;
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].id, FIXTURE_ID);
    assert!(
        errors[0].message().contains("fixture failed on purpose"),
        "{:?}",
        errors[0]
    );
    assert!(!host.main_thread.extensions.is_activated(FIXTURE_ID));
    host.shutdown(Duration::from_secs(5));
}

#[test]
fn shutdown_ends_the_host_cleanly() {
    let Some(host) = boot("exthost-exit", &[], json!({})) else {
        return;
    };
    // Terminate through the session; the host exits by itself.
    let exit = host.shutdown(Duration::from_secs(5)).unwrap();
    assert!(exit.exited_cleanly, "{}", exit.stderr);
}
