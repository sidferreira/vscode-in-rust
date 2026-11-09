//! Workbench colors for the first window.
//!
//! upstream: extensions/theme-defaults/themes/dark_modern.json @ 50e1350fc78d
//!
//! Hard-coded for now; a real theme service (JSON themes, `include` chains,
//! token colors) belongs in `vsr-platform` later. See ADR-0006.

/// UI-agnostic color: 8-bit RGBA (straight alpha). The view layer converts it
/// (see `ui::to_gpui`), so a framework switch never touches this file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

/// Parse `#RRGGBB` or `#RRGGBBAA` (the formats VS Code theme files use).
pub fn hex(s: &str) -> Option<Rgba8> {
    let h = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Rgba8::rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Rgba8(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

fn c(s: &str) -> Rgba8 {
    hex(s).expect("valid built-in color")
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub editor_bg: Rgba8,
    pub editor_fg: Rgba8,
    pub line_number_fg: Rgba8,
    pub line_number_active_fg: Rgba8,
    pub chrome_bg: Rgba8, // activity bar, side bar, title bar, status bar, tab strip
    pub border: Rgba8,
    pub sidebar_fg: Rgba8,
    pub activity_fg: Rgba8,
    pub activity_inactive_fg: Rgba8,
    pub accent: Rgba8, // focusBorder / activeBorder / tab.activeBorderTop
    pub tab_active_bg: Rgba8,
    pub tab_active_fg: Rgba8,
    pub tab_inactive_fg: Rgba8,
    pub list_hover_bg: Rgba8,
    pub list_active_bg: Rgba8,
    pub error_fg: Rgba8,
    // ---- Settings editor (settingsEditor2) ----
    pub foreground: Rgba8,                    // foreground
    pub description_fg: Rgba8,                // descriptionForeground
    pub focus_border: Rgba8,                  // focusBorder
    pub settings_header_fg: Rgba8,            // settings.headerForeground
    pub settings_modified_indicator: Rgba8,   // settings.modifiedItemIndicator
    pub settings_focused_row_bg: Rgba8,       // settings.focusedRowBackground (list.hoverBackground @ 60%)
    pub settings_row_hover_bg: Rgba8,         // settings.rowHoverBackground (list.hoverBackground @ 30%)
    pub settings_dropdown_bg: Rgba8,          // settings.dropdownBackground
    pub settings_dropdown_border: Rgba8,      // settings.dropdownBorder
    pub settings_dropdown_fg: Rgba8,          // dropdown.foreground
    pub settings_dropdown_list_bg: Rgba8,     // dropdown.listBackground
    pub settings_checkbox_bg: Rgba8,          // settings.checkboxBackground (checkbox.background)
    pub settings_checkbox_border: Rgba8,      // settings.checkboxBorder (checkbox.border)
    pub settings_checkbox_fg: Rgba8,          // settings.checkboxForeground (checkbox.foreground)
    pub settings_text_input_bg: Rgba8,        // settings.textInputBackground (input.background)
    pub settings_number_input_bg: Rgba8,      // settings.numberInputBackground (input.background)
    pub input_fg: Rgba8,                      // input.foreground
    pub input_border: Rgba8,                  // input.border
    pub input_placeholder_fg: Rgba8,          // input.placeholderForeground
    pub settings_header_border: Rgba8,        // settings.headerBorder (panel.border)
    pub text_link_fg: Rgba8,                  // textLink.foreground
    pub text_link_active_fg: Rgba8,           // textLink.activeForeground
    pub text_preformat_fg: Rgba8,             // textPreformat.foreground
    pub text_preformat_bg: Rgba8,             // textPreformat.background
    pub setting_error_fg: Rgba8,              // errorForeground
    pub validation_error_bg: Rgba8,           // inputValidation.errorBackground
    pub validation_error_border: Rgba8,       // inputValidation.errorBorder
    pub menu_bg: Rgba8,                       // menu.background
    pub menu_selection_bg: Rgba8,             // menu.selectionBackground
    pub menu_border: Rgba8,                   // menu.border
    pub cursor: Rgba8,                 // editorCursor.foreground
    pub selection_bg: Rgba8,           // editor.selectionBackground
    pub inactive_selection_bg: Rgba8,  // editor.inactiveSelectionBackground
    pub line_highlight_border: Rgba8,  // editor.lineHighlightBorder
    pub whitespace_fg: Rgba8,          // editorWhitespace.foreground (registry dark)
    pub ruler_fg: Rgba8,               // editorRuler.foreground (registry dark)
    pub indent_guide_fg: Rgba8,        // editorIndentGuide.background1 (dark_vs.json)
    pub indent_guide_active_fg: Rgba8, // editorIndentGuide.activeBackground1 (dark_vs.json)
    pub bracket_match_bg: Rgba8,       // editorBracketMatch.background (registry dark)
    pub bracket_match_border: Rgba8,   // editorBracketMatch.border (registry dark)
    pub selection_highlight_bg: Rgba8, // editor.selectionHighlightBackground (dark_vs.json)
    pub word_highlight_bg: Rgba8,      // editor.wordHighlightBackground (registry dark)
    // ---- Search view (searchview.css, inputOption.*, badge.*) ----
    pub input_option_active_bg: Rgba8,        // inputOption.activeBackground (dark_modern)
    pub input_option_active_border: Rgba8,    // inputOption.activeBorder (dark_modern)
    pub input_option_active_fg: Rgba8,        // inputOption.activeForeground (registry dark)
    pub input_option_hover_bg: Rgba8,         // inputOption.hoverBackground (registry dark)
    pub list_active_selection_bg: Rgba8,      // list.activeSelectionBackground (registry dark)
    pub list_active_selection_fg: Rgba8,      // list.activeSelectionForeground (registry dark)
    pub find_match_highlight_bg: Rgba8,       // editor.findMatchHighlightBackground (registry dark)
    pub diff_removed_text_bg: Rgba8,          // diffEditor.removedTextBackground (registry dark): replaced text in Search
    pub diff_inserted_text_bg: Rgba8,         // diffEditor.insertedTextBackground (registry dark): replacement in Search
    pub diff_inserted_line_bg: Rgba8,         // diffEditor.insertedLineBackground (registry dark): replacement border
    pub button_bg: Rgba8,                     // button.background (dark_modern)
    pub button_fg: Rgba8,                     // button.foreground (dark_modern)
    pub button_hover_bg: Rgba8,               // button.hoverBackground (dark_modern)
    pub button_secondary_fg: Rgba8,           // button.secondaryForeground (dark_modern)
    pub button_secondary_hover_bg: Rgba8,     // button.secondaryHoverBackground (dark_modern)
    pub button_border: Rgba8,                 // button.border (dark_modern)
    pub search_results_info_fg: Rgba8,        // search.resultsInfoForeground (foreground @ 65%)
    pub badge_bg: Rgba8,                      // badge.background (dark_modern)
    pub badge_fg: Rgba8,                      // badge.foreground (dark_modern)
    pub toolbar_hover_bg: Rgba8,              // toolbar.hoverBackground (registry dark)
    // ---- Quick input (quickInput.css, quickpickColors.ts) ----
    pub quick_input_bg: Rgba8,                // quickInput.background (dark_modern)
    pub quick_input_fg: Rgba8,                // quickInput.foreground (dark_modern)
    pub quick_input_list_focus_bg: Rgba8,     // quickInputList.focusBackground (= list.activeSelectionBackground)
    pub quick_input_list_focus_fg: Rgba8,     // quickInputList.focusForeground (= list.activeSelectionForeground)
    #[allow(dead_code)] // used by the Command Palette's group labels
    pub picker_group_fg: Rgba8,               // pickerGroup.foreground (registry dark)
    pub picker_group_border: Rgba8,           // pickerGroup.border (dark_modern)
    pub widget_shadow: Rgba8,                 // widget.shadow (registry dark: black @ 36%)
    pub widget_border: Rgba8,                 // widget.border (dark_modern)
    pub list_highlight_fg: Rgba8,             // list.highlightForeground (registry dark)
    pub list_focus_highlight_fg: Rgba8,       // list.focusHighlightForeground (= list.highlightForeground)
    // ---- Key binding labels (keybindingLabel.css, inputColors.ts) ----
    pub keybinding_label_bg: Rgba8,           // keybindingLabel.background (registry dark: #808080 @ 17%)
    pub keybinding_label_fg: Rgba8,           // keybindingLabel.foreground (dark_modern)
    pub keybinding_label_border: Rgba8,       // keybindingLabel.border (registry dark: #333333 @ 60%)
    pub keybinding_label_bottom_border: Rgba8, // keybindingLabel.bottomBorder (registry dark: #444444 @ 60%)
}

impl Theme {
    pub fn dark_modern() -> Self {
        Self {
            editor_bg: c("#1F1F1F"),
            editor_fg: c("#CCCCCC"),
            line_number_fg: c("#6E7681"),
            line_number_active_fg: c("#CCCCCC"),
            chrome_bg: c("#181818"),
            border: c("#2B2B2B"),
            sidebar_fg: c("#CCCCCC"),
            activity_fg: c("#D7D7D7"),
            activity_inactive_fg: c("#868686"),
            accent: c("#0078D4"),
            tab_active_bg: c("#1F1F1F"),
            tab_active_fg: c("#FFFFFF"),
            tab_inactive_fg: c("#9D9D9D"),
            list_hover_bg: c("#2A2D2E"),
            list_active_bg: c("#37373D"),
            error_fg: c("#F14C4C"),
            foreground: c("#CCCCCC"),
            description_fg: c("#9D9D9D"),
            focus_border: c("#0078D4"),
            settings_header_fg: c("#FFFFFF"),
            settings_modified_indicator: c("#BB800966"),
            settings_focused_row_bg: c("#2A2D2E99"),
            settings_row_hover_bg: c("#2A2D2E4D"),
            settings_dropdown_bg: c("#313131"),
            settings_dropdown_border: c("#3C3C3C"),
            settings_dropdown_fg: c("#CCCCCC"),
            settings_dropdown_list_bg: c("#1F1F1F"),
            settings_checkbox_bg: c("#313131"),
            settings_checkbox_border: c("#3C3C3C"),
            settings_checkbox_fg: c("#CCCCCC"),
            settings_text_input_bg: c("#313131"),
            settings_number_input_bg: c("#313131"),
            input_fg: c("#CCCCCC"),
            input_border: c("#3C3C3C"),
            input_placeholder_fg: c("#989898"),
            settings_header_border: c("#2B2B2B"),
            text_link_fg: c("#4DAAFC"),
            text_link_active_fg: c("#4DAAFC"),
            text_preformat_fg: c("#D0D0D0"),
            text_preformat_bg: c("#3C3C3C"),
            setting_error_fg: c("#F85149"),
            validation_error_bg: c("#5A1D1D"),
            validation_error_border: c("#BE1100"),
            menu_bg: c("#1F1F1F"),
            menu_selection_bg: c("#0078D4"),
            menu_border: c("#454545"),
            // Registry defaults for dark themes (editorColorRegistry.ts / editorColors.ts); dark_vs
            // overrides the inactive selection.
            cursor: c("#AEAFAD"),
            selection_bg: c("#264F78"),
            inactive_selection_bg: c("#3A3D41"),
            line_highlight_border: c("#282828"),
            whitespace_fg: c("#E3E4E229"),
            ruler_fg: c("#5A5A5A"),
            indent_guide_fg: c("#404040"),
            indent_guide_active_fg: c("#707070"),
            bracket_match_bg: c("#0064001A"),
            bracket_match_border: c("#888888"),
            selection_highlight_bg: c("#ADD6FF26"),
            word_highlight_bg: c("#575757B8"),
            input_option_active_bg: c("#2489DB82"),
            input_option_active_border: c("#2488DB"),
            input_option_active_fg: c("#FFFFFF"),
            input_option_hover_bg: c("#5A5D5E80"),
            list_active_selection_bg: c("#04395E"),
            list_active_selection_fg: c("#FFFFFF"),
            find_match_highlight_bg: c("#EA5C0055"),
            diff_removed_text_bg: c("#FF000033"),
            diff_inserted_text_bg: c("#9CCC2C33"),
            diff_inserted_line_bg: c("#9BB95533"),
            button_bg: c("#0078D4"),
            button_fg: c("#FFFFFF"),
            button_hover_bg: c("#026EC1"),
            button_secondary_fg: c("#CCCCCC"),
            button_secondary_hover_bg: c("#2B2B2B"),
            button_border: c("#FFFFFF1A"),
            search_results_info_fg: c("#CCCCCCA6"),
            badge_bg: c("#616161"),
            badge_fg: c("#F8F8F8"),
            toolbar_hover_bg: c("#5A5D5E50"),
            quick_input_bg: c("#222222"),
            quick_input_fg: c("#CCCCCC"),
            quick_input_list_focus_bg: c("#04395E"),
            quick_input_list_focus_fg: c("#FFFFFF"),
            picker_group_fg: c("#3794FF"),
            picker_group_border: c("#3C3C3C"),
            widget_shadow: c("#0000005C"),
            widget_border: c("#313131"),
            list_highlight_fg: c("#2AAAFF"),
            list_focus_highlight_fg: c("#2AAAFF"),
            keybinding_label_bg: c("#8080802B"),
            keybinding_label_fg: c("#CCCCCC"),
            keybinding_label_border: c("#33333399"),
            keybinding_label_bottom_border: c("#44444499"),
        }
    }
}

/// Classic workbench metrics (ADR-0006).
pub mod metrics {
    pub const TITLE_BAR_HEIGHT: f32 = 35.0;
    pub const ACTIVITY_BAR_WIDTH: f32 = 48.0;
    pub const SIDEBAR_WIDTH: f32 = 260.0;
    pub const TAB_HEIGHT: f32 = 35.0;
    pub const STATUS_BAR_HEIGHT: f32 = 22.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rgb_and_rgba_hex() {
        assert_eq!(hex("#1F1F1F"), Some(Rgba8::rgb(0x1F, 0x1F, 0x1F)));
        assert_eq!(hex("#0078D4"), Some(Rgba8::rgb(0, 0x78, 0xD4)));
        assert_eq!(
            hex("#FFFFFF80"),
            Some(Rgba8(255, 255, 255, 0x80))
        );
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(hex("1F1F1F"), None);
        assert_eq!(hex("#1F1F"), None);
        assert_eq!(hex("#GG0000"), None);
    }

    #[test]
    fn packs_rgba() {
        assert_eq!(Rgba8(0x12, 0x34, 0x56, 0x78).to_u32(), 0x1234_5678);
    }

    #[test]
    fn dark_modern_builds() {
        assert_eq!(Theme::dark_modern().editor_bg, Rgba8::rgb(0x1F, 0x1F, 0x1F));
    }
}
