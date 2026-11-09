//! A port of the colour class VS Code themes are computed with (`Color`, `RGBA`, `HSLA`).
//!
//! upstream: src/vs/base/common/color.ts @ 50e1350fc78d
//!
//! The arithmetic is kept exactly as upstream (channels truncated, alpha and HSL parts rounded to
//! three decimals, `lighten` / `darken` going through HSL) so that a colour derived by the
//! registry's `transparent()` / `lighten()` / ... comes out as the same `#RRGGBBAA` as in VS Code.
//! Not ported: HSVA (colour picker), `ensureContrast`, `reduceRelativeLuminace`, named CSS keywords.

/// `RGBA`: channels 0..=255, alpha 0..=1 rounded to three decimals.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

fn round_float(n: f64, decimals: i32) -> f64 {
    let d = 10f64.powi(decimals);
    (n * d).round() / d
}

/// `Math.min(255, Math.max(0, n)) | 0`.
fn channel(n: f64) -> u8 {
    n.clamp(0.0, 255.0) as u8
}

impl Rgba {
    pub fn new(r: f64, g: f64, b: f64, a: f64) -> Self {
        Self { r: channel(r), g: channel(g), b: channel(b), a: round_float(a.clamp(0.0, 1.0), 3) }
    }

    /// Opaque.
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }
}

/// `HSLA`: hue 0..=360 (whole degrees), saturation, lightness and alpha 0..=1 (three decimals).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsla {
    pub h: f64,
    pub s: f64,
    pub l: f64,
    pub a: f64,
}

impl Hsla {
    pub fn new(h: f64, s: f64, l: f64, a: f64) -> Self {
        Self {
            h: h.clamp(0.0, 360.0).trunc(),
            s: round_float(s.clamp(0.0, 1.0), 3),
            l: round_float(l.clamp(0.0, 1.0), 3),
            a: round_float(a.clamp(0.0, 1.0), 3),
        }
    }

    pub fn from_rgba(rgba: Rgba) -> Self {
        let r = f64::from(rgba.r) / 255.0;
        let g = f64::from(rgba.g) / 255.0;
        let b = f64::from(rgba.b) / 255.0;
        let max = r.max(g).max(b);
        let min = r.min(g).min(b);
        let mut h = 0.0;
        let mut s = 0.0;
        let l = (min + max) / 2.0;
        let chroma = max - min;
        if chroma > 0.0 {
            s = (if l <= 0.5 { chroma / (2.0 * l) } else { chroma / (2.0 - 2.0 * l) }).min(1.0);
            // The same comparison order as the `switch (max)` upstream.
            if max == r {
                h = (g - b) / chroma + if g < b { 6.0 } else { 0.0 };
            } else if max == g {
                h = (b - r) / chroma + 2.0;
            } else {
                h = (r - g) / chroma + 4.0;
            }
            h *= 60.0;
            h = h.round();
        }
        Self::new(h, s, l, rgba.a)
    }

    fn hue_to_rgb(p: f64, q: f64, t: f64) -> f64 {
        let t = if t < 0.0 {
            t + 1.0
        } else if t > 1.0 {
            t - 1.0
        } else {
            t
        };
        if t < 1.0 / 6.0 {
            p + (q - p) * 6.0 * t
        } else if t < 1.0 / 2.0 {
            q
        } else if t < 2.0 / 3.0 {
            p + (q - p) * (2.0 / 3.0 - t) * 6.0
        } else {
            p
        }
    }

    pub fn to_rgba(self) -> Rgba {
        let h = self.h / 360.0;
        let (s, l, a) = (self.s, self.l, self.a);
        let (r, g, b) = if s == 0.0 {
            (l, l, l)
        } else {
            let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
            let p = 2.0 * l - q;
            (Self::hue_to_rgb(p, q, h + 1.0 / 3.0), Self::hue_to_rgb(p, q, h), Self::hue_to_rgb(p, q, h - 1.0 / 3.0))
        };
        Rgba::new((r * 255.0).round(), (g * 255.0).round(), (b * 255.0).round(), a)
    }
}

/// `Color`. Built from HSLA it keeps that HSLA (as upstream does), so `hsla()` of a lightened
/// colour is the value that produced it, not a round trip through RGB.
#[derive(Clone, Copy, Debug)]
pub struct Color {
    pub rgba: Rgba,
    hsla: Option<Hsla>,
}

impl PartialEq for Color {
    fn eq(&self, other: &Self) -> bool {
        self.rgba == other.rgba && self.hsla() == other.hsla()
    }
}

impl From<Rgba> for Color {
    fn from(rgba: Rgba) -> Self {
        Self { rgba, hsla: None }
    }
}

impl From<Hsla> for Color {
    fn from(hsla: Hsla) -> Self {
        Self { rgba: hsla.to_rgba(), hsla: Some(hsla) }
    }
}

impl Color {
    pub const WHITE: Color = Color { rgba: Rgba { r: 255, g: 255, b: 255, a: 1.0 }, hsla: None };
    pub const BLACK: Color = Color { rgba: Rgba { r: 0, g: 0, b: 0, a: 1.0 }, hsla: None };
    pub const TRANSPARENT: Color = Color { rgba: Rgba { r: 0, g: 0, b: 0, a: 0.0 }, hsla: None };

    pub fn rgba(r: f64, g: f64, b: f64, a: f64) -> Self {
        Rgba::new(r, g, b, a).into()
    }

    /// `Color.fromHex`: red when the text is not a hex colour (as upstream).
    pub fn from_hex(hex: &str) -> Self {
        parse_hex(hex).unwrap_or(Color { rgba: Rgba { r: 255, g: 0, b: 0, a: 1.0 }, hsla: None })
    }

    pub fn hsla(&self) -> Hsla {
        self.hsla.unwrap_or_else(|| Hsla::from_rgba(self.rgba))
    }

    fn component_luminance(c: u8) -> f64 {
        let c = f64::from(c) / 255.0;
        if c <= 0.03928 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
    }

    /// http://www.w3.org/TR/WCAG20/#relativeluminancedef, rounded to four decimals.
    pub fn relative_luminance(&self) -> f64 {
        let l = 0.2126 * Self::component_luminance(self.rgba.r)
            + 0.7152 * Self::component_luminance(self.rgba.g)
            + 0.0722 * Self::component_luminance(self.rgba.b);
        round_float(l, 4)
    }

    pub fn is_lighter_than(&self, other: &Color) -> bool {
        self.relative_luminance() > other.relative_luminance()
    }

    pub fn is_darker_than(&self, other: &Color) -> bool {
        self.relative_luminance() < other.relative_luminance()
    }

    pub fn lighten(&self, factor: f64) -> Color {
        let h = self.hsla();
        Hsla::new(h.h, h.s, h.l + h.l * factor, h.a).into()
    }

    pub fn darken(&self, factor: f64) -> Color {
        let h = self.hsla();
        Hsla::new(h.h, h.s, h.l - h.l * factor, h.a).into()
    }

    pub fn transparent(&self, factor: f64) -> Color {
        let Rgba { r, g, b, a } = self.rgba;
        Rgba::new(f64::from(r), f64::from(g), f64::from(b), a * factor).into()
    }

    pub fn is_opaque(&self) -> bool {
        self.rgba.a == 1.0
    }

    /// Composites `c` over this colour (`Color.blend`).
    pub fn blend(&self, c: &Color) -> Color {
        let this_a = self.rgba.a;
        let color_a = c.rgba.a;
        let a = this_a + color_a * (1.0 - this_a);
        if a < 1e-6 {
            return Color::TRANSPARENT;
        }
        let mix = |t: u8, o: u8| f64::from(t) * this_a / a + f64::from(o) * color_a * (1.0 - this_a) / a;
        Color::rgba(mix(self.rgba.r, c.rgba.r), mix(self.rgba.g, c.rgba.g), mix(self.rgba.b, c.rgba.b), a)
    }

    /// `factor` 0 is this colour, 1 the other, clamped.
    pub fn mix(&self, other: &Color, factor: f64) -> Color {
        let n = factor.clamp(0.0, 1.0);
        let (t, o) = (self.rgba, other.rgba);
        let m = |a: f64, b: f64| a + (b - a) * n;
        Color::rgba(
            m(f64::from(t.r), f64::from(o.r)),
            m(f64::from(t.g), f64::from(o.g)),
            m(f64::from(t.b), f64::from(o.b)),
            m(t.a, o.a),
        )
    }

    /// The opaque colour that looks like this one over `background` (only for an opaque background).
    pub fn make_opaque(&self, background: &Color) -> Color {
        if self.is_opaque() || background.rgba.a != 1.0 {
            return *self;
        }
        let Rgba { r, g, b, a } = self.rgba;
        let bg = background.rgba;
        Color::rgba(
            f64::from(bg.r) - a * (f64::from(bg.r) - f64::from(r)),
            f64::from(bg.g) - a * (f64::from(bg.g) - f64::from(g)),
            f64::from(bg.b) - a * (f64::from(bg.b) - f64::from(b)),
            1.0,
        )
    }

    pub fn lighter_color(of: &Color, relative: &Color, factor: Option<f64>) -> Color {
        if of.is_lighter_than(relative) {
            return *of;
        }
        let factor = factor.filter(|f| *f != 0.0).unwrap_or(0.5);
        let (l1, l2) = (of.relative_luminance(), relative.relative_luminance());
        of.lighten(factor * (l2 - l1) / l2)
    }

    pub fn darker_color(of: &Color, relative: &Color, factor: Option<f64>) -> Color {
        if of.is_darker_than(relative) {
            return *of;
        }
        let factor = factor.filter(|f| *f != 0.0).unwrap_or(0.5);
        let (l1, l2) = (of.relative_luminance(), relative.relative_luminance());
        of.darken(factor * (l1 - l2) / l1)
    }

    /// `rgba << 24 | ...`: `0xRRGGBBAA`.
    pub fn to_number_32bit(&self) -> u32 {
        let a = (self.rgba.a * 255.0) as u32; // `a * 0xFF` truncated by the bit operations
        (u32::from(self.rgba.r) << 24) | (u32::from(self.rgba.g) << 16) | (u32::from(self.rgba.b) << 8) | a
    }

    /// `#rrggbb` when opaque, otherwise `rgba(r, g, b, a)` (`Color.toString`).
    pub fn format(&self) -> String {
        if self.is_opaque() {
            self.format_hex()
        } else {
            format!("rgba({}, {}, {}, {})", self.rgba.r, self.rgba.g, self.rgba.b, trim_float((self.rgba.a * 100.0).round() / 100.0))
        }
    }

    /// `#rrggbb` (lower case).
    pub fn format_hex(&self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.rgba.r, self.rgba.g, self.rgba.b)
    }

    /// `#rrggbbaa`; with `compact`, `#rrggbb` when opaque.
    pub fn format_hex_a(&self, compact: bool) -> String {
        if compact && self.is_opaque() {
            return self.format_hex();
        }
        format!("{}{:02x}", self.format_hex(), (self.rgba.a * 255.0).round() as u8)
    }

    /// The straight-alpha 8-bit colour the UI paints with.
    pub fn to_rgba8(&self) -> Rgba8 {
        Rgba8(self.rgba.r, self.rgba.g, self.rgba.b, (self.rgba.a * 255.0).round() as u8)
    }
}

/// `+(n).toFixed(2)`: no trailing zeros.
fn trim_float(n: f64) -> String {
    let s = format!("{n}");
    s
}

/// UI colour: 8-bit RGBA, straight alpha. The view layer converts it (the app's `ui::to_gpui`),
/// so a framework switch never touches the theme data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba8(pub u8, pub u8, pub u8, pub u8);

impl Rgba8 {
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self(r, g, b, 0xFF)
    }

    /// Packed `0xRRGGBBAA`.
    pub const fn to_u32(self) -> u32 {
        u32::from_be_bytes([self.0, self.1, self.2, self.3])
    }
}

fn hex_digit(b: u8) -> f64 {
    match b {
        b'0'..=b'9' => f64::from(b - b'0'),
        b'a'..=b'f' => f64::from(b - b'a' + 10),
        b'A'..=b'F' => f64::from(b - b'A' + 10),
        _ => 0.0, // upstream maps every other character to 0
    }
}

/// `Color.Format.CSS.parseHex`: `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA`. A character that is not a
/// hex digit counts as 0 (as upstream); any other length (or a missing `#`) is `None`.
pub fn parse_hex(hex: &str) -> Option<Color> {
    let b = hex.as_bytes();
    if b.first() != Some(&b'#') || !hex.is_ascii() {
        return None;
    }
    let d = |i: usize| hex_digit(b[i]);
    let pair = |i: usize| 16.0 * d(i) + d(i + 1);
    match b.len() {
        7 => Some(Color::rgba(pair(1), pair(3), pair(5), 1.0)),
        9 => Some(Color::rgba(pair(1), pair(3), pair(5), pair(7) / 255.0)),
        4 => Some(Color::rgba(16.0 * d(1) + d(1), 16.0 * d(2) + d(2), 16.0 * d(3) + d(3), 1.0)),
        5 => Some(Color::rgba(16.0 * d(1) + d(1), 16.0 * d(2) + d(2), 16.0 * d(3) + d(3), (16.0 * d(4) + d(4)) / 255.0)),
        _ => None,
    }
}

/// `Color.Format.CSS.parse`: `transparent`, hex, `rgb(...)` and `rgba(...)`.
pub fn parse_css(css: &str) -> Option<Color> {
    if css == "transparent" {
        return Some(Color::TRANSPARENT);
    }
    if css.starts_with('#') {
        return parse_hex(css);
    }
    let (args, with_alpha) = match (css.strip_prefix("rgba("), css.strip_prefix("rgb(")) {
        (Some(rest), _) => (rest.strip_suffix(')')?, true),
        (None, Some(rest)) => (rest.strip_suffix(')')?, false),
        _ => return None,
    };
    let parts: Vec<&str> = args.split(',').map(str::trim_start).collect();
    if parts.len() != if with_alpha { 4 } else { 3 } {
        return None;
    }
    let int = |s: &str| -> Option<f64> {
        let digits = s.strip_prefix(['+', '-']).unwrap_or(s);
        (!digits.is_empty() && digits.bytes().all(|c| c.is_ascii_digit())).then(|| s.parse::<i64>().ok().map(|n| n as f64))?
    };
    let (r, g, b) = (int(parts[0])?, int(parts[1])?, int(parts[2])?);
    let a = if with_alpha { parts[3].parse::<f64>().ok()? } else { 1.0 };
    Some(Color::rgba(r, g, b, a))
}
