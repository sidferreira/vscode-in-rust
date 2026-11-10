//! Code folding in the view: the fold controls in the gutter, the `⋯` after a folded line, the
//! folded-lines background, and clicks on them (ADR-0042).
//!
//! upstream: src/vs/editor/contrib/folding/browser/folding.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/folding/browser/foldingDecorations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/folding/browser/folding.css @ 50e1350fc78d
//!
//! The model is `vsr_editor` (regions, hidden lines in the view-line projection); this module only
//! draws. A collapsed line always shows its chevron (right) unless `editor.showFoldingControls` is
//! `never`; an expanded one shows its chevron (down) with `always`, or while the mouse is over the
//! gutter with `mouseover`.

use gpui_kit::{AnyElement, InteractiveElement, IntoElement, ParentElement, Styled, div, px};
use vsr_editor::{FoldInfo, FoldingParams};

use crate::config::folding::ShowFoldingControls;
use crate::config::settings::EditorSettings;
use crate::icons;
use crate::model::OpenEditor;
use crate::theme::Theme;
use crate::ui::to_gpui;

/// Width of the fold control column, at the right end of the gutter.
pub const CONTROL_WIDTH: f32 = 18.0;

/// What the paint of one frame needs besides the settings.
#[derive(Debug, Clone, Copy, Default)]
pub struct Paint {
    /// The mouse is over the gutter (`showFoldingControls: mouseover`).
    pub gutter_hover: bool,
}

/// The folding parameters of a document: its language's rules and the settings. `None` when
/// `editor.folding` is off.
pub fn params(cfg: &EditorSettings, language: &str) -> Option<FoldingParams> {
    if !cfg.folding.enabled {
        return None;
    }
    // Both strategies compute indentation ranges: there are no language server range providers.
    let rules = vsr_languages::bundled::folding_rules(language);
    Some(FoldingParams {
        off_side: rules.is_some_and(|r| r.off_side),
        markers: rules.and_then(|r| r.markers.as_ref()).map(|m| (m.start.clone(), m.end.clone())),
        max_regions: cfg.folding.maximum_regions,
    })
}

/// Gives the editor its folding parameters (cheap when unchanged).
pub fn sync(e: &OpenEditor, cfg: &EditorSettings) {
    e.editor.set_folding(params(cfg, e.language_id()));
}

/// The fold control of a row whose line starts a region, `None` when it is hidden or `never`.
pub fn control(info: Option<FoldInfo>, cfg: &EditorSettings, t: &Theme, gutter: f32, paint: Paint, line: usize) -> Option<AnyElement> {
    let info = info?;
    let show = match cfg.folding.show_controls {
        ShowFoldingControls::Never => false,
        ShowFoldingControls::Always => true,
        ShowFoldingControls::Mouseover => info.collapsed || paint.gutter_hover,
    };
    if !show {
        return None;
    }
    let glyph = if info.collapsed { icons::CHEVRON_RIGHT } else { icons::CHEVRON_DOWN };
    Some(
        div()
            .absolute()
            .top_0()
            .left(px(gutter - CONTROL_WIDTH))
            .w(px(CONTROL_WIDTH - 2.0))
            .h(px(cfg.line_height))
            .flex()
            .items_center()
            .justify_center()
            .font_family(icons::FAMILY)
            .text_size(px(16.0))
            .text_color(to_gpui(t.folding_control_fg))
            .debug_selector(|| format!("fold:{line}"))
            .child(icons::glyph(glyph))
            .into_any_element(),
    )
}

/// The `⋯` after the text of a collapsed line, `x` pixels from the text's left edge.
pub fn placeholder(t: &Theme, cfg: &EditorSettings, x: f32, line: usize) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left(px(x + 2.0))
        .h(px(cfg.line_height))
        .text_color(to_gpui(t.fold_placeholder_fg))
        .debug_selector(|| format!("fold-placeholder:{line}"))
        .child("\u{22ef}")
        .into_any_element()
}

/// The background of a collapsed line (`editor.foldingHighlight`).
pub fn highlight(t: &Theme, gutter: f32, line: usize) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .bottom_0()
        .left(px(gutter))
        .right_0()
        .bg(to_gpui(t.fold_bg))
        .debug_selector(|| format!("fold-bg:{line}"))
        .into_any_element()
}

/// Whether a click at text x `text_x` on the end column of a collapsed line unfolds it: always on the
/// `⋯` (it sits just after the text, `end_x`), after it only with `editor.unfoldOnClickAfterEndOfLine`.
pub fn unfolds_on_click(text_x: f32, end_x: f32, char_width: f32, unfold_after_end_of_line: bool) -> bool {
    text_x <= end_x + char_width * 2.0 + 4.0 || unfold_after_end_of_line
}

/// Whether a mouse x (relative to the editor's left edge) is on the fold control column.
pub fn in_control_column(x: f32, gutter: f32) -> bool {
    x >= gutter - CONTROL_WIDTH && x < gutter
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_column_is_the_right_end_of_the_gutter() {
        assert!(in_control_column(60.0, 70.0));
        assert!(!in_control_column(40.0, 70.0), "the line numbers");
        assert!(!in_control_column(70.0, 70.0), "the text");
    }

    #[test]
    fn a_click_unfolds_on_the_placeholder_or_after_the_end_by_setting() {
        assert!(unfolds_on_click(105.0, 100.0, 7.0, false), "on the placeholder");
        assert!(!unfolds_on_click(300.0, 100.0, 7.0, false), "far after the end");
        assert!(unfolds_on_click(300.0, 100.0, 7.0, true), "editor.unfoldOnClickAfterEndOfLine");
    }

    #[test]
    fn params_follow_the_language_and_the_setting() {
        let mut cfg = EditorSettings::default();
        let rust = params(&cfg, "rust").unwrap();
        assert!(!rust.off_side && rust.markers.is_some());
        assert!(params(&cfg, "python").unwrap().off_side);
        assert!(params(&cfg, "plaintext").unwrap().markers.is_none());
        cfg.folding.enabled = false;
        assert!(params(&cfg, "rust").is_none());
    }

    #[test]
    fn every_bundled_region_marker_compiles() {
        for id in vsr_languages::bundled::folding_rule_language_ids() {
            if let Some(m) = vsr_languages::bundled::folding_rules(id).and_then(|r| r.markers.as_ref()) {
                assert!(vsr_editor::folding::FoldingMarkers::new(&m.start, &m.end).is_some(), "{id}");
            }
        }
    }
}
