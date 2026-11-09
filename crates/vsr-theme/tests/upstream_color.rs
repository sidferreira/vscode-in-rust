//! upstream: src/vs/base/test/common/color.test.ts @ 50e1350fc78d
//!
//! Ports of VS Code's `Color` tests onto `vsr_theme::Color`. Left out: the `HSVA` suite, the
//! `named keyword` and `format` (HSL) tests and `ensureContrastRatio` (not ported: the theme
//! service does not need them). The `rgb()` / `rgba()` tests keep a representative subset of the
//! valid colours of the long lists.

use vsr_theme::{Color, Hsla, Rgba, parse_css, parse_hex};

fn hsla(h: f64, s: f64, l: f64, a: f64) -> Hsla {
    Hsla::new(h, s, l, a)
}

fn rgba(r: u8, g: u8, b: u8, a: f64) -> Rgba {
    Rgba::new(f64::from(r), f64::from(g), f64::from(b), a)
}

fn from_hsla(h: f64, s: f64, l: f64, a: f64) -> Color {
    Color::from(hsla(h, s, l, a))
}

#[test]
fn is_lighter_color() {
    // upstream: Color > isLighterColor
    let color1 = from_hsla(60.0, 1.0, 0.5, 1.0);
    let color2 = from_hsla(0.0, 0.0, 0.753, 1.0);
    assert!(color1.is_lighter_than(&color2));

    // Abyss theme
    assert!(Color::from_hex("#770811").is_lighter_than(&Color::from_hex("#000c18")));
}

#[test]
fn get_lighter_color() {
    // upstream: Color > getLighterColor
    let color1 = from_hsla(60.0, 1.0, 0.5, 1.0);
    let color2 = from_hsla(0.0, 0.0, 0.753, 1.0);

    assert_eq!(color1.hsla(), Color::lighter_color(&color1, &color2, None).hsla());
    assert_eq!(hsla(0.0, 0.0, 0.916, 1.0), Color::lighter_color(&color2, &color1, None).hsla());
    assert_eq!(hsla(0.0, 0.0, 0.851, 1.0), Color::lighter_color(&color2, &color1, Some(0.3)).hsla());
    assert_eq!(hsla(0.0, 0.0, 0.981, 1.0), Color::lighter_color(&color2, &color1, Some(0.7)).hsla());
    assert_eq!(hsla(0.0, 0.0, 1.0, 1.0), Color::lighter_color(&color2, &color1, Some(1.0)).hsla());
}

#[test]
fn is_darker_color() {
    // upstream: Color > isDarkerColor
    let color1 = from_hsla(60.0, 1.0, 0.5, 1.0);
    let color2 = from_hsla(0.0, 0.0, 0.753, 1.0);
    assert!(color2.is_darker_than(&color1));
}

#[test]
fn get_darker_color() {
    // upstream: Color > getDarkerColor
    let color1 = from_hsla(60.0, 1.0, 0.5, 1.0);
    let color2 = from_hsla(0.0, 0.0, 0.753, 1.0);

    assert_eq!(color2.hsla(), Color::darker_color(&color2, &color1, None).hsla());
    assert_eq!(hsla(60.0, 1.0, 0.392, 1.0), Color::darker_color(&color1, &color2, None).hsla());
    assert_eq!(hsla(60.0, 1.0, 0.435, 1.0), Color::darker_color(&color1, &color2, Some(0.3)).hsla());
    assert_eq!(hsla(60.0, 1.0, 0.349, 1.0), Color::darker_color(&color1, &color2, Some(0.7)).hsla());
    assert_eq!(hsla(60.0, 1.0, 0.284, 1.0), Color::darker_color(&color1, &color2, Some(1.0)).hsla());

    // Abyss theme
    assert_eq!(
        hsla(355.0, 0.874, 0.157, 1.0),
        Color::darker_color(&Color::from_hex("#770811"), &Color::from_hex("#000c18"), Some(0.4)).hsla()
    );
}

#[test]
fn luminance() {
    // upstream: Color > luminance
    let lum = |r, g, b| Color::from(rgba(r, g, b, 1.0)).relative_luminance();
    assert_eq!(0.0, lum(0, 0, 0));
    assert_eq!(1.0, lum(255, 255, 255));

    assert_eq!(0.2126, lum(255, 0, 0));
    assert_eq!(0.7152, lum(0, 255, 0));
    assert_eq!(0.0722, lum(0, 0, 255));

    assert_eq!(0.9278, lum(255, 255, 0));
    assert_eq!(0.7874, lum(0, 255, 255));
    assert_eq!(0.2848, lum(255, 0, 255));

    assert_eq!(0.5271, lum(192, 192, 192));

    assert_eq!(0.2159, lum(128, 128, 128));
    assert_eq!(0.0459, lum(128, 0, 0));
    assert_eq!(0.2003, lum(128, 128, 0));
    assert_eq!(0.1544, lum(0, 128, 0));
    assert_eq!(0.0615, lum(128, 0, 128));
    assert_eq!(0.17, lum(0, 128, 128));
    assert_eq!(0.0156, lum(0, 0, 128));
}

#[test]
fn blending() {
    // upstream: Color > blending
    let c = |r, g, b, a| Color::from(rgba(r, g, b, a));
    assert_eq!(c(0, 0, 0, 0.0).blend(&c(243, 34, 43, 1.0)), c(243, 34, 43, 1.0));
    assert_eq!(c(255, 255, 255, 1.0).blend(&c(243, 34, 43, 1.0)), c(255, 255, 255, 1.0));
    assert_eq!(c(122, 122, 122, 0.7).blend(&c(243, 34, 43, 1.0)), c(158, 95, 98, 1.0));
    assert_eq!(c(0, 0, 0, 0.58).blend(&c(255, 255, 255, 0.33)), c(49, 49, 49, 0.719));
}

#[test]
fn to_string_alpha_channel() {
    // upstream: Color > toString > alpha channel
    assert_eq!(Color::from_hex("#00000000").format(), "rgba(0, 0, 0, 0)");
    assert_eq!(Color::from_hex("#00000080").format(), "rgba(0, 0, 0, 0.5)");
    assert_eq!(Color::from_hex("#000000FF").format(), "#000000");
}

const OPAQUE_HEXES: &[&str] = &[
    "#000000", "#FFFFFF", "#FF0000", "#00FF00", "#0000FF", "#FFFF00", "#00FFFF", "#FF00FF", "#C0C0C0", "#808080", "#800000", "#808000",
    "#008000", "#800080", "#008080", "#000080", "#010203", "#040506", "#070809", "#0a0A0a", "#0b0B0b", "#0c0C0c", "#0d0D0d", "#0e0E0e",
    "#0f0F0f", "#a0A0a0",
];

#[test]
fn to_string_opaque() {
    // upstream: Color > toString > opaque
    for hex in OPAQUE_HEXES {
        assert_eq!(Color::from_hex(hex).format().to_uppercase(), hex.to_uppercase());
    }
}

#[test]
fn to_number_32bit_alpha_channel() {
    // upstream: Color > toNumber32Bit > alpha channel
    assert_eq!(Color::from_hex("#00000000").to_number_32bit(), 0x0000_0000);
    assert_eq!(Color::from_hex("#00000080").to_number_32bit(), 0x0000_0080);
    assert_eq!(Color::from_hex("#000000FF").to_number_32bit(), 0x0000_00FF);
}

#[test]
fn to_number_32bit_opaque() {
    // upstream: Color > toNumber32Bit > opaque
    for hex in OPAQUE_HEXES {
        let expected = (u32::from_str_radix(&hex[1..], 16).unwrap() << 8) | 0xFF;
        assert_eq!(Color::from_hex(hex).to_number_32bit(), expected, "{hex}");
    }
}

#[test]
fn hsla_to_rgba() {
    // upstream: Color > HSLA > HSLA.toRGBA
    let to = |h, s, l, a| hsla(h, s, l, a).to_rgba();
    assert_eq!(to(0.0, 0.0, 0.0, 0.0), rgba(0, 0, 0, 0.0));
    assert_eq!(to(0.0, 0.0, 0.0, 1.0), rgba(0, 0, 0, 1.0));
    assert_eq!(to(0.0, 0.0, 1.0, 1.0), rgba(255, 255, 255, 1.0));

    assert_eq!(to(0.0, 1.0, 0.5, 1.0), rgba(255, 0, 0, 1.0));
    assert_eq!(to(120.0, 1.0, 0.5, 1.0), rgba(0, 255, 0, 1.0));
    assert_eq!(to(240.0, 1.0, 0.5, 1.0), rgba(0, 0, 255, 1.0));

    assert_eq!(to(60.0, 1.0, 0.5, 1.0), rgba(255, 255, 0, 1.0));
    assert_eq!(to(180.0, 1.0, 0.5, 1.0), rgba(0, 255, 255, 1.0));
    assert_eq!(to(300.0, 1.0, 0.5, 1.0), rgba(255, 0, 255, 1.0));

    assert_eq!(to(0.0, 0.0, 0.753, 1.0), rgba(192, 192, 192, 1.0));

    assert_eq!(to(0.0, 0.0, 0.502, 1.0), rgba(128, 128, 128, 1.0));
    assert_eq!(to(0.0, 1.0, 0.251, 1.0), rgba(128, 0, 0, 1.0));
    assert_eq!(to(60.0, 1.0, 0.251, 1.0), rgba(128, 128, 0, 1.0));
    assert_eq!(to(120.0, 1.0, 0.251, 1.0), rgba(0, 128, 0, 1.0));
    assert_eq!(to(300.0, 1.0, 0.251, 1.0), rgba(128, 0, 128, 1.0));
    assert_eq!(to(180.0, 1.0, 0.251, 1.0), rgba(0, 128, 128, 1.0));
    assert_eq!(to(240.0, 1.0, 0.251, 1.0), rgba(0, 0, 128, 1.0));
}

#[test]
fn hsla_from_rgba() {
    // upstream: Color > HSLA > HSLA.fromRGBA
    let from = |r, g, b, a| Hsla::from_rgba(rgba(r, g, b, a));
    assert_eq!(from(0, 0, 0, 0.0), hsla(0.0, 0.0, 0.0, 0.0));
    assert_eq!(from(0, 0, 0, 1.0), hsla(0.0, 0.0, 0.0, 1.0));
    assert_eq!(from(255, 255, 255, 1.0), hsla(0.0, 0.0, 1.0, 1.0));

    assert_eq!(from(255, 0, 0, 1.0), hsla(0.0, 1.0, 0.5, 1.0));
    assert_eq!(from(0, 255, 0, 1.0), hsla(120.0, 1.0, 0.5, 1.0));
    assert_eq!(from(0, 0, 255, 1.0), hsla(240.0, 1.0, 0.5, 1.0));

    assert_eq!(from(255, 255, 0, 1.0), hsla(60.0, 1.0, 0.5, 1.0));
    assert_eq!(from(0, 255, 255, 1.0), hsla(180.0, 1.0, 0.5, 1.0));
    assert_eq!(from(255, 0, 255, 1.0), hsla(300.0, 1.0, 0.5, 1.0));

    assert_eq!(from(192, 192, 192, 1.0), hsla(0.0, 0.0, 0.753, 1.0));

    assert_eq!(from(128, 128, 128, 1.0), hsla(0.0, 0.0, 0.502, 1.0));
    assert_eq!(from(128, 0, 0, 1.0), hsla(0.0, 1.0, 0.251, 1.0));
    assert_eq!(from(128, 128, 0, 1.0), hsla(60.0, 1.0, 0.251, 1.0));
    assert_eq!(from(0, 128, 0, 1.0), hsla(120.0, 1.0, 0.251, 1.0));
    assert_eq!(from(128, 0, 128, 1.0), hsla(300.0, 1.0, 0.251, 1.0));
    assert_eq!(from(0, 128, 128, 1.0), hsla(180.0, 1.0, 0.251, 1.0));
    assert_eq!(from(0, 0, 128, 1.0), hsla(240.0, 1.0, 0.251, 1.0));
}

#[test]
fn css_parse_invalid() {
    // upstream: Color > Format > CSS > parse > invalid
    assert_eq!(parse_css(""), None);
    assert_eq!(parse_css("#"), None);
    assert_eq!(parse_css("#0102030"), None);
}

#[test]
fn css_parse_transparent() {
    // upstream: Color > Format > CSS > parse > transparent
    assert_eq!(parse_css("transparent").unwrap().rgba, rgba(0, 0, 0, 0.0));
}

/// The opaque hexes of the `hex-color` / `parseHex` tests with the colour they parse to.
fn hex_cases() -> Vec<(&'static str, Rgba)> {
    vec![
        ("#000000", rgba(0, 0, 0, 1.0)),
        ("#FFFFFF", rgba(255, 255, 255, 1.0)),
        ("#FF0000", rgba(255, 0, 0, 1.0)),
        ("#00FF00", rgba(0, 255, 0, 1.0)),
        ("#0000FF", rgba(0, 0, 255, 1.0)),
        ("#FFFF00", rgba(255, 255, 0, 1.0)),
        ("#00FFFF", rgba(0, 255, 255, 1.0)),
        ("#FF00FF", rgba(255, 0, 255, 1.0)),
        ("#C0C0C0", rgba(192, 192, 192, 1.0)),
        ("#808080", rgba(128, 128, 128, 1.0)),
        ("#800000", rgba(128, 0, 0, 1.0)),
        ("#808000", rgba(128, 128, 0, 1.0)),
        ("#008000", rgba(0, 128, 0, 1.0)),
        ("#800080", rgba(128, 0, 128, 1.0)),
        ("#008080", rgba(0, 128, 128, 1.0)),
        ("#000080", rgba(0, 0, 128, 1.0)),
        ("#010203", rgba(1, 2, 3, 1.0)),
        ("#040506", rgba(4, 5, 6, 1.0)),
        ("#070809", rgba(7, 8, 9, 1.0)),
        ("#0a0A0a", rgba(10, 10, 10, 1.0)),
        ("#0b0B0b", rgba(11, 11, 11, 1.0)),
        ("#0c0C0c", rgba(12, 12, 12, 1.0)),
        ("#0d0D0d", rgba(13, 13, 13, 1.0)),
        ("#0e0E0e", rgba(14, 14, 14, 1.0)),
        ("#0f0F0f", rgba(15, 15, 15, 1.0)),
        ("#a0A0a0", rgba(160, 160, 160, 1.0)),
        ("#CFA", rgba(204, 255, 170, 1.0)),
        ("#CFA8", rgba(204, 255, 170, 0.533)),
    ]
}

#[test]
fn css_parse_hex_color() {
    // upstream: Color > Format > CSS > parse > hex-color
    // somewhat valid
    assert_eq!(parse_css("#FFFFG0").unwrap().rgba, rgba(255, 255, 0, 1.0));
    assert_eq!(parse_css("#FFFFg0").unwrap().rgba, rgba(255, 255, 0, 1.0));
    assert_eq!(parse_css("#-FFF00").unwrap().rgba, rgba(15, 255, 0, 1.0));
    // valid
    for (hex, expected) in hex_cases() {
        assert_eq!(parse_css(hex).unwrap().rgba, expected, "{hex}");
    }
}

#[test]
fn css_parse_rgb() {
    // upstream: Color > Format > CSS > parse > rgb()
    // somewhat valid / unusual
    assert_eq!(parse_css("rgb(-255, 0, 0)").unwrap().rgba, rgba(0, 0, 0, 1.0));
    assert_eq!(parse_css("rgb(+255, 0, 0)").unwrap().rgba, rgba(255, 0, 0, 1.0));
    assert_eq!(parse_css("rgb(800, 0, 0)").unwrap().rgba, rgba(255, 0, 0, 1.0));
    // valid
    for (r, g, b) in [(0, 0, 0), (255, 255, 255), (255, 0, 0), (0, 255, 0), (192, 192, 192), (128, 0, 128), (1, 2, 3), (15, 15, 15)] {
        assert_eq!(parse_css(&format!("rgb({r}, {g}, {b})")).unwrap().rgba, rgba(r, g, b, 1.0));
    }
}

#[test]
fn css_parse_rgba() {
    // upstream: Color > Format > CSS > parse > rgba()
    // somewhat valid / unusual
    assert_eq!(parse_css("rgba(0, 0, 0, 255)").unwrap().rgba, rgba(0, 0, 0, 1.0));
    assert_eq!(parse_css("rgba(-255, 0, 0, 1)").unwrap().rgba, rgba(0, 0, 0, 1.0));
    assert_eq!(parse_css("rgba(+255, 0, 0, 1)").unwrap().rgba, rgba(255, 0, 0, 1.0));
    assert_eq!(parse_css("rgba(800, 0, 0, 1)").unwrap().rgba, rgba(255, 0, 0, 1.0));
    // alpha values
    assert_eq!(parse_css("rgba(255, 0, 0, 0.2)").unwrap().rgba, rgba(255, 0, 0, 0.2));
    assert_eq!(parse_css("rgba(255, 0, 0, 0.5)").unwrap().rgba, rgba(255, 0, 0, 0.5));
    assert_eq!(parse_css("rgba(255, 0, 0, 0.75)").unwrap().rgba, rgba(255, 0, 0, 0.75));
    // valid
    for (r, g, b) in [(0, 0, 0), (255, 255, 255), (255, 0, 0), (0, 255, 255), (192, 192, 192), (128, 128, 0), (1, 2, 3), (15, 15, 15)] {
        assert_eq!(parse_css(&format!("rgba({r}, {g}, {b}, 1)")).unwrap().rgba, rgba(r, g, b, 1.0));
    }
}

#[test]
fn css_parse_hex() {
    // upstream: Color > Format > CSS > parseHex
    // invalid
    assert!(parse_hex("").is_none());
    assert!(parse_hex("#").is_none());
    assert!(parse_hex("#0102030").is_none());
    // somewhat valid
    assert_eq!(parse_hex("#FFFFG0").unwrap().rgba, rgba(255, 255, 0, 1.0));
    assert_eq!(parse_hex("#FFFFg0").unwrap().rgba, rgba(255, 255, 0, 1.0));
    assert_eq!(parse_hex("#-FFF00").unwrap().rgba, rgba(15, 255, 0, 1.0));
    // valid
    for (hex, expected) in hex_cases() {
        assert_eq!(parse_hex(hex).unwrap().rgba, expected, "{hex}");
    }
}
