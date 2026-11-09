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
use vsr_search::settings::{CollapseResults, ExcludeMap};
use vsr_settings::Configuration;

use super::minimap::MinimapSettings;
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

/// The `editor.find.*` settings that change how the Find widget behaves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FindSettings {
    pub seed_search_string_from_selection: SeedSearchString,
    pub auto_find_in_selection: AutoFindInSelection,
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
    pub render_whitespace: RenderWhitespace,
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
    pub trim_trailing_whitespace: bool,
    pub insert_final_newline: bool,
    pub find: FindSettings,
    pub minimap: MinimapSettings,
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
            render_whitespace: RenderWhitespace::Selection,
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
            trim_trailing_whitespace: false,
            insert_final_newline: false,
            find: FindSettings::default(),
            minimap: MinimapSettings::default(),
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
            render_whitespace: c.get_enum("editor.renderWhitespace").unwrap_or(RenderWhitespace::Selection),
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
            trim_trailing_whitespace: c.get_bool("files.trimTrailingWhitespace").unwrap_or(false),
            insert_final_newline: c.get_bool("files.insertFinalNewline").unwrap_or(false),
            find: FindSettings::from_configuration(c),
            minimap: MinimapSettings::from_configuration(c),
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
        if self.cursor_width > 0.0 { self.cursor_width.min(self.char_width()) } else { 2.0 }
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
                 "editor.find.closeOnResult": true }"#,
        ));
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
}

#[cfg(test)]
mod search_tests {
    use vsr_search::SearchSettings;
    use vsr_search::settings::CollapseResults;
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
}
