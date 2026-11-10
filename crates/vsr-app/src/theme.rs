//! Workbench colors of the active theme.
//!
//! upstream: extensions/theme-defaults/themes/dark_modern.json @ 50e1350fc78d
//!
//! This file is the mapping of what rscode paints (the [`Theme`] fields) to VS Code colour keys:
//! a field is the first of its keys the active theme has (customisation, theme file, registry
//! default for the theme type), so a switch of theme or a `workbench.colorCustomizations` entry
//! reaches every surface. The theme itself (JSON loading, include chains, defaults) is
//! `vsr-theme`; the active theme and the settings are `theme_service.rs`. ADR-0006, ADR-0037.

use vsr_theme::{Color, ColorTheme};
pub use vsr_theme::{Rgba8, hex};

/// A colour that is not set in the theme and has no default: painted as nothing (VS Code draws
/// no border or background for such a colour either).
const UNSET: Rgba8 = Rgba8(0, 0, 0, 0);

macro_rules! theme_fields {
    ($($field:ident: [$($key:literal),+],)+) => {
        /// What the workbench paints with. Built from a [`ColorTheme`]; see `KEYS` for the colour
        /// key of every field.
        #[derive(Debug, Clone, PartialEq)]
        pub struct Theme {
            $(pub $field: Rgba8,)+
        }

        impl Theme {
            /// The colour keys each field is read from (first one the theme has wins), by field name.
            #[cfg(test)]
            pub const KEYS: &'static [(&'static str, &'static [&'static str])] = &[$((stringify!($field), &[$($key),+])),+];

            /// Every field by name (for tests and diagnostics).
            #[cfg(test)]
            pub fn fields(&self) -> Vec<(&'static str, Rgba8)> {
                vec![$((stringify!($field), self.$field)),+]
            }

            /// Reads every field from `theme` (customisations applied).
            pub fn from_color_theme(theme: &ColorTheme) -> Self {
                Self { $($field: first_color(theme, &[$($key),+]),)+ }
            }
        }
    };
}

fn first_color(theme: &ColorTheme, keys: &[&str]) -> Rgba8 {
    keys.iter().find_map(|k| theme.color(k)).map_or(UNSET, |c: Color| c.to_rgba8())
}

theme_fields! {
    editor_bg: ["editor.background"],
    editor_fg: ["editor.foreground"],
    line_number_fg: ["editorLineNumber.foreground"],
    line_number_active_fg: ["editorLineNumber.activeForeground"],
    activity_bar_bg: ["activityBar.background"],
    side_bar_bg: ["sideBar.background"],
    title_bar_bg: ["titleBar.activeBackground"],
    status_bar_bg: ["statusBar.background"],
    tab_strip_bg: ["editorGroupHeader.tabsBackground"],
    tab_inactive_bg: ["tab.inactiveBackground"],
    activity_bar_border: ["activityBar.border"],
    side_bar_border: ["sideBar.border"],
    title_bar_border: ["titleBar.border"],
    status_bar_border: ["statusBar.border"],
    tabs_border: ["editorGroupHeader.tabsBorder"],
    tab_border: ["tab.border"],
    breadcrumb_bg: ["breadcrumb.background"],  // the breadcrumbs bar (ADR-0047)
    breadcrumb_fg: ["breadcrumb.foreground"],
    breadcrumb_focus_fg: ["breadcrumb.focusForeground"],
    breadcrumb_active_fg: ["breadcrumb.activeSelectionForeground"],
    side_bar_fg: ["sideBar.foreground", "foreground"],  // sideBar.foreground, else foreground (inherited)
    title_bar_fg: ["titleBar.activeForeground"],
    status_bar_fg: ["statusBar.foreground"],
    activity_fg: ["activityBar.foreground"],
    activity_inactive_fg: ["activityBar.inactiveForeground"],
    activity_active_border: ["activityBar.activeBorder"],
    tab_active_border_top: ["tab.activeBorderTop"],
    sash_hover_border: ["sash.hoverBorder"],
    tab_active_bg: ["tab.activeBackground"],
    tab_active_fg: ["tab.activeForeground"],
    tab_inactive_fg: ["tab.inactiveForeground"],
    tab_active_modified_border: ["tab.activeModifiedBorder"],  // top border of a modified tab with highlightModifiedTabs
    tab_inactive_modified_border: ["tab.inactiveModifiedBorder"],
    list_hover_bg: ["list.hoverBackground"],
    list_active_bg: ["list.inactiveSelectionBackground"],
    error_fg: ["editorError.foreground"],
    foreground: ["foreground"],  // Settings editor (settingsEditor2)
    description_fg: ["descriptionForeground"],
    focus_border: ["focusBorder"],
    settings_header_fg: ["settings.headerForeground"],
    settings_modified_indicator: ["settings.modifiedItemIndicator"],
    settings_focused_row_bg: ["settings.focusedRowBackground"],
    settings_row_hover_bg: ["settings.rowHoverBackground"],
    settings_dropdown_bg: ["settings.dropdownBackground"],
    settings_dropdown_border: ["settings.dropdownBorder"],
    settings_dropdown_fg: ["settings.dropdownForeground"],
    settings_dropdown_list_bg: ["dropdown.listBackground"],
    settings_checkbox_bg: ["settings.checkboxBackground"],
    settings_checkbox_border: ["settings.checkboxBorder"],
    settings_checkbox_fg: ["settings.checkboxForeground"],
    settings_text_input_bg: ["settings.textInputBackground"],
    settings_number_input_bg: ["settings.numberInputBackground"],
    input_fg: ["input.foreground"],
    input_border: ["input.border"],
    input_placeholder_fg: ["input.placeholderForeground"],
    settings_header_border: ["settings.headerBorder"],
    text_link_fg: ["textLink.foreground"],
    text_link_active_fg: ["textLink.activeForeground"],
    text_preformat_fg: ["textPreformat.foreground"],
    text_preformat_bg: ["textPreformat.background"],
    setting_error_fg: ["errorForeground"],
    validation_error_bg: ["inputValidation.errorBackground"],
    validation_error_border: ["inputValidation.errorBorder"],
    menu_bg: ["menu.background"],
    menu_selection_bg: ["menu.selectionBackground"],
    menu_border: ["menu.border"],
    panel_border: ["panel.border"],
    cursor: ["editorCursor.foreground"],
    selection_bg: ["editor.selectionBackground"],
    inactive_selection_bg: ["editor.inactiveSelectionBackground"],
    line_highlight_border: ["editor.lineHighlightBorder"],
    whitespace_fg: ["editorWhitespace.foreground"],
    ruler_fg: ["editorRuler.foreground"],
    indent_guide_fg: ["editorIndentGuide.background1"],
    indent_guide_active_fg: ["editorIndentGuide.activeBackground1"],
    bracket_match_bg: ["editorBracketMatch.background"],
    bracket_match_border: ["editorBracketMatch.border"],
    selection_highlight_bg: ["editor.selectionHighlightBackground"],
    word_highlight_bg: ["editor.wordHighlightBackground"],
    fold_bg: ["editor.foldBackground"],  // folding (ADR-0042)
    fold_placeholder_fg: ["editor.foldPlaceholderForeground"],
    folding_control_fg: ["editorGutter.foldingControlForeground"],
    input_option_active_bg: ["inputOption.activeBackground"],  // Search view (searchview.css, inputOption.*, badge.*)
    input_option_active_border: ["inputOption.activeBorder"],
    input_option_active_fg: ["inputOption.activeForeground"],
    input_option_hover_bg: ["inputOption.hoverBackground"],
    list_active_selection_bg: ["list.activeSelectionBackground"],
    list_active_selection_fg: ["list.activeSelectionForeground"],
    find_match_highlight_bg: ["editor.findMatchHighlightBackground"],
    diff_removed_text_bg: ["diffEditor.removedTextBackground"],  // replaced text in Search
    diff_inserted_text_bg: ["diffEditor.insertedTextBackground"],  // replacement in Search
    diff_inserted_line_bg: ["diffEditor.insertedLineBackground"],  // replacement border
    button_bg: ["button.background"],
    button_fg: ["button.foreground"],
    button_hover_bg: ["button.hoverBackground"],
    button_secondary_bg: ["button.secondaryBackground"],
    button_secondary_fg: ["button.secondaryForeground"],
    button_secondary_hover_bg: ["button.secondaryHoverBackground"],
    button_border: ["button.border"],
    search_results_info_fg: ["search.resultsInfoForeground"],
    badge_bg: ["badge.background"],
    badge_fg: ["badge.foreground"],
    toolbar_hover_bg: ["toolbar.hoverBackground"],
    quick_input_bg: ["quickInput.background"],  // Quick input (quickInput.css, quickpickColors.ts)
    quick_input_fg: ["quickInput.foreground"],
    quick_input_list_focus_bg: ["quickInputList.focusBackground"],
    quick_input_list_focus_fg: ["quickInputList.focusForeground"],
    picker_group_fg: ["pickerGroup.foreground"],  // used by the Command Palette's group labels
    picker_group_border: ["pickerGroup.border"],
    widget_shadow: ["widget.shadow"],
    widget_border: ["widget.border"],
    list_highlight_fg: ["list.highlightForeground"],
    list_focus_highlight_fg: ["list.focusHighlightForeground"],
    keybinding_label_bg: ["keybindingLabel.background"],  // Key binding labels (keybindingLabel.css, inputColors.ts)
    keybinding_label_fg: ["keybindingLabel.foreground"],
    keybinding_label_border: ["keybindingLabel.border"],
    keybinding_label_bottom_border: ["keybindingLabel.bottomBorder"],
    editor_widget_bg: ["editorWidget.background"],  // Find widget (editorColors.ts)
    disabled_fg: ["disabledForeground"],
    find_match_bg: ["editor.findMatchBackground"],
    find_range_highlight_bg: ["editor.findRangeHighlightBackground"],
}

impl Theme {
    /// The bundled theme with the given settings id, without customisations.
    fn bundled(id: &str) -> Self {
        let catalog = vsr_theme::ThemeCatalog::builtin();
        let theme = catalog.find(id, None).and_then(|t| catalog.load(t).ok()).expect("the bundled theme loads");
        Self::from_color_theme(&theme)
    }

    /// The default dark theme without customisations: what a fresh install shows (Dark 2026,
    /// ADR-0049). Code that has an app context uses `ThemeService::theme(cx)` instead.
    pub fn default_dark() -> Self {
        Self::bundled(vsr_theme::DEFAULT_DARK)
    }

    /// The bundled "Dark Modern" theme, whatever the default is.
    pub fn dark_modern() -> Self {
        Self::bundled("Dark Modern")
    }
}

/// Classic workbench metrics (ADR-0006).
pub mod metrics {
    pub const TITLE_BAR_HEIGHT: f32 = 35.0;
    /// Editor tab height with `window.density.editorTabHeight: compact` (upstream: 22px).
    pub const COMPACT_TAB_HEIGHT: f32 = 22.0;
    /// Width of the command center search box in the title bar (`window.commandCenter`).
    pub const COMMAND_CENTER_WIDTH: f32 = 420.0;
    pub const ACTIVITY_BAR_WIDTH: f32 = 48.0;
    pub const SIDEBAR_WIDTH: f32 = 260.0;
    pub const TAB_HEIGHT: f32 = 35.0;
    pub const STATUS_BAR_HEIGHT: f32 = 22.0;
    /// The breadcrumbs bar under the tab strip (upstream breadcrumbsControl.css: 22px).
    pub const BREADCRUMBS_HEIGHT: f32 = 22.0;
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_theme::ThemeCatalog;

    #[test]
    fn parses_rgb_and_rgba_hex() {
        assert_eq!(hex("#1F1F1F"), Some(Rgba8::rgb(0x1F, 0x1F, 0x1F)));
        assert_eq!(hex("#0078D4"), Some(Rgba8::rgb(0, 0x78, 0xD4)));
        assert_eq!(hex("#FFFFFF80"), Some(Rgba8(255, 255, 255, 0x80)));
        assert_eq!(hex("#FA0"), Some(Rgba8::rgb(0xFF, 0xAA, 0x00)));
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(hex("1F1F1F"), None);
        assert_eq!(hex("#1F1F1"), None);
        assert_eq!(hex("#GG0000"), None);
    }

    #[test]
    fn packs_rgba() {
        assert_eq!(Rgba8(0x12, 0x34, 0x56, 0x78).to_u32(), 0x1234_5678);
    }

    /// The colours the app painted before the theme service existed (the hard-coded Dark Modern of
    /// `theme.rs`), by field. The JSON theme must reproduce every one of them.
    const LEGACY_DARK_MODERN: &[(&str, &str)] = &[
        ("editor_bg", "#1F1F1F"),
        ("editor_fg", "#CCCCCC"),
        ("line_number_fg", "#6E7681"),
        ("line_number_active_fg", "#CCCCCC"),
        ("activity_bar_bg", "#181818"),
        ("side_bar_bg", "#181818"),
        ("title_bar_bg", "#181818"),
        ("status_bar_bg", "#181818"),
        ("tab_strip_bg", "#181818"),
        ("tab_inactive_bg", "#181818"),
        ("activity_bar_border", "#2B2B2B"),
        ("side_bar_border", "#2B2B2B"),
        ("title_bar_border", "#2B2B2B"),
        ("status_bar_border", "#2B2B2B"),
        ("tabs_border", "#2B2B2B"),
        ("tab_border", "#2B2B2B"),
        ("side_bar_fg", "#CCCCCC"),
        ("title_bar_fg", "#CCCCCC"),
        ("status_bar_fg", "#CCCCCC"),
        ("activity_fg", "#D7D7D7"),
        ("activity_inactive_fg", "#868686"),
        ("activity_active_border", "#0078D4"),
        ("tab_active_border_top", "#0078D4"),
        ("sash_hover_border", "#0078D4"),
        ("tab_active_bg", "#1F1F1F"),
        ("tab_active_fg", "#FFFFFF"),
        ("tab_inactive_fg", "#9D9D9D"),
        ("tab_active_modified_border", "#3399CC"),
        ("tab_inactive_modified_border", "#3399CC80"),
        ("list_hover_bg", "#2A2D2E"),
        ("list_active_bg", "#37373D"),
        ("error_fg", "#F14C4C"),
        ("foreground", "#CCCCCC"),
        ("description_fg", "#9D9D9D"),
        ("focus_border", "#0078D4"),
        ("settings_header_fg", "#FFFFFF"),
        ("settings_modified_indicator", "#BB800966"),
        ("settings_focused_row_bg", "#2A2D2E99"),
        ("settings_row_hover_bg", "#2A2D2E4D"),
        ("settings_dropdown_bg", "#313131"),
        ("settings_dropdown_border", "#3C3C3C"),
        ("settings_dropdown_fg", "#CCCCCC"),
        ("settings_dropdown_list_bg", "#1F1F1F"),
        ("settings_checkbox_bg", "#313131"),
        ("settings_checkbox_border", "#3C3C3C"),
        ("settings_checkbox_fg", "#CCCCCC"),
        ("settings_text_input_bg", "#313131"),
        ("settings_number_input_bg", "#313131"),
        ("input_fg", "#CCCCCC"),
        ("input_border", "#3C3C3C"),
        ("input_placeholder_fg", "#989898"),
        ("settings_header_border", "#2B2B2B"),
        ("text_link_fg", "#4DAAFC"),
        ("text_link_active_fg", "#4DAAFC"),
        ("text_preformat_fg", "#D0D0D0"),
        ("text_preformat_bg", "#3C3C3C"),
        ("setting_error_fg", "#F85149"),
        ("validation_error_bg", "#5A1D1D"),
        ("validation_error_border", "#BE1100"),
        ("menu_bg", "#1F1F1F"),
        ("menu_selection_bg", "#0078D4"),
        ("menu_border", "#454545"),
        ("panel_border", "#2B2B2B"), // added after the golden snapshot (dark_modern.json)
        ("cursor", "#AEAFAD"),
        ("selection_bg", "#264F78"),
        ("inactive_selection_bg", "#3A3D41"),
        ("line_highlight_border", "#282828"),
        ("whitespace_fg", "#E3E4E229"),
        ("ruler_fg", "#5A5A5A"),
        ("indent_guide_fg", "#404040"),
        ("indent_guide_active_fg", "#707070"),
        ("bracket_match_bg", "#0064001A"),
        ("bracket_match_border", "#888888"),
        ("selection_highlight_bg", "#ADD6FF26"),
        ("word_highlight_bg", "#575757B8"),
        ("fold_bg", "#264F784D"),
        ("fold_placeholder_fg", "#808080"),
        ("folding_control_fg", "#CCCCCC"),
        ("input_option_active_bg", "#2489DB82"),
        ("input_option_active_border", "#2488DB"),
        ("input_option_active_fg", "#FFFFFF"),
        ("input_option_hover_bg", "#5A5D5E80"),
        ("list_active_selection_bg", "#04395E"),
        ("list_active_selection_fg", "#FFFFFF"),
        ("find_match_highlight_bg", "#EA5C0055"),
        ("diff_removed_text_bg", "#FF000033"),
        ("diff_inserted_text_bg", "#9CCC2C33"),
        ("diff_inserted_line_bg", "#9BB95533"),
        ("button_bg", "#0078D4"),
        ("button_fg", "#FFFFFF"),
        ("button_hover_bg", "#026EC1"),
        ("button_secondary_bg", "#00000000"), // added after the golden snapshot (dark_modern.json)
        ("button_secondary_fg", "#CCCCCC"),
        ("button_secondary_hover_bg", "#2B2B2B"),
        ("button_border", "#FFFFFF1A"),
        ("search_results_info_fg", "#CCCCCCA6"),
        ("badge_bg", "#616161"),
        ("badge_fg", "#F8F8F8"),
        ("toolbar_hover_bg", "#5A5D5E50"),
        ("quick_input_bg", "#222222"),
        ("quick_input_fg", "#CCCCCC"),
        ("quick_input_list_focus_bg", "#04395E"),
        ("quick_input_list_focus_fg", "#FFFFFF"),
        ("picker_group_fg", "#3794FF"),
        ("picker_group_border", "#3C3C3C"),
        ("widget_shadow", "#0000005C"),
        ("widget_border", "#313131"),
        ("list_highlight_fg", "#2AAAFF"),
        ("list_focus_highlight_fg", "#2AAAFF"),
        ("keybinding_label_bg", "#8080802B"),
        ("keybinding_label_fg", "#CCCCCC"),
        ("keybinding_label_border", "#33333399"),
        ("keybinding_label_bottom_border", "#44444499"),
        ("editor_widget_bg", "#202020"),
        ("disabled_fg", "#CCCCCC80"),
        ("find_match_bg", "#9E6A03"),
        ("find_range_highlight_bg", "#3A3D4166"),
    ];

    #[test]
    fn dark_modern_from_the_json_equals_the_old_hard_coded_theme() {
        let theme = Theme::dark_modern();
        let actual = theme.fields();
        for (legacy_name, legacy) in LEGACY_DARK_MODERN {
            let (_, got) = actual.iter().find(|(name, _)| name == legacy_name).expect("a theme field");
            assert_eq!(*got, hex(legacy).unwrap(), "{legacy_name}: {got:?} != {legacy}");
        }
        // Fields added later (the breadcrumbs, ADR-0047) were never painted before, so the legacy
        // table has no colour for them: the theme must still set each one.
        for (name, got) in actual.iter().filter(|(name, _)| name.starts_with("breadcrumb")) {
            assert!(got.3 > 0, "{name} is not set by Dark Modern");
        }
    }

    #[test]
    fn every_key_is_a_registered_colour_or_set_by_a_bundled_theme() {
        let catalog = ThemeCatalog::builtin();
        let themes: Vec<_> = catalog.themes().iter().map(|t| catalog.load(t).unwrap()).collect();
        for (field, keys) in Theme::KEYS {
            for key in *keys {
                assert!(
                    vsr_theme::registry().contains(key) || themes.iter().any(|t| t.defines(key)),
                    "{field}: {key} is neither in the colour registry nor set by a bundled theme"
                );
            }
        }
    }

    #[test]
    fn every_bundled_theme_builds_a_theme_with_visible_text_colours() {
        let catalog = ThemeCatalog::builtin();
        for t in catalog.themes() {
            let theme = Theme::from_color_theme(&catalog.load(t).unwrap());
            for (name, c) in [("editor_fg", theme.editor_fg), ("foreground", theme.foreground), ("side_bar_fg", theme.side_bar_fg), ("tab_active_fg", theme.tab_active_fg)] {
                assert!(c.3 > 0, "{}: {name} is transparent", t.settings_id);
            }
            assert_eq!(theme.editor_bg.3, 0xFF, "{}: the editor background is opaque", t.settings_id);
        }
    }

    #[test]
    fn light_modern_differs_from_dark_modern() {
        let catalog = ThemeCatalog::builtin();
        let light = Theme::from_color_theme(&catalog.load(catalog.find("Light Modern", None).unwrap()).unwrap());
        assert_ne!(light.editor_bg, Theme::dark_modern().editor_bg);
        assert_eq!(light.editor_bg, Rgba8::rgb(0xFF, 0xFF, 0xFF));
    }
}
