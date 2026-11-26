//! The snapshot scenarios. Each builds a workbench over fixed files, reaches a state with keys and
//! workbench calls, and compares the offscreen render with `tests/ui-snapshots/<name>.png`.

use std::time::Duration;

use super::Harness;

const MAIN_RS: &str = r#"//! A small demo program.
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Point { x, y }
    }

    fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

fn main() {
    let mut seen: HashMap<String, Point> = HashMap::new();
    let a = Point::new(1.0, 2.0);
    let b = Point::new(4.0, 6.0);
    seen.insert("a".to_string(), a.clone());
    // alpha is five units away
    println!("distance = {}", a.distance(&b));
    for (name, p) in &seen {
        println!("{name}: {p:?} {}", 42);
    }
}
"#;

const FILES: &[(&str, &str)] = &[
    ("src/main.rs", MAIN_RS),
    ("src/lib.rs", "pub fn alpha() -> u32 {\n    1\n}\n"),
    ("docs/notes.md", "# Notes\n\nAlpha and beta.\nalpha again\n"),
    ("README.md", "# demo\n"),
];

const DARK: &str = "{ \"workbench.colorTheme\": \"Default Dark Modern\", \"editor.cursorBlinking\": \"solid\" }";
const LIGHT: &str = "{ \"workbench.colorTheme\": \"Default Light Modern\", \"editor.cursorBlinking\": \"solid\" }";

fn start(name: &str, settings: &str) -> Harness {
    let mut h = Harness::new(name, settings, FILES);
    let p = h.path("src/main.rs");
    h.workbench(|wb, cx| {
        wb.open(&p);
        cx.notify();
    });
    // Syntax colours arrive from a background parse.
    h.cx.allow_parking();
    for _ in 0..10 {
        h.advance(Duration::from_millis(100));
        std::thread::sleep(Duration::from_millis(30));
    }
    h.cx.forbid_parking();
    h
}

fn editor(name: &str, settings: &str) {
    let mut h = start(name, settings);
    h.snapshot(name);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn editor_rust_dark() {
    editor("editor_rust_dark", DARK);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn editor_rust_light() {
    editor("editor_rust_light", LIGHT);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn settings_editor_dark() {
    let mut h = start("settings_editor_dark", DARK);
    h.keys("cmd-,");
    h.snapshot("settings_editor_dark");
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn settings_editor_light() {
    let mut h = start("settings_editor_light", LIGHT);
    h.keys("cmd-,");
    h.snapshot("settings_editor_light");
}

fn search(name: &str, settings: &str) {
    let mut h = start(name, settings);
    h.keys("shift-cmd-f");
    h.type_text("alpha");
    h.wait_until("search results", |h| {
        h.workbench(|wb, cx| wb.search.as_ref().is_some_and(|s| s.read(cx).summary().is_some()))
    });
    h.snapshot(name);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn search_results_dark() {
    search("search_results_dark", DARK);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn search_results_light() {
    search("search_results_light", LIGHT);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn find_widget_dark() {
    let mut h = start("find_widget_dark", DARK);
    h.keys("cmd-f");
    h.type_text("point");
    h.snapshot("find_widget_dark");
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn find_widget_light() {
    let mut h = start("find_widget_light", LIGHT);
    h.keys("cmd-f");
    h.type_text("point");
    h.snapshot("find_widget_light");
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn quick_open_dark() {
    let mut h = start("quick_open_dark", DARK);
    h.keys("cmd-p");
    h.type_text("a");
    h.advance(Duration::from_millis(300));
    h.snapshot("quick_open_dark");
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn quick_open_light() {
    let mut h = start("quick_open_light", LIGHT);
    h.keys("cmd-p");
    h.type_text("a");
    h.advance(Duration::from_millis(300));
    h.snapshot("quick_open_light");
}

fn terminal(name: &str, settings: &str) {
    // A static shell: prints fixed lines, then waits (no prompt, so nothing depends on the user's rc files).
    let settings = settings.trim_end().trim_end_matches('}').to_string()
        + ", \"terminal.integrated.profiles.osx\": { \"t\": { \"path\": \"/bin/sh\", \"args\": [\"-c\", \"printf 'rscode terminal\\\\r\\\\n$ cargo build\\\\r\\\\n   Compiling demo v0.1.0\\\\r\\\\n    Finished dev\\\\r\\\\n$ '; cat\"] } }, \"terminal.integrated.defaultProfile.osx\": \"t\" }";
    let mut h = start(name, &settings);
    h.keys("ctrl-`");
    h.wait_until("terminal output", |h| {
        h.workbench(|wb, cx| {
            wb.panel
                .terminals
                .get(wb.panel.active_terminal)
                .and_then(|t| t.view.read(cx).snapshot())
                .is_some_and(|s| s.text().contains("Finished"))
        })
    });
    h.snapshot(name);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn terminal_panel_dark() {
    terminal("terminal_panel_dark", DARK);
}

#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn terminal_panel_light() {
    terminal("terminal_panel_light", LIGHT);
}

/// The renders are not blank: a guard that the offscreen path draws the workbench (many colours,
/// both themes differ), independent of any reference image.
#[test]
#[ignore = "offscreen Metal render; run with --ignored"]
fn renders_are_real_and_themes_differ() {
    let mut dark = start("sanity_dark", DARK);
    let mut light = start("sanity_light", LIGHT);
    let (d, l) = (dark.capture(), light.capture());
    assert_eq!(d.dimensions(), (2200, 1400));
    let colours = |img: &image::RgbaImage| img.pixels().map(|p| p.0).collect::<std::collections::HashSet<_>>().len();
    assert!(colours(&d) > 50 && colours(&l) > 50, "{} {}", colours(&d), colours(&l));
    // Syntax colours: saturated text pixels inside the editor area (right of the sidebar, above the status bar).
    let coloured = |img: &image::RgbaImage| {
        img.enumerate_pixels()
            .filter(|(x, y, p)| (700..2150).contains(x) && (150..1250).contains(y) && p.0[..3].iter().max().unwrap() - p.0[..3].iter().min().unwrap() > 90)
            .count()
    };
    assert!(coloured(&d) > 300, "no syntax colours in dark: {}", coloured(&d));
    assert!(coloured(&l) > 300, "no syntax colours in light: {}", coloured(&l));
    let mean = |img: &image::RgbaImage| img.pixels().map(|p| p.0[0] as u64).sum::<u64>() / (img.width() * img.height()) as u64;
    assert!(mean(&l) > mean(&d) + 80, "light {} dark {}", mean(&l), mean(&d));
}
