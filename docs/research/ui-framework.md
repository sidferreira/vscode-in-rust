# UI framework research for vscode-rust

Researched 2026-11-09. Target machine: macOS 27, Apple Silicon (arm64), rustc 1.99.1 stable.
Nothing was built or installed for this report. All versions and dates come from the crates.io API (`/api/v1/crates/<name>/versions`), GitHub, and the docs pages linked below.

Goal: an editor-class window (multi-pane layout, sidebar, tabs, a code-editor surface with fast text, IME later, files of many thousands of lines) running within about 1.5 hours of work.

---

## TL;DR

| Framework | Latest (crates.io) | Released | License | Editor-class fit | Verdict |
|---|---|---|---|---|---|
| **gpui (through gpui-kit)** | gpui-kit 0.9.1 → gpui-pre 0.3.12 | 2026-11-09 | Apache-2.0 | Best. Zed is built on it. Ships Dock, Tabs, Sidebar, Resizable, Tree, VirtualList, Command palette, Editor (rope + tree-sitter + folding + LSP hooks) | **Primary** |
| gpui (official crate) | 0.2.2 | 2025-10-22 | Apache-2.0 | Same engine, but this crate is about 13 months stale. Current API is only on git | Use only through gpui-kit |
| egui / eframe | 0.37.0 | 2026-10-15 | MIT OR Apache-2.0 | Good for fast iteration. The editor surface must be custom (virtualized rows + painter) | **Fallback** |
| iced | 0.14.0 | 2025-12-07 | MIT | Solid: `text_editor` widget, IME (new in 0.14), cosmic-text shaping. No virtual list or dock built in | Second fallback |
| slint | 1.18.1 | 2026-09-21 | GPL-3.0 / royalty-free / commercial | Weak for a code editor: no free-form custom-painted widget, DSL-first | No |
| floem (Lapce) | 0.2.0 on crates.io | 2024-11-14 | MIT | Has a real editor (Lapce's), but the current API is git-only (Lapce pins a rev) | No (git-only) |
| xilem / masonry | 0.4.0 | 2025-10-29 | Apache-2.0 | Promising (parley + vello + accesskit), still alpha, few app-level widgets | No |
| makepad | makepad-widgets 1.0.0 on crates.io; 2.0.0 on git `main` | 2025-05-13 (1.0) | MIT OR Apache-2.0 | Fast builds, own DSL; 2.0 is git-only and changes fast | No |
| freya | 0.5.1 | 2026-11-08 | MIT | Has a code-editor component (Valin), but pulls `freya-skia-safe` (prebuilt Skia download, hundreds of MB) | No (disk) |

---

## 1. gpui (Zed's framework)

### Where gpui is published (three lineages)

1. **Official `gpui` on crates.io: 0.2.2 (2025-10-22), Apache-2.0**, owned by Zed staff (`github:zed-industries:crates-io`).
   It has not been republished since. Zed `main` has since split the framework into `gpui`, `gpui_platform`, `gpui_macos`, `gpui_apple`, `gpui_web` and others. The README on `main` tells you to add `gpui_platform` from crates.io, but **`gpui_platform` is not on crates.io** (the API returns nothing). So the current upstream API is only available as a git dependency on the whole Zed monorepo, which is a very large clone.
   - https://crates.io/crates/gpui
   - https://github.com/zed-industries/zed/blob/main/crates/gpui/README.md
2. **`gpui-pre` 0.3.12 (2026-11-09), Apache-2.0**: weekly snapshots of Zed `main` ("gpui-pre snapshot of zed@d2a4fb6"), published by `huacnlee` (Longbridge, maintainer of gpui-component / GPUI Kit). The family includes `gpui-pre-platform`, `gpui-pre-macos`, `gpui-pre-apple` and others. It has about 441k recent downloads, mostly through gpui-kit. It is third-party published but tracks upstream closely. **This is the practical way to get current gpui from crates.io.**
   - https://crates.io/crates/gpui-pre
3. **Community forks:** `gpui-ce` 0.3.0 (2026-10-20, single owner, MSRV 1.95), `gpui-box` 0.1.1 (2026-08-12), and `gpui-standalone` 1.4.4 (2026-06-01, 74 downloads). These are small forks and are not recommended.
   - https://github.com/gpui-ce/gpui-ce

### GPUI Kit (formerly gpui-component), the layer to use

- `gpui-kit` 0.9.1 (2026-11-09), Apache-2.0 (docs prose CC BY 4.0). Repo: https://github.com/longbridge/gpui-kit (about 20k stars, pushed today). It releases weekly and pins `gpui-pre =0.3.12` and `gpui-pre-platform =0.3.12`.
- It re-exports gpui, so one dependency covers both. Components relevant to an editor (https://gpui-kit.com/component/):
  - **Dock** ("tabs, splits, and persistent state")
  - **Tabs**, **Sidebar**, **Resizable**, **Tree**, **VirtualList**, **Command** (command palette), **Menu**, **StatusBar**, **Notification**, **Theme** (about 38 themes)
  - **Editor**
- **Editor component** (https://gpui-kit.com/component/editor, v0.9.1):
  - Text is stored in a rope (`ropey =2.0.0-beta.1`).
  - Tree-sitter highlighting is enabled per language through Cargo features (`tree-sitter-rust` and others). `syntect` is a lighter alternative.
  - Line numbers, folding, multi-cursor, column selection, search and replace (Cmd-F), soft wrap, read-only mode, gutter and overview-ruler marks.
  - Completion provider hooks are LSP-shaped (it depends on `lsp-types`).
  - The README claims "stable performance at 200K lines with Tree-sitter highlighting and LSP diagnostics, completion, and hover". Large buffers are parsed in the background.
  - An LSP editor example is available (`cargo run -p example-editor`).
- **Real use:** Longbridge Pro, a commercial trading desktop app.

### Build on stable macOS: the Metal shader gotcha

- MSRV: GPUI Kit's installation docs say Rust 1.92 or later, so 1.99.1 is fine. Its docs also say macOS 15 or later plus the Xcode Command Line Tools (https://gpui-kit.com/docs/installation/).
- **Gotcha (confirmed in source):** `gpui-pre-apple`'s `build.rs` runs `xcrun -sdk macosx metal …` and `xcrun metallib …` to precompile the shaders **unless the `runtime_shaders` feature is on**. Command Line Tools alone do not include `metal`. On Xcode 26 and later, the Metal Toolchain is a separate download (`xcodebuild -downloadComponent MetalToolchain`) that must match the Xcode build, and it costs disk space.
  - Zed's own macOS build docs say to use `--features gpui/runtime_shaders` on macOS 26+: https://zed.dev/docs/development/macos
  - Apple docs on the component: https://developer.apple.com/tutorials/data/documentation/xcode/downloading-and-installing-additional-xcode-components.md
- `gpui-kit` does **not** expose `runtime_shaders`. Enable it by adding `gpui-pre-platform` directly at the same exact version. Cargo unifies the features, so this adds no new crate:
  ```toml
  gpui-pre-platform = { version = "=0.3.12", features = ["runtime_shaders", "font-kit"] }
  ```
  With this feature on, the shaders are compiled by Metal at runtime. You still need Command Line Tools (cbindgen, the SDK headers and the linker), but not full Xcode or the Metal Toolchain.
- `font-kit` is required for text on macOS when you depend on gpui directly. gpui-kit already enables it, but listing it does no harm.

### Maturity for a code editor

- **Text:** native Core Text shaping and rasterization on macOS through font-kit. Glyph quality matches Zed: subpixel positioning, ligatures, font fallback.
- **Custom drawing:** you can implement a custom `Element` with full paint control. Zed's editor is one of these. `uniform_list` and `list` are virtualized primitives.
- **Keyboard:** action and keymap system with key contexts. Zed's keybindings are built on it.
- **IME:** full `NSTextInputClient` support through `EntityInputHandler`. Zed handles CJK input with it.
- **Theming:** gpui-kit `Theme` with about 38 bundled themes.

### Size and compile time (estimates)

- Measured (third party): a plain GPUI hello world took **35 s cold build**, and its release binary was **6.6 MB**. Hardware was not stated. Source: https://github.com/Ernxst/gpuix/pull/603. A GPUI 0.2.2 benchmark on an M4 Max measured a 3.8 MiB binary, the fastest first frame (152 ms median), and a scroll p99 of 8.29 ms: https://github.com/zed-industries/zed/discussions/63832
- **Estimate** for gpui-kit hello world with default features: about 450–550 crates, **2–4 min cold debug build** on M-series, and a **debug `target/` of roughly 3–5 GB**. Each enabled `tree-sitter-<lang>` adds C compile time. The gpui-kit docs only say "may take several minutes". This is an estimate from the dependency graph: gpui-pre has 75 direct dependencies and gpui-component has 83.

### Hello world (gpui-kit 0.9.1)

Source: https://gpui-kit.com/docs/getting-started/ (Apache-2.0)

```rust
use gpui_kit::component::button::{Button, ButtonVariants};
use gpui_kit::*;

struct HelloWorld;

impl Render for HelloWorld {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .child("Hello, World!")
            .child(
                Button::new("hello")
                    .primary()
                    .label("Click me")
                    .on_click(|_, _, _| println!("Clicked!")),
            )
    }
}

fn main() {
    application()
        .with_assets(assets::Assets)
        .run(|cx| {
            init(cx);

            open_window(WindowOptions::default(), cx, |_, cx| {
                cx.new(|_| HelloWorld)
            })
            .expect("Failed to open window");
        });
}
```

Raw gpui without the kit: see the upstream example https://github.com/zed-industries/zed/blob/main/crates/gpui/examples/hello_world.rs. It uses `gpui_platform::application().run(|cx: &mut App| { cx.open_window(WindowOptions { .. }, |_, cx| cx.new(|_| HelloWorld { .. })) })`. With gpui-pre, rename the packages: `gpui = { package = "gpui-pre", version = "=0.3.12" }` and `gpui_platform = { package = "gpui-pre-platform", version = "=0.3.12", features = ["font-kit"] }`.

Editor snippet (https://gpui-kit.com/component/editor):

```rust
let editor = cx.new(|cx| {
    EditorState::new(window, cx)
        .language("rust")
        .line_number(true)
        .folding(true)
        .tab_size(TabSize { tab_size: 4, hard_tabs: false })
        .default_value("fn main() {\n    println!(\"Hello\");\n}")
});
Editor::new(&editor).h(px(320.))
```

**Risks:**
- The snapshots are weekly and the API churns. Pin with `=` and upgrade on purpose.
- gpui-pre is published by a third party, not by Zed.
- Documentation is thinner than for egui or iced. Reading the Zed source helps.

---

## 2. egui / eframe

- **Version:** 0.37.0 (2026-10-15), MIT OR Apache-2.0, MSRV 1.95.
  - https://crates.io/crates/eframe
  - https://github.com/emilk/egui/blob/main/CHANGELOG.md
- **Stable macOS:** yes. The default renderer is `wgpu`, which compiles shaders at runtime, so no system dependencies beyond the Command Line Tools linker. `glow` is optional.
- **Text:** 0.37 shapes with `harfrust`, outlines with `skrifa`, and rasterizes with `vello_cpu` (from epaint's dependencies). New in 0.37:
  - system font fallback (`egui_system_font_provider`) and color emoji
  - non-Latin scripts
  - `CompletionPopup` for TextEdit
  - an improved fallback syntax highlighter in `egui_extras`
  - Breaking: `default_fonts` is no longer a default feature of `egui`, but `eframe` still enables it by default.
- **Editor fit:**
  - Immediate mode makes custom widgets trivial: `ui.painter()` plus `ScrollArea::show_rows` gives a virtualized line view.
  - The built-in `TextEdit` lays out the whole buffer, so it is not suited to files of many thousands of lines. A real editor needs a custom rope-backed widget.
  - IME works through winit.
  - There is no built-in dock or tabs. The third-party `egui_dock` crate covers this; check that its version matches 0.37 before use.
  - Theming: Visuals plus the new experimental `StyleProvider`.
- **Real use:** Rerun viewer and many tools. No major code editor is built on it.
- **Compile:** measured "eframe compiles in about 10s on my M3 MacBook Pro" (clean build of the library; wgpu roughly doubles that, 2024): https://github.com/emilk/egui/issues/4481. **Estimate:** hello-world cold debug build 30–60 s, `target/` about 1–1.5 GB. Release binary about 5–9 MB (gpui-kit comparison page; Zed discussion #63832 measured 9.3 MiB).
- **Hello world** (copied from https://github.com/emilk/egui/blob/0.37.0/examples/hello_world_simple/src/main.rs, trimmed):

```rust
use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([320.0, 240.0]),
        ..Default::default()
    };
    let mut name = "Arthur".to_owned();
    eframe::run_ui_native("My egui App", options, move |ui, _frame| {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.heading("My egui Application");
            ui.text_edit_singleline(&mut name);
            ui.label(format!("Hello '{name}'"));
        });
    })
}
```

---

## 3. iced

- **Version:** 0.14.0 (2025-12-07), MIT, MSRV 1.88. The repo is active (pushed 2026-11-01) but there has been no 0.15 release yet.
  - https://github.com/iced-rs/iced/releases/tag/0.14.0
- **Stable macOS:** yes. Renders with wgpu, so no shader toolchain is needed.
- **Text:** cosmic-text 0.15 (rustybuzz/swash) for shaping.
- **New in 0.14:**
  - **input method (IME) support**
  - reactive rendering
  - `text_editor` improvements: indent and unindent, `LineEnding`, `move_to`
  - `table` and `grid` widgets
  - time-travel debugging, hot reloading, headless testing
- **Editor fit:**
  - `text_editor` has syntax highlighting through `iced::highlighter` (syntect) and is used by the official `editor` example.
  - There is no built-in virtual list or dock. Use `pane_grid` for splits.
  - Custom widgets go through `iced::advanced::Widget`, which is more ceremony than egui or gpui.
  - The Elm architecture gives clean state handling.
- **Real use:** COSMIC desktop (System76, through the libcosmic fork) including COSMIC Text Editor, and Halloy (an IRC client).
- **Compile estimate:** about 2–3 min cold debug, `target/` about 2 GB. Release binary about 11 MB (gpui-kit comparison page).
- **Hello world** (README at the 0.14.0 tag, https://github.com/iced-rs/iced/blob/0.14.0/README.md):

```rust
use iced::widget::{button, column, text, Column};

#[derive(Default)]
struct Counter { value: i32 }

#[derive(Debug, Clone, Copy)]
enum Message { Increment }

impl Counter {
    fn update(&mut self, message: Message) {
        match message { Message::Increment => self.value += 1 }
    }
    fn view(&self) -> Column<Message> {
        column![text(self.value).size(50), button("+").on_press(Message::Increment)]
    }
}

fn main() -> iced::Result {
    iced::run(Counter::update, Counter::view)
}
```

The full editor example uses `iced::application(Editor::new, Editor::update, Editor::view).theme(..).run()`: https://github.com/iced-rs/iced/blob/0.14.0/examples/editor/src/main.rs

---

## 4. Slint

- **Version:** 1.18.1 (2026-09-21); 1.18.0 came out 2026-09-16 (https://slint.dev/blog/slint-1.18-released). MSRV 1.92.
- **License:** `GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`. The royalty-free license allows desktop apps with attribution.
- **Stable macOS:** yes (winit + FemtoVG by default). There is an experimental Vello renderer in 1.18.
- **Editor fit:** poor.
  - UI is written in the `.slint` DSL. `ListView` is virtualized and `TextInput` supports IME.
  - There is no arbitrary custom-paint widget suited to a code editor surface with per-glyph styling. You would have to render to images or abuse `Path`.
  - Text uses parley/skrifa/swash internally.
- **Real use:** embedded HMIs and desktop tools. No code editors.
- **Compile estimate:** about 2 min cold debug (the Slint compiler runs inside a proc macro), `target/` about 1.5–2.5 GB. Release binary about 9–21 MB.
- **Hello world** (https://github.com/slint-ui/slint/blob/v1.18.1/api/rs/slint/README.md):

```rust
slint::slint!{
    export component HelloWorld {
        Text { text: "hello world"; color: green; }
    }
}
fn main() { HelloWorld::new().unwrap().run().unwrap(); }
```

---

## 5. Floem (Lapce)

- **crates.io:** 0.2.0 (2024-11-14), MIT. This release is stale: winit 0.29 and wgpu 22.
- **git `main`:** active (pushed 2026-09-24, rust-version 1.91). Lapce pins `git = "https://github.com/lapce/floem", rev = "31fa8f44…"` with features `["editor", …]`: https://github.com/lapce/lapce/blob/master/Cargo.toml
- **Editor fit:** good on paper.
  - It includes `floem-editor-core` and an `editor` view (Lapce's own editor).
  - It has `virtual_list` and `virtual_stack`.
  - IME is supported (Lapce supports it).
  - Fine-grained signals.
  - However, the current API is git-only, there are few docs, and Lapce itself releases slowly (last push 2026-10-09).
- **Hello world** for crates.io 0.2.0 (https://github.com/lapce/floem/blob/v0.2.0/README.md). Note that `main` has renamed these to `Stack::horizontal` and `Label::derived`.

```rust
use floem::prelude::*;

fn main() { floem::launch(counter_view); }

fn counter_view() -> impl IntoView {
    let mut counter = RwSignal::new(0);
    h_stack((
        button("Increment").action(move || counter += 1),
        label(move || format!("Value: {counter}")),
        button("Decrement").action(move || counter -= 1),
    ))
    .style(|s| s.size_full().items_center().justify_center().gap(10))
}
```

---

## 6. Xilem / Masonry (Linebender)

- **Version:** 0.4.0 (2025-10-29), Apache-2.0, MSRV 1.88. Active on git (pushed 2026-11-03), with no newer release.
- **Stack:** Masonry 0.4 uses parley 0.6 (text), vello 0.6 (GPU 2D) and accesskit. This is a modern text stack with IME through winit.
- **Editor fit:** still alpha. It has few high-level widgets (no dock or tabs), APIs churn between releases, and no editor has shipped on it.
- **Compile estimate:** about 2–3 min, `target/` about 2 GB (vello + wgpu).
- **Hello world** (https://github.com/linebender/xilem/blob/v0.4.0/xilem/README.md):

```rust
use winit::error::EventLoopError;
use xilem::view::{Axis, text_button, flex, label};
use xilem::{EventLoop, WindowOptions, WidgetView, Xilem};

#[derive(Default)]
struct Counter { num: i32 }

fn app_logic(data: &mut Counter) -> impl WidgetView<Counter> + use<> {
    flex(Axis::Vertical, (
        label(format!("{}", data.num)),
        text_button("increment", |data: &mut Counter| data.num += 1),
    ))
}

fn main() -> Result<(), EventLoopError> {
    let app = Xilem::new_simple(Counter::default(), app_logic, WindowOptions::new("Counter app"));
    app.run_in(EventLoop::with_user_event())?;
    Ok(())
}
```

---

## 7. Makepad

- **Version:** `makepad-widgets` 1.0.0 (2025-05-13) on crates.io, MIT OR Apache-2.0. Git `main` is **2.0.0** with a new `script_mod!` / Splash DSL, releases tagged daily (`release-2026-11-06.x`), and the repo pushed today. https://github.com/makepad/makepad
- **Stable macOS:** yes. It has its own Metal backend with very few dependencies and is known for fast compiles.
- **Editor fit:** it has a `PortalList` (virtualized), a dock, and a code editor inside Makepad Studio. However, it uses its own DSL and conventions, 2.0 is git-only and undocumented outside the repo, and its IME support is thinner.
- **Compile estimate:** under 1 min cold, `target/` well under 1 GB (few dependencies). This is the leanest option.
- Hello world: the 2.0 API is in flux and has no stable published sample, so none is copied here. See the `examples/` folder in the repo.

## 8. Newer contenders checked

- **Freya 0.5.1** (2026-11-08), MIT. Dioxus-style components rendered with Skia. It has a code-editor component and the Valin editor is built on it. **Disqualified for this machine:** `freya-engine` depends on `freya-skia-safe`, which downloads prebuilt Skia binaries of hundreds of MB.
- **Blitz / dioxus-native** 0.7.10 (blitz-dom 0.2.4): an HTML/CSS engine and not an editor toolkit. No.
- **Vizia** (active): a smaller ecosystem with no editor. No.

---

## Supporting crates (current versions)

| Purpose | Crate | Version | Note |
|---|---|---|---|
| Text buffer (rope) | `ropey` | **2.0.0-beta.1** (2025-08-02); stable line is 1.6.1 (2023) | gpui-kit pins `=2.0.0-beta.1`. Use the same version to avoid compiling two ropes. `crop` 0.4.3 (2025-04-25) is a good B-tree rope alternative. |
| Text shaping | gpui has its own (Core Text / font-kit) | — | Do not add cosmic-text or parley with gpui. For a fallback stack: `cosmic-text` 0.19.0 (2026-04-22) or `parley` 0.12.0 (2026-10-09). |
| Parsing / highlighting | `tree-sitter` **0.26.13** + `tree-sitter-highlight` 0.26.x + `tree-sitter-rust` 0.24.2 | latest is 0.27.2 (2026-11-07) | **tree-sitter declares `links = "tree-sitter"`**, so only one 0.x line can exist in the dependency graph. gpui-kit's `tree-sitter` feature requires `^0.26.13`, so stay on 0.26 when using it. Use 0.27.2 only without gpui-kit's editor. The simplest path is gpui-kit's built-in `tree-sitter-<lang>` features. |
| File watching | `notify` | **8.2.2** (2026-11-08); 9.0.0-rc.8 exists | gpui-kit depends on `notify ^8.2`. Stay on 8 to share it. License CC0-1.0. |
| Fuzzy matching | `nucleo` 0.5.0 (2024-04-02) / `nucleo-matcher` 0.3.1 | MPL-2.0 | Helix's matcher. It is stable and not abandoned; it just has not needed releases. Use `nucleo-matcher` alone for the command palette and `nucleo` for the threaded file finder. |

---

## Disk-saving tips (no build was run for this report)

- Add to `Cargo.toml`:
  ```toml
  [profile.dev]
  debug = "line-tables-only"   # far smaller target/ while keeping usable backtraces
  [profile.dev.package."*"]
  debug = false                # no debuginfo for dependencies
  ```
- Enable only the tree-sitter languages you need. Each grammar is a C compile.
- Pick a single framework. Two GPU stacks in one workspace roughly double `target/`.
- Run `cargo clean -p <crate>` and `cargo sweep` occasionally.

---

## Recommendation

### Primary: gpui through GPUI Kit 0.9.1 (gpui-pre 0.3.12 snapshot of Zed main)

**Why:**
- It is the engine of Zed, the best-in-class Rust editor: native Core Text text rendering, full IME, a keymap and action system, and virtualized lists.
- GPUI Kit ships the IDE scaffolding ready-made: Dock (tabs, splits, persisted layout), Sidebar, Resizable, Tree, VirtualList, Command palette, StatusBar, themes, and a rope-backed tree-sitter Editor that claims 200K-line performance.
- A VSCode-like window in about 1.5 h is realistic: `Dock` + `Tree` + `Editor` + `StatusBar`.

**Cargo.toml:**

```toml
[dependencies]
gpui-kit = { version = "=0.9.1", features = ["tree-sitter-rust"] }
# Not a new crate: enables gpui's runtime_shaders so the build does NOT need
# the Xcode Metal Toolchain (xcrun metal). Must match gpui-kit's exact pin.
gpui-pre-platform = { version = "=0.3.12", features = ["runtime_shaders", "font-kit"] }

ropey = "=2.0.0-beta.1"          # same as gpui-kit's pin
notify = "8.2.2"                 # same major as gpui-kit
nucleo-matcher = "0.3.1"
# tree-sitter comes in through gpui-kit's feature (0.26.x). If you add it
# directly, use tree-sitter = "0.26.13", NOT 0.27 (links conflict).
```

**Hello world:** use the `gpui-kit` getting-started code in section 1 above, copied from https://gpui-kit.com/docs/getting-started/.

**Gotchas:**
- Requires Xcode Command Line Tools (`xcode-select -p` must print a path) and Rust 1.92 or later (1.99.1 is fine).
- If the build still fails with `xcrun: error: unable to find utility "metal"`, the `runtime_shaders` feature is not being unified. Check `cargo tree -e features -i gpui-pre-apple`. **Do not** download the Metal Toolchain on this disk. Switch to the fallback instead.
- The snapshots are weekly. Keep the `=` pins and bump `gpui-kit` and `gpui-pre-platform` together.
- The first build takes several minutes and will likely need a few GB in `target/` (estimate).

### Fallback: egui / eframe 0.37.0

**Why:**
- Plain crates.io release with MSRV 1.95.
- Renders through wgpu with no shader toolchain.
- The fastest compile of the mainstream options (about 10 s for the library on M3, measured in 2024).
- Immediate mode makes a custom virtualized editor surface (`ScrollArea::show_rows` + painter + ropey) quick to write.
- 0.37 adds real shaping (harfrust), system fonts, and `CompletionPopup`.

**Downside:** you build the editor widget and the dock yourself (or use `egui_dock`), and the look is less polished than VSCode.

```toml
[dependencies]
eframe = "0.37.0"
ropey = "1.6.1"
tree-sitter = "0.27.2"
tree-sitter-highlight = "0.27.2"
tree-sitter-rust = "0.24.2"
notify = "8.2.2"
nucleo-matcher = "0.3.1"
```

**Hello world:** see section 2, from https://github.com/emilk/egui/blob/0.37.0/examples/hello_world_simple/src/main.rs.

**Second fallback**, if you prefer retained mode / Elm architecture: `iced = { version = "0.14.0", features = ["highlighter"] }`, using its `text_editor` widget and `pane_grid`.

---

## Sources

- crates.io API (versions, dates, licenses, dependencies, features): https://crates.io/api/v1/crates/{gpui,gpui-pre,gpui-pre-platform,gpui-pre-apple,gpui-kit,gpui-component,gpui-ce,iced,eframe,egui,slint,floem,xilem,masonry,makepad-widgets,freya,freya-engine,ropey,crop,cosmic-text,parley,tree-sitter,notify,nucleo,nucleo-matcher}
- gpui-pre-apple build.rs (Metal / `runtime_shaders`): https://docs.rs/crate/gpui-pre-apple/0.3.12/source/build.rs
- Zed gpui README / hello_world / gpui_macos & gpui_platform Cargo.toml: https://github.com/zed-industries/zed/tree/main/crates/gpui , https://github.com/zed-industries/zed/blob/main/crates/gpui_platform/Cargo.toml
- Zed macOS build docs (Metal Toolchain, runtime_shaders on macOS 26+): https://zed.dev/docs/development/macos
- Apple: Metal Toolchain as an extra Xcode component: https://developer.apple.com/tutorials/data/documentation/xcode/downloading-and-installing-additional-xcode-components.md
- GPUI Kit: https://github.com/longbridge/gpui-kit , https://gpui-kit.com/docs/getting-started/ , https://gpui-kit.com/docs/installation/ , https://gpui-kit.com/component/ , https://gpui-kit.com/component/editor , https://gpui-kit.com/docs/comparison/
- GPUI hello-world build benchmark: https://github.com/Ernxst/gpuix/pull/603
- 5-toolkit bake-off (GPUI 0.2.2, egui, Slint, Qt, AppKit; M4 Max): https://github.com/zed-industries/zed/discussions/63832
- egui changelog and example: https://github.com/emilk/egui/blob/main/CHANGELOG.md , https://github.com/emilk/egui/blob/0.37.0/examples/hello_world_simple/src/main.rs , compile-time issue https://github.com/emilk/egui/issues/4481
- iced 0.14: https://github.com/iced-rs/iced/releases/tag/0.14.0 , https://github.com/iced-rs/iced/blob/0.14.0/README.md , https://github.com/iced-rs/iced/blob/0.14.0/examples/editor/src/main.rs
- Slint 1.18: https://github.com/slint-ui/slint/releases , https://slint.dev/blog/slint-1.18-released
- Floem / Lapce: https://github.com/lapce/floem , https://github.com/lapce/lapce/blob/master/Cargo.toml
- Xilem: https://github.com/linebender/xilem/blob/v0.4.0/xilem/README.md
- Makepad: https://github.com/makepad/makepad
- Freya: https://docs.rs/crate/freya/0.5.0
