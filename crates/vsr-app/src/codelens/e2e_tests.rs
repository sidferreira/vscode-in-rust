//! Headless E2E tests of code lens (ADR-0155). No extension host runs here: the providers' lists and
//! the resolves come from `CodeLensState::fake`, and the clock is passed in, so the debounce is exact.
//!
//! Upstream source: src/vs/editor/contrib/codelens/browser/codelensController.ts

use std::cell::Cell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use vsr_exthost::language_features::dto::{CodeLens, CodeLensList, Command, Range};

use super::{DEBOUNCE, FakeLenses, Settings};
use crate::Workbench;
use crate::config::AppConfig;

/// A workbench over `<tmp>/proj` (`src/main.rs`), with `settings` as the user settings file.
fn workbench<'a>(cx: &'a mut TestAppContext, name: &str, settings: &str) -> (Entity<Workbench>, &'a mut VisualTestContext, PathBuf) {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-codelens-e2e-{name}-{}-{nanos}", std::process::id()));
    let proj = dir.join("proj");
    fs::create_dir_all(proj.join("src")).unwrap();
    fs::write(proj.join("src/main.rs"), "fn main() {}\nfn helper() {}\n").unwrap();
    let settings_file = dir.join("settings.json");
    fs::write(&settings_file, settings).unwrap();
    cx.update(|cx| {
        gpui_kit::init(cx);
        crate::icons::install(cx);
        crate::settings_view::install(cx);
        crate::editor_view::init(cx);
        crate::commands::init(cx);
        crate::breadcrumbs::init(cx);
        AppConfig::install(cx, Some(settings_file));
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

/// One lens on line 1 of the document, without a command.
fn lens_on_line_one() -> CodeLens {
    CodeLens { range: Range::new(1, 1, 1, 8), command: None, cache_id: None }
}

/// The fake: one provider (handle 3) lists a lens on line 1 of every document, and resolves it to
/// `main.run`. `asks` counts the requests the providers were asked.
fn fake(asks: Rc<Cell<u32>>) -> FakeLenses {
    FakeLenses {
        lists: Box::new(move |_| {
            asks.set(asks.get() + 1);
            vec![(3, CodeLensList { lenses: vec![lens_on_line_one()], cache_id: None })]
        }),
        resolve: Box::new(|_| Some(Command { id: "main.run".into(), title: "Run".into(), tooltip: None, arguments: vec![], ident: None })),
    }
}

fn on() -> Settings {
    Settings { enabled: true }
}

#[gpui::test]
fn code_lenses_are_asked_after_the_debounce_and_resolved(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "lenses", "{}");
    open(&wb, cx, &proj.join("src/main.rs"));
    // The frames after opening observed the document on the real clock: start from a clean state
    // so the test's clock alone decides when the debounce ends.
    wb.update(cx, |wb, _| wb.codelens.disable());
    let asks = Rc::new(Cell::new(0));
    let t0 = Instant::now();
    let file = proj.join("src/main.rs");
    wb.update(cx, |wb, _| wb.codelens.fake = Some(fake(asks.clone())));
    wb.update(cx, |wb, _| {
        wb.codelens_sync_at(t0, on());
        wb.codelens_sync_at(t0 + DEBOUNCE - Duration::from_millis(1), on());
    });
    assert_eq!(asks.get(), 0, "the providers are not asked before the debounce ends");
    wb.update(cx, |wb, _| wb.codelens_sync_at(t0 + DEBOUNCE, on()));
    assert_eq!(asks.get(), 1, "asked once when the debounce ends");
    wb.update(cx, |wb, _| {
        let lenses = wb.codelens.lenses(&file);
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].lens.command.as_ref().map(|c| c.title.as_str()), Some("Run"), "resolved with the command");
    });
}

#[gpui::test]
fn editor_code_lens_off_never_asks_the_providers(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "off", r#"{"editor.codeLens": false}"#);
    open(&wb, cx, &proj.join("src/main.rs"));
    let asks = Rc::new(Cell::new(0));
    wb.update(cx, |wb, _| wb.codelens.fake = Some(fake(asks.clone())));
    let t0 = Instant::now();
    let off = wb.update(cx, |_, cx| super::settings(cx));
    wb.update(cx, |wb, _| {
        wb.codelens_sync_at(t0, off);
        wb.codelens_sync_at(t0 + Duration::from_secs(5), off);
    });
    assert!(!off.enabled);
    assert_eq!(asks.get(), 0, "editor.codeLens is off: nothing is asked");
}

#[gpui::test]
fn a_lens_that_never_resolves_is_asked_once_and_closing_the_document_drops_its_lenses(cx: &mut TestAppContext) {
    let (wb, cx, proj) = workbench(cx, "never", "{}");
    let file = proj.join("src/main.rs");
    open(&wb, cx, &file);
    wb.update(cx, |wb, _| wb.codelens.disable());
    let resolves = Rc::new(Cell::new(0));
    let counted = resolves.clone();
    wb.update(cx, |wb, _| {
        wb.codelens.fake = Some(FakeLenses {
            lists: Box::new(|_| vec![(3, CodeLensList { lenses: vec![lens_on_line_one()], cache_id: None })]),
            resolve: Box::new(move |_| {
                counted.set(counted.get() + 1);
                None
            }),
        })
    });
    let t0 = Instant::now();
    wb.update(cx, |wb, _| {
        wb.codelens_sync_at(t0, on());
        for frame in 0..120 {
            wb.codelens_sync_at(t0 + DEBOUNCE + Duration::from_millis(16 * frame), on());
        }
    });
    assert_eq!(resolves.get(), 1, "a lens that stays commandless is resolved once, not every frame");
    wb.update(cx, |wb, _| assert_eq!(wb.codelens.lenses(&file).len(), 1));
    wb.update(cx, |wb, cx| {
        wb.editors.close(0);
        cx.notify();
    });
    wb.update(cx, |wb, _| {
        wb.codelens_sync_at(t0 + Duration::from_secs(10), on());
        assert!(wb.codelens.lenses(&file).is_empty(), "the closed document's lenses are dropped");
    });
}
