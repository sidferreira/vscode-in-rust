//! Minimap layout math, UI free: how wide the minimap is, how tall its lines are, which document
//! lines it shows, and where the slider sits for a given scroll position.
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/minimap/minimap.ts @ 50e1350fc78d
//!
//! Ports `EditorLayoutInfoComputer._computeMinimapLayout` (without the word-wrap memory, rscode has
//! no word wrap yet), `computeContainedMinimapLineCount`, `MinimapLayout` and the sampling lines of
//! `MinimapSamplingState.compute`. All sizes are CSS pixels unless a name says "canvas inner"
//! (device pixels). Numbers are `f64` like upstream's.

use crate::config::minimap::{MinimapSettings, MinimapSide, MinimapSize};

/// The width of the minimap gutter, in pixels (`MINIMAP_GUTTER_WIDTH`).
pub const MINIMAP_GUTTER_WIDTH: f64 = 8.0;

/// `RenderMinimap`: nothing, characters, or colour blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMinimap {
    None,
    Text,
    Blocks,
}

/// `EditorMinimapLayoutInfo`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapLayoutInfo {
    pub render: RenderMinimap,
    pub left: f64,
    pub width: f64,
    pub height_is_editor_height: bool,
    pub is_sampling: bool,
    pub scale: f64,
    pub line_height: f64,
    pub canvas_inner_width: f64,
    pub canvas_inner_height: f64,
    pub canvas_outer_width: f64,
    pub canvas_outer_height: f64,
}

/// What `_computeMinimapLayout` reads from the editor.
#[derive(Debug, Clone, Copy)]
pub struct MinimapLayoutInput<'a> {
    pub outer_width: f64,
    pub outer_height: f64,
    pub line_height: f64,
    pub typical_halfwidth_char_width: f64,
    pub pixel_ratio: f64,
    pub scroll_beyond_last_line: bool,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub minimap: &'a MinimapSettings,
    pub vertical_scrollbar_width: f64,
    pub view_line_count: f64,
    /// Width left for line decorations content and minimap: outer width minus glyph margin, line
    /// numbers and line decorations.
    pub remaining_width: f64,
}

/// `computeContainedMinimapLineCount`'s result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContainedLineCount {
    pub typical_viewport_line_count: f64,
    pub extra_lines_before_first_line: f64,
    pub extra_lines_beyond_last_line: f64,
    pub desired_ratio: f64,
    pub minimap_line_count: f64,
}

pub fn compute_contained_minimap_line_count(
    view_line_count: f64,
    scroll_beyond_last_line: bool,
    padding_top: f64,
    padding_bottom: f64,
    height: f64,
    line_height: f64,
    pixel_ratio: f64,
) -> ContainedLineCount {
    let typical_viewport_line_count = height / line_height;
    let extra_lines_before_first_line = (padding_top / line_height).floor();
    let mut extra_lines_beyond_last_line = (padding_bottom / line_height).floor();
    if scroll_beyond_last_line {
        extra_lines_beyond_last_line = extra_lines_beyond_last_line.max(typical_viewport_line_count - 1.0);
    }
    let desired_ratio =
        (extra_lines_before_first_line + view_line_count + extra_lines_beyond_last_line) / (pixel_ratio * height);
    let minimap_line_count = (view_line_count / desired_ratio).floor();
    ContainedLineCount {
        typical_viewport_line_count,
        extra_lines_before_first_line,
        extra_lines_beyond_last_line,
        desired_ratio,
        minimap_line_count,
    }
}

/// The configured scale in device pixels: doubled on high DPI screens, growing with the pixel
/// ratio in between so a small zoom change does not resize the minimap abruptly.
fn compute_minimap_scale(configured: f64, pixel_ratio: f64) -> f64 {
    if pixel_ratio >= 2.0 {
        return (configured * 2.0).round();
    }
    configured.max((configured * pixel_ratio).floor())
}

pub fn compute_minimap_layout(input: &MinimapLayoutInput) -> MinimapLayoutInfo {
    let (outer_width, outer_height, pixel_ratio) = (input.outer_width, input.outer_height, input.pixel_ratio);
    let m = input.minimap;
    if !m.enabled {
        return MinimapLayoutInfo {
            render: RenderMinimap::None,
            left: 0.0,
            width: 0.0,
            height_is_editor_height: false,
            is_sampling: false,
            scale: 1.0,
            line_height: 1.0,
            canvas_inner_width: 0.0,
            canvas_inner_height: (pixel_ratio * outer_height).floor(),
            canvas_outer_width: 0.0,
            canvas_outer_height: outer_height,
        };
    }

    let line_height = input.line_height;
    let mut scale = compute_minimap_scale(f64::from(m.scale), pixel_ratio);
    let max_column = f64::from(m.max_column);
    let view_line_count = input.view_line_count;
    let base_char_height = if m.render_characters { 2.0 } else { 3.0 };
    let mut canvas_inner_height = (pixel_ratio * outer_height).floor();
    let canvas_outer_height = canvas_inner_height / pixel_ratio;
    let mut height_is_editor_height = false;
    let mut is_sampling = false;
    let mut minimap_line_height = base_char_height * scale;
    let mut char_width = scale / pixel_ratio;
    let mut width_multiplier = 1.0;

    if matches!(m.size, MinimapSize::Fill | MinimapSize::Fit) {
        let c = compute_contained_minimap_line_count(
            view_line_count,
            input.scroll_beyond_last_line,
            input.padding_top,
            input.padding_bottom,
            outer_height,
            line_height,
            pixel_ratio,
        );
        // The ratio is intentionally not part of the layout, so it does not change all the time
        // while sampling.
        let ratio = view_line_count / c.minimap_line_count;
        if ratio > 1.0 {
            height_is_editor_height = true;
            is_sampling = true;
            scale = 1.0;
            minimap_line_height = 1.0;
            char_width = scale / pixel_ratio;
        } else {
            let mut fit_becomes_fill = false;
            let max_minimap_scale = scale + 1.0;
            if m.size == MinimapSize::Fit {
                let effective = ((c.extra_lines_before_first_line + view_line_count + c.extra_lines_beyond_last_line)
                    * minimap_line_height)
                    .ceil();
                fit_becomes_fill = effective > canvas_inner_height;
            }
            if m.size == MinimapSize::Fill || fit_becomes_fill {
                height_is_editor_height = true;
                let configured_scale = scale;
                minimap_line_height = (line_height * pixel_ratio).min(1.0_f64.max((1.0 / c.desired_ratio).floor()));
                scale = max_minimap_scale.min(1.0_f64.max((minimap_line_height / base_char_height).floor()));
                if scale > configured_scale {
                    width_multiplier = 2.0_f64.min(scale / configured_scale);
                }
                char_width = scale / pixel_ratio / width_multiplier;
                canvas_inner_height = (c.typical_viewport_line_count.max(
                    c.extra_lines_before_first_line + view_line_count + c.extra_lines_beyond_last_line,
                ) * minimap_line_height)
                    .ceil();
            }
        }
    }

    // minimapWidth = ((remainingWidth - verticalScrollbarWidth - 2) * charWidth) / (typicalCharWidth + charWidth)
    // (2px are left for the cursor to have space after the last character).
    let max_width = (max_column * char_width).floor();
    let width = max_width.min(
        0.0_f64
            .max(
                (((input.remaining_width - input.vertical_scrollbar_width - 2.0) * char_width)
                    / (input.typical_halfwidth_char_width + char_width))
                    .floor(),
            )
            + MINIMAP_GUTTER_WIDTH,
    );

    let mut canvas_inner_width = (pixel_ratio * width).floor();
    let canvas_outer_width = canvas_inner_width / pixel_ratio;
    canvas_inner_width = (canvas_inner_width * width_multiplier).floor();

    MinimapLayoutInfo {
        render: if m.render_characters { RenderMinimap::Text } else { RenderMinimap::Blocks },
        left: if m.side == MinimapSide::Left { 0.0 } else { outer_width - width - input.vertical_scrollbar_width },
        width,
        height_is_editor_height,
        is_sampling,
        scale,
        line_height: minimap_line_height,
        canvas_inner_width,
        canvas_inner_height,
        canvas_outer_width,
        canvas_outer_height,
    }
}

/// The options `MinimapLayout::create` reads (a subset of upstream's `MinimapOptions`).
#[derive(Debug, Clone, Copy)]
pub struct SliderOptions {
    pub pixel_ratio: f64,
    pub minimap_line_height: f64,
    pub canvas_inner_height: f64,
    pub line_height: f64,
    pub minimap_height_is_editor_height: bool,
    pub minimap_height: f64,
    pub padding_top: f64,
    pub padding_bottom: f64,
    pub scroll_beyond_last_line: bool,
}

/// What the editor is showing right now.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub start_line: f64,
    pub end_line: f64,
    pub start_line_vertical_offset: f64,
    pub start_line_height: f64,
    pub height: f64,
    pub contains_whitespace_gaps: bool,
    pub line_count: f64,
    pub real_line_count: f64,
    pub scroll_top: f64,
    pub scroll_height: f64,
}

/// `MinimapLayout`: the slider and the range of lines drawn.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinimapLayout {
    /// The given editor scrollTop (input).
    pub scroll_top: f64,
    /// The given editor scrollHeight (input).
    pub scroll_height: f64,
    pub slider_needed: bool,
    computed_slider_ratio: f64,
    /// Slider top (CSS px).
    pub slider_top: f64,
    /// Slider height (CSS px).
    pub slider_height: f64,
    /// Empty lines reserved at the top of the minimap.
    pub top_padding_line_count: f64,
    /// First minimap line drawn.
    pub start_line: f64,
    /// Last minimap line drawn.
    pub end_line: f64,
}

impl MinimapLayout {
    /// The scrollTop that moves the slider by `delta` (CSS px).
    pub fn desired_scroll_top_from_delta(&self, delta: f64) -> f64 {
        (self.scroll_top + delta / self.computed_slider_ratio).round()
    }

    /// The slider ratio (minimap px per document px), 0 when the document fits.
    #[cfg(test)]
    pub fn slider_ratio(&self) -> f64 {
        self.computed_slider_ratio
    }

    /// The document line the minimap line at inner `y` (CSS px) shows (`_pointerDown`, proportional).
    pub fn line_at_y(&self, y: f64, canvas_inner_height: f64, canvas_outer_height: f64, minimap_line_height: f64, line_count: f64) -> f64 {
        let internal_offset_y = (canvas_inner_height / canvas_outer_height) * y;
        let line_index = (internal_offset_y / minimap_line_height).floor();
        (line_index + self.start_line - self.top_padding_line_count).min(line_count)
    }

    pub fn create(options: &SliderOptions, vp: &Viewport, previous: Option<&MinimapLayout>) -> MinimapLayout {
        let pixel_ratio = options.pixel_ratio;
        let minimap_line_height = options.minimap_line_height;
        let minimap_lines_fitting = (options.canvas_inner_height / minimap_line_height).floor();
        let line_height = options.line_height;
        // `scrollHeight - viewportHeight` is 0 when everything fits: no slider is needed then, and
        // the ratio only has to stay finite.
        let ratio_of = |max_slider_top: f64| {
            let range = vp.scroll_height - vp.height;
            if range > 0.0 { max_slider_top / range } else { 0.0 }
        };

        if options.minimap_height_is_editor_height {
            let mut logical_scroll_height = vp.real_line_count * options.line_height + options.padding_top + options.padding_bottom;
            if options.scroll_beyond_last_line {
                logical_scroll_height += (vp.height - options.line_height - options.padding_bottom).max(0.0);
            }
            let slider_height = 1.0_f64.max((vp.height * vp.height / logical_scroll_height).floor());
            let max_minimap_slider_top = (options.minimap_height - slider_height).max(0.0);
            let computed_slider_ratio = ratio_of(max_minimap_slider_top);
            let slider_top = vp.scroll_top * computed_slider_ratio;
            let slider_needed = max_minimap_slider_top > 0.0;
            let max_lines_fitting = (options.canvas_inner_height / options.minimap_line_height).floor();
            let top_padding_line_count = (options.padding_top / options.line_height).floor();
            return MinimapLayout {
                scroll_top: vp.scroll_top,
                scroll_height: vp.scroll_height,
                slider_needed,
                computed_slider_ratio,
                slider_top,
                slider_height,
                top_padding_line_count,
                start_line: 1.0,
                end_line: vp.line_count.min(max_lines_fitting),
            };
        }

        // The slider height: stable, or following the visible line count when whitespace gaps are in view.
        let slider_height = if vp.contains_whitespace_gaps && vp.end_line != vp.line_count {
            let viewport_line_count = vp.end_line - vp.start_line + 1.0;
            (viewport_line_count * minimap_line_height / pixel_ratio).floor()
        } else {
            let expected_viewport_line_count = vp.height / line_height;
            (expected_viewport_line_count * minimap_line_height / pixel_ratio).floor()
        };

        let extra_lines_at_the_top = (options.padding_top / line_height).floor();
        let mut extra_lines_at_the_bottom = (options.padding_bottom / line_height).floor();
        if options.scroll_beyond_last_line {
            let expected_viewport_line_count = vp.height / line_height;
            extra_lines_at_the_bottom = extra_lines_at_the_bottom.max(expected_viewport_line_count - 1.0);
        }

        let mut max_minimap_slider_top = if extra_lines_at_the_bottom > 0.0 {
            // Dragged all the way down, the slider contains the last line at its top.
            let expected_viewport_line_count = vp.height / line_height;
            (extra_lines_at_the_top + vp.line_count + extra_lines_at_the_bottom - expected_viewport_line_count - 1.0)
                * minimap_line_height
                / pixel_ratio
        } else {
            // Dragged all the way down, the slider contains the last line at its bottom.
            0.0_f64.max((extra_lines_at_the_top + vp.line_count) * minimap_line_height / pixel_ratio - slider_height)
        };
        max_minimap_slider_top = max_minimap_slider_top.min(options.minimap_height - slider_height);

        // The slider moves from 0 to `max_minimap_slider_top` like scrollTop from 0 to scrollHeight - viewportHeight.
        let computed_slider_ratio = ratio_of(max_minimap_slider_top);
        let slider_top = vp.scroll_top * computed_slider_ratio;

        if minimap_lines_fitting >= extra_lines_at_the_top + vp.line_count + extra_lines_at_the_bottom {
            // All lines fit in the minimap.
            let slider_needed = max_minimap_slider_top > 0.0;
            return MinimapLayout {
                scroll_top: vp.scroll_top,
                scroll_height: vp.scroll_height,
                slider_needed,
                computed_slider_ratio,
                slider_top,
                slider_height,
                top_padding_line_count: extra_lines_at_the_top,
                start_line: 1.0,
                end_line: vp.line_count,
            };
        }

        let considering_start_line = if vp.start_line > 1.0 {
            vp.start_line + extra_lines_at_the_top
        } else {
            1.0_f64.max(vp.scroll_top / line_height)
        };

        let mut top_padding_line_count;
        let mut start_line = 1.0_f64.max((considering_start_line - slider_top * pixel_ratio / minimap_line_height).floor());
        if start_line < extra_lines_at_the_top {
            top_padding_line_count = extra_lines_at_the_top - start_line + 1.0;
            start_line = 1.0;
        } else {
            top_padding_line_count = 0.0;
            start_line = 1.0_f64.max(start_line - extra_lines_at_the_top);
        }

        // Avoid flickering caused by a partial viewport start line by being consistent with the
        // previous layout decision.
        if let Some(prev) = previous.filter(|p| p.scroll_height == vp.scroll_height) {
            if prev.scroll_top > vp.scroll_top {
                // Scrolling up: never increase the start line.
                start_line = start_line.min(prev.start_line);
                top_padding_line_count = top_padding_line_count.max(prev.top_padding_line_count);
            }
            if prev.scroll_top < vp.scroll_top {
                // Scrolling down: never decrease the start line.
                start_line = start_line.max(prev.start_line);
                top_padding_line_count = top_padding_line_count.min(prev.top_padding_line_count);
            }
        }

        let end_line = vp.line_count.min(start_line - top_padding_line_count + minimap_lines_fitting - 1.0);
        // When the viewport starts inside a view zone, the top padding or a taller line, the first
        // visible line does not start at scrollTop; clamp so the slider never moves backwards. A
        // custom line height can round to 0px, and such a line has no partial progress.
        let partial_line = if vp.start_line_height > 0.0 {
            (vp.scroll_top - vp.start_line_vertical_offset).max(0.0).min(vp.start_line_height) / vp.start_line_height
        } else {
            0.0
        };

        let slider_top_aligned = if vp.scroll_top >= options.padding_top {
            (vp.start_line - start_line + top_padding_line_count + partial_line) * minimap_line_height / pixel_ratio
        } else {
            (vp.scroll_top / options.padding_top) * (top_padding_line_count + partial_line) * minimap_line_height
                / pixel_ratio
        };

        MinimapLayout {
            scroll_top: vp.scroll_top,
            scroll_height: vp.scroll_height,
            slider_needed: true,
            computed_slider_ratio,
            slider_top: slider_top_aligned,
            slider_height,
            top_padding_line_count,
            start_line,
            end_line,
        }
    }
}

/// The document line of each minimap line when the minimap samples (`MinimapSamplingState.compute`,
/// first computation): fewer minimap lines than document lines, so lines are picked at a ratio.
pub fn sampling_lines(minimap_line_count: usize, view_line_count: usize) -> Vec<usize> {
    if minimap_line_count == 0 {
        return Vec::new();
    }
    let ratio = view_line_count as f64 / minimap_line_count as f64;
    let half_ratio = ratio / 2.0;
    let mut result = vec![1usize; minimap_line_count];
    if minimap_line_count > 1 {
        for (i, slot) in result.iter_mut().enumerate().take(minimap_line_count - 1) {
            *slot = (i as f64 * ratio + half_ratio).round() as usize;
        }
        result[minimap_line_count - 1] = view_line_count;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(m: &MinimapSettings) -> MinimapLayoutInput<'_> {
        MinimapLayoutInput {
            outer_width: 1000.0,
            outer_height: 800.0,
            line_height: 16.0,
            typical_halfwidth_char_width: 10.0,
            pixel_ratio: 1.0,
            scroll_beyond_last_line: false,
            padding_top: 0.0,
            padding_bottom: 0.0,
            minimap: m,
            vertical_scrollbar_width: 0.0,
            view_line_count: 999.0,
            remaining_width: 990.0,
        }
    }

    #[test]
    fn sampling_picks_evenly_spaced_lines() {
        assert_eq!(sampling_lines(4, 8), vec![1, 3, 5, 8]);
        assert_eq!(sampling_lines(1, 10), vec![1]);
        assert!(sampling_lines(0, 10).is_empty());
    }

    #[test]
    fn disabled_minimap_has_no_width() {
        let m = MinimapSettings { enabled: false, ..MinimapSettings::default() };
        let l = compute_minimap_layout(&input(&m));
        assert_eq!((l.render, l.width, l.canvas_inner_width), (RenderMinimap::None, 0.0, 0.0));
    }

    #[test]
    fn short_documents_need_no_slider() {
        let options = SliderOptions {
            pixel_ratio: 1.0,
            minimap_line_height: 2.0,
            canvas_inner_height: 400.0,
            line_height: 20.0,
            minimap_height_is_editor_height: false,
            minimap_height: 400.0,
            padding_top: 0.0,
            padding_bottom: 0.0,
            scroll_beyond_last_line: false,
        };
        let vp = Viewport {
            start_line: 1.0,
            end_line: 3.0,
            start_line_vertical_offset: 0.0,
            start_line_height: 20.0,
            height: 400.0,
            contains_whitespace_gaps: false,
            line_count: 3.0,
            real_line_count: 3.0,
            scroll_top: 0.0,
            scroll_height: 400.0,
        };
        let l = MinimapLayout::create(&options, &vp, None);
        assert!(!l.slider_needed);
        assert_eq!((l.start_line, l.end_line, l.slider_top), (1.0, 3.0, 0.0));
        assert!(l.slider_ratio().is_finite());
    }
}
