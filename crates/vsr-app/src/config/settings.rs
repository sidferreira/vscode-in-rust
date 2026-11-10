//! Typed views of the settings the workbench and the editor read. UI free: they are built from a
//! `Configuration` and compared with `PartialEq` to see what changed.
//!
//! upstream: src/vs/editor/common/config/editorOptions.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/config/fontInfo.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/layout.ts @ 50e1350fc78d

use serde::Deserialize;
use serde_json::Value;
use vsr_editor::EditorOptions;
use vsr_search::SearchSettings;
use vsr_search::settings::{CollapseResults, ExcludeMap, ResultSortOrder};
use vsr_settings::Configuration;
use vsr_text::DefaultEol;

use super::files::AutoSaveMode;
use crate::model::SaveTransforms;

use super::minimap::MinimapSettings;
use super::scrollbar::ScrollbarVisibility;
use crate::theme::{Rgba8, hex};

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

/// `editor.occurrencesHighlight`. `multiFile` is read as `singleFile` (there is no multi-file
/// search for the highlights).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OccurrencesHighlight {
    Off,
    SingleFile,
    MultiFile,
}

/// `editor.matchBrackets`: `always` also highlights the enclosing pair, `near` only the bracket
/// next to the caret.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MatchBrackets {
    Always,
    Near,
    Never,
}

/// `editor.wordWrap`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WordWrap {
    Off,
    On,
    #[serde(rename = "wordWrapColumn")]
    Column,
    Bounded,
}

/// `editor.wrappingIndent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WrappingIndentSetting {
    None,
    Same,
    Indent,
    DeepIndent,
}

/// `editor.wrappingStrategy`: `advanced` measures text with the browser; with a monospace font it
/// breaks where `simple` does, so both run the monospace computer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WrappingStrategy {
    Simple,
    Advanced,
}

/// `editor.wordBreak`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WordBreakSetting {
    Normal,
    KeepAll,
}

/// `editor.renderWhitespace`: which spaces and tabs get a marker (middle dot, arrow).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderWhitespace {
    None,
    Boundary,
    Selection,
    Trailing,
    All,
}

/// `editor.renderFinalNewline`: the line number of the empty line after a final line break.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RenderFinalNewline {
    On,
    Off,
    Dimmed,
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

/// `editor.find.seedSearchStringFromSelection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SeedSearchString {
    Never,
    Always,
    Selection,
}

/// `editor.find.autoFindInSelection`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AutoFindInSelection {
    Never,
    Always,
    Multiline,
}

/// `editor.find.history` and `editor.find.replaceHistory`: `never` keeps no history, `workspace`
/// keeps it in the session state of the folder (ADR-0040).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FindHistoryMode {
    Never,
    Workspace,
}

/// The `editor.find.*` settings that change how the Find widget behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindSettings {
    pub seed_search_string_from_selection: SeedSearchString,
    pub auto_find_in_selection: AutoFindInSelection,
    /// `editor.find.globalFindClipboard`: the macOS find pasteboard is read when the widget opens and
    /// written when the search string changes.
    pub global_clipboard: bool,
    /// `editor.find.history` / `editor.find.replaceHistory`.
    pub history: FindHistoryMode,
    pub replace_history: FindHistoryMode,
    /// `editor.find.loop`: wrap around at the first / last match.
    pub loop_: bool,
    /// `editor.find.cursorMoveOnType`: jump to the next match while typing.
    pub cursor_move_on_type: bool,
    /// `editor.find.findOnType`: search while typing (otherwise on Enter).
    pub find_on_type: bool,
    /// `editor.find.closeOnResult`: close the widget after Enter lands on a match.
    pub close_on_result: bool,
}

impl Default for FindSettings {
    /// The registered defaults.
    fn default() -> Self {
        Self {
            seed_search_string_from_selection: SeedSearchString::Always,
            auto_find_in_selection: AutoFindInSelection::Never,
            global_clipboard: false,
            history: FindHistoryMode::Workspace,
            replace_history: FindHistoryMode::Workspace,
            loop_: true,
            cursor_move_on_type: true,
            find_on_type: true,
            close_on_result: false,
        }
    }
}

impl FindSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let d = Self::default();
        Self {
            seed_search_string_from_selection: c
                .get_enum("editor.find.seedSearchStringFromSelection")
                .unwrap_or(d.seed_search_string_from_selection),
            auto_find_in_selection: c.get_enum("editor.find.autoFindInSelection").unwrap_or(d.auto_find_in_selection),
            global_clipboard: c.get_bool("editor.find.globalFindClipboard").unwrap_or(d.global_clipboard),
            history: c.get_enum("editor.find.history").unwrap_or(d.history),
            replace_history: c.get_enum("editor.find.replaceHistory").unwrap_or(d.replace_history),
            loop_: c.get_bool("editor.find.loop").unwrap_or(d.loop_),
            cursor_move_on_type: c.get_bool("editor.find.cursorMoveOnType").unwrap_or(d.cursor_move_on_type),
            find_on_type: c.get_bool("editor.find.findOnType").unwrap_or(d.find_on_type),
            close_on_result: c.get_bool("editor.find.closeOnResult").unwrap_or(d.close_on_result),
        }
    }
}

/// Upstream `GOLDEN_LINE_HEIGHT_RATIO` on macOS (`fontInfo.ts`).
const GOLDEN_LINE_HEIGHT_RATIO: f64 = 1.5;
const MINIMUM_LINE_HEIGHT: f64 = 8.0;
/// Menlo's advance as a fraction of the font size (7.224px at 12px), used until real metrics exist.
const MENLO_ADVANCE_RATIO: f32 = 0.602;

/// `window.zoomLevel` bounds (the setting's minimum and maximum).
pub const ZOOM_LEVEL_RANGE: (f64, f64) = (-8.0, 8.0);

/// The zoom factor for a zoom level: each step is 20% (upstream `getZoomFactor`, 1.2^level). The
/// level is clamped to `ZOOM_LEVEL_RANGE`.
pub fn zoom_factor(level: f64) -> f32 {
    1.2_f64.powf(level.clamp(ZOOM_LEVEL_RANGE.0, ZOOM_LEVEL_RANGE.1)) as f32
}

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

/// `editor.guides.highlightActiveIndentation`: `true`, `"always"` or `false`. Anything else is the
/// default, `true`.
pub fn parse_highlight_active(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Bool(on)) => *on,
        Some(Value::String(s)) if s == "always" => true,
        _ => true,
    }
}

/// `editor.fontLigatures` as font features (`"tag" value` pairs, as the CSS `font-feature-settings`
/// property). `true` turns `liga` and `calt` on, `false` (the default) turns them off, a string is
/// read as a list of `'tag' on|off|N` entries. Entries that do not parse are skipped.
pub fn parse_font_ligatures(value: Option<&Value>) -> Vec<(String, u32)> {
    let on = || vec![("liga".to_string(), 1), ("calt".to_string(), 1)];
    let off = || vec![("liga".to_string(), 0), ("calt".to_string(), 0)];
    match value {
        None | Some(Value::Bool(false)) | Some(Value::Null) => off(),
        Some(Value::Bool(true)) => on(),
        Some(Value::String(s)) => match s.trim() {
            "" | "false" => off(),
            "true" => on(),
            list => list.split(',').filter_map(parse_font_feature).collect(),
        },
        Some(_) => off(),
    }
}

/// One `'tag' value` entry; the value is `on` (1), `off` (0) or a number, and 1 when omitted.
fn parse_font_feature(entry: &str) -> Option<(String, u32)> {
    let mut parts = entry.split_whitespace();
    let tag = parts.next()?.trim_matches(|c| c == '\'' || c == '"');
    if tag.len() != 4 {
        return None;
    }
    let value = match parts.next() {
        None | Some("on") => 1,
        Some("off") => 0,
        Some(n) => n.parse().ok()?,
    };
    parts.next().is_none().then(|| (tag.to_string(), value))
}

/// One entry of `editor.rulers`: a column, optionally with its own colour (`#rrggbb[aa]`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ruler {
    pub column: usize,
    pub color: Option<Rgba8>,
}

/// `editor.rulers`: numbers and `{ column, color }` objects. Entries of any other shape, negative
/// or fractional columns and unparsable colours are dropped (the ruler keeps the default colour
/// when only the colour is wrong). Sorted by column, as upstream does.
pub fn parse_rulers(value: Option<&Value>) -> Vec<Ruler> {
    let Some(Value::Array(items)) = value else { return Vec::new() };
    let mut rulers: Vec<Ruler> = items
        .iter()
        .filter_map(|item| match item {
            Value::Number(_) => item.as_u64().map(|column| Ruler { column: column as usize, color: None }),
            Value::Object(o) => {
                let column = o.get("column")?.as_u64()? as usize;
                let color = o.get("color").and_then(Value::as_str).and_then(hex);
                Some(Ruler { column, color })
            }
            _ => None,
        })
        .collect();
    rulers.sort_by_key(|r| r.column);
    rulers
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
    /// `editor.cursorHeight`: the line caret's height, 0 for the full line height.
    pub cursor_height: f32,
    /// `editor.cursorSurroundingLines`: rows kept above and below the caret when it scrolls into view.
    pub cursor_surrounding_lines: usize,
    /// `editor.cursorSurroundingLinesStyle: all` (`default` enforces the margin for keyboard and API
    /// moves only, not for a click).
    pub cursor_surrounding_lines_all: bool,
    pub render_whitespace: RenderWhitespace,
    /// `editor.renderControlCharacters`: C0 controls and DEL are drawn as pictures.
    pub render_control_characters: bool,
    /// `editor.renderFinalNewline`.
    pub render_final_newline: RenderFinalNewline,
    /// `editor.tabFocusMode`: Tab moves focus out of the editor instead of inserting a tab.
    pub tab_focus_mode: bool,
    pub rulers: Vec<Ruler>,
    /// `editor.guides.indentation`.
    pub guides_indentation: bool,
    /// `editor.guides.highlightActiveIndentation` (`true` and `"always"` both highlight).
    pub guides_highlight_active: bool,
    pub match_brackets: MatchBrackets,
    /// `editor.selectionHighlight`.
    pub selection_highlight: bool,
    /// `editor.selectionHighlightMaxLength`: longer selections are not matched.
    pub selection_highlight_max_length: usize,
    /// `editor.occurrencesHighlight`.
    pub occurrences_highlight: OccurrencesHighlight,
    /// `editor.roundedSelection`: the selection boxes have 3px corners.
    pub rounded_selection: bool,
    /// `editor.renderLineHighlightOnlyWhenFocus`.
    pub line_highlight_only_when_focus: bool,
    /// `editor.fontLigatures` as font features (see `parse_font_ligatures`).
    pub font_features: Vec<(String, u32)>,
    /// `editor.scrollBeyondLastLine`: the last line can scroll up to the top of the viewport.
    pub scroll_beyond_last_line: bool,
    /// `editor.scrollBeyondLastColumn`: columns of room after the widest line when not wrapping
    /// (upstream `ViewLayout`: `maxLineWidth + scrollBeyondLastColumn * typicalHalfwidthCharacterWidth`).
    pub scroll_beyond_last_column: usize,
    /// `editor.scrollbar.vertical` / `horizontal`, the sizes of the bars and `editor.scrollbar.scrollByPage`.
    pub scrollbar_vertical: ScrollbarVisibility,
    pub scrollbar_horizontal: ScrollbarVisibility,
    pub scrollbar_vertical_size: f32,
    pub scrollbar_horizontal_size: f32,
    pub scrollbar_scroll_by_page: bool,
    /// `editor.overviewRulerBorder` and `editor.hideCursorInOverviewRuler` (the overview ruler, ADR-0055).
    pub overview_ruler_border: bool,
    pub hide_cursor_in_overview_ruler: bool,
    /// `editor.wordWrap`, `editor.wordWrapColumn`, `editor.wrappingIndent`, `editor.wrappingStrategy`,
    /// `editor.wordWrapIndicator`, `editor.wordBreak`.
    pub word_wrap: WordWrap,
    pub word_wrap_column: usize,
    pub wrapping_indent: WrappingIndentSetting,
    pub wrapping_strategy: WrappingStrategy,
    pub word_wrap_indicator: bool,
    pub word_break: WordBreakSetting,
    /// `editor.wordSeparators`.
    pub word_separators: String,
    pub trim_trailing_whitespace: bool,
    pub insert_final_newline: bool,
    /// `files.trimFinalNewlines`.
    pub trim_final_newlines: bool,
    /// `files.trimTrailingWhitespaceInRegexAndStrings` (default true).
    pub trim_in_regex_and_strings: bool,
    /// `files.saveConflictResolution` is `askUser` (true) or `overwriteFileOnDisk` (false, ADR-0039).
    pub save_conflict_ask: bool,
    /// `files.eol`: the line break of a file that has none yet (ADR-0034). `auto` is LF on macOS.
    pub eol: DefaultEol,
    /// `files.autoSave`, `files.autoSaveDelay` (ms) and `files.autoSaveWorkspaceFilesOnly`.
    pub auto_save: AutoSaveMode,
    pub auto_save_delay_ms: u64,
    pub auto_save_workspace_only: bool,
    pub find: FindSettings,
    pub minimap: MinimapSettings,
    /// Folding, sticky scroll and minimap section headers (ADR-0042).
    pub folding: super::folding::FoldingSettings,
    /// Typing rules: `editor.autoClosing*`, `autoSurround`, `autoIndent`, `useTabStops`,
    /// `trimAutoWhitespace`, `trimWhitespaceOnDelete`, `indentSize` (ADR-0046).
    pub typing: vsr_editor::TypingOptions,
    /// `editor.comments.insertSpace` and `editor.comments.ignoreEmptyLines`.
    pub comments_insert_space: bool,
    pub comments_ignore_empty_lines: bool,
    /// `editor.detectIndentation`: tabs or spaces and the indent size come from the file.
    pub detect_indentation: bool,
    /// `editor.emptySelectionClipboard`: copy and cut without a selection use the current line.
    pub empty_selection_clipboard: bool,
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
            cursor_height: 0.0,
            cursor_surrounding_lines: 0,
            cursor_surrounding_lines_all: false,
            render_whitespace: RenderWhitespace::Selection,
            render_control_characters: true,
            render_final_newline: RenderFinalNewline::On,
            tab_focus_mode: false,
            rulers: Vec::new(),
            guides_indentation: true,
            guides_highlight_active: true,
            match_brackets: MatchBrackets::Always,
            selection_highlight: true,
            selection_highlight_max_length: 200,
            occurrences_highlight: OccurrencesHighlight::SingleFile,
            rounded_selection: true,
            line_highlight_only_when_focus: false,
            font_features: parse_font_ligatures(None),
            scroll_beyond_last_line: true,
            scroll_beyond_last_column: 4,
            scrollbar_vertical: ScrollbarVisibility::Auto,
            scrollbar_horizontal: ScrollbarVisibility::Auto,
            scrollbar_vertical_size: 14.0,
            scrollbar_horizontal_size: 12.0,
            scrollbar_scroll_by_page: false,
            overview_ruler_border: true,
            hide_cursor_in_overview_ruler: false,
            word_wrap: WordWrap::Off,
            word_wrap_column: 80,
            wrapping_indent: WrappingIndentSetting::Same,
            wrapping_strategy: WrappingStrategy::Simple,
            word_wrap_indicator: false,
            word_break: WordBreakSetting::Normal,
            word_separators: vsr_editor::USUAL_WORD_SEPARATORS.to_string(),
            trim_trailing_whitespace: false,
            insert_final_newline: false,
            trim_final_newlines: false,
            trim_in_regex_and_strings: true,
            save_conflict_ask: true,
            eol: DefaultEol::Lf,
            auto_save: AutoSaveMode::Off,
            auto_save_delay_ms: 1000,
            auto_save_workspace_only: false,
            find: FindSettings::default(),
            minimap: MinimapSettings::default(),
            folding: Default::default(),
            typing: Default::default(),
            comments_insert_space: true,
            comments_ignore_empty_lines: true,
            detect_indentation: true,
            empty_selection_clipboard: true,
        }
    }
}

/// `editor.autoClosingBrackets`, `Quotes` and `Comments`.
fn parse_auto_closing(c: &Configuration, key: &str) -> vsr_editor::AutoClosing {
    use vsr_editor::AutoClosing::*;
    match c.get_str(key) {
        Some("always") => Always,
        Some("beforeWhitespace") => BeforeWhitespace,
        Some("never") => Never,
        _ => LanguageDefined,
    }
}

/// `editor.autoClosingDelete` and `autoClosingOvertype`.
fn parse_auto_closing_edit(c: &Configuration, key: &str) -> vsr_editor::AutoClosingEdit {
    use vsr_editor::AutoClosingEdit::*;
    match c.get_str(key) {
        Some("always") => Always,
        Some("never") => Never,
        _ => Auto,
    }
}

/// The typing options of the `editor.*` settings (ADR-0046).
pub fn typing_options(c: &Configuration) -> vsr_editor::TypingOptions {
    use vsr_editor::{AutoIndentStrategy, AutoSurround};
    vsr_editor::TypingOptions {
        auto_closing_brackets: parse_auto_closing(c, "editor.autoClosingBrackets"),
        auto_closing_quotes: parse_auto_closing(c, "editor.autoClosingQuotes"),
        auto_closing_comments: parse_auto_closing(c, "editor.autoClosingComments"),
        auto_closing_delete: parse_auto_closing_edit(c, "editor.autoClosingDelete"),
        auto_closing_overtype: parse_auto_closing_edit(c, "editor.autoClosingOvertype"),
        auto_surround: match c.get_str("editor.autoSurround") {
            Some("quotes") => AutoSurround::Quotes,
            Some("brackets") => AutoSurround::Brackets,
            Some("never") => AutoSurround::Never,
            _ => AutoSurround::LanguageDefined,
        },
        auto_indent: match c.get_str("editor.autoIndent") {
            Some("none") => AutoIndentStrategy::None,
            Some("keep") => AutoIndentStrategy::Keep,
            Some("brackets") => AutoIndentStrategy::Brackets,
            Some("advanced") => AutoIndentStrategy::Advanced,
            _ => AutoIndentStrategy::Full,
        },
        use_tab_stops: c.get_bool("editor.useTabStops").unwrap_or(true),
        trim_whitespace_on_delete: c.get_bool("editor.trimWhitespaceOnDelete").unwrap_or(false),
        trim_auto_whitespace: c.get_bool("editor.trimAutoWhitespace").unwrap_or(true),
        // `"tabSize"` (the default) or a number of spaces.
        indent_size: c.get_f64("editor.indentSize").map(|n| n.round().max(1.0) as usize),
    }
}

impl EditorSettings {
    /// The save participants these settings ask for (ADR-0034). `auto_save` is the save reason.
    pub fn save_transforms(&self, auto_save: bool) -> SaveTransforms {
        SaveTransforms {
            trim_trailing_whitespace: self.trim_trailing_whitespace,
            insert_final_newline: self.insert_final_newline,
            trim_final_newlines: self.trim_final_newlines,
            keep_in_regex_and_strings: !self.trim_in_regex_and_strings,
            auto_save,
        }
    }

    pub fn from_configuration(c: &Configuration) -> Self {
        // `window.zoomLevel` scales the editor text (the chrome is laid out in fixed pixels, so it
        // does not zoom yet; see ADR-0035).
        let zoom = zoom_factor(c.get_f64("window.zoomLevel").unwrap_or(0.0)) as f64;
        let font_size = c.get_f64("editor.fontSize").unwrap_or(12.0) * zoom;
        let families = parse_font_families(c.get_str("editor.fontFamily").unwrap_or_default());
        Self {
            font_size: font_size as f32,
            font_family: families.first().cloned().unwrap_or_else(|| "Menlo".into()),
            font_families: families,
            font_weight: c.get_value("editor.fontWeight").map_or(400.0, parse_font_weight),
            line_height: compute_line_height(c.get_f64("editor.lineHeight").unwrap_or(0.0) * zoom, font_size),
            tab_size: c.get_f64("editor.tabSize").unwrap_or(4.0).round().max(1.0) as usize,
            insert_spaces: c.get_bool("editor.insertSpaces").unwrap_or(true),
            line_numbers: c.get_enum("editor.lineNumbers").unwrap_or(LineNumbers::On),
            render_line_highlight: c.get_enum("editor.renderLineHighlight").unwrap_or(LineHighlight::Line),
            cursor_style: c.get_enum("editor.cursorStyle").unwrap_or(CursorStyle::Line),
            cursor_blinking: c.get_enum("editor.cursorBlinking").unwrap_or(CursorBlinking::Blink),
            cursor_width: c.get_f64("editor.cursorWidth").unwrap_or(0.0) as f32,
            cursor_height: c.get_f64("editor.cursorHeight").unwrap_or(0.0).max(0.0) as f32,
            cursor_surrounding_lines: c.get_f64("editor.cursorSurroundingLines").unwrap_or(0.0).max(0.0) as usize,
            cursor_surrounding_lines_all: c.get_str("editor.cursorSurroundingLinesStyle") == Some("all"),
            render_whitespace: c.get_enum("editor.renderWhitespace").unwrap_or(RenderWhitespace::Selection),
            render_control_characters: c.get_bool("editor.renderControlCharacters").unwrap_or(true),
            render_final_newline: c.get_enum("editor.renderFinalNewline").unwrap_or(RenderFinalNewline::On),
            tab_focus_mode: c.get_bool("editor.tabFocusMode").unwrap_or(false),
            rulers: parse_rulers(c.get_value("editor.rulers")),
            guides_indentation: c.get_bool("editor.guides.indentation").unwrap_or(true),
            guides_highlight_active: parse_highlight_active(c.get_value("editor.guides.highlightActiveIndentation")),
            match_brackets: c.get_enum("editor.matchBrackets").unwrap_or(MatchBrackets::Always),
            selection_highlight: c.get_bool("editor.selectionHighlight").unwrap_or(true),
            selection_highlight_max_length: c.get_f64("editor.selectionHighlightMaxLength").unwrap_or(200.0).max(0.0) as usize,
            occurrences_highlight: c.get_enum("editor.occurrencesHighlight").unwrap_or(OccurrencesHighlight::SingleFile),
            rounded_selection: c.get_bool("editor.roundedSelection").unwrap_or(true),
            line_highlight_only_when_focus: c.get_bool("editor.renderLineHighlightOnlyWhenFocus").unwrap_or(false),
            font_features: parse_font_ligatures(c.get_value("editor.fontLigatures")),
            scroll_beyond_last_line: c.get_bool("editor.scrollBeyondLastLine").unwrap_or(true),
            scroll_beyond_last_column: c.get_f64("editor.scrollBeyondLastColumn").unwrap_or(4.0).round().max(0.0) as usize,
            scrollbar_vertical: c.get_enum("editor.scrollbar.vertical").unwrap_or(ScrollbarVisibility::Auto),
            scrollbar_horizontal: c.get_enum("editor.scrollbar.horizontal").unwrap_or(ScrollbarVisibility::Auto),
            scrollbar_vertical_size: c.get_f64("editor.scrollbar.verticalScrollbarSize").unwrap_or(14.0).round().max(0.0) as f32,
            scrollbar_horizontal_size: c.get_f64("editor.scrollbar.horizontalScrollbarSize").unwrap_or(12.0).round().max(0.0) as f32,
            scrollbar_scroll_by_page: c.get_bool("editor.scrollbar.scrollByPage").unwrap_or(false),
            overview_ruler_border: c.get_bool("editor.overviewRulerBorder").unwrap_or(true),
            hide_cursor_in_overview_ruler: c.get_bool("editor.hideCursorInOverviewRuler").unwrap_or(false),
            word_wrap: c.get_enum("editor.wordWrap").unwrap_or(WordWrap::Off),
            word_wrap_column: c.get_f64("editor.wordWrapColumn").unwrap_or(80.0).round().max(1.0) as usize,
            wrapping_indent: c.get_enum("editor.wrappingIndent").unwrap_or(WrappingIndentSetting::Same),
            wrapping_strategy: c.get_enum("editor.wrappingStrategy").unwrap_or(WrappingStrategy::Simple),
            word_wrap_indicator: c.get_bool("editor.wordWrapIndicator").unwrap_or(false),
            word_break: c.get_enum("editor.wordBreak").unwrap_or(WordBreakSetting::Normal),
            word_separators: c
                .get_str("editor.wordSeparators")
                .map_or_else(|| vsr_editor::USUAL_WORD_SEPARATORS.to_string(), str::to_string),
            trim_trailing_whitespace: c.get_bool("files.trimTrailingWhitespace").unwrap_or(false),
            insert_final_newline: c.get_bool("files.insertFinalNewline").unwrap_or(false),
            trim_final_newlines: c.get_bool("files.trimFinalNewlines").unwrap_or(false),
            trim_in_regex_and_strings: c.get_bool("files.trimTrailingWhitespaceInRegexAndStrings").unwrap_or(true),
            save_conflict_ask: c.get_str("files.saveConflictResolution") != Some("overwriteFileOnDisk"),
            eol: match c.get_str("files.eol") {
                Some("\r\n") => DefaultEol::CrLf,
                _ => DefaultEol::Lf,
            },
            auto_save: AutoSaveMode::parse(c.get_str("files.autoSave").unwrap_or("off")),
            auto_save_delay_ms: c.get_f64("files.autoSaveDelay").unwrap_or(1000.0).max(0.0) as u64,
            auto_save_workspace_only: c.get_bool("files.autoSaveWorkspaceFilesOnly").unwrap_or(false),
            find: FindSettings::from_configuration(c),
            minimap: MinimapSettings::from_configuration(c),
            folding: super::folding::FoldingSettings::from_configuration(c),
            typing: typing_options(c),
            comments_insert_space: c.get_bool("editor.comments.insertSpace").unwrap_or(true),
            comments_ignore_empty_lines: c.get_bool("editor.comments.ignoreEmptyLines").unwrap_or(true),
            detect_indentation: c.get_bool("editor.detectIndentation").unwrap_or(true),
            empty_selection_clipboard: c.get_bool("editor.emptySelectionClipboard").unwrap_or(true),
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

    /// The wrapping parameters for a wrapping column (see `wrapping_column`), in units of one
    /// character advance.
    pub fn wrap_params(&self, wrapping_column: i64) -> vsr_editor::wrap::WrapParams {
        use vsr_editor::wrap::{WordBreak, WrapParams, WrappingIndent};
        let indent = match self.wrapping_indent {
            WrappingIndentSetting::None => WrappingIndent::None,
            WrappingIndentSetting::Same => WrappingIndent::Same,
            WrappingIndentSetting::Indent => WrappingIndent::Indent,
            WrappingIndentSetting::DeepIndent => WrappingIndent::DeepIndent,
        };
        let word_break = match self.word_break {
            WordBreakSetting::Normal => WordBreak::Normal,
            WordBreakSetting::KeepAll => WordBreak::KeepAll,
        };
        // `wrappingStrategy: advanced` shapes text in the browser upstream; here both strategies
        // use the monospace computer.
        WrapParams::monospace(self.tab_size, wrapping_column, indent).with_word_break(word_break)
    }

    /// Width of the line cursor: `editor.cursorWidth`, or 2 when it is 0 (viewCursor.ts).
    pub fn line_cursor_width(&self) -> f32 {
        if self.cursor_width > 0.0 { self.cursor_width.min(self.char_width()) } else { 2.0 }
    }

    /// Height of the line caret: `editor.cursorHeight`, at most one line; 0 means the full line
    /// (viewCursor.ts, `lineCursorHeight`).
    pub fn line_cursor_height(&self) -> f32 {
        if self.cursor_height > 0.0 { self.cursor_height.min(self.line_height) } else { self.line_height }
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
    /// `workbench.sash.size`: the width in pixels of the area that drags a sash (default 4).
    pub sash_size: f32,
    /// `workbench.sash.autoHideGrippers` and `autoHideGrippersDelay`: whether the sash's grip recedes when the
    /// mouse is idle, and after how many milliseconds (upstream `SashGrippersController`, ADR-0056).
    pub sash_auto_hide_grippers: bool,
    pub sash_auto_hide_grippers_delay: u64,
    /// `window.commandCenter`: the search box in the title bar.
    pub command_center: bool,
    /// `window.density.editorTabHeight`: compact tabs are shorter (22px, default 35px).
    pub compact_tabs: bool,
    /// `workbench.activityBar.compact`: smaller items and a narrower bar, location `default` only.
    pub activity_bar_compact: bool,
    /// `workbench.activityBar.iconClickBehavior: focus`: clicking the icon of the shown view keeps
    /// the side bar open (`toggle`, the default, hides it).
    pub activity_bar_click_focus: bool,
}

/// `workbench.startupEditor`: what opens when the app starts with no editor restored. `WelcomePage`
/// opens the Welcome page (ADR-0040), `Readme` the folder's README when there is one and
/// `NewUntitledFile` an empty untitled text (ADR-0058). rscode has no terminal yet, so the other
/// values open nothing (ADR-0035).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartupEditor {
    None,
    WelcomePage,
    Readme,
    NewUntitledFile,
    WelcomePageInEmptyWorkbench,
    Terminal,
    AgentSessionsWelcomePage,
}

/// The configured `workbench.startupEditor` (read once at startup, as upstream does).
pub fn startup_editor(c: &Configuration) -> StartupEditor {
    c.get_enum("workbench.startupEditor").unwrap_or(StartupEditor::WelcomePage)
}

/// `window.density.editorTabHeight`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabHeight {
    Default,
    Compact,
}

/// `workbench.sash.size`, clamped to upstream's range (1 to 20 pixels).
pub fn sash_size(c: &Configuration) -> f32 {
    c.get_f64("workbench.sash.size").unwrap_or(4.0).clamp(1.0, 20.0) as f32
}

impl LayoutSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            side_bar: c.get_enum("workbench.sideBar.location").unwrap_or(SideBarLocation::Left),
            activity_bar: c.get_enum("workbench.activityBar.location").unwrap_or(ActivityBarLocation::Default),
            status_bar_visible: c.get_bool("workbench.statusBar.visible").unwrap_or(true),
            show_tabs: c.get_enum("workbench.editor.showTabs").unwrap_or(ShowTabs::Multiple),
            tree_indent: c.get_f64("workbench.tree.indent").unwrap_or(8.0) as f32,
            sash_size: sash_size(c),
            sash_auto_hide_grippers: c.get_bool("workbench.sash.autoHideGrippers").unwrap_or(true),
            sash_auto_hide_grippers_delay: c.get_f64("workbench.sash.autoHideGrippersDelay").unwrap_or(2000.0).max(0.0) as u64,
            command_center: c.get_bool("window.commandCenter").unwrap_or(true),
            compact_tabs: c.get_enum::<TabHeight>("window.density.editorTabHeight") == Some(TabHeight::Compact),
            activity_bar_compact: c.get_bool("workbench.activityBar.compact").unwrap_or(false),
            activity_bar_click_focus: c.get_str("workbench.activityBar.iconClickBehavior") == Some("focus"),
        }
    }
}

/// `workbench.editor.openPositioning`: where a new editor tab goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpenPositioning {
    Left,
    #[default]
    Right,
    First,
    Last,
}

/// `workbench.editor.labelFormat`: how a tab's label is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LabelFormat {
    #[default]
    Default,
    Short,
    Medium,
    Long,
}

/// Where an editor open comes from. The source decides whether it opens as a preview
/// (`EditorGroupSettings::opens_pinned`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenSource {
    /// Explorer click: a single click previews, a double click pins.
    Explorer { double_click: bool },
    /// A Search result (single click).
    Search,
    /// Quick Open, Go to File, Go to Line.
    QuickOpen,
    /// Code navigation (go to definition and so on). rscode has none yet, so nothing opens from it.
    #[allow(dead_code)]
    CodeNavigation,
}

/// The `workbench.editor.*` settings that place, pin, close and label editors. Read by the
/// editor model (`model::Editors`), so it is UI free. Defaults are the registered defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorGroupSettings {
    pub open_positioning: OpenPositioning,
    pub focus_recent_editor_after_close: bool,
    pub enable_preview: bool,
    pub enable_preview_from_quick_open: bool,
    pub enable_preview_from_code_navigation: bool,
    pub close_on_file_delete: bool,
    pub limit_enabled: bool,
    pub limit_value: usize,
    pub limit_exclude_dirty: bool,
    pub highlight_modified_tabs: bool,
    pub label_format: LabelFormat,
    /// `workbench.editor.untitled.labelFormat`: the tab title of an untitled text (ADR-0059).
    pub untitled_label_format: vsr_editor::untitled::LabelFormat,
    pub scroll_to_switch_tabs: bool,
    /// `workbench.editor.empty.hint` is `text` (the hint is shown) rather than `hidden`.
    pub empty_hint: bool,
}

impl Default for EditorGroupSettings {
    fn default() -> Self {
        Self {
            open_positioning: OpenPositioning::Right,
            focus_recent_editor_after_close: true,
            enable_preview: true,
            enable_preview_from_quick_open: false,
            enable_preview_from_code_navigation: false,
            close_on_file_delete: false,
            limit_enabled: false,
            limit_value: 10,
            limit_exclude_dirty: false,
            highlight_modified_tabs: false,
            label_format: LabelFormat::Default,
            untitled_label_format: vsr_editor::untitled::LabelFormat::Content,
            scroll_to_switch_tabs: false,
            empty_hint: true,
        }
    }
}

impl EditorGroupSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let d = Self::default();
        Self {
            open_positioning: c.get_enum("workbench.editor.openPositioning").unwrap_or(d.open_positioning),
            focus_recent_editor_after_close: c
                .get_bool("workbench.editor.focusRecentEditorAfterClose")
                .unwrap_or(d.focus_recent_editor_after_close),
            enable_preview: c.get_bool("workbench.editor.enablePreview").unwrap_or(d.enable_preview),
            enable_preview_from_quick_open: c
                .get_bool("workbench.editor.enablePreviewFromQuickOpen")
                .unwrap_or(d.enable_preview_from_quick_open),
            enable_preview_from_code_navigation: c
                .get_bool("workbench.editor.enablePreviewFromCodeNavigation")
                .unwrap_or(d.enable_preview_from_code_navigation),
            close_on_file_delete: c.get_bool("workbench.editor.closeOnFileDelete").unwrap_or(d.close_on_file_delete),
            limit_enabled: c.get_bool("workbench.editor.limit.enabled").unwrap_or(d.limit_enabled),
            limit_value: c.get_f64("workbench.editor.limit.value").map_or(d.limit_value, |n| n.max(1.0) as usize),
            limit_exclude_dirty: c.get_bool("workbench.editor.limit.excludeDirty").unwrap_or(d.limit_exclude_dirty),
            highlight_modified_tabs: c
                .get_bool("workbench.editor.highlightModifiedTabs")
                .unwrap_or(d.highlight_modified_tabs),
            label_format: c.get_enum("workbench.editor.labelFormat").unwrap_or(d.label_format),
            untitled_label_format: c
                .get_str("workbench.editor.untitled.labelFormat")
                .map(vsr_editor::untitled::LabelFormat::from_setting)
                .unwrap_or(d.untitled_label_format),
            scroll_to_switch_tabs: c.get_bool("workbench.editor.scrollToSwitchTabs").unwrap_or(d.scroll_to_switch_tabs),
            empty_hint: c.get_str("workbench.editor.empty.hint") != Some("hidden"),
        }
    }

    /// Whether an open from `source` keeps its editor (upstream `openEditorPinned`): with
    /// previews off everything is pinned; Quick Open and code navigation preview only when their
    /// own setting says so.
    pub fn opens_pinned(&self, source: OpenSource) -> bool {
        if !self.enable_preview {
            return true;
        }
        match source {
            OpenSource::Explorer { double_click } => double_click,
            OpenSource::Search => false,
            OpenSource::QuickOpen => !self.enable_preview_from_quick_open,
            OpenSource::CodeNavigation => !self.enable_preview_from_code_navigation,
        }
    }
}

/// The settings the Search view reads (`search.*` and `files.exclude`), built from the
/// configuration. Object values come from `Configuration::get_value`, which already merges the
/// registered default with the user's map (upstream `mergeContents`), so defaults are not layered
/// on again. Entries whose value is not a boolean (`when` clauses) are ignored by the search.
pub fn search_settings(c: &Configuration) -> SearchSettings {
    let d = SearchSettings::default();
    let exclude_map = |key: &str, fallback: &ExcludeMap| -> ExcludeMap {
        match c.get_value(key).and_then(Value::as_object) {
            Some(map) => map.iter().filter_map(|(glob, on)| on.as_bool().map(|on| (glob.clone(), on))).collect(),
            None => fallback.clone(),
        }
    };
    // Registered as `["number", "null"]`, so unvalidated here: null is unlimited.
    let max_results = match c.get_value("search.maxResults") {
        Some(Value::Null) => None,
        Some(v) => v.as_f64().filter(|n| n.is_finite() && *n >= 0.0).map_or(d.max_results, |n| Some(n as usize)),
        None => d.max_results,
    };
    SearchSettings {
        files_exclude: exclude_map("files.exclude", &d.files_exclude),
        search_exclude: exclude_map("search.exclude", &d.search_exclude),
        use_ignore_files: c.get_bool("search.useIgnoreFiles").unwrap_or(d.use_ignore_files),
        use_global_ignore_files: c.get_bool("search.useGlobalIgnoreFiles").unwrap_or(d.use_global_ignore_files),
        use_parent_ignore_files: c.get_bool("search.useParentIgnoreFiles").unwrap_or(d.use_parent_ignore_files),
        follow_symlinks: c.get_bool("search.followSymlinks").unwrap_or(d.follow_symlinks),
        smart_case: c.get_bool("search.smartCase").unwrap_or(d.smart_case),
        max_results,
        search_on_type: c.get_bool("search.searchOnType").unwrap_or(d.search_on_type),
        search_on_type_debounce_ms: c
            .get_f64("search.searchOnTypeDebouncePeriod")
            .map_or(d.search_on_type_debounce_ms, |n| n.max(0.0) as u64),
        show_line_numbers: c.get_bool("search.showLineNumbers").unwrap_or(d.show_line_numbers),
        collapse_results: match c.get_str("search.collapseResults") {
            Some("auto") => CollapseResults::Auto,
            Some("alwaysCollapse") => CollapseResults::AlwaysCollapse,
            _ => CollapseResults::AlwaysExpand,
        },
        threads: c.get_f64("search.ripgrep.maxThreads").map_or(d.threads, |n| n.max(0.0) as usize),
        sort_order: match c.get_str("search.sortOrder") {
            Some("fileNames") => ResultSortOrder::FileNames,
            Some("type") => ResultSortOrder::Type,
            Some("modified") => ResultSortOrder::Modified,
            Some("countDescending") => ResultSortOrder::CountDescending,
            Some("countAscending") => ResultSortOrder::CountAscending,
            _ => ResultSortOrder::Default,
        },
        seed_on_focus: c.get_bool("search.seedOnFocus").unwrap_or(d.seed_on_focus),
        seed_with_nearest_word: c.get_bool("search.seedWithNearestWord").unwrap_or(d.seed_with_nearest_word),
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
        assert_eq!(e.render_whitespace, RenderWhitespace::Selection);
        assert!(e.rulers.is_empty());
        assert!(e.guides_indentation && e.guides_highlight_active);
        assert_eq!(e.match_brackets, MatchBrackets::Always);
        assert!(e.selection_highlight && e.selection_highlight_max_length == 200);
        assert_eq!(e.occurrences_highlight, OccurrencesHighlight::SingleFile);
        assert!(e.rounded_selection && !e.line_highlight_only_when_focus && e.scroll_beyond_last_line);
        assert_eq!(e.font_features, [("liga".to_string(), 0), ("calt".to_string(), 0)]);
        assert_eq!((e.word_wrap, e.word_wrap_column, e.wrapping_indent), (WordWrap::Off, 80, WrappingIndentSetting::Same));
        assert_eq!((e.wrapping_strategy, e.word_wrap_indicator, e.word_break), (WrappingStrategy::Simple, false, WordBreakSetting::Normal));
        assert_eq!(e.word_separators, vsr_editor::USUAL_WORD_SEPARATORS);
        let l = LayoutSettings::from_configuration(&config(""));
        assert_eq!(l, LayoutSettings {
            side_bar: SideBarLocation::Left,
            activity_bar: ActivityBarLocation::Default,
            status_bar_visible: true,
            show_tabs: ShowTabs::Multiple,
            tree_indent: 8.0,
            sash_size: 4.0,
            sash_auto_hide_grippers: true,
            sash_auto_hide_grippers_delay: 2000,
            command_center: true,
            compact_tabs: false,
            activity_bar_compact: false,
            activity_bar_click_focus: false,
        });
    }

    #[test]
    fn startup_editor_values() {
        assert_eq!(startup_editor(&config("")), StartupEditor::WelcomePage, "upstream default");
        assert_eq!(startup_editor(&config(r#"{ "workbench.startupEditor": "readme" }"#)), StartupEditor::Readme);
        assert_eq!(startup_editor(&config(r#"{ "workbench.startupEditor": "none" }"#)), StartupEditor::None);
        assert_eq!(startup_editor(&config(r#"{ "workbench.startupEditor": "bogus" }"#)), StartupEditor::WelcomePage);
    }

    #[test]
    fn zoom_level_scales_the_editor_text() {
        assert_eq!(zoom_factor(0.0), 1.0);
        assert!((zoom_factor(1.0) - 1.2).abs() < 1e-6);
        assert!((zoom_factor(-2.0) - 1.0 / 1.44).abs() < 1e-6);
        assert_eq!(zoom_factor(100.0), zoom_factor(8.0), "clamped to the setting's maximum");
        assert_eq!(zoom_factor(-100.0), zoom_factor(-8.0), "clamped to the setting's minimum");

        let e = EditorSettings::from_configuration(&config(r#"{ "window.zoomLevel": 1, "editor.fontSize": 10 }"#));
        assert!((e.font_size - 12.0).abs() < 1e-4, "{}", e.font_size);
        assert!((e.line_height - 18.0).abs() < 1e-4, "auto line height follows the zoomed font: {}", e.line_height);
    }

    #[test]
    fn word_wrap_settings_are_read_and_clamped() {
        let e = EditorSettings::from_configuration(&config(
            r#"{"editor.wordWrap":"bounded","editor.wordWrapColumn":0,"editor.wrappingIndent":"deepIndent","editor.wordBreak":"keepAll","editor.wordSeparators":"."}"#,
        ));
        assert_eq!(e.word_wrap, WordWrap::Bounded);
        assert_eq!(e.word_wrap_column, 1, "wordWrapColumn has a minimum of 1");
        assert_eq!(e.wrapping_indent, WrappingIndentSetting::DeepIndent);
        assert_eq!(e.word_separators, ".");
        let p = e.wrap_params(30);
        assert_eq!(p.breaks.wrapping_column, 30);
        assert_eq!(p.breaks.wrapping_indent, vsr_editor::wrap::WrappingIndent::DeepIndent);
        assert_eq!(p.breaks.word_break, vsr_editor::wrap::WordBreak::KeepAll);
        let bad = EditorSettings::from_configuration(&config(r#"{"editor.wordWrap":"sideways"}"#));
        assert_eq!(bad.word_wrap, WordWrap::Off, "an invalid value falls back to the default");
    }

    #[test]
    fn cursor_height_and_surrounding_lines_are_read() {
        let e = EditorSettings::from_configuration(&config("{}"));
        assert_eq!((e.cursor_height, e.cursor_surrounding_lines, e.cursor_surrounding_lines_all), (0.0, 0, false));
        assert_eq!(e.line_cursor_height(), e.line_height, "0 is the full line");
        let e = EditorSettings::from_configuration(&config(
            r#"{ "editor.cursorHeight": 6, "editor.cursorSurroundingLines": 3, "editor.cursorSurroundingLinesStyle": "all" }"#,
        ));
        assert_eq!((e.cursor_height, e.cursor_surrounding_lines, e.cursor_surrounding_lines_all), (6.0, 3, true));
        assert_eq!(e.line_cursor_height(), 6.0);
        let tall = EditorSettings::from_configuration(&config(r#"{ "editor.cursorHeight": 500 }"#));
        assert_eq!(tall.line_cursor_height(), tall.line_height, "at most one line");
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
                 "files.insertFinalNewline": true, "editor.renderWhitespace": "boundary" }"#,
        ));
        assert_eq!(e.render_whitespace, RenderWhitespace::Boundary);
        assert!(e.rulers.is_empty(), "no rulers unless configured");
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
    fn find_settings_defaults_and_values() {
        assert_eq!(FindSettings::from_configuration(&config("")), FindSettings::default());
        let f = FindSettings::from_configuration(&config(
            r#"{ "editor.find.seedSearchStringFromSelection": "selection", "editor.find.autoFindInSelection": "multiline",
                 "editor.find.loop": false, "editor.find.cursorMoveOnType": false, "editor.find.findOnType": false,
                 "editor.find.closeOnResult": true, "editor.find.globalFindClipboard": true,
                 "editor.find.history": "never", "editor.find.replaceHistory": "never" }"#,
        ));
        assert!(f.global_clipboard);
        assert_eq!((f.history, f.replace_history), (FindHistoryMode::Never, FindHistoryMode::Never));
        assert_eq!(f.seed_search_string_from_selection, SeedSearchString::Selection);
        assert_eq!(f.auto_find_in_selection, AutoFindInSelection::Multiline);
        assert!(!f.loop_ && !f.cursor_move_on_type && !f.find_on_type && f.close_on_result);
        let bad = FindSettings::from_configuration(&config(r#"{ "editor.find.seedSearchStringFromSelection": "sometimes" }"#));
        assert_eq!(bad.seed_search_string_from_selection, SeedSearchString::Always, "invalid values fall back");
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
    fn rulers_accept_numbers_and_objects_and_drop_bad_entries() {
        let e = EditorSettings::from_configuration(&config(
            r##"{ "editor.rulers": [120, 80, { "column": 100, "color": "#ff000080" }, { "column": 4, "color": "red" },
                 "x", -1, 10.5, { "color": "#000000" }] }"##,
        ));
        assert_eq!(e.rulers, [
            Ruler { column: 4, color: None },
            Ruler { column: 80, color: None },
            Ruler { column: 100, color: Some(Rgba8(255, 0, 0, 0x80)) },
            Ruler { column: 120, color: None },
        ]);
        assert!(parse_rulers(Some(&json!("80"))).is_empty(), "not an array");
        assert!(!parse_highlight_active(Some(&json!(false))));
        assert!(parse_highlight_active(Some(&json!("always"))));
        assert!(parse_highlight_active(Some(&json!("sometimes"))), "invalid: default");
    }

    #[test]
    fn font_ligatures_boolean_and_string_forms() {
        let on = [("liga".to_string(), 1), ("calt".to_string(), 1)];
        assert_eq!(parse_font_ligatures(Some(&json!(true))), on);
        assert_eq!(parse_font_ligatures(Some(&json!("true"))), on);
        assert_eq!(parse_font_ligatures(Some(&json!("'ss01' 1, \"calt\" off, 'liga'"))), [
            ("ss01".to_string(), 1),
            ("calt".to_string(), 0),
            ("liga".to_string(), 1),
        ]);
        assert_eq!(parse_font_ligatures(Some(&json!("'ss01' 2, 'bad' x, 'toolong' 1"))), [("ss01".to_string(), 2)]);
        assert!(parse_font_ligatures(Some(&json!(""))).iter().all(|(_, v)| *v == 0));
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

    #[test]
    fn sash_size_is_clamped_to_upstream_range() {
        assert_eq!(LayoutSettings::from_configuration(&config("")).sash_size, 4.0);
        assert_eq!(LayoutSettings::from_configuration(&config(r#"{ "workbench.sash.size": 9 }"#)).sash_size, 9.0);
        assert_eq!(LayoutSettings::from_configuration(&config(r#"{ "workbench.sash.size": 0 }"#)).sash_size, 1.0);
        assert_eq!(LayoutSettings::from_configuration(&config(r#"{ "workbench.sash.size": 99 }"#)).sash_size, 20.0);
    }
}

#[cfg(test)]
mod search_tests {
    use vsr_search::SearchSettings;
    use vsr_search::settings::{CollapseResults, ResultSortOrder};
    use vsr_settings::{SettingsRegistry, UserSettings};

    use super::*;

    fn config(user: &str) -> Configuration {
        Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap())
    }

    #[test]
    fn defaults_are_the_upstream_defaults() {
        let s = search_settings(&config(""));
        let d = SearchSettings::default();
        assert_eq!(s.max_results, d.max_results);
        assert_eq!(s.smart_case, d.smart_case);
        assert_eq!(s.search_on_type_debounce_ms, 300);
        assert_eq!(s.collapse_results, CollapseResults::AlwaysExpand);
        assert_eq!(s.exclude_globs(false).len(), d.exclude_globs(false).len(), "{:?}", s.exclude_globs(false));
        assert!(s.exclude_globs(false).contains(&"**/node_modules".to_string()));
    }

    #[test]
    fn exclude_maps_merge_with_defaults_and_ignore_when_clauses() {
        let s = search_settings(&config(
            r#"{ "search.exclude": { "**/docs": true, "**/node_modules": false, "**/*.map": { "when": "$(basename).ts" } } }"#,
        ));
        let g = s.exclude_globs(false);
        assert!(g.contains(&"**/docs".to_string()));
        assert!(!g.contains(&"**/node_modules".to_string()), "false turns a default off");
        assert!(!g.iter().any(|x| x.ends_with(".map")), "when clauses are ignored: {g:?}");
        assert!(g.contains(&"**/bower_components".to_string()), "the rest of the default stays");
    }

    #[test]
    fn max_results_null_is_unlimited_and_bad_values_fall_back() {
        assert_eq!(search_settings(&config(r#"{ "search.maxResults": null }"#)).max_results, None);
        assert_eq!(search_settings(&config(r#"{ "search.maxResults": 7 }"#)).max_results, Some(7));
        assert_eq!(search_settings(&config(r#"{ "search.maxResults": "lots" }"#)).max_results, Some(20000));
    }

    #[test]
    fn toggles_and_enums_are_read() {
        let s = search_settings(&config(
            r#"{ "search.smartCase": true, "search.useIgnoreFiles": false, "search.followSymlinks": false,
                 "search.searchOnType": false, "search.searchOnTypeDebouncePeriod": 50,
                 "search.collapseResults": "alwaysCollapse", "search.showLineNumbers": true }"#,
        ));
        assert!(s.smart_case && !s.use_ignore_files && !s.follow_symlinks && !s.search_on_type && s.show_line_numbers);
        assert_eq!(s.search_on_type_debounce_ms, 50);
        assert_eq!(s.collapse_results, CollapseResults::AlwaysCollapse);
        let bad = search_settings(&config(r#"{ "search.collapseResults": "sideways" }"#));
        assert_eq!(bad.collapse_results, CollapseResults::AlwaysExpand);
    }

    #[test]
    fn threads_sort_and_seeding_are_read() {
        let s = search_settings(&config(
            r#"{ "search.ripgrep.maxThreads": 3, "search.sortOrder": "countAscending",
                 "search.seedOnFocus": true, "search.seedWithNearestWord": true }"#,
        ));
        assert_eq!(s.threads, 3);
        assert_eq!(s.sort_order, ResultSortOrder::CountAscending);
        assert!(s.seed_on_focus && s.seed_with_nearest_word);
        let bad = search_settings(&config(r#"{ "search.sortOrder": "random", "search.ripgrep.maxThreads": -2 }"#));
        assert_eq!((bad.sort_order, bad.threads), (ResultSortOrder::Default, 0));
    }
    #[test]
    fn typing_settings_default_to_upstream_and_are_read() {
        use vsr_editor::{AutoClosing, AutoClosingEdit, AutoIndentStrategy, AutoSurround};
        let d = EditorSettings::from_configuration(&config("{}"));
        assert_eq!(d.typing, vsr_editor::TypingOptions::default(), "the registered defaults");
        assert_eq!(d.typing.auto_indent, AutoIndentStrategy::Full);
        assert!(d.detect_indentation && d.empty_selection_clipboard && d.comments_insert_space && d.comments_ignore_empty_lines);
        assert_eq!(d.typing.indent_size, None, "indentSize `tabSize` follows the tab size");

        let e = EditorSettings::from_configuration(&config(
            r#"{ "editor.autoClosingBrackets": "never", "editor.autoClosingQuotes": "beforeWhitespace",
                 "editor.autoClosingComments": "always", "editor.autoClosingDelete": "never",
                 "editor.autoClosingOvertype": "always", "editor.autoSurround": "quotes", "editor.autoIndent": "brackets",
                 "editor.useTabStops": false, "editor.trimAutoWhitespace": false, "editor.trimWhitespaceOnDelete": true,
                 "editor.indentSize": 2, "editor.detectIndentation": false, "editor.emptySelectionClipboard": false,
                 "editor.comments.insertSpace": false, "editor.comments.ignoreEmptyLines": false }"#,
        ));
        let t = e.typing;
        assert_eq!((t.auto_closing_brackets, t.auto_closing_quotes, t.auto_closing_comments), (AutoClosing::Never, AutoClosing::BeforeWhitespace, AutoClosing::Always));
        assert_eq!((t.auto_closing_delete, t.auto_closing_overtype), (AutoClosingEdit::Never, AutoClosingEdit::Always));
        assert_eq!((t.auto_surround, t.auto_indent), (AutoSurround::Quotes, AutoIndentStrategy::Brackets));
        assert!(!t.use_tab_stops && !t.trim_auto_whitespace && t.trim_whitespace_on_delete);
        assert_eq!(t.indent_size, Some(2));
        assert!(!e.detect_indentation && !e.empty_selection_clipboard && !e.comments_insert_space && !e.comments_ignore_empty_lines);
        let bad = EditorSettings::from_configuration(&config(r#"{ "editor.autoIndent": "sideways", "editor.autoSurround": 3 }"#));
        assert_eq!((bad.typing.auto_indent, bad.typing.auto_surround), (AutoIndentStrategy::Full, AutoSurround::LanguageDefined));
    }
}
