//! Headless E2E tests of the document outline (ADR-0086): the symbols of the active document are
//! asked for after the debounce, answered in-process (no extension host runs in these tests), and
//! kept per document. The breadcrumbs' symbol crumbs are tested with the breadcrumbs.
//!
//! Upstream source: src/vs/editor/contrib/documentSymbols/browser/outlineModel.ts

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_exthost::language_features::dto::{DocumentSymbol, Range};
use vsr_exthost::language_features::outline::ProviderSymbols;

use crate::Workbench;
use crate::config::AppConfig;
use super::view;

/// A workbench over `<tmp>/proj` (`src/main.rs`, `src/lib.rs`), settings in a temp file.
fn workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    workbench_with(cx, name, "{}")
}

/// [`workbench`] with `settings_json` as the user's settings.
fn workbench_with<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
    settings_json: &str,
) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!(
        "rscode-outline-e2e-{name}-{}-{nanos}",
        std::process::id()
    ));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(proj.join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
    let settings = dir.join("settings.json");
    fs::write(&settings, settings_json).unwrap();
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

/// One function `main` on line 1 of any document.
fn main_symbol() -> Vec<ProviderSymbols> {
    let range = Range::new(1, 1, 1, 13);
    let main = DocumentSymbol {
        name: "main".into(),
        detail: String::new(),
        kind: 11,
        tags: vec![],
        container_name: None,
        range,
        selection_range: range,
        children: vec![],
    };
    vec![ProviderSymbols::new(Some("fake"), vec![main])]
}

#[gpui::test]
fn symbols_are_asked_once_per_version_and_kept_per_document(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "per-document");
    let asked = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&asked);
    wb.update(cx, |wb, _| {
        wb.outline.fake = Some(Box::new(move |_| {
            counter.set(counter.get() + 1);
            main_symbol()
        }));
    });
    let main = proj.join("src/main.rs");
    open(&wb, cx, &main);
    let later = || Instant::now() + Duration::from_secs(1);
    wb.update(cx, |wb, _| wb.outline_sync_at(later()));
    assert_eq!(asked.get(), 1, "the active document is asked for once");
    let model_of =
        |wb: &Workbench, path: &Path| wb.outline.model(path).map(|m| m.list_of_symbols().len());
    assert_eq!(wb.read_with(cx, |wb, _| model_of(wb, &main)), Some(1));

    // Another document: asked for on its own, and the first one's symbols are kept.
    let lib = proj.join("src/lib.rs");
    open(&wb, cx, &lib);
    wb.update(cx, |wb, _| wb.outline_sync_at(later()));
    assert_eq!(asked.get(), 2);
    assert_eq!(
        wb.read_with(cx, |wb, _| (model_of(wb, &main), model_of(wb, &lib))),
        (Some(1), Some(1))
    );

    // Asking again without a change is answered from the cache.
    wb.update(cx, |wb, _| wb.outline_sync_at(later()));
    assert_eq!(asked.get(), 2);
}

#[gpui::test]
fn an_edit_asks_again_after_the_debounce(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "edit");
    let asked = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&asked);
    wb.update(cx, |wb, _| {
        wb.outline.fake = Some(Box::new(move |_| {
            counter.set(counter.get() + 1);
            main_symbol()
        }));
    });
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| {
        wb.outline_sync_at(Instant::now() + Duration::from_secs(1))
    });
    assert_eq!(asked.get(), 1);

    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.expect("a file is open");
        wb.editors.open[i]
            .editor
            .set_cursor(vsr_editor::Position::new(0, 0), false);
        wb.editors.open[i].editor.type_text("// ");
        cx.notify();
    });
    cx.run_until_parked();
    wb.update(cx, |wb, _| {
        wb.outline_sync_at(Instant::now() + Duration::from_secs(1))
    });
    assert_eq!(
        asked.get(),
        2,
        "a new version is asked for once the document is still"
    );
}

/// Opens `main.txt` (the fixture's symbols: `fn alpha`, `fn beta`, `struct Point` with a field
/// `x`) in a workspace with the language-features fixture and checks the outline answers with
/// them. The fixture's selector is patched to `plaintext`, the language of a `.txt` file.
#[gpui::test]
fn the_extension_hosts_symbols_reach_the_outline(cx: &mut TestAppContext) {
    const SRC: &str = "fn alpha(a, b) {\n\talpha(1, 2) \n}\nfn beta() {}\n// see https://example.com/x\nstruct Point";
    let Some((wb, cx, s)) = crate::extensions::e2e::start_fixture(
        cx,
        "outline-host",
        "lang-features-fixture",
        &["*"],
        &[("main.txt", SRC)],
        &[],
        |js| js.replace("'fakelang'", "'plaintext'"),
    ) else {
        return;
    };
    let path = s.ws.join("main.txt");
    wb.update(cx, |wb, cx| {
        wb.open(&path);
        cx.notify();
    });
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        wb.update(cx, |wb, cx| {
            wb.ext_sync();
            wb.outline_sync_at(Instant::now() + Duration::from_secs(1));
            cx.notify();
        });
        cx.run_until_parked();
        let names = wb.read_with(cx, |wb, _| {
            wb.outline.model(&path).map(|m| {
                m.list_of_symbols()
                    .into_iter()
                    .map(|s| s.name)
                    .collect::<Vec<_>>()
            })
        });
        if let Some(names) = names.filter(|n| !n.is_empty()) {
            assert_eq!(names, ["alpha", "beta", "Point", "x"]);
            return;
        }
        assert!(Instant::now() < deadline, "the host answered no symbols");
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// `alpha` (line 1) and `beta` (line 2) for `fn alpha() {}` / `fn beta() {}`.
fn alpha_beta() -> Vec<ProviderSymbols> {
    let sym = |name: &str, line: u32| DocumentSymbol {
        name: name.into(),
        detail: String::new(),
        kind: 11,
        tags: vec![],
        container_name: None,
        range: Range::new(line, 1, line, 14),
        selection_range: Range::new(line, 4, line, 4 + name.len() as u32),
        children: vec![],
    };
    vec![ProviderSymbols::new(
        Some("fake"),
        vec![sym("alpha", 1), sym("beta", 2)],
    )]
}

/// The labels of the rows of the open Quick Pick.
fn picker_labels(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    let p = wb
        .read_with(cx, |wb, _| wb.quick.picker().cloned())
        .expect("the picker is open");
    p.read_with(cx, |p, _| {
        p.list()
            .rows()
            .iter()
            .filter_map(|r| r.item().map(|i| i.label.clone()))
            .collect()
    })
}

#[gpui::test]
fn go_to_symbol_lists_the_symbols_of_the_file_and_enter_selects_one(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "goto-symbol");
    fs::write(proj.join("src/main.rs"), "fn alpha() {}\nfn beta() {}\n").unwrap();
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| {
        wb.outline.fake = Some(Box::new(|_| alpha_beta()));
        wb.outline_sync_at(Instant::now() + Duration::from_secs(1));
    });
    wb.update_in(cx, |wb, window, cx| wb.show_quick_access("@", window, cx));
    cx.run_until_parked();
    assert_eq!(picker_labels(&wb, cx), ["alpha", "beta"]);

    // Typing after `@` narrows the rows.
    wb.update_in(cx, |wb, window, cx| {
        if let Some(p) = wb.quick.pick.clone() {
            p.update(cx, |p, cx| p.set_value("@bet", window, cx));
        }
        wb.quick_access_changed("@bet", window, cx);
    });
    cx.run_until_parked();
    assert_eq!(picker_labels(&wb, cx), ["beta"]);

    // Enter on the row selects the symbol's selection range (line 2, columns 4..8, 1-based).
    let p = wb
        .read_with(cx, |wb, _| wb.quick.picker().cloned())
        .expect("the picker is open");
    p.update(cx, |p, cx| {
        let id = p
            .list()
            .rows()
            .iter()
            .find_map(|r| r.item().map(|i| i.id))
            .expect("a row");
        p.set_active_item(id);
        p.accept(false, cx);
    });
    cx.run_until_parked();
    let sel = wb
        .read_with(cx, |wb, _| {
            wb.editors.active_editor().map(|e| e.editor.selection())
        })
        .expect("an editor is active");
    assert_eq!((sel.anchor.line, sel.anchor.column), (2, 4));
    assert_eq!((sel.active.line, sel.active.column), (2, 8));
}

/// A `class Point` (lines 1-3) with a field `x` (line 2), and a function `main` on line 5.
fn nested_symbols() -> Vec<ProviderSymbols> {
    let sym = |name: &str, kind: u32, range: Range, children: Vec<DocumentSymbol>| DocumentSymbol {
        name: name.into(),
        detail: String::new(),
        kind,
        tags: vec![],
        container_name: None,
        range,
        selection_range: range,
        children,
    };
    let x = sym("x", 7, Range::new(2, 3, 2, 8), vec![]);
    let point = sym("Point", 4, Range::new(1, 1, 3, 2), vec![x]);
    let main = sym("main", 11, Range::new(5, 1, 5, 14), vec![]);
    vec![ProviderSymbols::new(Some("fake"), vec![point, main])]
}

/// The rows of the Outline view as (name, depth).
fn view_rows(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<(String, usize)> {
    wb.read_with(cx, |wb, app| wb.outline_rows(app).into_iter().map(|r| (r.name, r.depth)).collect())
}

fn view_row_id(wb: &Entity<Workbench>, cx: &mut VisualTestContext, name: &str) -> String {
    wb.read_with(cx, |wb, app| wb.outline_rows(app).into_iter().find(|r| r.name == name).expect("the row").id)
}

/// Debug selectors are `&'static str`; a test leaks the few it needs.
fn selector(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn click(cx: &mut VisualTestContext, key: &'static str) {
    let at = cx.debug_bounds(key).unwrap_or_else(|| panic!("{key} is on screen")).center();
    cx.simulate_mouse_move(at, None, Default::default());
    cx.simulate_click(at, Default::default());
    cx.run_until_parked();
}

/// Asks for the symbols of the open file, as the debounce does after a change.
fn ask(wb: &Entity<Workbench>, cx: &mut VisualTestContext) {
    wb.update(cx, |wb, _| wb.outline_sync_at(Instant::now() + Duration::from_secs(1)));
    cx.run_until_parked();
}

#[gpui::test]
fn the_view_lists_the_symbols_of_the_active_file_depth_first(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-rows");
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    assert_eq!(
        view_rows(&wb, cx),
        [("Point".to_string(), 0), ("x".to_string(), 1), ("main".to_string(), 0)],
        "children follow their parent, expanded by default"
    );
    let status = wb.read_with(cx, |wb, app| wb.outline_status(app));
    assert_eq!(status, view::OutlineStatus::Rows);
}

#[gpui::test]
fn the_section_opens_from_its_header_and_draws_its_rows(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-section");
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    let main = view_row_id(&wb, cx, "main");
    let row = selector(format!("outline-row:{main}"));
    assert!(cx.debug_bounds(row).is_none(), "the section starts collapsed, as upstream's");

    click(cx, "outline-header");
    assert!(wb.read_with(cx, |wb, _| wb.outline.view.open));
    assert!(cx.debug_bounds(row).is_some(), "the open section draws the row");

    click(cx, "outline-header");
    assert!(cx.debug_bounds(row).is_none(), "a second click closes it");
}

#[gpui::test]
fn clicking_a_symbol_selects_its_range_in_the_editor(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-select");
    fs::write(proj.join("src/main.rs"), "struct Point {\n    x: u8,\n}\n\nfn main() {}\n").unwrap();
    open(&wb, cx, &proj.join("src/main.rs"));
    // `main`'s name is at line 5, columns 4..8 (1-based, as the symbols and the editor are).
    wb.update(cx, |wb, _| {
        wb.outline.fake = Some(Box::new(|_| {
            let main = DocumentSymbol {
                name: "main".into(),
                detail: String::new(),
                kind: 11,
                tags: vec![],
                container_name: None,
                range: Range::new(5, 1, 5, 14),
                selection_range: Range::new(5, 4, 5, 8),
                children: vec![],
            };
            vec![ProviderSymbols::new(Some("fake"), vec![main])]
        }))
    });
    ask(&wb, cx);
    click(cx, "outline-header");
    let main = view_row_id(&wb, cx, "main");
    click(cx, selector(format!("outline-row:{main}")));
    let sel = wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.editor.selection())).expect("an editor");
    assert_eq!((sel.anchor.line, sel.anchor.column), (5, 4));
    assert_eq!((sel.active.line, sel.active.column), (5, 8));
}

#[gpui::test]
fn collapse_items_always_collapse_starts_items_closed_and_the_chevron_opens_them(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench_with(cx, "view-collapse", r#"{"outline.collapseItems": "alwaysCollapse"}"#);
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("main".to_string(), 0)]);

    click(cx, "outline-header");
    let point = view_row_id(&wb, cx, "Point");
    click(cx, selector(format!("outline-toggle:{point}")));
    assert_eq!(
        view_rows(&wb, cx),
        [("Point".to_string(), 0), ("x".to_string(), 1), ("main".to_string(), 0)],
        "the chevron expands the item"
    );
    click(cx, selector(format!("outline-toggle:{point}")));
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("main".to_string(), 0)], "and collapses it again");
}

#[gpui::test]
fn the_view_says_why_it_is_empty(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-empty");
    let status = |wb: &Entity<Workbench>, cx: &mut VisualTestContext| wb.read_with(cx, |wb, app| wb.outline_status(app));
    assert_eq!(status(&wb, cx), view::OutlineStatus::NoEditor, "no editor shows a file");

    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| Vec::new())));
    ask(&wb, cx);
    assert_eq!(status(&wb, cx), view::OutlineStatus::NoProvider, "no provider answered");

    wb.update(cx, |wb, _| {
        wb.outline.fake = Some(Box::new(|_| vec![ProviderSymbols::new(Some("fake"), Vec::new())]));
    });
    let other = proj.join("src/lib.rs");
    open(&wb, cx, &other);
    ask(&wb, cx);
    assert_eq!(status(&wb, cx), view::OutlineStatus::NoSymbols("lib.rs".into()), "a provider with no symbols");
}

#[gpui::test]
fn an_edit_refreshes_the_rows_after_the_debounce(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-refresh");
    open(&wb, cx, &proj.join("src/main.rs"));
    let answers = Rc::new(Cell::new(0u32));
    let counter = Rc::clone(&answers);
    wb.update(cx, |wb, _| {
        wb.outline.fake = Some(Box::new(move |_| {
            counter.set(counter.get() + 1);
            if counter.get() == 1 { main_symbol() } else { nested_symbols() }
        }));
    });
    ask(&wb, cx);
    assert_eq!(view_rows(&wb, cx), [("main".to_string(), 0)]);
    wb.update(cx, |wb, cx| {
        let i = wb.editors.active.expect("a file is open");
        wb.editors.open[i].editor.set_cursor(vsr_editor::Position::new(0, 0), false);
        wb.editors.open[i].editor.type_text("// ");
        cx.notify();
    });
    cx.run_until_parked();
    ask(&wb, cx);
    assert_eq!(view_rows(&wb, cx).len(), 3, "the new answer replaces the rows");
}

/// The names of the rows of the Outline view that are selected (follow cursor).
fn view_selected(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, app| wb.outline_rows(app).into_iter().filter(|r| r.selected).map(|r| r.name).collect())
}

#[gpui::test]
fn follow_cursor_selects_the_symbol_at_the_caret_and_opens_its_parent(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench_with(cx, "view-follow", r#"{"outline.collapseItems": "alwaysCollapse"}"#);
    fs::write(proj.join("src/main.rs"), "struct Point {\n    x: u8,\n}\n\nfn main() {}\n").unwrap();
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    click(cx, "outline-header");
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("main".to_string(), 0)]);

    click(cx, "outline-follow");
    // The caret inside `x` (line 2, columns 3..8): `x` is selected, and `Point`, its parent, opens.
    wb.update(cx, |wb, _| {
        let i = wb.editors.active.expect("a file is open");
        wb.editors.open[i].editor.set_cursor(vsr_editor::Position::new(2, 4), false);
    });
    cx.run_until_parked();
    assert_eq!(view_selected(&wb, cx), ["x"]);
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("x".to_string(), 1), ("main".to_string(), 0)]);
}

#[gpui::test]
fn sort_by_cycles_position_name_and_category_from_the_header(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-sort");
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    click(cx, "outline-header");
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("x".to_string(), 1), ("main".to_string(), 0)], "position");
    click(cx, "outline-sort");
    assert_eq!(view_rows(&wb, cx), [("main".to_string(), 0), ("Point".to_string(), 0), ("x".to_string(), 1)], "name");
    click(cx, "outline-sort");
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("x".to_string(), 1), ("main".to_string(), 0)], "category: class before function");
    click(cx, "outline-sort");
    assert_eq!(wb.read_with(cx, |wb, _| wb.outline.view.sort_by), view::SortBy::Position, "and back to position");
}

#[gpui::test]
fn filter_on_type_narrows_the_rows_as_you_type_and_escape_clears_them(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-filter");
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    click(cx, "outline-header");
    click(cx, "outline-filter");
    assert!(wb.read_with(cx, |wb, _| wb.outline.view.filter_on_type));

    cx.simulate_keystrokes("m");
    cx.run_until_parked();
    assert_eq!(view_rows(&wb, cx), [("main".to_string(), 0)], "the rows whose name contains m");
    cx.simulate_keystrokes("backspace");
    cx.run_until_parked();
    assert_eq!(view_rows(&wb, cx).len(), 3, "backspace takes the text back");
    cx.simulate_keystrokes("x");
    cx.run_until_parked();
    assert_eq!(view_rows(&wb, cx), [("Point".to_string(), 0), ("x".to_string(), 1)], "a match keeps its parent");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(view_rows(&wb, cx).len(), 3, "escape clears the text");
}

/// Sets a setting in the user's settings, as the Settings editor does.
fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// A diagnostic `error` on line 5, columns 4..8 (the name of `main` in `nested_symbols`).
fn main_error() -> serde_json::Value {
    serde_json::json!({"severity": 8, "startLineNumber": 5, "startColumn": 4, "endLineNumber": 5, "endColumn": 8, "message": "boom"})
}

/// Puts the extension host's diagnostics of `path` in place, as the host would publish them.
fn set_diagnostics(wb: &Entity<Workbench>, cx: &mut VisualTestContext, path: &Path, markers: Vec<serde_json::Value>) {
    let key = crate::extensions::snapshot::resource_key(path);
    wb.update(cx, |wb, _| {
        wb.ext.snapshot.diagnostics.insert(key, markers);
    });
}

#[gpui::test]
fn a_diagnostic_marks_its_symbol_and_shows_a_badge_on_its_row(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-problems");
    let main_file = proj.join("src/main.rs");
    open(&wb, cx, &main_file);
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    set_diagnostics(&wb, cx, &main_file, vec![main_error()]);
    ask(&wb, cx);
    click(cx, "outline-header");
    let main = view_row_id(&wb, cx, "main");
    let top = wb.read_with(cx, |wb, app| wb.outline_rows(app).into_iter().find(|r| r.name == "main").and_then(|r| r.marker));
    assert_eq!(top.map(|m| m.top_severity), Some(vsr_exthost::language_features::outline::severity::ERROR));
    assert!(cx.debug_bounds(selector(format!("outline-badge:{main}"))).is_some(), "the badge is drawn");
}

#[gpui::test]
fn problems_settings_turn_the_badges_and_the_markers_off(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench_with(cx, "view-problems-off", r#"{"outline.problems.badges": false}"#);
    let main_file = proj.join("src/main.rs");
    open(&wb, cx, &main_file);
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    set_diagnostics(&wb, cx, &main_file, vec![main_error()]);
    ask(&wb, cx);
    click(cx, "outline-header");
    let main = view_row_id(&wb, cx, "main");
    assert!(cx.debug_bounds(selector(format!("outline-badge:{main}"))).is_none(), "outline.problems.badges off");
    assert!(
        wb.read_with(cx, |wb, app| wb.outline_rows(app).iter().any(|r| r.marker.is_some())),
        "the problem is still counted for the colour"
    );

    set(cx, "outline.problems.enabled", serde_json::json!(false));
    cx.run_until_parked();
    assert!(
        wb.read_with(cx, |wb, app| wb.outline_rows(app).iter().all(|r| r.marker.is_none())),
        "outline.problems.enabled off: no problems on any symbol"
    );
}

/// `outline.show<Kind>`, in the order of the symbol kinds (`SymbolKind` 0 to 25, as upstream lists them).
const SHOW_SETTINGS: [&str; 26] = [
    "showFiles",
    "showModules",
    "showNamespaces",
    "showPackages",
    "showClasses",
    "showMethods",
    "showProperties",
    "showFields",
    "showConstructors",
    "showEnums",
    "showInterfaces",
    "showFunctions",
    "showVariables",
    "showConstants",
    "showStrings",
    "showNumbers",
    "showBooleans",
    "showArrays",
    "showObjects",
    "showKeys",
    "showNull",
    "showEnumMembers",
    "showStructs",
    "showEvents",
    "showOperators",
    "showTypeParameters",
];

/// One top-level symbol of each kind, `k<kind>` on line `kind + 1`.
fn one_per_kind() -> Vec<ProviderSymbols> {
    let symbols = (0..26u32)
        .map(|kind| DocumentSymbol {
            name: format!("k{kind}"),
            detail: String::new(),
            kind,
            tags: vec![],
            container_name: None,
            range: Range::new(kind + 1, 1, kind + 1, 5),
            selection_range: Range::new(kind + 1, 1, kind + 1, 5),
            children: vec![],
        })
        .collect();
    vec![ProviderSymbols::new(Some("fake"), symbols)]
}

#[gpui::test]
fn each_show_kind_setting_hides_only_its_kind_from_the_view(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-kinds");
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| one_per_kind())));
    ask(&wb, cx);
    let names = |cx: &mut VisualTestContext| -> Vec<String> {
        view_rows(&wb, cx).into_iter().map(|(name, _)| name).collect()
    };
    assert_eq!(names(cx).len(), 26, "every kind shows by default");
    for (kind, setting) in SHOW_SETTINGS.iter().enumerate() {
        set(cx, &format!("outline.{setting}"), serde_json::json!(false));
        let shown = names(cx);
        assert_eq!(shown.len(), 25, "{setting} hides one row");
        assert!(!shown.contains(&format!("k{kind}")), "{setting} hides the kind {kind}");
        set(cx, &format!("outline.{setting}"), serde_json::json!(true));
    }
}

#[gpui::test]
fn outline_icons_off_draws_no_kind_icon(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "view-icons");
    open(&wb, cx, &proj.join("src/main.rs"));
    wb.update(cx, |wb, _| wb.outline.fake = Some(Box::new(|_| nested_symbols())));
    ask(&wb, cx);
    click(cx, "outline-header");
    let main = view_row_id(&wb, cx, "main");
    let icon = selector(format!("outline-icon:{main}"));
    assert!(cx.debug_bounds(icon).is_some(), "the icon is drawn by default");
    set(cx, "outline.icons", serde_json::json!(false));
    assert!(cx.debug_bounds(icon).is_none(), "outline.icons off");
}
