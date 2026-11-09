//! Workbench colors for the first window.
//!
//! upstream: extensions/theme-defaults/themes/dark_modern.json @ 50e1350fc78d
//!
//! Hard-coded for now; a real theme service (JSON themes, `include` chains,
//! token colors) belongs in `vsr-platform` later. See ADR-0006.

use eframe::egui::Color32;

/// Parse `#RRGGBB` or `#RRGGBBAA` (the formats VS Code theme files use).
pub fn hex(s: &str) -> Option<Color32> {
    let h = s.strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Color32::from_rgb(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color32::from_rgba_unmultiplied(byte(0)?, byte(2)?, byte(4)?, byte(6)?)),
        _ => None,
    }
}

fn c(s: &str) -> Color32 {
    hex(s).expect("valid built-in color")
}

#[derive(Debug, Clone)]
pub struct Theme {
    pub editor_bg: Color32,
    pub editor_fg: Color32,
    pub line_number_fg: Color32,
    pub line_number_active_fg: Color32,
    pub chrome_bg: Color32, // activity bar, side bar, title bar, status bar, tab strip
    pub border: Color32,
    pub sidebar_fg: Color32,
    pub activity_fg: Color32,
    pub activity_inactive_fg: Color32,
    pub accent: Color32, // focusBorder / activeBorder / tab.activeBorderTop
    pub tab_active_bg: Color32,
    pub tab_active_fg: Color32,
    pub tab_inactive_fg: Color32,
    pub list_hover_bg: Color32,
    pub list_active_bg: Color32,
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
        assert_eq!(hex("#1F1F1F"), Some(Color32::from_rgb(0x1F, 0x1F, 0x1F)));
        assert_eq!(hex("#0078D4"), Some(Color32::from_rgb(0, 0x78, 0xD4)));
        assert_eq!(
            hex("#FFFFFF80"),
            Some(Color32::from_rgba_unmultiplied(255, 255, 255, 0x80))
        );
    }

    #[test]
    fn rejects_malformed_hex() {
        assert_eq!(hex("1F1F1F"), None);
        assert_eq!(hex("#1F1F"), None);
        assert_eq!(hex("#GG0000"), None);
    }

    #[test]
    fn dark_modern_builds() {
        assert_eq!(Theme::dark_modern().editor_bg, Color32::from_rgb(0x1F, 0x1F, 0x1F));
    }
}
