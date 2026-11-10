//! The folding, sticky scroll and minimap section header settings, typed (ADR-0042). UI free;
//! built from a `Configuration`.
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d
//!
//! `editor.foldingStrategy: auto` falls back to indentation: there are no language server folding
//! range providers. `editor.stickyScroll.defaultModel` is read but every model uses the folding
//! ranges (there is no outline provider either).

use serde::Deserialize;
use vsr_settings::Configuration;

/// `editor.showFoldingControls`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShowFoldingControls {
    Always,
    Never,
    Mouseover,
}

/// `editor.foldingStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FoldingStrategy {
    Auto,
    Indentation,
}

/// `editor.stickyScroll.defaultModel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum StickyModel {
    #[serde(rename = "outlineModel")]
    Outline,
    #[serde(rename = "foldingProviderModel")]
    FoldingProvider,
    #[serde(rename = "indentationModel")]
    Indentation,
}

/// Upstream's default `editor.minimap.markSectionHeaderRegex`.
pub const DEFAULT_MARK_REGEX: &str = r"\bMARK:\s*(?<separator>-?)\s*(?<label>.*)$";

#[derive(Debug, Clone, PartialEq)]
pub struct FoldingSettings {
    /// `editor.folding`.
    pub enabled: bool,
    /// `editor.foldingStrategy`.
    pub strategy: FoldingStrategy,
    /// `editor.foldingHighlight`: collapsed lines get the fold background.
    pub highlight: bool,
    /// `editor.foldingMaximumRegions`, clamped to 10..=65000.
    pub maximum_regions: usize,
    /// `editor.showFoldingControls`.
    pub show_controls: ShowFoldingControls,
    /// `editor.unfoldOnClickAfterEndOfLine`.
    pub unfold_on_click_after_end_of_line: bool,
    /// `editor.stickyScroll.enabled`.
    pub sticky_enabled: bool,
    /// `editor.stickyScroll.maxLineCount`, clamped to 1..=20.
    pub sticky_max_line_count: usize,
    /// `editor.stickyScroll.defaultModel`.
    pub sticky_model: StickyModel,
    /// `editor.stickyScroll.scrollWithEditor`.
    pub sticky_scroll_with_editor: bool,
    /// `editor.minimap.showRegionSectionHeaders`.
    pub minimap_region_headers: bool,
    /// `editor.minimap.showMarkSectionHeaders`.
    pub minimap_mark_headers: bool,
    /// `editor.minimap.markSectionHeaderRegex`.
    pub minimap_mark_regex: String,
    /// `editor.minimap.sectionHeaderFontSize`.
    pub minimap_header_font_size: f32,
}

impl Default for FoldingSettings {
    /// The registered defaults.
    fn default() -> Self {
        Self {
            enabled: true,
            strategy: FoldingStrategy::Auto,
            highlight: true,
            maximum_regions: 5000,
            show_controls: ShowFoldingControls::Mouseover,
            unfold_on_click_after_end_of_line: false,
            sticky_enabled: true,
            sticky_max_line_count: 5,
            sticky_model: StickyModel::Outline,
            sticky_scroll_with_editor: true,
            minimap_region_headers: true,
            minimap_mark_headers: true,
            minimap_mark_regex: DEFAULT_MARK_REGEX.to_string(),
            minimap_header_font_size: 9.0,
        }
    }
}

impl FoldingSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let d = Self::default();
        let int = |key: &str, default: usize, min: f64, max: f64| {
            c.get_f64(key).map_or(default, |v| v.round().clamp(min, max) as usize)
        };
        Self {
            enabled: c.get_bool("editor.folding").unwrap_or(d.enabled),
            strategy: c.get_enum("editor.foldingStrategy").unwrap_or(d.strategy),
            highlight: c.get_bool("editor.foldingHighlight").unwrap_or(d.highlight),
            maximum_regions: int("editor.foldingMaximumRegions", d.maximum_regions, 10.0, 65000.0),
            show_controls: c.get_enum("editor.showFoldingControls").unwrap_or(d.show_controls),
            unfold_on_click_after_end_of_line: c.get_bool("editor.unfoldOnClickAfterEndOfLine").unwrap_or(d.unfold_on_click_after_end_of_line),
            sticky_enabled: c.get_bool("editor.stickyScroll.enabled").unwrap_or(d.sticky_enabled),
            sticky_max_line_count: int("editor.stickyScroll.maxLineCount", d.sticky_max_line_count, 1.0, 20.0),
            sticky_model: c.get_enum("editor.stickyScroll.defaultModel").unwrap_or(d.sticky_model),
            sticky_scroll_with_editor: c.get_bool("editor.stickyScroll.scrollWithEditor").unwrap_or(d.sticky_scroll_with_editor),
            minimap_region_headers: c.get_bool("editor.minimap.showRegionSectionHeaders").unwrap_or(d.minimap_region_headers),
            minimap_mark_headers: c.get_bool("editor.minimap.showMarkSectionHeaders").unwrap_or(d.minimap_mark_headers),
            minimap_mark_regex: c.get_str("editor.minimap.markSectionHeaderRegex").map_or_else(|| d.minimap_mark_regex.clone(), str::to_string),
            minimap_header_font_size: c.get_f64("editor.minimap.sectionHeaderFontSize").unwrap_or(9.0).max(0.0) as f32,
        }
    }
}
