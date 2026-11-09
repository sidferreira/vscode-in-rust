//! Typed views of the settings the workbench and the editor read. UI free: they are built from a
//! `Configuration` and compared with `PartialEq` to see what changed.
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/config/fontInfo.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/layout.ts @ 50e1350fc78d

use serde::Deserialize;
use serde_json::Value;
use vsr_editor::EditorOptions;
use vsr_settings::Configuration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineNumbers {
    On,
    Off,
    Relative,
    Interval,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LineHighlight {
    None,
    Gutter,
    Line,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorStyle {
    Line,
    Block,
    Underline,
    LineThin,
    BlockOutline,
    UnderlineThin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CursorBlinking {
    Blink,
    Smooth,
    Phase,
    Expand,
    Solid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ActivityBarLocation {
    Default,
    Top,
    Bottom,
    Hidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ShowTabs {
    Multiple,
    Single,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SideBarLocation {
    Left,
    Right,
}

/// Upstream `GOLDEN_LINE_HEIGHT_RATIO` on macOS (`fontInfo.ts`).
const GOLDEN_LINE_HEIGHT_RATIO: f64 = 1.5;
const MINIMUM_LINE_HEIGHT: f64 = 8.0;
/// Menlo's advance as a fraction of the font size (7.224px at 12px), used until real metrics exist.
const MENLO_ADVANCE_RATIO: f32 = 0.602;

/// `editor.lineHeight` to pixels, like `BareFontInfo._create`: 0 is 1.5 x font size, values below 8
/// are a multiple of the font size, the result is rounded and at least 8.
pub fn compute_line_height(line_height: f64, font_size: f64) -> f32 {
    let mut h = line_height;
    if h == 0.0 {
        h = GOLDEN_LINE_HEIGHT_RATIO * font_size;
    } else if h < MINIMUM_LINE_HEIGHT {
        h *= font_size;
    }
    h = h.round();
    if h < MINIMUM_LINE_HEIGHT {
        h = MINIMUM_LINE_HEIGHT;
    }
    h as f32
}

/// `editor.fontFamily` split on commas, quotes removed (upstream `EDITOR_FONT_DEFAULTS` use CSS lists).
pub fn parse_font_families(list: &str) -> Vec<String> {
    list.split(',')
        .map(|f| f.trim().trim_matches(|c| c == '\'' || c == '"').trim().to_string())
        .filter(|f| !f.is_empty())
        .collect()
}

/// `editor.fontWeight`: "normal", "bold", or a number from 1 to 1000. Anything else is "normal".
pub fn parse_font_weight(value: &Value) -> f32 {
    let n = match value {
        Value::String(s) => match s.trim() {
            "normal" => Some(400.0),
            "bold" => Some(700.0),
            other => other.parse::<f64>().ok(),
        },
        Value::Number(n) => n.as_f64(),
        _ => None,
    };
    n.filter(|n| (1.0..=1000.0).contains(n)).map_or(400.0, |n| n.round() as f32)
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorSettings {
    pub font_size: f32,
    /// The families in `editor.fontFamily`, in order.
    pub font_families: Vec<String>,
    /// The first family that is installed (set by `resolve_font`; the first listed until then).
    pub font_family: String,
    pub font_weight: f32,
    pub line_height: f32,
    pub tab_size: usize,
    pub insert_spaces: bool,
    pub line_numbers: LineNumbers,
    pub render_line_highlight: LineHighlight,
    pub cursor_style: CursorStyle,
    pub cursor_blinking: CursorBlinking,
    pub cursor_width: f32,
    pub trim_trailing_whitespace: bool,
    pub insert_final_newline: bool,
}

impl Default for EditorSettings {
    /// The registered defaults (macOS).
    fn default() -> Self {
        Self {
            font_size: 12.0,
            font_families: vec!["Menlo".into(), "Monaco".into(), "Courier New".into(), "monospace".into()],
            font_family: "Menlo".into(),
            font_weight: 400.0,
            line_height: 18.0,
            tab_size: 4,
            insert_spaces: true,
            line_numbers: LineNumbers::On,
            render_line_highlight: LineHighlight::Line,
            cursor_style: CursorStyle::Line,
            cursor_blinking: CursorBlinking::Blink,
            cursor_width: 0.0,
            trim_trailing_whitespace: false,
            insert_final_newline: false,
        }
    }
}

impl EditorSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let font_size = c.get_f64("editor.fontSize").unwrap_or(12.0);
        let families = parse_font_families(c.get_str("editor.fontFamily").unwrap_or_default());
        Self {
            font_size: font_size as f32,
            font_family: families.first().cloned().unwrap_or_else(|| "Menlo".into()),
            font_families: families,
            font_weight: c.get_value("editor.fontWeight").map_or(400.0, parse_font_weight),
            line_height: compute_line_height(c.get_f64("editor.lineHeight").unwrap_or(0.0), font_size),
            tab_size: c.get_f64("editor.tabSize").unwrap_or(4.0).round().max(1.0) as usize,
            insert_spaces: c.get_bool("editor.insertSpaces").unwrap_or(true),
            line_numbers: c.get_enum("editor.lineNumbers").unwrap_or(LineNumbers::On),
            render_line_highlight: c.get_enum("editor.renderLineHighlight").unwrap_or(LineHighlight::Line),
            cursor_style: c.get_enum("editor.cursorStyle").unwrap_or(CursorStyle::Line),
            cursor_blinking: c.get_enum("editor.cursorBlinking").unwrap_or(CursorBlinking::Blink),
            cursor_width: c.get_f64("editor.cursorWidth").unwrap_or(0.0) as f32,
            trim_trailing_whitespace: c.get_bool("files.trimTrailingWhitespace").unwrap_or(false),
            insert_final_newline: c.get_bool("files.insertFinalNewline").unwrap_or(false),
        }
    }

    /// Picks the first listed family that `is_installed`; `monospace` maps to Menlo (macOS).
    /// When none is installed the first listed family is kept.
    pub fn resolve_font(&mut self, is_installed: impl Fn(&str) -> bool) {
        let pick = self.font_families.iter().find_map(|f| {
            let name = if f.eq_ignore_ascii_case("monospace") { "Menlo" } else { f.as_str() };
            is_installed(name).then(|| name.to_string())
        });
        self.font_family = pick
            .or_else(|| self.font_families.first().cloned())
            .unwrap_or_else(|| "Menlo".into());
    }

    pub fn editor_options(&self) -> EditorOptions {
        EditorOptions { tab_size: self.tab_size, insert_spaces: self.insert_spaces }
    }

    /// Advance of one character, estimated from the font size (see `MENLO_ADVANCE_RATIO`).
    pub fn char_width(&self) -> f32 {
        self.font_size * MENLO_ADVANCE_RATIO
    }

    /// Width of the line cursor: `editor.cursorWidth`, or 2 when it is 0 (viewCursor.ts).
    pub fn line_cursor_width(&self) -> f32 {
        if self.cursor_width > 0.0 { self.cursor_width } else { 2.0 }
    }
}

/// The workbench layout settings.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutSettings {
    pub side_bar: SideBarLocation,
    pub activity_bar: ActivityBarLocation,
    pub status_bar_visible: bool,
    pub show_tabs: ShowTabs,
    pub tree_indent: f32,
}

impl LayoutSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            side_bar: c.get_enum("workbench.sideBar.location").unwrap_or(SideBarLocation::Left),
            activity_bar: c.get_enum("workbench.activityBar.location").unwrap_or(ActivityBarLocation::Default),
            status_bar_visible: c.get_bool("workbench.statusBar.visible").unwrap_or(true),
            show_tabs: c.get_enum("workbench.editor.showTabs").unwrap_or(ShowTabs::Multiple),
            tree_indent: c.get_f64("workbench.tree.indent").unwrap_or(8.0) as f32,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vsr_settings::{SettingsRegistry, UserSettings};

    use super::*;

    fn config(user: &str) -> Configuration {
        Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap())
    }

    #[test]
    fn defaults_match_upstream() {
        let e = EditorSettings::from_configuration(&config(""));
        assert_eq!((e.font_size, e.line_height, e.tab_size, e.insert_spaces), (12.0, 18.0, 4, true));
        assert_eq!(e.font_family, "Menlo");
        assert_eq!(e.font_weight, 400.0);
        assert_eq!(e.line_numbers, LineNumbers::On);
        assert_eq!(e.render_line_highlight, LineHighlight::Line);
        assert_eq!((e.cursor_style, e.cursor_blinking), (CursorStyle::Line, CursorBlinking::Blink));
        assert_eq!(e.line_cursor_width(), 2.0);
        let l = LayoutSettings::from_configuration(&config(""));
        assert_eq!(l, LayoutSettings {
            side_bar: SideBarLocation::Left,
            activity_bar: ActivityBarLocation::Default,
            status_bar_visible: true,
            show_tabs: ShowTabs::Multiple,
            tree_indent: 8.0,
        });
    }

    #[test]
    fn line_height_follows_font_info() {
        assert_eq!(compute_line_height(0.0, 12.0), 18.0);
        assert_eq!(compute_line_height(0.0, 14.0), 21.0);
        assert_eq!(compute_line_height(0.0, 13.0), 20.0, "19.5 rounds up");
        assert_eq!(compute_line_height(24.0, 12.0), 24.0);
        assert_eq!(compute_line_height(2.0, 12.0), 24.0, "below 8 is a multiple of the font size");
        assert_eq!(compute_line_height(1.0, 6.0), 8.0, "never below 8");
    }

    #[test]
    fn user_values_flow_through() {
        let e = EditorSettings::from_configuration(&config(
            r#"{ "editor.fontSize": 16, "editor.lineNumbers": "relative", "editor.cursorStyle": "block-outline",
                 "editor.renderLineHighlight": "all", "editor.cursorBlinking": "solid", "editor.cursorWidth": 4,
                 "editor.tabSize": 2, "editor.insertSpaces": false, "editor.fontWeight": "bold",
                 "files.insertFinalNewline": true }"#,
        ));
        assert_eq!(e.font_size, 16.0);
        assert_eq!(e.line_height, 24.0);
        assert_eq!(e.line_numbers, LineNumbers::Relative);
        assert_eq!(e.cursor_style, CursorStyle::BlockOutline);
        assert_eq!(e.render_line_highlight, LineHighlight::All);
        assert_eq!(e.cursor_blinking, CursorBlinking::Solid);
        assert_eq!(e.line_cursor_width(), 4.0);
        assert_eq!(e.editor_options(), EditorOptions { tab_size: 2, insert_spaces: false });
        assert_eq!(e.font_weight, 700.0);
        assert!(e.insert_final_newline && !e.trim_trailing_whitespace);
    }

    #[test]
    fn font_family_list_and_resolution() {
        assert_eq!(parse_font_families("Menlo, Monaco, 'Courier New', monospace"), ["Menlo", "Monaco", "Courier New", "monospace"]);
        let mut e = EditorSettings::from_configuration(&config(r#"{ "editor.fontFamily": "'Fira Code', Monaco, monospace" }"#));
        e.resolve_font(|f| f == "Monaco");
        assert_eq!(e.font_family, "Monaco");
        e.resolve_font(|f| f == "Menlo");
        assert_eq!(e.font_family, "Menlo", "monospace maps to Menlo");
        e.resolve_font(|_| false);
        assert_eq!(e.font_family, "Fira Code", "nothing installed: first listed");
    }

    #[test]
    fn font_weight_parsing() {
        assert_eq!(parse_font_weight(&json!("bold")), 700.0);
        assert_eq!(parse_font_weight(&json!("600")), 600.0);
        assert_eq!(parse_font_weight(&json!(300)), 300.0);
        assert_eq!(parse_font_weight(&json!("heavy")), 400.0);
        assert_eq!(parse_font_weight(&json!(5000)), 400.0);
    }

    #[test]
    fn invalid_layout_values_fall_back() {
        let l = LayoutSettings::from_configuration(&config(r#"{ "workbench.sideBar.location": "top", "workbench.editor.showTabs": "x" }"#));
        assert_eq!(l.side_bar, SideBarLocation::Left);
        assert_eq!(l.show_tabs, ShowTabs::Multiple);
    }
}
