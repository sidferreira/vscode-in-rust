//! Headless E2E tests of the go-to commands (ADR-0108). No extension host runs here: the providers'
//! answer comes from `NavState::fake`, and the tests record which go-to was asked for.
//!
//! Upstream source: src/vs/editor/contrib/gotoSymbol/browser/goToCommands.ts

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_editor::Position;
use vsr_exthost::language_features::dto::{LocationLink, Range};
use vsr_exthost::language_features::uri::Uri;

use super::Nav;
use crate::Workbench;
use crate::config::AppConfig;

/// A workbench over `<tmp>/proj` (`src/main.rs`, `src/lib.rs`), settings in a temp file.
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-nav-e2e-{name}-{}-{nanos}", std::process::id()));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\nlib();\n").unwrap();
    fs::write(proj.join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
    let settings = dir.join("settings.json");
    fs::write(&settings, "{}").unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        crate::breadcrumbs::init(cx);
        AppConfig::install(cx, Some(settings));
    });
    let root = proj.clone();
    let (wb, vcx) = cx.add_window_view(move |_, cx| Workbench::new(root, cx));
    vcx.run_until_parked();
    (wb, vcx, proj)
}

fn open(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path) {
    let path = path.to_path_buf();
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    cx.run_until_parked();
}

/// Puts the caret of the active editor at the 1-based `line`, `column`.
fn caret(wb: &Entity<Workbench>, cx: &mut VisualTestContext, line: usize, column: usize) {
    wb.update(cx, |wb, _| {
        if let Some(i) = wb.editors.active
            && let Some(e) = wb.editors.open.get_mut(i)
        {
            e.editor.set_cursor(Position::new(line, column), false);
        }
    });
}

fn active_path(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.path.clone()))
}

/// The active selection as `(anchor line, anchor column, active line, active column)`.
fn selection(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<(usize, usize, usize, usize)> {
    wb.read_with(cx, |wb, _| {
        wb.editors.active_editor().map(|e| {
            let s = e.editor.selection();
            (s.anchor.line, s.anchor.column, s.active.line, s.active.column)
        })
    })
}

fn link(path: &Path, range: Range, selection: Option<Range>) -> LocationLink {
    LocationLink {
        origin_selection_range: None,
        uri: Uri::file(&path.to_string_lossy()),
        range,
        target_selection_range: selection,
    }
}

/// Installs an in-process answer. `answer` returns the links for a request of `Nav` from the caret;
/// every request is recorded in the returned log.
fn fake(
    wb: &Entity<Workbench>,
    cx: &mut VisualTestContext,
    answer: impl Fn(Nav) -> Vec<LocationLink> + 'static,
) -> Rc<RefCell<Vec<Nav>>> {
    let log = Rc::new(RefCell::new(Vec::new()));
    let seen = log.clone();
    wb.update(cx, |wb, _| {
        wb.navigation.fake = Some(Box::new(move |_, nav| {
            seen.borrow_mut().push(nav);
            answer(nav)
        }));
    });
    log
}

/// Asks for `nav` from the caret, as F12 or Cmd+F12 does (through the editor's command hook).
fn go(wb: &Entity<Workbench>, cx: &mut VisualTestContext, nav: Nav) {
    wb.update_in(cx, |wb, window, cx| {
        wb.navigation_command(nav, window, cx);
    });
    cx.run_until_parked();
}

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

#[gpui::test]
fn f12_opens_the_definition_in_its_file_and_selects_its_selection_range(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "f12");
    let main = proj.join("src/main.rs");
    let lib = proj.join("src/lib.rs");
    open(&wb, cx, &main);
    caret(&wb, cx, 2, 1);
    // `lib` is on line 1, columns 8..11 (1-based); the whole function is wider.
    let log = fake(&wb, cx, move |_| {
        vec![link(&lib, Range::new(1, 1, 1, 16), Some(Range::new(1, 8, 1, 11)))]
    });
    let lib = proj.join("src/lib.rs");
    go(&wb, cx, Nav::Definition);
    assert_eq!(*log.borrow(), [Nav::Definition]);
    assert_eq!(active_path(&wb, cx), Some(lib));
    assert_eq!(selection(&wb, cx), Some((1, 8, 1, 11)), "the selection range is selected");
}

#[gpui::test]
fn cmd_f12_asks_the_providers_for_implementations(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "impl");
    open(&wb, cx, &proj.join("src/main.rs"));
    caret(&wb, cx, 2, 1);
    let log = fake(&wb, cx, |_| Vec::new());
    go(&wb, cx, Nav::Implementation);
    assert_eq!(*log.borrow(), [Nav::Implementation]);
}

#[gpui::test]
fn no_target_says_so_and_leaves_the_editor_alone(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "none");
    let main = proj.join("src/main.rs");
    open(&wb, cx, &main);
    caret(&wb, cx, 2, 1);
    fake(&wb, cx, |_| Vec::new());
    go(&wb, cx, Nav::TypeDefinition);
    assert_eq!(
        wb.read_with(cx, |wb, _| wb.navigation.message.clone()),
        Some("No type definition found".to_string())
    );
    assert_eq!(active_path(&wb, cx), Some(main));
}

#[gpui::test]
fn with_the_caret_on_the_only_target_the_default_alternative_does_nothing(cx: &mut TestAppContext) {
    // `editor.gotoLocation.alternativeDefinitionCommand` defaults to `goToReferences`, which
    // rscode has not got: the caret stays put.
    let (wb, cx, proj) = workbench(cx, "alt-default");
    let main = proj.join("src/main.rs");
    open(&wb, cx, &main);
    caret(&wb, cx, 1, 4);
    let here = main.clone();
    let log = fake(&wb, cx, move |_| vec![link(&here, Range::new(1, 4, 1, 8), None)]);
    go(&wb, cx, Nav::Definition);
    assert_eq!(*log.borrow(), [Nav::Definition]);
    assert_eq!(active_path(&wb, cx), Some(main));
}

#[gpui::test]
fn the_alternative_setting_runs_when_the_caret_is_already_on_the_only_target(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "alt-impl");
    let main = proj.join("src/main.rs");
    let lib = proj.join("src/lib.rs");
    open(&wb, cx, &main);
    set(cx, "editor.gotoLocation.alternativeDefinitionCommand", serde_json::json!("editor.action.goToImplementation"));
    caret(&wb, cx, 1, 4);
    let here = main.clone();
    let log = fake(&wb, cx, move |nav| match nav {
        Nav::Definition => vec![link(&here, Range::new(1, 4, 1, 8), None)],
        _ => vec![link(&lib, Range::new(1, 1, 1, 16), None)],
    });
    go(&wb, cx, Nav::Definition);
    assert_eq!(*log.borrow(), [Nav::Definition, Nav::Implementation]);
    assert_eq!(active_path(&wb, cx), Some(proj.join("src/lib.rs")));
}

#[gpui::test]
fn each_alternative_setting_is_read_for_its_own_command(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "alt-each");
    let main = proj.join("src/main.rs");
    open(&wb, cx, &main);
    for (nav, key) in [
        (Nav::Declaration, "editor.gotoLocation.alternativeDeclarationCommand"),
        (Nav::TypeDefinition, "editor.gotoLocation.alternativeTypeDefinitionCommand"),
        (Nav::Implementation, "editor.gotoLocation.alternativeImplementationCommand"),
    ] {
        set(cx, key, serde_json::json!("editor.action.revealDefinition"));
        caret(&wb, cx, 1, 4);
        let here = main.clone();
        let log = fake(&wb, cx, move |n| match n {
            Nav::Definition => vec![link(&here, Range::new(2, 1, 2, 4), None)],
            _ => vec![link(&here, Range::new(1, 4, 1, 8), None)],
        });
        go(&wb, cx, nav);
        assert_eq!(*log.borrow(), [nav, Nav::Definition], "{key} runs the definition command");
    }
}

#[gpui::test]
fn an_answer_for_an_older_request_is_dropped(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "stale");
    let main = proj.join("src/main.rs");
    let lib = proj.join("src/lib.rs");
    open(&wb, cx, &main);
    caret(&wb, cx, 2, 1);
    let old = wb.update(cx, |wb, _| wb.navigation.start(Nav::Definition, main.clone(), 2, 1));
    let _newer = wb.update(cx, |wb, _| wb.navigation.start(Nav::Definition, main.clone(), 2, 1));
    let answer = vec![link(&lib, Range::new(1, 8, 1, 11), None)];
    wb.update_in(cx, |wb, window, cx| wb.navigation_deliver(old.id, Nav::Definition, answer, window, cx));
    cx.run_until_parked();
    assert_eq!(active_path(&wb, cx), Some(main), "the older request was replaced");
}
