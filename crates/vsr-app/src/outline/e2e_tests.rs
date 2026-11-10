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

/// A workbench over `<tmp>/proj` (`src/main.rs`, `src/lib.rs`), settings in a temp file.
fn workbench<'a>(
    cx: &'a mut TestAppContext,
    name: &str,
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

    // Enter on the row selects the symbol's selection range (line 2, columns 4..8, 0-based 1:3..1:7).
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
    assert_eq!((sel.anchor.line, sel.anchor.column), (1, 3));
    assert_eq!((sel.active.line, sel.active.column), (1, 7));
}
