//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//!
//! The APIs ErrorLens uses, against upstream's real extension host, through the fixture extension
//! (ADR-0065): commands, configuration (fed from vsr-settings), a status bar item, decorations,
//! diagnostics, and the document / editor mirror they depend on. The models in
//! `host.main_thread` are what the app would paint. Skips without Node and `RSCODE_CODE_OSS`.

mod common;
use common::*;
use serde_json::{Value, json};
use std::time::Duration;
use vsr_exthost::diagnostics::{ERROR, uri_key};
use vsr_exthost::documents::TextEdit;
use vsr_exthost::host::ExtensionHost;
use vsr_rpc::codec::Reply;
use vsr_rpc::handshake::file_uri;

fn activated(name: &str, user: Value) -> Option<ExtensionHost> {
    let mut host = boot(name, &[], user)?;
    host.activate_by_event("onCommand:fixture.hello", T)
        .unwrap();
    assert!(
        host.main_thread.extensions.activated.len() == 1,
        "{:?}",
        host.main_thread.extensions
    );
    Some(host)
}

fn report(host: &mut ExtensionHost) -> Value {
    match host
        .execute_contributed_command("fixture.report", vec![], T)
        .unwrap()
    {
        Reply::Json(v) => v,
        other => panic!("{other:?}"),
    }
}

fn finish(host: ExtensionHost) {
    let exit = host.shutdown(Duration::from_secs(5)).unwrap();
    assert!(exit.exited_cleanly, "{}", exit.stderr);
}

#[test]
fn registering_commands_and_status_bar_item_reach_the_models() {
    let Some(host) = activated("api-basic", json!({})) else {
        return;
    };
    let mt = &host.main_thread;
    // MainThreadCommands.$registerCommand for each command the extension registered.
    for id in [
        "fixture.hello",
        "fixture.report",
        "fixture.scan",
        "fixture.callApp",
    ] {
        assert!(
            mt.commands.registered.contains(id),
            "{id}: {:?}",
            mt.commands.registered
        );
    }
    // The status bar item: text from the contributed default prefix, right aligned, priority 100.
    let entries = mt.status_bar.sorted();
    assert_eq!(entries.len(), 1, "{:?}", mt.status_bar);
    let e = entries[0];
    assert_eq!(e.text, "$(error) 0");
    assert!(!e.align_left);
    assert_eq!(e.priority, Some(100.0));
    assert_eq!(e.tooltip, "Fixture status");
    assert_eq!(e.command["id"], "fixture.hello");
    assert_eq!(e.extension_id.as_deref(), Some(FIXTURE_ID));
    // The decoration type and the diagnostic collection exist.
    assert_eq!(mt.editors.decoration_types.len(), 1);
    let (ext, options) = mt.editors.decoration_types.values().next().unwrap();
    assert_eq!(ext, FIXTURE_ID);
    assert_eq!(options["isWholeLine"], true);
    // Nothing the shapes above cover fell through to the default answer.
    for u in &mt.unhandled {
        eprintln!("unhandled: {} {}", u.proxy, u.method);
    }
    finish(host);
}

#[test]
fn an_extension_runs_an_app_command_and_gets_its_result() {
    let Some(mut host) = activated("api-app-command", json!({})) else {
        return;
    };
    host.main_thread.commands.set_executor(Box::new(|id, args| {
        assert_eq!(id, "rscode.test.appCommand");
        Ok(Some(json!({ "args": args })))
    }));
    let reply = host
        .execute_contributed_command("fixture.callApp", vec![], T)
        .unwrap();
    assert_eq!(reply, Reply::Json(json!({ "args": [1, "two"] })));
    assert_eq!(
        host.main_thread.commands.executed,
        [(
            "rscode.test.appCommand".to_string(),
            vec![json!(1), json!("two")]
        )]
    );
    finish(host);
}

#[test]
fn configuration_comes_from_vsr_settings_and_changes_reach_the_extension() {
    use vsr_settings::{Configuration, SettingsRegistry, UserSettings};
    let registry = SettingsRegistry::load_default();
    let user = |text: &str| Configuration::new(&registry, UserSettings::parse(text).unwrap());
    let before = user(r#"{ "fixture.message": "from user", "editor.fontSize": 17 }"#);
    let (defaults, user_layer) = vsr_exthost::configuration::layers(&registry, &before);
    assert_eq!(user_layer["fixture"]["message"], "from user");
    assert_eq!(defaults["editor"]["tabSize"], 4);

    let name = "api-config";
    let Some(mut host) = boot_with(name, &[], defaults.clone(), user_layer) else {
        return;
    };
    host.activate_by_event("onCommand:fixture.hello", T)
        .unwrap();
    let seen = report(&mut host);
    assert_eq!(
        seen["message"], "from user",
        "user layer beats the contributed default"
    );
    assert_eq!(
        seen["enabled"], true,
        "the contributed default still shows for unset keys"
    );

    // The user edits settings.json: the extension gets onDidChangeConfiguration and re-reads.
    let after = user(r#"{ "fixture.message": "edited", "fixture.statusPrefix": "ERR" }"#);
    let (defaults, user_layer) = vsr_exthost::configuration::layers(&registry, &after);
    host.configuration_changed(
        defaults,
        user_layer,
        &["fixture.message", "fixture.statusPrefix"],
        T,
    )
    .unwrap();
    let seen = report(&mut host);
    assert_eq!(seen["message"], "edited");
    assert_eq!(seen["configChanges"], json!([true]));
    wait_until(&mut host, |h| {
        h.main_thread.status_bar.sorted()[0].text == "ERR 0"
    });

    // And back: the extension updates a setting; the app is told what to write.
    host.execute_contributed_command("fixture.updateConfig", vec![], T)
        .unwrap();
    let u = &host.main_thread.configuration.updates;
    assert_eq!(u.len(), 1, "{u:?}");
    assert_eq!(
        (u[0].key.as_str(), u[0].value.clone()),
        ("fixture.message", Some(json!("from the extension")))
    );
    assert_eq!(
        u[0].target,
        Some(2),
        "update(.., true) is ConfigurationTarget.USER"
    );
    finish(host);
}

#[test]
fn diagnostics_and_decorations_follow_the_open_editor_like_errorlens() {
    let Some(mut host) = activated("api-editor", json!({})) else {
        return;
    };
    let ws = scratch("api-editor").join("ws");
    let file = ws.join("a.txt");
    let uri = uri_key(&file_uri(&file));
    host.open_document(&file, "fine\nERROR one\nfine\nERROR two\n", "plaintext", T)
        .unwrap();
    // No editor yet: nothing to scan.
    assert!(host.main_thread.diagnostics.by_owner.is_empty());
    let editor = host.show_editor(&file, T).unwrap();
    // The extension reacts to onDidChangeActiveTextEditor; its calls arrive as requests, so pump.
    // (The status bar item batches its updates, so wait for the text too.)
    wait_until(&mut host, |h| {
        h.main_thread.diagnostics.counts().0 == 2
            && h.main_thread.status_bar.sorted()[0].text == "$(error) 2"
    });

    let mt = &host.main_thread;
    let markers = mt.diagnostics.markers_for(&uri);
    assert_eq!(markers.len(), 2, "{:?}", mt.diagnostics);
    assert_eq!(markers[0]["severity"], ERROR);
    assert_eq!(markers[0]["message"], "ERROR one");
    assert_eq!(
        (
            markers[0]["startLineNumber"].as_u64(),
            markers[0]["endColumn"].as_u64()
        ),
        (Some(2), Some(10))
    );
    // Decorations, with the per-instance render options (so the non-fast form).
    let key = mt.editors.decoration_types.keys().next().unwrap().clone();
    let d = mt.editors.decorations_for(&editor);
    assert_eq!(d.len(), 2, "{d:?}");
    assert!(d.iter().all(|(k, _)| *k == key));
    assert_eq!(
        (d[0].1.start_line, d[0].1.start_column, d[0].1.end_column),
        (2, 1, 10)
    );
    assert_eq!(d[0].1.render_options["after"]["contentText"], " ERROR one");
    // The status bar item shows the count.
    assert_eq!(mt.status_bar.sorted()[0].text, "$(error) 2");

    // Edit the document: remove the second error line. The extension rescans on the change.
    host.edit_document(
        &file,
        &[TextEdit {
            start_line: 4,
            start_column: 1,
            end_line: 4,
            end_column: 10,
            text: "fine".into(),
        }],
        T,
    )
    .unwrap();
    wait_until(&mut host, |h| {
        h.main_thread.diagnostics.counts().0 == 1
            && h.main_thread.status_bar.sorted()[0].text == "$(error) 1"
    });
    assert_eq!(host.main_thread.editors.decorations_for(&editor).len(), 1);
    assert_eq!(host.main_thread.status_bar.sorted()[0].text, "$(error) 1");
    assert_eq!(
        host.main_thread.documents.open[&uri].text,
        "fine\nERROR one\nfine\nfine\n"
    );

    // Closing the editor.
    host.close_editor(&editor, T).unwrap();
    assert!(host.main_thread.documents.active_editor.is_none());

    // The extension saves the active document through MainThreadDocuments.
    host.show_editor(&file, T).unwrap();
    host.execute_contributed_command("fixture.saveActive", vec![], T)
        .unwrap();
    assert_eq!(host.main_thread.documents.saved, std::slice::from_ref(&uri));
    finish(host);
}

#[test]
fn the_compact_decoration_form_lands_in_the_same_model() {
    // Drive MainThreadTextEditors directly (no host needed for the model): the compact form
    // `$trySetDecorationsFast` lands in the same model as `$trySetDecorations`.
    use vsr_rpc::codec::Arg;
    use vsr_rpc::rpc::Call;
    let call = |method: &str, args: Vec<Value>| Call {
        req: 1,
        rpc_id: vsr_rpc::proxy_ids::main::MAIN_THREAD_TEXT_EDITORS,
        method: method.into(),
        args: args.into_iter().map(Arg::Json).collect(),
        cancellable: false,
    };
    use vsr_exthost::main_thread::MainThread;
    let mut mt = MainThread::new();
    mt.handle(&call(
        "$trySetDecorationsFast",
        vec![json!("e1"), json!("k"), json!([1, 1, 1, 5, 3, 2, 3, 4])],
    ))
    .unwrap();
    let d = mt.editors.decorations_for("e1");
    assert_eq!(d.len(), 2);
    assert_eq!(
        (
            d[1].1.start_line,
            d[1].1.start_column,
            d[1].1.end_line,
            d[1].1.end_column
        ),
        (3, 2, 3, 4)
    );
    mt.handle(&call(
        "$trySetDecorationsFast",
        vec![json!("e1"), json!("k"), json!([])],
    ))
    .unwrap();
    assert!(
        mt.editors.decorations_for("e1").is_empty(),
        "an empty set clears"
    );
}
