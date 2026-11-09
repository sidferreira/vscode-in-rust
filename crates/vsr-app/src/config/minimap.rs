//! The `editor.minimap.*` settings, typed. UI free; built from a `Configuration`.
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d
//!
//! Invalid values fall back to the registered default (upstream's `stringSet`/`boolean`/
//! `clampedInt` validation): `scale` is clamped to 1..=3, `maxColumn` to 1..=10000.
//! The section header settings are not read (see `wired.rs`).

use serde::Deserialize;
use vsr_settings::Configuration;

/// `editor.minimap.side`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinimapSide {
    Left,
    Right,
}

/// `editor.minimap.size`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinimapSize {
    Proportional,
    Fill,
    Fit,
}

/// `editor.minimap.showSlider`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShowSlider {
    Always,
    Mouseover,
}

/// `editor.minimap.autohide`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MinimapAutohide {
    None,
    Mouseover,
    Scroll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapSettings {
    pub enabled: bool,
    pub side: MinimapSide,
    pub size: MinimapSize,
    pub show_slider: ShowSlider,
    pub autohide: MinimapAutohide,
    pub render_characters: bool,
    pub max_column: u32,
    pub scale: u32,
}

impl Default for MinimapSettings {
    /// The registered defaults.
    fn default() -> Self {
        Self {
            enabled: true,
            side: MinimapSide::Right,
            size: MinimapSize::Proportional,
            show_slider: ShowSlider::Mouseover,
            autohide: MinimapAutohide::None,
            render_characters: true,
            max_column: 120,
            scale: 1,
        }
    }
}

/// `EditorIntOption.clampedInt`: a number is rounded and clamped, anything else is the default.
fn clamped_int(v: Option<f64>, default: u32, min: u32, max: u32) -> u32 {
    v.filter(|n| n.is_finite()).map_or(default, |n| n.round().clamp(f64::from(min), f64::from(max)) as u32)
}

impl MinimapSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let d = Self::default();
        Self {
            enabled: c.get_bool("editor.minimap.enabled").unwrap_or(d.enabled),
            side: c.get_enum("editor.minimap.side").unwrap_or(d.side),
            size: c.get_enum("editor.minimap.size").unwrap_or(d.size),
            show_slider: c.get_enum("editor.minimap.showSlider").unwrap_or(d.show_slider),
            autohide: c.get_enum("editor.minimap.autohide").unwrap_or(d.autohide),
            render_characters: c.get_bool("editor.minimap.renderCharacters").unwrap_or(d.render_characters),
            max_column: clamped_int(c.get_f64("editor.minimap.maxColumn"), d.max_column, 1, 10_000),
            scale: clamped_int(c.get_f64("editor.minimap.scale"), d.scale, 1, 3),
        }
    }
}

#[cfg(test)]
mod tests {
    use vsr_settings::{SettingsRegistry, UserSettings};

    use super::*;

    fn config(user: &str) -> Configuration {
        Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap())
    }

    #[test]
    fn defaults_match_upstream() {
        assert_eq!(MinimapSettings::from_configuration(&config("")), MinimapSettings::default());
    }

    #[test]
    fn values_flow_through_and_invalid_ones_fall_back() {
        let s = MinimapSettings::from_configuration(&config(
            r#"{ "editor.minimap.enabled": false, "editor.minimap.side": "left", "editor.minimap.size": "fit",
                 "editor.minimap.showSlider": "always", "editor.minimap.autohide": "scroll",
                 "editor.minimap.renderCharacters": false, "editor.minimap.maxColumn": 80, "editor.minimap.scale": 3 }"#,
        ));
        assert_eq!(
            s,
            MinimapSettings {
                enabled: false,
                side: MinimapSide::Left,
                size: MinimapSize::Fit,
                show_slider: ShowSlider::Always,
                autohide: MinimapAutohide::Scroll,
                render_characters: false,
                max_column: 80,
                scale: 3,
            }
        );
        let bad = MinimapSettings::from_configuration(&config(
            r#"{ "editor.minimap.side": "top", "editor.minimap.scale": 9, "editor.minimap.maxColumn": 0 }"#,
        ));
        // The registry rejects out-of-enum `scale` values (default 1); `maxColumn` is clamped.
        assert_eq!((bad.side, bad.scale, bad.max_column), (MinimapSide::Right, 1, 1));
        assert_eq!((clamped_int(Some(9.4), 1, 1, 3), clamped_int(Some(0.0), 1, 1, 3), clamped_int(None, 120, 1, 10_000)), (3, 1, 120));
    }
}
