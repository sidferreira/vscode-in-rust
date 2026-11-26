//! UI snapshot tests (docs/testing.md tier 3, ADR-0161..0163): pixel snapshots of real workbench
//! states, rendered offscreen. No OS window exists: gpui's test platform hosts the window, the real
//! CoreText text system shapes the text, and gpui's `MetalHeadlessRenderer` rasterizes the scene
//! into an offscreen texture that is read back as an image. Nothing is shown, focused or activated.
//!
//! Run with `cargo test -p vsr-app ui_snapshots -- --ignored` (they are `#[ignore]`d: the GPU is
//! needed and the output depends on the installed fonts). `UPDATE_SNAPSHOTS=1` rewrites the
//! reference images in `tests/ui-snapshots/`.

mod compare;
mod scenarios;

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gpui::{AnyWindowHandle, App, AppContext as _, HeadlessAppContext, Keystroke, WindowHandle, px, size};

use crate::Workbench;
use crate::config::AppConfig;

/// The window size in points; the test platform renders at scale factor 2, so images are 2x.
pub const WINDOW: (f32, f32) = (1100., 700.);

/// A workbench in an offscreen window, driven by keys and settled by running the executors.
pub struct Harness {
    pub cx: HeadlessAppContext,
    pub window: WindowHandle<Workbench>,
    pub dir: PathBuf,
}

/// A fresh scratch directory under the system temp dir (never deleted recursively by tests).
pub fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let dir = std::env::temp_dir().join(format!("rscode-ui-snap-{name}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    dir
}

impl Harness {
    /// `files` are written under a fresh workspace folder; `settings` is the user settings.json text
    /// (in a fresh temp directory: the real settings file is never read).
    pub fn new(name: &str, settings: &str, files: &[(&str, &str)]) -> Harness {
        let dir = scratch(name);
        let root = dir.join("workspace");
        fs::create_dir_all(&root).unwrap();
        for (rel, text) in files {
            let p = root.join(rel);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(p, text).unwrap();
        }
        let settings_path = dir.join("settings.json");
        fs::write(&settings_path, settings).unwrap();

        let text_system = Arc::new(gpui_apple::AppleTextSystem::new());
        let mut cx = HeadlessAppContext::with_platform(text_system, Arc::new(()), || {
            Ok(Some(Box::new(gpui_apple::metal_renderer::MetalHeadlessRenderer::new()?)))
        });
        cx.update(|cx| {
            gpui_kit::init(cx);
            crate::icons::install(cx);
            crate::settings_view::install(cx);
            crate::editor_view::init(cx);
            crate::search_view::init(cx);
            crate::quick_open::init(cx);
            crate::commands::init(cx);
            AppConfig::install(cx, Some(settings_path));
            crate::theme_service::ThemeService::install(cx);
        });
        let window = cx
            .update(|cx: &mut App| {
                cx.open_window(
                    gpui::WindowOptions {
                        window_bounds: Some(gpui::WindowBounds::Windowed(gpui::Bounds {
                            origin: gpui::point(px(0.), px(0.)),
                            size: size(px(WINDOW.0), px(WINDOW.1)),
                        })),
                        focus: true,
                        show: true,
                        ..Default::default()
                    },
                    |_, cx| cx.new(|cx| Workbench::new(root, cx)),
                )
            })
            .expect("open the offscreen window");
        let mut h = Harness { cx, window, dir };
        h.settle();
        h
    }

    pub fn handle(&self) -> AnyWindowHandle {
        self.window.into()
    }

    /// Runs pending tasks and draws twice (layout-dependent state settles on the second frame).
    pub fn settle(&mut self) {
        for _ in 0..2 {
            self.cx.run_until_parked();
            self.draw();
        }
        self.cx.run_until_parked();
        self.draw();
    }

    fn draw(&mut self) {
        let h = self.handle();
        self.cx.update_window(h, |_, window, cx| {
            let done = window.draw(cx);
            done.clear(cx);
        })
        .unwrap();
    }

    /// Space-separated keystrokes, e.g. `"cmd-p"`.
    pub fn keys(&mut self, keystrokes: &str) -> &mut Self {
        let h = self.handle();
        for k in keystrokes.split_whitespace() {
            let ks = Keystroke::parse_with_os(k, gpui::OperatingSystem::Mac).unwrap_or_else(|_| panic!("bad keystroke {k}"));
            self.cx.update_window(h, |_, window, cx| {
                window.dispatch_keystroke(ks, cx);
            })
            .unwrap();
        }
        self.settle();
        self
    }

    /// Types text as if from the keyboard.
    pub fn type_text(&mut self, text: &str) -> &mut Self {
        let h = self.handle();
        for c in text.chars() {
            let key = c.to_string();
            let ks = Keystroke { modifiers: Default::default(), key: key.clone(), key_char: Some(key) };
            self.cx.update_window(h, |_, window, cx| {
                window.dispatch_keystroke(ks, cx);
            })
            .unwrap();
        }
        self.settle();
        self
    }

    /// Waits (real time) for background work such as searches and shells: advances the fake clock
    /// and lets real threads run until `done` holds.
    pub fn wait_until(&mut self, what: &str, mut done: impl FnMut(&mut Harness) -> bool) {
        self.cx.allow_parking();
        for _ in 0..400 {
            self.advance(Duration::from_millis(20));
            if done(self) {
                self.cx.forbid_parking();
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        panic!("timed out waiting for {what}");
    }

    pub fn advance(&mut self, by: Duration) -> &mut Self {
        self.cx.advance_clock(by);
        self.settle();
        self
    }

    pub fn workbench<R>(&mut self, f: impl FnOnce(&mut Workbench, &mut gpui::Context<Workbench>) -> R) -> R {
        let w = self.window;
        let out = self.cx.update(|cx| w.update(cx, |wb, _, cx| f(wb, cx))).expect("workbench update");
        self.settle();
        out
    }

    pub fn path(&self, rel: &str) -> PathBuf {
        self.dir.join("workspace").join(rel)
    }

    /// Renders the current frame offscreen.
    pub fn capture(&mut self) -> image::RgbaImage {
        self.settle();
        let h = self.handle();
        self.cx.capture_screenshot(h).expect("offscreen render")
    }

    /// Captures and compares against `tests/ui-snapshots/<name>.png` (see `compare`).
    pub fn snapshot(&mut self, name: &str) {
        let img = self.capture();
        compare::assert_snapshot(name, &img);
    }
}

/// `tests/ui-snapshots/` at the repository root.
pub fn snapshot_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/ui-snapshots")
}
