//! Headless E2E tests of the breadcrumbs (docs/testing.md, tier 2): the bar shows the path of the
//! active file, a crumb's dropdown lists its siblings, folders open in place and files open, the
//! three `breadcrumbs.*` settings that are wired, the theme colours and the keyboard focus.
//!
//! Scenario source: tests/e2e-scenarios/workbench.md (rscode-only scenarios, Breadcrumbs).

use std::fs;
use std::path::{Path, PathBuf};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use serde_json::json;
use vsr_quickinput::pick::PickRow;

use super::model::BreadcrumbsSettings;
use crate::Workbench;
use crate::config::AppConfig;
use crate::quick_input::QuickPick;
use crate::theme_service::ThemeService;

/// A workbench over `<tmp>/proj` (`src/main.rs`, `src/lib.rs`, `Cargo.toml`) with its settings in a
/// temp file, so the real user settings are never touched.
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-crumbs-e2e-{name}-{}-{nanos}", std::process::id()));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(proj.join("src/lib.rs"), "pub fn lib() {}\n").unwrap();
    fs::write(proj.join("Cargo.toml"), "[package]\n").unwrap();
    let settings = dir.join("settings.json");
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        super::init(cx);
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

fn set(cx: &mut VisualTestContext, key: &str, value: serde_json::Value) {
    cx.update(|_, cx| AppConfig::set_value(cx, key, Some(value))).unwrap();
    cx.run_until_parked();
}

/// The crumbs the bar shows, by name.
fn crumb_names(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, cx| {
        let settings = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration());
        wb.crumb_segments(&settings).into_iter().map(|s| s.name).collect()
    })
}

fn picker(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Option<Entity<QuickPick>> {
    wb.read_with(cx, |wb, _| wb.quick.picker().cloned())
}

/// The labels of the dropdown rows, in order.
fn dropdown_labels(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    let p = picker(wb, cx).expect("the dropdown is open");
    p.read_with(cx, |p, _| p.list().rows().iter().filter_map(|r| r.item().map(|i| i.label.clone())).collect())
}

/// Accepts the dropdown row with `label` (as Enter does on the selected row).
fn accept_row(wb: &Entity<Workbench>, cx: &mut VisualTestContext, label: &str) {
    let p = picker(wb, cx).expect("the dropdown is open");
    p.update(cx, |p, cx| {
        let id = p.list().rows().iter().find_map(|r| match r {
            PickRow::Item(i) if i.label == label => Some(i.id),
            _ => None,
        });
        let id = id.unwrap_or_else(|| panic!("no row {label}"));
        p.set_active_item(id);
        p.accept(false, cx);
    });
    cx.run_until_parked();
}

fn click_crumb(cx: &mut VisualTestContext, index: usize) {
    let selector: &'static str = match index {
        0 => "breadcrumb:0",
        1 => "breadcrumb:1",
        2 => "breadcrumb:2",
        _ => panic!("no test uses crumb {index}"),
    };
    let bounds = cx.debug_bounds(selector).unwrap_or_else(|| panic!("{selector} is not rendered"));
    cx.simulate_mouse_move(bounds.center(), None, Default::default());
    cx.simulate_click(bounds.center(), Default::default());
    cx.run_until_parked();
}

fn bar_is_shown(cx: &mut VisualTestContext) -> bool {
    cx.debug_bounds("breadcrumbs").is_some()
}

#[gpui_kit::test]
fn the_bar_shows_the_path_of_the_active_file(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "path");
    open(&wb, cx, &proj.join("src/main.rs"));
    assert_eq!(crumb_names(&wb, cx), ["src", "main.rs"]);
    assert!(bar_is_shown(cx));
    assert!(cx.debug_bounds("breadcrumb:0").is_some());
    assert!(cx.debug_bounds("breadcrumb:1").is_some());
}

#[gpui_kit::test]
fn clicking_a_crumb_lists_its_siblings_with_the_crumb_selected(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "siblings");
    open(&wb, cx, &proj.join("src/main.rs"));
    click_crumb(cx, 0);
    // `src` is a crumb of the root folder, so its siblings are the root's entries.
    assert_eq!(dropdown_labels(&wb, cx), ["src", "Cargo.toml"]);
    let p = picker(&wb, cx).unwrap();
    let active = p.read_with(cx, |p, _| p.active_item().map(|i| i.label.clone()));
    assert_eq!(active.as_deref(), Some("src"));
}

#[gpui_kit::test]
fn accepting_a_folder_lists_its_entries_and_accepting_a_file_opens_it(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "open");
    open(&wb, cx, &proj.join("src/main.rs"));
    click_crumb(cx, 0);
    accept_row(&wb, cx, "src");
    // The folder's own entries; the dropdown stays open.
    assert_eq!(dropdown_labels(&wb, cx), ["lib.rs", "main.rs"]);
    accept_row(&wb, cx, "lib.rs");
    assert!(picker(&wb, cx).is_none(), "accepting a file closes the dropdown");
    let active = wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.path.clone()));
    assert_eq!(active, Some(proj.join("src/lib.rs")));
}

#[gpui_kit::test]
fn file_path_last_shows_only_the_file_name(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "last");
    open(&wb, cx, &proj.join("src/main.rs"));
    set(cx, "breadcrumbs.filePath", json!("last"));
    assert_eq!(crumb_names(&wb, cx), ["main.rs"]);
    assert!(cx.debug_bounds("breadcrumb:1").is_none());
}

#[gpui_kit::test]
fn file_path_off_hides_the_bar(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "off");
    open(&wb, cx, &proj.join("src/main.rs"));
    set(cx, "breadcrumbs.filePath", json!("off"));
    assert!(crumb_names(&wb, cx).is_empty());
    assert!(!bar_is_shown(cx));
    set(cx, "breadcrumbs.filePath", json!("on"));
    assert!(bar_is_shown(cx));
}

#[gpui_kit::test]
fn breadcrumbs_enabled_false_hides_the_bar(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "enabled");
    open(&wb, cx, &proj.join("src/main.rs"));
    set(cx, "breadcrumbs.enabled", json!(false));
    assert!(!bar_is_shown(cx));
    set(cx, "breadcrumbs.enabled", json!(true));
    assert!(bar_is_shown(cx));
}

#[gpui_kit::test]
fn the_bar_takes_its_colours_from_the_colour_theme(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "theme");
    open(&wb, cx, &proj.join("src/main.rs"));
    let dark = wb.read_with(cx, |wb, _| (wb.theme.breadcrumb_bg, wb.theme.breadcrumb_fg));
    set(cx, "workbench.colorTheme", json!("Light Modern"));
    let light = wb.read_with(cx, |wb, _| (wb.theme.breadcrumb_bg, wb.theme.breadcrumb_fg));
    assert_ne!(dark, light, "the bar follows the theme");
    let service = cx.update(|_, cx| ThemeService::theme(cx));
    assert_eq!(light, (service.breadcrumb_bg, service.breadcrumb_fg));
    assert!(light.0.3 > 0 && light.1.3 > 0, "the theme sets both colours");
}

#[gpui_kit::test]
fn cmd_shift_semicolon_focuses_the_last_crumb_and_arrows_move_between_crumbs(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "keys");
    open(&wb, cx, &proj.join("src/main.rs"));
    cx.simulate_keystrokes("cmd-shift-;");
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.crumbs.active), Some(1));
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.crumbs.active), Some(0));
    cx.simulate_keystrokes("left");
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.crumbs.active), Some(0), "stops at the first crumb");
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert_eq!(wb.read_with(cx, |wb, _| wb.crumbs.active), None);
}

#[gpui_kit::test]
fn cmd_shift_period_focuses_and_opens_the_dropdown_of_the_last_crumb(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "select");
    open(&wb, cx, &proj.join("src/main.rs"));
    cx.simulate_keystrokes("cmd-shift-.");
    cx.run_until_parked();
    // The last crumb is the file: its dropdown lists the folder it is in.
    assert_eq!(dropdown_labels(&wb, cx), ["lib.rs", "main.rs"]);
}

// ---- symbols after the file path (ADR-0087) ----

use vsr_editor::Position as EditorPosition;
use vsr_exthost::language_features::dto::DocumentSymbol;
use vsr_exthost::language_features::outline::ProviderSymbols;

use super::symbols::SymbolSettings;

/// `src/main.rs` for the symbol tests: `struct Point` (lines 1-5) with `fn len` inside.
const POINT_RS: &str = "struct Point {\n    fn len(&self) -> i32 {\n        self.x\n    }\n}\n";

/// A symbol with 1-based `(start line, start column, end line, end column)` ranges.
fn sym(name: &str, kind: u32, range: (u32, u32, u32, u32), selection: (u32, u32, u32, u32), children: Vec<DocumentSymbol>) -> DocumentSymbol {
    let r = |(a, b, c, d): (u32, u32, u32, u32)| vsr_exthost::language_features::dto::Range::new(a, b, c, d);
    DocumentSymbol {
        name: name.into(),
        detail: String::new(),
        kind,
        tags: vec![],
        container_name: None,
        range: r(range),
        selection_range: r(selection),
        children,
    }
}

/// What an extension answers for `POINT_RS`: class `Point` (kind 4) with method `len` (kind 5).
fn point_symbols() -> Vec<ProviderSymbols> {
    let len = sym("len", 5, (2, 5, 4, 6), (2, 8, 2, 11), vec![]);
    let point = sym("Point", 4, (1, 1, 5, 2), (1, 8, 1, 13), vec![len]);
    vec![ProviderSymbols::new(Some("fake"), vec![point])]
}

/// The workbench with `POINT_RS` open, its symbols answered in-process, and the caret on line 3
/// (`self.x`), inside `len`.
fn open_point<'a>(cx: &'a mut TestAppContext, name: &str) -> (Entity<Workbench>, &'a mut VisualTestContext) {
    let (wb, vcx, proj) = workbench(cx, name);
    fs::write(proj.join("src/main.rs"), POINT_RS).unwrap();
    open(&wb, vcx, &proj.join("src/main.rs"));
    wb.update(vcx, |wb, _| {
        wb.outline.fake = Some(Box::new(|_| point_symbols()));
        wb.outline_sync_at(std::time::Instant::now() + std::time::Duration::from_secs(1));
        wb.editors.active.and_then(|i| wb.editors.open.get_mut(i)).expect("the file is open").editor.set_cursor(EditorPosition::new(2, 8), false);
    });
    vcx.run_until_parked();
    (wb, vcx)
}

/// The crumbs the bar shows: the file crumbs, then the symbols at the caret.
fn bar_names(wb: &Entity<Workbench>, cx: &mut VisualTestContext) -> Vec<String> {
    wb.read_with(cx, |wb, cx| {
        let settings = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration());
        let symbols = SymbolSettings::from_configuration(AppConfig::get(cx).configuration());
        let mut names: Vec<String> = wb.crumb_segments(&settings).into_iter().map(|s| s.name).collect();
        names.extend(wb.symbol_crumbs(&symbols).into_iter().map(|c| c.name));
        names
    })
}

#[gpui::test]
fn the_symbols_at_the_caret_follow_the_file_crumbs(cx: &mut TestAppContext) {
    let (wb, cx) = open_point(cx, "symbols-path");
    assert_eq!(bar_names(&wb, cx), ["src", "main.rs", "Point", "len"]);
}

#[gpui::test]
fn breadcrumbs_symbol_path_last_keeps_only_the_innermost_symbol(cx: &mut TestAppContext) {
    let (wb, cx) = open_point(cx, "symbols-last");
    set(cx, "breadcrumbs.symbolPath", json!("last"));
    assert_eq!(bar_names(&wb, cx), ["src", "main.rs", "len"]);
}

#[gpui::test]
fn breadcrumbs_symbol_path_off_hides_the_symbols(cx: &mut TestAppContext) {
    let (wb, cx) = open_point(cx, "symbols-off");
    set(cx, "breadcrumbs.symbolPath", json!("off"));
    assert_eq!(bar_names(&wb, cx), ["src", "main.rs"]);
}

#[gpui::test]
fn clicking_a_symbol_crumb_lists_its_siblings_and_accepting_selects_the_symbol(cx: &mut TestAppContext) {
    let (wb, cx) = open_point(cx, "symbols-dropdown");
    // Crumbs: src (0), main.rs (1), Point (2), len (3).
    wb.update_in(cx, |wb, window, cx| wb.show_crumb_picker(2, window, cx));
    cx.run_until_parked();
    assert_eq!(dropdown_labels(&wb, cx), ["Point"], "the siblings of Point at the top");
    wb.update_in(cx, |wb, window, cx| wb.show_crumb_picker(3, window, cx));
    cx.run_until_parked();
    assert_eq!(dropdown_labels(&wb, cx), ["len"], "the siblings of len inside Point");
    accept_row(&wb, cx, "len");
    cx.run_until_parked();
    let sel = wb.read_with(cx, |wb, _| wb.editors.active_editor().map(|e| e.editor.selection()));
    let sel = sel.expect("an editor is active");
    assert_eq!((sel.anchor.line, sel.anchor.column), (1, 7), "the selection range of len, 0-based");
    assert_eq!((sel.active.line, sel.active.column), (1, 10));
}

#[gpui::test]
fn breadcrumbs_show_method_false_hides_the_method_from_its_dropdown(cx: &mut TestAppContext) {
    let (wb, cx) = open_point(cx, "symbols-show");
    set(cx, "breadcrumbs.showMethods", json!(false));
    wb.update_in(cx, |wb, window, cx| wb.show_crumb_picker(3, window, cx));
    cx.run_until_parked();
    assert_eq!(dropdown_labels(&wb, cx), Vec::<String>::new());
}
