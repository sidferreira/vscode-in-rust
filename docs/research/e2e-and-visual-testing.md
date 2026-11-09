# E2E and visual testing for the Rust/gpui VS Code port

Status: research, 2026-11-09. Upstream pin: 50e1350fc78d (read-only checkout `vscode-original`).
gpui sources cited below are in `~/.cargo/registry/src/*/` (abbreviated `$R/`). `gpui-kit-0.9.0`
pins `gpui-pre =0.3.11` (`$R/gpui-kit-0.9.0/Cargo.toml:270-272`), so the 0.3.11 sources are the ones
that matter. `gpui-pre-ios` and a zed-style `gpui` are NOT in the local registry.

## 1. Summary

- Upstream has NO pixel-diff tests for the workbench. Smoke tests are Playwright-driven and
  DOM-based; screenshots are only failure artifacts. Pixel comparison exists only for component
  fixtures (a CI report, `test/componentFixtures`), and it is not a gating baseline suite.
- gpui can render a window to an RGBA image offscreen on macOS through the Metal device, with no
  visible window and no Screen Recording permission (`Window::render_to_image`,
  `$R/gpui-pre-0.3.11/src/window.rs:3092`). Zed itself uses this for its own visual tests.
- Tier 2 (headless E2E) is feasible now: `TestAppContext` + `VisualTestContext` give
  `simulate_keystrokes`, `simulate_input`, `simulate_click`, `dispatch_action`, `run_until_parked`,
  `advance_clock`, and `debug_bounds`; gpui-kit adds `find(id)`, `click`, `press`, `input`.
- Tier 3 (visual) should use `HeadlessAppContext::capture_screenshot` (or Zed's
  `VisualTestAppContext`), NOT OS window capture. Compare with per-channel tolerance 2 plus a
  99% match ratio (Zed's numbers), with masks for caret/clock regions.
- Policy: baselines are app-window-only, live in gitignored `tests/visual/baselines/`, and nothing
  image-like is committed until the user approves it. First run records via an env var.

## 2. Upstream summary (VS Code @ 50e1350fc78d)

### 2.1 test/smoke (Playwright, Electron and web)

Entry `test/smoke/src/main.ts` (CLI options incl. `--shard`, `--web`; `setup()` at line 385;
`describe('VSCode Smoke Tests (Web|Electron)')` at line 448; shard selection `parseShard` /
`selectShardSuites` from `./shard`, used at main.ts:474-477). Scenario counts by `grep -rcE "^\s*it\("`
per area under `test/smoke/src/areas/` (total 108, matching the expectation):

| area | it() | area | it() |
|---|---|---|---|
| terminal (10 files incl. helpers) | 41 | search | 6 |
| chat | 12 | statusbar | 4 |
| workbench | 10 | task | 4 |
| agentsWindow | 9 | languages | 4 |
| browserView | 7 | accessibility | 3 |
| extensions | 3 | multiroot | 2 |
| preferences | 2 | policy | 1 |
| notebook | 0 (uses other helpers) | | |

Launch: `test/automation/src/playwrightElectron.ts:21` `launch()` resolves the Electron binary and
args (`electron.ts`), then `playwrightImpl._electron.launch({executablePath,...})` (line 49) and
wraps the first window in a `PlaywrightDriver`. Web mode uses `playwrightBrowser.ts` against a
served build. Parallelism is by SHARDS (processes split by suite), not workers inside one app.

### 2.2 test/automation (driver and page objects)

`test/automation/src/index.ts` re-exports ~30 modules: `Application` (application.ts),
`Code` (code.ts, the driver facade), `Workbench` (workbench.ts:32; `start/restart/stop`, tracing),
page objects `Editor`, `Editors`, `Explorer`, `Search`, `SCM`, `Terminal`, `Statusbar`,
`Quickinput`/`Quickaccess`, `Settings`, `Keybindings`, `Debug`, `Problems`, `Extensions`, `Task`,
`Chat`, `AgentsWindow`, `ActivityBar`, `Viewlet`. Public surface: a test builds an `Application`,
calls `workbench.<area>.<verb>()` which resolves to selector waits, clicks, and keystrokes through
`PlaywrightDriver` (`playwrightDriver.ts`). It is DOM-selector driven. A Rust analogue would be
a `WorkbenchDriver` struct over the test context exposing the same verbs.

### 2.3 Other suites

- `test/integration/{browser,electron,agentHost}`: extension-API and server integration tests run
  inside a real Electron or browser instance (README: `scripts/test-integration.sh`,
  `scripts/test-web-integration.sh`); `agentHost` has its own `runner.ts`.
- `test/unit/{electron,browser,node}`: mocha runners. Electron runs all `*.test.js` in a renderer
  (`scripts/test.sh`), with `--workers N` parallel Electron processes (README) and
  `--rerun-failed-files`. Browser runner uses `index.js` + `renderer.html`.
- `test/componentFixtures`: Playwright component explorer (`component-explorer.json`,
  `screenshotDir: .screenshots`, `playwright/tests/*.spec.ts` incl. `screenshotStability.spec.ts`).
  `blocks-ci-screenshots.md` is an auto-generated CI report: one screenshot per fixture variant
  (e.g. `chat/widget/chatWidget/PendingMessages/Queued/Light`) hosted externally, for human review.
  `component-explorer-diff.json` supports comparing two sessions. This is a review aid.

### 2.4 Screenshots / pixel diffs upstream

Grep for `toHaveScreenshot|pixelmatch|toMatchSnapshot` in `test/**/*.ts` finds no assertions.
Screenshot use: `playwrightDriver.ts:353-358` (`screenshotBuffer`) and `:706-708` (persist
`playwright-screenshot-N.png` to the logs path on failure), collected as CI artifacts by
`test/smoke/src/utils.ts:431-436`. Conclusion: workbench behavior is verified through DOM state;
there is no upstream pixel-regression baseline to port. Component screenshots are review-only.

## 3. gpui capabilities (gpui-pre 0.3.11, gpui-kit 0.9.0)

- `test-support` feature: `$R/gpui-pre-0.3.11/Cargo.toml:79-89` (enables collections/http_client
  test-support, wayland, x11, proptest, futures-concurrency, postage, web-time). Platform crates
  forward it (`gpui-pre-platform-0.3.11/Cargo.toml:54-58`, `gpui-pre-macos-0.3.11/Cargo.toml:47-49`);
  gpui-kit forwards it at `gpui-kit-0.9.0/Cargo.toml:76-82`.
- Headless contexts all use a deterministic `TestDispatcher` (seed from env `SEED`).

| need | API | citation (`$R/gpui-pre-0.3.11/src/` unless noted) |
|---|---|---|
| headless app, no real window | `TestAppContext` (`add_window`, `open_window`, `add_empty_window`, `add_window_view`) | app/test_context.rs:21, 284, 331, 352 |
| window-scoped context | `VisualTestContext` | app/test_context.rs:868 |
| settle async work | `run_until_parked` | test_context.rs:600, 895; executor.rs:232 |
| fake time | `advance_clock` | executor.rs:215; visual_test_context.rs:159 |
| keys | `simulate_keystrokes("cmd-shift-p b enter")`, `simulate_input("text")` | test_context.rs:622, 639, 924, 930 |
| mouse | `simulate_mouse_move/down/up`, `simulate_click(pos, modifiers)` | test_context.rs:935-980 |
| modifiers, resize, scale | `simulate_modifiers_change`, `simulate_resize`, `simulate_scale_factor_change` | test_context.rs:997, 1013, 1023 |
| raw events | `simulate_event` | test_context.rs:1068 |
| actions | `dispatch_action` | test_context.rs:605, 900 |
| native dialogs | `simulate_new_path_selection`, `simulate_prompt_answer` | test_context.rs:397-420 |
| layout read | `VisualTestContext::debug_bounds("sel")` + `.debug_selector(|| ..)` on divs | test_context.rs:1031; elements/div.rs:909; window.rs:1053-1090 |
| draw an element tree | `VisualTestContext::draw(origin, space, f)` | test_context.rs:1036 |
| painted output without pixels | `Window::painted_quads()`, `painted_underlines()` | window.rs:3102, 3108 |
| pixels (test platform) | `HeadlessAppContext::with_platform(text_system, assets, renderer_factory)`, `capture_screenshot(window)` | app/headless_app_context.rs:65, 168 |
| headless Metal renderer | `gpui_platform::current_headless_renderer()` -> `MetalHeadlessRenderer` | gpui-pre-platform-0.3.11/src/gpui_platform.rs:102; gpui-pre-apple-0.3.11/src/metal_renderer.rs:1889 |
| pixels (real mac platform) | `VisualTestAppContext` (`open_offscreen_window` at -10000,-10000; `capture_screenshot`) | app/visual_test_context.rs:25, 385 |
| Window to image | `Window::render_to_image` | window.rs:3092 -> macos/window.rs:2401 -> metal_renderer.rs:710 |
| no-window render to image | `render_scene_to_image(scene, size)` | metal_renderer.rs:~735 |
| ergonomic wrapper | `TestApp` | app/test_app.rs:1-40 |
| element ids/tree for tests | gpui-kit `TestWindowExt::find/click/press/input/render_frame`, `ElementSnapshot`, `TestQueryExt::elements/find_by_label/find_all_by_role`, `TestAppContextExt::wait_for` | gpui-kit-0.9.0/src/test.rs:33-75, 572; src/test/query.rs:9-17 |
| test macro | `#[gpui_kit::test]` / `#[gpui::test]` | gpui-kit TESTING.md; gpui-component-0.9.0/src/switch.rs:405 |

Can a test build a real view tree and hit-test it? Yes. `add_window_view` builds the real `Render`
tree on a `TestWindow`; `simulate_click` goes through gpui's real hitbox dispatch (`window.rs:1166`
`hit_test`); gpui-kit's TESTING.md states "Clicks use real hit testing" and shows an example at
`gpui-kit-0.9.0/tests/ui.rs:1-60`. Example of kit integration suites: `tests/integration/*.rs`
(dock, semantic_controls, text_selection, painted_text).

Offscreen render on macOS: PARTLY-YES, i.e. yes with caveats.
- `Window::render_to_image` is gated `#[cfg(any(test, feature = "test-support"))]` (window.rs:3091).
  It renders the last frame's scene into an owned Metal texture; it does not present and does not
  need the window visible (doc comment window.rs:3088-3090; metal_renderer.rs:700-722).
- The `TestPlatform` window returns an error unless a `HeadlessRenderer` is configured
  (platform/test/window.rs:574-583). The factory on macOS is `MetalHeadlessRenderer::new()`
  (gpui_platform.rs:102-112), so it needs a Metal device (any Mac GPU; a CI VM without Metal
  would fail) but no display, no window server session beyond what Metal needs, no Screen Recording.
- gpui-kit already ships such a test: `gpui-kit-0.9.0/tests/rendering.rs` (header: "need GPUI's
  real offscreen renderer: Metal on macOS or WGPU on Linux"); it builds a `HeadlessAppContext`,
  opens a window with `show: false`, calls `window.render_frame(cx)` then `capture_screenshot`, and
  asserts pixels are deterministic across two runs (lines 63-110).
- `VisualTestAppContext` (real `MacPlatform`) needs the macOS main thread; its tests are
  `#[ignore]` and run with `--ignored --test-threads=1` (gpui_platform.rs:126-136). Text is
  rendered with the platform text system (CoreText), so baselines are per-OS and per-font.
- Caveat: `VisualTestAppContext` uses a no-op asset source by default (SVG icons will not render
  unless `with_asset_source` is used) (visual_test_context.rs:49-53).
- Not available: enumerating anonymous rendered text. gpui-kit TESTING.md says the pinned gpui
  "does not expose an enumeration of anonymous rendered text", so text assertions need explicit
  `debug_selector`/accessibility labels, or shaped-line inspection in our own elements.

How Zed drives UI (web, read-only):
- Visual runner: https://github.com/zed-industries/zed/blob/main/crates/zed/src/visual_test_runner.rs
  (macOS only; `VisualTestAppContext`; baselines in `crates/zed/test_fixtures/visual_tests`;
  `UPDATE_BASELINE` records; `VISUAL_TEST_OUTPUT_DIR` default `target/visual_tests`; per-channel
  tolerance 2; pass at >= 99% matching pixels; `_diff.png` red/green on mismatch; no Screen
  Recording permission, no chrome in the image).
- Behavior tests: `crates/gpui/src/app/test_context.rs` and editor tests use `#[gpui::test]`,
  `TestAppContext`, `simulate_keystrokes`, `run_until_parked` (same APIs as local source above).

## 4. Alternatives

| option | what | verdict |
|---|---|---|
| (a) `TestAppContext` headless E2E | drive real workbench model/views: actions, keys, mouse, fake clock; assert on model + `debug_bounds` + gpui-kit `find()` | ADOPT as main E2E tier. Cannot: show real pixels, real IME, real OS menus/dialogs, real font metrics (test text system), GPU/compositor bugs, multi-process (extension host) timing realism |
| (b) layout/element snapshots | assert bounds, labels, state; text snapshot via `insta`/`expect-test`/`similar` | ADOPT as the cheap visual proxy. Crates: `similar 2.7.0` and `image 0.25.10` are in the local registry; `insta` and `expect-test` are NOT in the local registry (both well-known; need a firewall/approval decision). Until then a hand-rolled snapshot (write `.snap` text file, diff with `similar`) works. Limit: no anonymous-text enumeration; only `debug_selector`/labeled elements and `painted_quads()` |
| (c) OS window capture | `screencapture -l <windowID>` or `CGWindowListCreateImage(.., kCGWindowListOptionIncludingWindow, id, ..)` for ONE window only | FALLBACK ONLY. Needs Screen Recording permission (granted to the terminal/CI agent; headless macOS runners often cannot grant it), a visible, non-occluded window, a real display session, and the app actually running (which this research did not do). Never full screen. Use only for smoke checks of the real binary |
| (d) pixel diff | per-channel tolerance + match ratio | ADOPT for tier 3, see 5(c). SSIM/perceptual (`dssim`, `image-compare`) not in local registry; unneeded since rendering is deterministic on one machine |

Mask for dynamic regions: render with the caret hidden (blink state controlled via `advance_clock`
or a test flag that freezes it), inject a fixed clock for any time display, disable animations
(spring/transition) and `run_until_parked`, then, for anything left, zero out rectangles from a
named mask list (selector -> `debug_bounds`) in BOTH images before comparing.

## 5. Recommended 3-tier plan

### (a) Unit: ported upstream tests
`cargo test` in each crate (`crates/vsr-text/tests/upstream_piece_tree.rs` is the model). Each test
carries `// upstream: <path> > <suite> > <test> @ 50e1350fc78d`. No gpui needed.

### (b) Headless E2E from `tests/e2e-scenarios/*.md`
Directory does not exist yet. Each scenario markdown (steps in plain language, derived from the 108
smoke `it()`s and `test/automation` verbs) is implemented as a Rust test in `crates/vsr-app/tests/`
using `gpui-kit` with `features = ["test-support"]` as a dev-dependency:
- `#[gpui_kit::test] fn scenario(cx: &mut TestAppContext)`; `cx.update(gpui_kit::init)`.
- `let (workbench, cx) = cx.add_window_view(|w, cx| Workbench::new(w, cx));` (real model + views).
- Drive with `cx.simulate_keystrokes("cmd-p ...")`, `cx.simulate_input`, `cx.dispatch_action(..)`,
  `cx.simulate_click(pos, Modifiers::none())` with `pos` from `cx.debug_bounds("explorer-row-0")`
  (give elements `.debug_selector(|| ..)`) or gpui-kit `window.find("id").bounds().center()`.
- Settle with `cx.run_until_parked()`; time with `cx.executor().advance_clock(..)`; async with
  `TestAppContextExt::wait_for`.
- Assert on workbench model state first, then `debug_bounds` / `ElementSnapshot`, then optional
  text snapshot of the element tree (5 below tier b).
- A `WorkbenchDriver` helper (mirrors `test/automation` `Workbench`) keeps scenarios readable.
Only blocked by ADR-0007: the app is egui today; scenarios must target the model layer
(`crates/vsr-app/src/model.rs`) until gpui lands, then gain view-level steps.

### (c) Visual regression
Preferred: `HeadlessAppContext` with `current_headless_renderer` (gpui-kit `rendering.rs` pattern),
`capture_screenshot(window)` -> `RgbaImage` of the app window only (offscreen texture, so never the
full screen). Fixed window size (e.g. 1280x800 at scale 2.0), fixed theme/fonts/assets
(`with_asset_source` so icons render), frozen clock and caret.
Comparison (Zed's numbers): per-channel abs diff <= 2 counts as equal; pass if >= 99% of pixels
equal; size mismatch fails; on failure write `<name>_actual.png` and `<name>_diff.png` (red
differing, green matching) under `target/visual_tests/` (gitignored by `/target`).
Record mode: env `UPDATE_BASELINE=1` writes/overwrites baselines; with a missing baseline and no
env var the test fails and says to rerun with it. Use `image` (in local registry) for PNG I/O.

Policy (explicit):
- Baselines are APP-WINDOW-ONLY images, never full-screen captures.
- They live OUTSIDE git in `tests/visual/baselines/`, which is gitignored: add
  `/tests/visual/baselines/` to the repo-root `.gitignore` (currently only `/target`, `*.rs.bk`,
  `.DS_Store`, `__pycache__/`, `*.pyc`). Not done by this document.
- NO image is committed (baselines, diffs, actuals) until the user explicitly approves committing
  images. Until then, tests with a missing baseline must SKIP (not fail) unless
  `UPDATE_BASELINE=1` is set, so a fresh clone and CI stay green.
- First run on a machine: `UPDATE_BASELINE=1 cargo test -p vsr-app --test visual` records baselines
  locally; later runs compare. Baselines are per-machine (CoreText/GPU), so cross-machine
  comparison is not promised.
- Mark Metal-dependent tests so CI without a Metal device skips them.

## 6. Open questions for the user

1. May we add `insta` or `expect-test` (not in local registry; firewall) for text snapshots, or
   should we hand-roll snapshots with `similar`?
2. Approve the 2-per-channel / 99% match threshold, or prefer exact match since renders are
   deterministic on one machine?
3. When may images be committed (never, only for selected golden screens, or LFS)?
4. Should tier 2 start against the model layer now (egui era) or wait for the gpui migration?
5. Is a Metal-capable macOS CI runner available, so tier 3 runs in CI rather than locally only?
