//! Ports of upstream's minimap layout tests.
//!
//! upstream: src/vs/editor/test/browser/config/editorLayoutProvider.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/browser/view/minimapLayout.test.ts @ 50e1350fc78d
//!
//! From `editorLayoutProvider.test.ts` only the minimap cases are ported (the rest of the editor
//! layout, glyph margin and word wrap, is not rscode's yet); the layout helper below does the small
//! part of `computeLayout` those cases need (decorations width 10, no glyph margin or line numbers).
//! `minimapLayout.test.ts` scrolls through documents with view zones and custom line heights, which
//! rscode's editor does not have: only its "no zones" document is ported. Not ported:
//! `minimapCharRenderer.test.ts` (needs a canvas sampled font) and `minimapDirtyRows.test.ts`
//! (rscode repaints the whole minimap image).

use super::layout::{
    MinimapLayout, MinimapLayoutInfo, MinimapLayoutInput, RenderMinimap, SliderOptions, Viewport,
    compute_minimap_layout,
};
use crate::config::minimap::{MinimapSettings, MinimapSide, MinimapSize};

struct Case {
    side: MinimapSide,
    size: MinimapSize,
    scale: u32,
    pixel_ratio: f64,
    view_line_count: f64,
}

/// What `EditorLayoutInfoComputer.computeLayout` returns for the minimap cases.
struct Computed {
    minimap: MinimapLayoutInfo,
    content_left: f64,
    content_width: f64,
    viewport_column: f64,
}

fn compute(c: &Case) -> Computed {
    let settings = MinimapSettings {
        side: c.side,
        size: c.size,
        scale: c.scale,
        render_characters: true,
        max_column: 150,
        ..MinimapSettings::default()
    };
    let (outer_width, decorations_width) = (1000.0, 10.0);
    let remaining_width = outer_width - decorations_width;
    let minimap = compute_minimap_layout(&MinimapLayoutInput {
        outer_width,
        outer_height: 800.0,
        line_height: 16.0,
        typical_halfwidth_char_width: 10.0,
        pixel_ratio: c.pixel_ratio,
        scroll_beyond_last_line: false,
        padding_top: 0.0,
        padding_bottom: 0.0,
        minimap: &settings,
        vertical_scrollbar_width: 0.0,
        view_line_count: c.view_line_count,
        remaining_width,
    });
    let content_left = decorations_width + if minimap.left == 0.0 { minimap.width } else { 0.0 };
    let content_width = remaining_width - minimap.width;
    let viewport_column = 1.0_f64.max(((content_width - 0.0 - 2.0) / 10.0).floor());
    Computed { minimap, content_left, content_width, viewport_column }
}

fn case(side: MinimapSide, size: MinimapSize, pixel_ratio: f64, view_line_count: f64) -> Case {
    Case { side, size, scale: 1, pixel_ratio, view_line_count }
}

/// (left, width, heightIsEditorHeight, isSampling, scale, lineHeight, innerW, innerH, outerW)
type Expected = (f64, f64, bool, bool, f64, f64, f64, f64, f64);

fn check(c: Case, e: Expected, content: (f64, f64, f64)) {
    let got = compute(&c);
    let m = got.minimap;
    assert_eq!(m.render, RenderMinimap::Text);
    assert_eq!(
        (m.left, m.width, m.height_is_editor_height, m.is_sampling, m.scale, m.line_height, m.canvas_inner_width, m.canvas_inner_height, m.canvas_outer_width),
        e
    );
    assert_eq!(m.canvas_outer_height, 800.0);
    assert_eq!((got.content_left, got.content_width, got.viewport_column), content);
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 9 - render minimap
#[test]
fn render_minimap() {
    check(case(MinimapSide::Right, MinimapSize::Proportional, 1.0, 9.0), (903.0, 97.0, false, false, 1.0, 2.0, 97.0, 800.0, 97.0), (10.0, 893.0, 89.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 9 - render minimap with pixelRatio = 2
#[test]
fn render_minimap_with_pixel_ratio_2() {
    check(case(MinimapSide::Right, MinimapSize::Proportional, 2.0, 9.0), (903.0, 97.0, false, false, 2.0, 4.0, 194.0, 1600.0, 97.0), (10.0, 893.0, 89.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 9 - render minimap with pixelRatio = 4
#[test]
fn render_minimap_with_pixel_ratio_4() {
    check(case(MinimapSide::Right, MinimapSize::Proportional, 4.0, 9.0), (945.0, 55.0, false, false, 2.0, 4.0, 220.0, 3200.0, 55.0), (10.0, 935.0, 93.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 10 - render minimap to left
#[test]
fn render_minimap_to_left() {
    check(case(MinimapSide::Left, MinimapSize::Proportional, 4.0, 9.0), (0.0, 55.0, false, false, 2.0, 4.0, 220.0, 3200.0, 55.0), (65.0, 935.0, 93.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 11 - minimap mode cover without sampling
#[test]
fn minimap_mode_cover_without_sampling() {
    check(case(MinimapSide::Right, MinimapSize::Fill, 2.0, 120.0), (903.0, 97.0, true, false, 3.0, 13.0, 291.0, 1560.0, 97.0), (10.0, 893.0, 89.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 12 - minimap mode cover with sampling
#[test]
fn minimap_mode_cover_with_sampling() {
    check(case(MinimapSide::Right, MinimapSize::Fill, 2.0, 2500.0), (945.0, 55.0, true, true, 1.0, 1.0, 110.0, 1600.0, 55.0), (10.0, 935.0, 93.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 13 - minimap mode contain without sampling
#[test]
fn minimap_mode_contain_without_sampling() {
    check(case(MinimapSide::Right, MinimapSize::Fit, 2.0, 120.0), (903.0, 97.0, false, false, 2.0, 4.0, 194.0, 1600.0, 97.0), (10.0, 893.0, 89.0));
}

// ports the dynamically titled case (a `for` over `minimapCases`, so no `upstream:` ref): Editor ViewLayout - EditorLayoutProvider > EditorLayoutProvider 14 - minimap mode contain with sampling
#[test]
fn minimap_mode_contain_with_sampling() {
    check(case(MinimapSide::Right, MinimapSize::Fit, 2.0, 2500.0), (945.0, 55.0, true, true, 1.0, 1.0, 110.0, 1600.0, 55.0), (10.0, 935.0, 93.0));
}

// upstream: Editor ViewLayout - EditorLayoutProvider > issue #236999: the minimap scale does not jump when the pixel ratio crosses 2
#[test]
fn minimap_scale_does_not_jump_when_the_pixel_ratio_crosses_2() {
    let actual: Vec<Vec<f64>> = [1u32, 2, 3]
        .into_iter()
        .map(|scale| {
            [1.0, 1.5, 1.99, 2.0, 2.5]
                .into_iter()
                .map(|pixel_ratio| {
                    compute(&Case { side: MinimapSide::Right, size: MinimapSize::Proportional, scale, pixel_ratio, view_line_count: 9.0 })
                        .minimap
                        .scale
                })
                .collect()
        })
        .collect();
    assert_eq!(
        actual,
        vec![vec![1.0, 1.0, 1.0, 2.0, 2.0], vec![2.0, 3.0, 3.0, 4.0, 4.0], vec![3.0, 4.0, 5.0, 6.0, 6.0]]
    );
}

// upstream: MinimapLayout > slider moves only forward when scrolling down through view zones, padding, taller and zero-height lines
// (only the document without view zones, custom line heights or padding: rscode's editor has none)
#[test]
fn slider_moves_only_forward_when_scrolling_down() {
    let (line_count, line_height, viewport_height, minimap_height) = (1000.0_f64, 20.0, 400.0, 400.0);
    let minimap_line_height = 2.0;
    let options = SliderOptions {
        pixel_ratio: 1.0,
        minimap_line_height,
        canvas_inner_height: minimap_height,
        line_height,
        minimap_height_is_editor_height: false,
        minimap_height,
        padding_top: 0.0,
        padding_bottom: 0.0,
        scroll_beyond_last_line: false,
    };
    let scroll_height = line_count * line_height;
    let max_scroll_top = scroll_height - viewport_height;
    let (mut previous, mut previous_slider_line): (Option<MinimapLayout>, f64) = (None, 0.0);
    let (mut backward_moves, mut min_slider_top, mut max_slider_bottom) = (0, f64::MAX, 0.0_f64);
    let mut scroll_top = 0.0;
    while scroll_top <= max_scroll_top {
        let start_line = (scroll_top / line_height).floor() + 1.0;
        let end_line = ((scroll_top + viewport_height) / line_height).ceil().min(line_count);
        let layout = MinimapLayout::create(
            &options,
            &Viewport {
                start_line,
                end_line,
                start_line_vertical_offset: (start_line - 1.0) * line_height,
                start_line_height: line_height,
                height: viewport_height,
                contains_whitespace_gaps: false,
                line_count,
                real_line_count: line_count,
                scroll_top,
                scroll_height,
            },
            previous.as_ref(),
        );
        // The minimap canvas may re-anchor beneath the slider: track the slider's document line.
        let slider_line = layout.start_line - layout.top_padding_line_count + layout.slider_top / minimap_line_height;
        if previous.is_some() && slider_line < previous_slider_line - 1e-9 {
            backward_moves += 1;
        }
        min_slider_top = min_slider_top.min(layout.slider_top);
        max_slider_bottom = max_slider_bottom.max((layout.slider_top + layout.slider_height).round());
        previous = Some(layout);
        previous_slider_line = slider_line;
        scroll_top += 10.0;
    }
    // noZones: { backwardMoves: 0, minSliderTop: 0, maxSliderBottom: 401 }
    assert_eq!((backward_moves, min_slider_top, max_slider_bottom), (0, 0.0, 401.0));
}
