//! Draws minimap lines into a pixel buffer, like upstream's `InnerMinimap._renderLine` with the
//! `MinimapCharRenderer`: each character becomes a tiny `scale` x `2 * scale` pixel glyph (or a
//! flat block), blended onto the background in the foreground colour.
//!
//! upstream: src/vs/editor/browser/viewParts/minimap/minimapCharRenderer.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/minimap/minimapCharSheet.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/minimap/minimapCharRendererFactory.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/minimap/minimap.ts @ 50e1350fc78d
//!
//! Scales 1 and 2 use upstream's pre-baked sheets. Upstream samples the editor font from a canvas
//! for scale 3; rscode has no canvas, so scale 3 resamples the scale 2 sheet (nearest neighbour).

use super::layout::{MINIMAP_GUTTER_WIDTH, RenderMinimap};
use super::prebaked;

const START_CH_CODE: i64 = 32;
const CHAR_COUNT: usize = 96; // END_CH_CODE (126) - START_CH_CODE + 2
const BASE_CHAR_WIDTH: usize = 1;
const BASE_CHAR_HEIGHT: usize = 2;

fn decode(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|p| {
            let d = |b: u8| (b as char).to_digit(16).unwrap_or(0) as u8;
            (d(p[0]) << 4) | (d(p[1]) & 0xF)
        })
        .collect()
}

/// A BGRA pixel buffer (the byte order gpui's `RenderImage` wants), always opaque.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: usize,
    pub height: usize,
    pub data: Vec<u8>,
}

impl Canvas {
    pub fn new(width: usize, height: usize, bg: [u8; 3]) -> Self {
        let mut data = Vec::with_capacity(width * height * 4);
        for _ in 0..width * height {
            data.extend_from_slice(&[bg[2], bg[1], bg[0], 255]);
        }
        Self { width, height, data }
    }

    /// The pixel at (x, y) as `[r, g, b]`.
    pub fn rgb(&self, x: usize, y: usize) -> [u8; 3] {
        let i = (y * self.width + x) * 4;
        [self.data[i + 2], self.data[i + 1], self.data[i]]
    }
}

/// `MinimapCharRenderer`: coverage data for one scale.
pub struct CharRenderer {
    pub scale: usize,
    normal: Vec<u8>,
    light: Vec<u8>,
}

fn soften(input: &[u8], ratio: f64) -> Vec<u8> {
    input.iter().map(|&b| (f64::from(b) * ratio).clamp(0.0, 255.0) as u8).collect()
}

impl CharRenderer {
    pub fn new(scale: usize) -> Self {
        let scale = scale.max(1);
        let data = match scale {
            1 => decode(prebaked::SCALE_1),
            2 => decode(prebaked::SCALE_2),
            _ => {
                // Nearest-neighbour upsample of the 2x sheet to `scale`.
                let src = decode(prebaked::SCALE_2);
                let (sw, sh) = (2 * BASE_CHAR_WIDTH, 2 * BASE_CHAR_HEIGHT);
                let (dw, dh) = (scale * BASE_CHAR_WIDTH, scale * BASE_CHAR_HEIGHT);
                let mut out = Vec::with_capacity(CHAR_COUNT * dw * dh);
                for c in 0..CHAR_COUNT {
                    for y in 0..dh {
                        for x in 0..dw {
                            out.push(src[c * sw * sh + (y * sh / dh) * sw + x * sw / dw]);
                        }
                    }
                }
                out
            }
        };
        Self { scale, normal: soften(&data, 12.0 / 15.0), light: soften(&data, 50.0 / 60.0) }
    }

    /// `getCharIndex`.
    fn char_index(ch: u32, font_scale: usize) -> usize {
        let c = i64::from(ch) - START_CH_CODE;
        let n = CHAR_COUNT as i64;
        if c < 0 || c > n {
            if font_scale <= 2 {
                // For smaller scales any ASCII character will do.
                return ((c + n) % n) as usize;
            }
            return CHAR_COUNT - 1; // unknown symbol
        }
        (c as usize).min(CHAR_COUNT - 1)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn render_char(
        &self,
        target: &mut Canvas,
        dx: usize,
        dy: usize,
        ch: u32,
        color: [u8; 3],
        fg_alpha: u8,
        bg: [u8; 3],
        font_scale: usize,
        use_lighter_font: bool,
        force_1px_height: bool,
    ) {
        let (cw, ch_h) = (BASE_CHAR_WIDTH * self.scale, BASE_CHAR_HEIGHT * self.scale);
        let render_height = if force_1px_height { 1 } else { ch_h };
        if dx + cw > target.width || dy + render_height > target.height {
            return;
        }
        let data = if use_lighter_font { &self.light } else { &self.normal };
        let mut src = Self::char_index(ch, font_scale) * cw * ch_h;
        for y in 0..render_height {
            let mut i = ((dy + y) * target.width + dx) * 4;
            for _ in 0..cw {
                let c = (f64::from(data[src]) / 255.0) * (f64::from(fg_alpha) / 255.0);
                src += 1;
                // BGRA: channel 2 (red) is stored last.
                for channel in [2usize, 1, 0] {
                    let b = f64::from(bg[channel]);
                    target.data[i] = (b + (f64::from(color[channel]) - b) * c) as u8;
                    i += 1;
                }
                target.data[i] = 255;
                i += 1;
            }
        }
    }

    pub fn block_render_char(
        &self,
        target: &mut Canvas,
        dx: usize,
        dy: usize,
        color: [u8; 3],
        fg_alpha: u8,
        bg: [u8; 3],
        force_1px_height: bool,
    ) {
        let (cw, ch_h) = (BASE_CHAR_WIDTH * self.scale, BASE_CHAR_HEIGHT * self.scale);
        let render_height = if force_1px_height { 1 } else { ch_h };
        if dx + cw > target.width || dy + render_height > target.height {
            return;
        }
        let c = 0.5 * (f64::from(fg_alpha) / 255.0);
        let mix = |i: usize| (f64::from(bg[i]) + (f64::from(color[i]) - f64::from(bg[i])) * c) as u8;
        let px = [mix(2), mix(1), mix(0), 255];
        for y in 0..render_height {
            let mut i = ((dy + y) * target.width + dx) * 4;
            for _ in 0..cw {
                target.data[i..i + 4].copy_from_slice(&px);
                i += 4;
            }
        }
    }
}

/// `strings.isFullWidthCharacter` on a UTF-16 code unit.
fn is_full_width(unit: u16) -> bool {
    (0x2E80..=0xD7AF).contains(&unit) || (0xF900..=0xFAFF).contains(&unit) || (0xFF01..=0xFF5E).contains(&unit)
}

/// Everything `render_lines` needs besides the text.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RasterParams {
    pub render: RenderMinimap,
    pub font_scale: usize,
    /// Minimap line height in device pixels.
    pub line_height: usize,
    /// Width of one column in device pixels (`BASE_CHAR_WIDTH * font_scale`).
    pub char_width: usize,
    pub canvas_width: usize,
    pub canvas_height: usize,
    pub tab_size: usize,
    pub fg: [u8; 3],
    pub fg_alpha: u8,
    pub bg: [u8; 3],
    /// The background is light, so the thinner font weight is used.
    pub lighter_font: bool,
    /// Empty lines at the top of the minimap.
    pub top_padding_lines: usize,
}

/// Draws `lines` (the first one at minimap line `top_padding_lines`) into a fresh canvas.
pub fn render_lines<'a>(p: &RasterParams, lines: impl Iterator<Item = &'a str>) -> Canvas {
    let mut canvas = Canvas::new(p.canvas_width, p.canvas_height, p.bg);
    if p.canvas_width == 0 || p.canvas_height == 0 {
        return canvas;
    }
    let renderer = CharRenderer::new(p.font_scale);
    let base_char_height = if p.render == RenderMinimap::Text { 2 } else { 3 };
    let render_line_height = base_char_height * p.font_scale;
    let inner_padding = if p.line_height > render_line_height { (p.line_height - render_line_height) / 2 } else { 0 };
    let force_1px = p.line_height == 1;
    let max_dx = p.canvas_width as isize - p.char_width as isize;
    let mut dy = p.top_padding_lines * p.line_height;
    for line in lines {
        if dy >= p.canvas_height {
            break;
        }
        render_line(&mut canvas, &renderer, p, line, dy + inner_padding, max_dx, force_1px);
        dy += p.line_height;
    }
    canvas
}

fn render_line(
    canvas: &mut Canvas,
    renderer: &CharRenderer,
    p: &RasterParams,
    content: &str,
    dy: usize,
    max_dx: isize,
    force_1px: bool,
) {
    let mut dx = MINIMAP_GUTTER_WIDTH as usize;
    let mut tabs_char_delta = 0usize;
    for (char_index, unit) in content.encode_utf16().enumerate() {
        if dx as isize > max_dx {
            return; // hit the edge of the minimap
        }
        match unit {
            9 => {
                let spaces = p.tab_size - (char_index + tabs_char_delta) % p.tab_size;
                tabs_char_delta += spaces - 1;
                dx += spaces * p.char_width; // a tab is invisible
            }
            32 => dx += p.char_width,
            _ => {
                // Full width characters are drawn twice.
                let count = if is_full_width(unit) { 2 } else { 1 };
                for _ in 0..count {
                    if p.render == RenderMinimap::Blocks {
                        renderer.block_render_char(canvas, dx, dy, p.fg, p.fg_alpha, p.bg, force_1px);
                    } else {
                        renderer.render_char(
                            canvas,
                            dx,
                            dy,
                            u32::from(unit),
                            p.fg,
                            p.fg_alpha,
                            p.bg,
                            p.font_scale,
                            p.lighter_font,
                            force_1px,
                        );
                    }
                    dx += p.char_width;
                    if dx as isize > max_dx {
                        return;
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prebaked_sheets_have_the_expected_size() {
        assert_eq!(decode(prebaked::SCALE_1).len(), CHAR_COUNT * 2);
        assert_eq!(decode(prebaked::SCALE_2).len(), CHAR_COUNT * 2 * 4);
        assert_eq!(CharRenderer::new(3).normal.len(), CHAR_COUNT * 3 * 6);
    }

    fn params(render: RenderMinimap, scale: usize) -> RasterParams {
        RasterParams {
            render,
            font_scale: scale,
            line_height: 2 * scale,
            char_width: scale,
            canvas_width: 40,
            canvas_height: 20,
            tab_size: 4,
            fg: [255, 255, 255],
            fg_alpha: 255,
            bg: [0, 0, 0],
            lighter_font: false,
            top_padding_lines: 0,
        }
    }

    #[test]
    fn characters_are_drawn_from_the_gutter_and_spaces_are_blank() {
        let c = render_lines(&params(RenderMinimap::Text, 1), ["a b"].into_iter());
        let lit = |x: usize| (0..2).any(|y| c.rgb(x, y) != [0, 0, 0]);
        assert!(!lit(7), "nothing in the gutter");
        assert!(lit(8), "'a' starts at the gutter edge");
        assert!(!lit(9), "space");
        assert!(lit(10), "'b'");
        assert!(!lit(11));
    }

    #[test]
    fn blocks_are_flat_at_half_intensity() {
        let c = render_lines(&params(RenderMinimap::Blocks, 1), ["x"].into_iter());
        assert_eq!(c.rgb(8, 0), [127, 127, 127]);
        assert_eq!(c.rgb(8, 1), [127, 127, 127]);
    }

    #[test]
    fn tabs_advance_to_the_next_tab_stop_and_long_lines_stop_at_the_edge() {
        let c = render_lines(&params(RenderMinimap::Blocks, 1), ["\tx"].into_iter());
        assert_eq!(c.rgb(8 + 4, 0), [127, 127, 127]);
        assert_eq!(c.rgb(8 + 3, 0), [0, 0, 0]);
        let long = "x".repeat(200);
        let c = render_lines(&params(RenderMinimap::Blocks, 1), [long.as_str()].into_iter());
        assert_eq!(c.rgb(38, 0), [127, 127, 127]);
        assert_eq!(c.rgb(39, 0), [127, 127, 127], "the last column that fits");
    }

    #[test]
    fn top_padding_and_force_1px_lines() {
        let mut p = params(RenderMinimap::Blocks, 1);
        p.top_padding_lines = 2;
        let c = render_lines(&p, ["x"].into_iter());
        assert_eq!(c.rgb(8, 0), [0, 0, 0]);
        assert_eq!(c.rgb(8, 4), [127, 127, 127]);
        p.top_padding_lines = 0;
        p.line_height = 1;
        let c = render_lines(&p, ["x", "x"].into_iter());
        assert_eq!((c.rgb(8, 0), c.rgb(8, 1), c.rgb(8, 2)), ([127; 3], [127; 3], [0; 3]));
    }
}
