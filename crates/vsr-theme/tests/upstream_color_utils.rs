//! upstream: src/vs/platform/theme/test/common/colorUtils.test.ts @ 50e1350fc78d
//!
//! Ports of the colour transform tests. `TestColorTheme` (testThemeService.ts) is a map of colour keys to colours.

use std::collections::HashMap;

use vsr_theme::{Color, ColorLookup, ColorTransform, ColorValue, Rgba, execute_transform};

/// `TestColorTheme`: the colours it is given, no defaults.
struct TestColorTheme(HashMap<String, Color>);

impl TestColorTheme {
    fn new(colors: &[(&str, &str)]) -> Self {
        Self(colors.iter().map(|(k, v)| ((*k).to_string(), Color::from_hex(v))).collect())
    }
}

impl ColorLookup for TestColorTheme {
    fn color(&self, key: &str) -> Option<Color> {
        self.0.get(key).copied()
    }
    fn defines(&self, key: &str) -> bool {
        self.0.contains_key(key)
    }
}

fn with_opacity(value: ColorValue, opacity: f64) -> ColorTransform {
    ColorTransform::Opacity { value, opacity }
}

fn rgba(r: u8, g: u8, b: u8, a: f64) -> Rgba {
    Rgba::new(f64::from(r), f64::from(g), f64::from(b), a)
}

#[test]
fn with_opacity_replaces_alpha_without_changing_rgb() {
    // upstream: Color transforms > withOpacity replaces alpha without changing RGB
    let theme = TestColorTheme::new(&[]);
    for color in ["#123456", "#12345622", "#12345600"] {
        let got = execute_transform(&with_opacity(ColorValue::parse(color), 0.8), &theme).map(|c| c.rgba);
        assert_eq!(got, Some(rgba(18, 52, 86, 0.8)), "{color}");
    }
}

#[test]
fn with_opacity_resolves_theme_colours_nested_transforms_and_missing_colours() {
    // upstream: Color transforms > withOpacity resolves theme colours, nested transforms and missing colours
    let theme = TestColorTheme::new(&[("source", "#12345622")]);
    let lighten = ColorTransform::Lighten { value: ColorValue::parse("source"), factor: 1.0 };
    assert_eq!(execute_transform(&with_opacity(ColorValue::parse("source"), 0.8), &theme).map(|c| c.rgba), Some(rgba(18, 52, 86, 0.8)));
    assert_eq!(
        execute_transform(&with_opacity(ColorValue::transform(lighten), 0.6), &theme).map(|c| c.rgba),
        Some(rgba(36, 104, 172, 0.6))
    );
    assert_eq!(execute_transform(&with_opacity(ColorValue::parse("missing"), 0.8), &theme), None);
}

#[test]
fn with_opacity_clamps_alpha_to_its_valid_range() {
    // upstream: Color transforms > withOpacity clamps alpha to its valid range
    let theme = TestColorTheme::new(&[]);
    let expected = [0.0, 1.0, 0.0, 1.0];
    for (opacity, expected) in [0.0, 1.0, -1.0, 2.0].into_iter().zip(expected) {
        let got = execute_transform(&with_opacity(ColorValue::parse("#12345622"), opacity), &theme).map(|c| c.rgba);
        assert_eq!(got, Some(rgba(18, 52, 86, expected)), "opacity {opacity}");
    }
}
