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
    pub const EDITOR_FONT_SIZE: f32 = 12.0; // macOS default editor.fontSize
    pub const EDITOR_LINE_HEIGHT: f32 = 18.0;
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
