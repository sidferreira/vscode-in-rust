//! Headless end-to-end tests of the extension host in the workbench (ADR-0066, ADR-0067): a real
//! `Workbench` in a headless window, the real Code-OSS extension host under Node, and a fixture
//! extension (an ErrorLens stand-in: `crates/vsr-exthost/tests/fixtures/errorlens-fixture`).
//! They skip, printing why, without `RSCODE_CODE_OSS` or Node. Nothing opens a visible window.
//!
//! Upstream source: src/vs/workbench/api/common/extHostExtensionService.ts

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::{Value, json};
use vsr_rpc::handshake::Launch;

use super::status;
use crate::Workbench;
use crate::commands::{Commands, Execution};
use crate::config::AppConfig;

struct Scratch {
    ws: PathBuf,
    ext: PathBuf,
}

fn scratch(name: &str, activation: &[&str], files: &[(&str, &str)], patch: impl Fn(String) -> String) -> Scratch {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let base = std::env::temp_dir().join(format!("rscode-e2e-ext-{name}-{}-{nanos}", std::process::id()));
    let (ws, ext) = (base.join("ws"), base.join("exts/errorlens-fixture"));
    fs::create_dir_all(&ws).unwrap();
    fs::create_dir_all(&ext).unwrap();
    for (n, text) in files {
        fs::write(ws.join(n), text).unwrap();
    }
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vsr-exthost/tests/fixtures/errorlens-fixture");
    let mut manifest: Value = serde_json::from_str(&fs::read_to_string(fixture.join("package.json")).unwrap()).unwrap();
    manifest["activationEvents"] = json!(activation);
    manifest["contributes"]["commands"] = json!([
        { "command": "fixture.hello", "title": "Fixture: hello" },
        { "command": "fixture.updateConfig", "title": "Update Config", "category": "Fixture" },
        { "command": "fixture.saveActive", "title": "Save Active", "category": "Fixture" },
        { "command": "fixture.callApp", "title": "Call App", "category": "Fixture" },
    ]);
    fs::write(ext.join("package.json"), manifest.to_string()).unwrap();
    // The status item runs a command that runs an app command (toggle the side bar).
    let js = fs::read_to_string(fixture.join("extension.js")).unwrap();
    let js = js
        .replace("item.command = 'fixture.hello'", "item.command = 'fixture.callApp'")
        .replace("rscode.test.appCommand', 1, 'two'", "workbench.action.toggleSidebarVisibility'");
    fs::write(ext.join("extension.js"), patch(js)).unwrap();
    Scratch { ws, ext }
}

/// A workbench over the scratch workspace with the host started on the fixture; `None` (after
/// printing why) without a build.
fn start<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    activation: &[&str],
    files: &[(&str, &str)],
) -> Option<(Entity<Workbench>, &'a mut VisualTestContext, Scratch)> {
    start_with_env(cx, name, activation, files, &[])
}

fn start_with_env<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    activation: &[&str],
    files: &[(&str, &str)],
    env: &[(&str, &str)],
) -> Option<(Entity<Workbench>, &'a mut VisualTestContext, Scratch)> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let s = scratch(name, activation, files, |js| js);
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        crate::quick_open::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None); // in memory: never the real settings file
    });
    cx.executor().allow_parking();
    let root = s.ws.clone();
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    let mut config = cx.update(|_, cx| super::config(launch, vec![s.ext.parent().unwrap().to_path_buf()], s.ws.join("../storage"), &s.ws, cx));
    config.env = env.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
    wb.update_in(cx, |wb, window, cx| wb.start_extensions(config, window, cx));
    Some((wb, cx, s))
}

/// Let the workbench and the host's thread run until `done` holds (30 s at most).
fn wait_for(wb: &Entity<Workbench>, cx: &mut VisualTestContext, what: &str, done: impl Fn(&Workbench) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        wb.update(cx, |wb, cx| {
            wb.ext_sync();
            cx.notify();
        });
        cx.run_until_parked();
        if wb.read_with(cx, |wb, _| done(wb)) {
            return;
        }
        assert!(Instant::now() < deadline, "timed out waiting for: {what}");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// `debug_bounds` takes a `'static` selector.
fn selector_of(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn status_texts(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, cx| wb.ext_status_items(cx).iter().map(status::Item::plain_text).collect::<Vec<_>>())
}

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    wb.update(cx, |wb, cx| {
        wb.open(path);
        cx.notify();
    });
}

fn run(wb: &Entity<Workbench>, cx: &mut VisualTestContext, id: &str) {
    wb.update_in(cx, |wb, window, cx| wb.execute_command(id, window, cx));
}

#[gpui::test]
fn contributed_commands_reach_the_palette_and_the_host_shuts_down_cleanly(cx: &mut TestAppContext) {
    let Some((wb, cx, _s)) = start(cx, "palette", &["onStartupFinished"], &[]) else { return };
    wait_for(&wb, cx, "the extension to load", |wb| wb.ext.ready);
    cx.update(|_, cx| {
        let commands = cx.global::<Commands>();
        let d = commands.registry.descriptor("fixture.callApp").expect("contributed commands are registered");
        assert_eq!(d.label(), "Fixture: Call App");
        assert!(d.f1, "listed in the palette");
        assert!(matches!(commands.execution("fixture.callApp"), Some(Execution::Extension)));
        let palette = commands.registry.palette_commands(&crate::commands::context_keys(false, true));
        assert!(palette.iter().any(|d| d.id == "fixture.updateConfig"));
    });
    // Running it activates the extension (the status item appears) and runs the app command it
    // calls: the side bar toggles.
    assert!(wb.read_with(cx, |wb, _| wb.sidebar_visible));
    run(&wb, cx, "fixture.callApp");
    wait_for(&wb, cx, "the extension to run the app command", |wb| !wb.sidebar_visible);
    wait_for(&wb, cx, "the status item the activation created", |wb| !wb.ext.snapshot.status.is_empty());
    assert_eq!(status_texts(&wb, cx), ["0"]);
    wb.update(cx, |wb, _| wb.ext_shutdown());
    assert!(!wb.read_with(cx, |wb, _| wb.ext.is_running()), "the host is stopped");
}

#[gpui::test]
fn open_documents_drive_decorations_squiggles_and_the_status_bar(cx: &mut TestAppContext) {
    let Some((wb, cx, s)) = start(cx, "lens", &["*"], &[("a.txt", "fine\nERROR boom\nfine\n")]) else { return };
    let file = s.ws.join("a.txt").canonicalize().unwrap();
    open(&wb, cx, &file);
    wait_for(&wb, cx, "one diagnostic", |wb| wb.ext.snapshot.counts == (1, 0));
    wait_for(&wb, cx, "the paint model", |wb| wb.editor_view.ext_paint.is_some());
    cx.run_until_parked();
    // Painted: the text after the line, the box behind it and the squiggle under it.
    for selector in ["ext-after:2", "ext-box:2:1", "ext-squiggle:2:1:error", "ext-status:", "status-problems"] {
        let found = if selector == "ext-status:" {
            let id = wb.read_with(cx, |wb, _| wb.ext.snapshot.status[0].id.clone());
            cx.debug_bounds(selector_of(&format!("ext-status:{id}"))).is_some()
        } else {
            cx.debug_bounds(selector).is_some()
        };
        assert!(found, "{selector} is laid out");
    }
    assert!(cx.debug_bounds("ext-after:1").is_none() && cx.debug_bounds("ext-box:1:1").is_none(), "only the ERROR line");
    assert_eq!(status_texts(&wb, cx), ["1"]);
    let line2 = wb.read_with(cx, |wb, _| wb.editor_view.ext_paint.as_ref().unwrap().line(2).cloned().unwrap());
    assert_eq!(line2.after[0].text, " ERROR boom");

    // An edit reaches the extension: another ERROR line.
    wb.update(cx, |wb, cx| {
        wb.editors.open[0].type_text("ERROR again\n");
        cx.notify();
    });
    wait_for(&wb, cx, "two diagnostics", |wb| wb.ext.snapshot.counts == (2, 0));
    assert_eq!(status_texts(&wb, cx), ["2"]);

    // Clicking the status item runs its command: the extension asks the app to toggle the side bar.
    let id = wb.read_with(cx, |wb, _| wb.ext.snapshot.status[0].id.clone());
    let at = cx.debug_bounds(selector_of(&format!("ext-status:{id}"))).expect("laid out").center();
    cx.simulate_mouse_move(at, None, gpui::Modifiers::default());
    cx.simulate_click(at, gpui::Modifiers::default());
    wait_for(&wb, cx, "the click to run the command", |wb| !wb.sidebar_visible);

    wb.update(cx, |wb, _| wb.ext_shutdown());
}

#[gpui::test]
fn configuration_flows_both_ways_and_an_extension_can_save(cx: &mut TestAppContext) {
    let Some((wb, cx, s)) = start(cx, "config", &["*"], &[("a.txt", "ERROR one\n")]) else { return };
    let file = s.ws.join("a.txt").canonicalize().unwrap();
    open(&wb, cx, &file);
    wait_for(&wb, cx, "the status item", |wb| !wb.ext.snapshot.status.is_empty());
    // Settings → host: the extension reads `fixture.statusPrefix` when the configuration changes.
    cx.update(|_, cx| AppConfig::set_value(cx, "fixture.statusPrefix", Some(json!("Z")))).unwrap();
    wait_for(&wb, cx, "the new prefix", |wb| wb.ext.snapshot.status.iter().any(|e| e.text.starts_with("Z ")));
    // Host → settings: `WorkspaceConfiguration.update`.
    run(&wb, cx, "fixture.updateConfig");
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        cx.run_until_parked();
        let v = cx.update(|_, cx| AppConfig::get(cx).configuration().get_str("fixture.message").map(String::from));
        if v.as_deref() == Some("from the extension") {
            break;
        }
        assert!(Instant::now() < deadline, "the extension's update never reached the settings");
        std::thread::sleep(Duration::from_millis(25));
    }
    // Save from the extension: the dirty editor is written.
    wb.update(cx, |wb, cx| {
        wb.editors.open[0].type_text("// edited\n");
        cx.notify();
    });
    wait_for(&wb, cx, "the edit to be mirrored", |wb| {
        wb.ext.mirror.docs.get(&file) == Some(&wb.editors.open[0].editor.version())
    });
    run(&wb, cx, "fixture.saveActive");
    let deadline = Instant::now() + Duration::from_secs(30);
    while !fs::read_to_string(&file).unwrap().starts_with("// edited") {
        cx.run_until_parked();
        assert!(Instant::now() < deadline, "the extension's save never reached the file");
        std::thread::sleep(Duration::from_millis(25));
    }
    wb.update(cx, |wb, _| wb.ext_shutdown());
}

#[gpui::test]
fn language_and_workspace_events_activate_extensions(cx: &mut TestAppContext) {
    // onLanguage: nothing runs until a plaintext document opens.
    let Some((wb, cx, s)) = start(cx, "on-language", &["onLanguage:plaintext"], &[("a.txt", "x\n")]) else { return };
    wait_for(&wb, cx, "the host", |wb| wb.ext.ready);
    assert!(wb.read_with(cx, |wb, _| wb.ext.snapshot.status.is_empty()), "not activated yet");
    open(&wb, cx, &s.ws.join("a.txt").canonicalize().unwrap());
    wait_for(&wb, cx, "activation by language", |wb| !wb.ext.snapshot.status.is_empty());
    wb.update(cx, |wb, _| wb.ext_shutdown());
}

#[gpui::test]
fn workspace_contains_activates_at_startup(cx: &mut TestAppContext) {
    let Some((wb, cx, _s)) = start(cx, "contains", &["workspaceContains:**/*.lintme"], &[("deep.lintme", "")]) else { return };
    wait_for(&wb, cx, "activation by workspaceContains", |wb| !wb.ext.snapshot.status.is_empty());
    wb.update(cx, |wb, _| wb.ext_shutdown());
}

#[gpui::test]
fn an_activation_error_at_startup_reaches_the_workbench(cx: &mut TestAppContext) {
    let Some((wb, cx, _s)) = start_with_env(cx, "fail", &["*"], &[], &[("FIXTURE_FAIL", "1")]) else { return };
    wait_for(&wb, cx, "the activation error", |wb| wb.ext.problems.iter().any(|p| p.contains("fixture failed on purpose")));
    let shown = wb.read_with(cx, |wb, _| wb.error.clone().unwrap_or_default());
    assert!(shown.contains("Extensions: activation failed") && shown.contains("rscode-test.errorlens-fixture"), "{shown}");
    wb.update(cx, |wb, _| wb.ext_shutdown());
}

#[gpui::test]
fn without_a_build_extensions_are_off_and_nothing_is_reported(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::search_view::init(cx);
        crate::quick_open::init(cx);
        crate::commands::init(cx);
        AppConfig::install(cx, None);
    });
    let dir = scratch("off", &[], &[], |js| js).ws;
    let (wb, cx) = cx.add_window_view(move |_, cx| Workbench::new(dir, cx));
    wb.update_in(cx, |wb, window, cx| wb.ext_autostart(Err("RSCODE_CODE_OSS is not set".into()), window, cx));
    cx.run_until_parked();
    wb.read_with(cx, |wb, _| {
        assert!(!wb.ext.is_running() && wb.ext.problems.is_empty() && wb.error.is_none());
    });
}
