//! A loaded colour theme: the colours of a VS Code theme JSON (with its `include` chain), the
//! user's `workbench.colorCustomizations` and `editor.tokenColorCustomizations` on top, and the
//! registry defaults for every colour the theme leaves unset.
//!
//! upstream: src/vs/workbench/services/themes/common/colorThemeData.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/themes/common/themeCompatibility.ts @ 50e1350fc78d
//!
//! Syntax highlighting is somebody else's job: the textmate rules are handed through untouched
//! ([`ColorTheme::token_colors`], [`TokenColorRule`]). Scope selectors are not matched here.
//! Not ported: tmTheme (plist) files, theme storage, transient colours, token style resolution.

use std::collections::HashMap;
use std::fmt;

use serde_json::{Map, Value};
use vsr_settings::jsonc::strip_jsonc;

use crate::color::{Color, Rgba8, parse_hex};
use crate::registry::{ColorLookup, ColorScheme, registry};

/// `DEFAULT_COLOR_CONFIG_VALUE`: a colour customised to this takes the registry default.
pub const DEFAULT_COLOR_CONFIG_VALUE: &str = "default";

/// Where theme files come from (the `IExtensionResourceLoaderService` of the loader).
pub trait ThemeSource {
    /// The text of the file at `path` (relative to the theme root, `/` separated).
    fn read(&self, path: &str) -> Option<String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    /// The file could not be read.
    Missing(String),
    /// `Problems parsing JSON theme file`.
    Parse { path: String, message: String },
    /// `Invalid format for JSON theme file: Object expected.` and friends.
    Invalid { path: String, message: String },
    /// `tokenColors` points at a tmTheme (plist) file: not supported.
    UnsupportedTextMateTheme(String),
}

impl fmt::Display for ThemeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeError::Missing(p) => write!(f, "Cannot read theme file {p}"),
            ThemeError::Parse { path, message } => write!(f, "Problems parsing JSON theme file {path}: {message}"),
            ThemeError::Invalid { path, message } => write!(f, "Problem parsing color theme file: {path}. {message}"),
            ThemeError::UnsupportedTextMateTheme(p) => write!(f, "tmTheme files ({p}) are not supported"),
        }
    }
}

impl std::error::Error for ThemeError {}

/// `ITextMateThemingRule.settings`. Values are as written in the theme (`foreground` is a hex
/// string, `fontStyle` a space separated list of `bold italic underline strikethrough`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenSettings {
    pub foreground: Option<String>,
    pub background: Option<String>,
    pub font_style: Option<String>,
}

/// `ITextMateThemingRule`: one textmate theme rule, as the theme JSON gives it.
///
/// `scope` holds the selector strings exactly as written: a JSON string is one entry (it may
/// contain commas and the `-` / `>` / space operators of the textmate selector grammar), a JSON
/// array is one entry per element. Empty for the default rule (no scope).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenColorRule {
    pub name: Option<String>,
    pub scope: Vec<String>,
    pub settings: TokenSettings,
}

/// A semantic token rule (`semanticTokenColors`): the selector as written (`type.modifier:language`)
/// and its style. Not matched here.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SemanticTokenRule {
    pub selector: String,
    pub foreground: Option<String>,
    pub font_style: Option<String>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
}

/// What a theme file chain contributes.
#[derive(Clone, Debug, Default)]
struct Loaded {
    colors: HashMap<String, Color>,
    token_colors: Vec<TokenColorRule>,
    semantic_rules: Vec<SemanticTokenRule>,
    semantic_highlighting: bool,
}

/// The customisations of the user's settings: `workbench.colorCustomizations`,
/// `editor.tokenColorCustomizations` and `editor.semanticTokenColorCustomizations` (any may be
/// `Value::Null`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Customizations {
    pub colors: Value,
    pub token_colors: Value,
    pub semantic_token_colors: Value,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Custom {
    Color(Color),
    Default,
}

/// `ColorThemeData`.
#[derive(Clone, Debug)]
pub struct ColorTheme {
    id: String,
    label: String,
    settings_id: String,
    scheme: ColorScheme,
    theme: Loaded,
    custom_colors: HashMap<String, Custom>,
    custom_token_colors: Vec<TokenColorRule>,
    custom_semantic_rules: Vec<SemanticTokenRule>,
    custom_semantic_highlighting: Option<bool>,
    custom_semantic_highlighting_deprecated: Option<bool>,
    token_colors: Vec<TokenColorRule>,
}

const TOKEN_GROUP_SCOPES: &[(&str, &[&str])] = &[
    ("comments", &["comment", "punctuation.definition.comment"]),
    ("strings", &["string", "meta.embedded.assembly"]),
    ("keywords", &["keyword - keyword.operator", "keyword.control", "storage", "storage.type"]),
    ("numbers", &["constant.numeric"]),
    ("types", &["entity.name.type", "entity.name.class", "support.type", "support.class"]),
    ("functions", &["entity.name.function", "support.function"]),
    ("variables", &["variable", "entity.name.variable"]),
];

impl ColorTheme {
    /// A theme with no colours of its own (every colour is a registry default), of the given type.
    pub fn empty(id: &str, label: &str, settings_id: &str, scheme: ColorScheme) -> Self {
        let mut t = Self {
            id: id.to_string(),
            label: label.to_string(),
            settings_id: settings_id.to_string(),
            scheme,
            theme: Loaded::default(),
            custom_colors: HashMap::new(),
            custom_token_colors: Vec::new(),
            custom_semantic_rules: Vec::new(),
            custom_semantic_highlighting: None,
            custom_semantic_highlighting_deprecated: None,
            token_colors: Vec::new(),
        };
        t.rebuild_token_colors();
        t
    }

    /// Loads the theme JSON at `path` and everything it `include`s.
    pub fn load(id: &str, label: &str, settings_id: &str, scheme: ColorScheme, path: &str, source: &dyn ThemeSource) -> Result<Self, ThemeError> {
        let mut loaded = Loaded::default();
        load_theme_file(source, path, &mut loaded, 0)?;
        let mut t = Self::empty(id, label, settings_id, scheme);
        t.theme = loaded;
        t.rebuild_token_colors();
        Ok(t)
    }

    /// A theme from colours given directly (tests, programmatic themes): key -> `#hex`.
    pub fn from_colors(id: &str, scheme: ColorScheme, colors: &[(&str, &str)]) -> Self {
        let mut t = Self::empty(id, "", &format!("__{id}"), scheme);
        for (k, v) in colors {
            t.theme.colors.insert((*k).to_string(), Color::from_hex(v));
        }
        t.rebuild_token_colors();
        t
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn label(&self) -> &str {
        &self.label
    }

    /// The value of `workbench.colorTheme` that selects this theme.
    pub fn settings_id(&self) -> &str {
        &self.settings_id
    }

    /// `ColorThemeData.type`.
    pub fn scheme(&self) -> ColorScheme {
        self.scheme
    }

    /// `getColor(colorId)`: the customisation, else the theme's colour, else the registry default.
    pub fn color(&self, key: &str) -> Option<Color> {
        self.get_color(key, true)
    }

    /// `getColor(colorId, useDefault)`.
    pub fn get_color(&self, key: &str, use_default: bool) -> Option<Color> {
        match self.custom_colors.get(key) {
            Some(Custom::Color(c)) => return Some(*c),
            Some(Custom::Default) => {}
            None => {
                if let Some(c) = self.theme.colors.get(key) {
                    return Some(*c);
                }
            }
        }
        if use_default { self.default_color(key) } else { None }
    }

    /// `getDefault(colorId)`: the registry default for this theme's type.
    pub fn default_color(&self, key: &str) -> Option<Color> {
        registry().resolve_default(key, self.scheme, self)
    }

    /// `defines(colorId)`: the theme or a customisation sets the colour (a default does not count).
    pub fn defines(&self, key: &str) -> bool {
        match self.custom_colors.get(key) {
            Some(Custom::Color(_)) => true,
            Some(Custom::Default) => false,
            None => self.theme.colors.contains_key(key),
        }
    }

    /// `getColorCustomization(colorId)`.
    pub fn color_customization(&self, key: &str) -> Option<Color> {
        match self.custom_colors.get(key) {
            Some(Custom::Color(c)) => Some(*c),
            _ => None,
        }
    }

    /// The colour as the UI paints it.
    pub fn rgba8(&self, key: &str) -> Option<Rgba8> {
        self.color(key).map(|c| c.to_rgba8())
    }

    /// The colours the theme file chain sets (not the defaults).
    pub fn theme_colors(&self) -> impl Iterator<Item = (&str, &Color)> {
        self.theme.colors.iter().map(|(k, v)| (k.as_str(), v))
    }

    /// `ColorThemeData.tokenColors`: the textmate rules for the highlighter, in the order they
    /// apply (later rules win on equal specificity). First the default rule (no scope: the editor
    /// foreground and background), then the theme's rules, then the user's
    /// `editor.tokenColorCustomizations`, then `token.info-token` and friends unless a rule defines
    /// them. Colours are normalised to upper case `#RRGGBB[AA]`.
    pub fn token_colors(&self) -> &[TokenColorRule] {
        &self.token_colors
    }

    /// The theme's own `semanticTokenColors`, then the user's `editor.semanticTokenColorCustomizations`
    /// rules: later entries win.
    pub fn semantic_token_rules(&self) -> impl Iterator<Item = &SemanticTokenRule> {
        self.theme.semantic_rules.iter().chain(self.custom_semantic_rules.iter())
    }

    /// `semanticHighlighting`: whether semantic tokens colour the text.
    pub fn semantic_highlighting(&self) -> bool {
        self.custom_semantic_highlighting.or(self.custom_semantic_highlighting_deprecated).unwrap_or(self.theme.semantic_highlighting)
    }

    // ---- customisations --------------------------------------------------------------------

    /// `setCustomizations`: replaces what the user's settings customise.
    pub fn set_customizations(&mut self, c: &Customizations) {
        self.set_custom_colors(&c.colors);
        self.set_custom_token_colors(&c.token_colors);
        self.set_custom_semantic_token_colors(&c.semantic_token_colors);
        self.rebuild_token_colors();
    }

    /// This theme with the customisations applied.
    pub fn with_customizations(mut self, c: &Customizations) -> Self {
        self.set_customizations(c);
        self
    }

    fn set_custom_colors(&mut self, colors: &Value) {
        self.custom_colors.clear();
        let Some(obj) = colors.as_object() else { return };
        self.overwrite_custom_colors(obj);
        if let Some(specific) = self.theme_specific(obj) {
            self.overwrite_custom_colors(&specific);
        }
    }

    fn overwrite_custom_colors(&mut self, colors: &Map<String, Value>) {
        for (id, val) in colors {
            match val.as_str() {
                Some(DEFAULT_COLOR_CONFIG_VALUE) => {
                    self.custom_colors.insert(id.clone(), Custom::Default);
                }
                Some(hex) => {
                    self.custom_colors.insert(id.clone(), Custom::Color(Color::from_hex(hex)));
                }
                None => {}
            }
        }
    }

    fn set_custom_token_colors(&mut self, customs: &Value) {
        self.custom_token_colors.clear();
        self.custom_semantic_highlighting_deprecated = None;
        let Some(obj) = customs.as_object() else { return };
        self.add_custom_token_colors(obj);
        if let Some(specific) = self.theme_specific(obj) {
            self.add_custom_token_colors(&specific);
        }
    }

    fn add_custom_token_colors(&mut self, customs: &Map<String, Value>) {
        // The general groups first, so that a specific textMateRules entry can override them.
        for (group, scopes) in TOKEN_GROUP_SCOPES {
            let settings = match customs.get(*group) {
                Some(Value::String(s)) if !s.is_empty() => TokenSettings { foreground: Some(s.clone()), ..Default::default() },
                Some(v @ Value::Object(_)) => token_settings(v),
                _ => continue,
            };
            for scope in *scopes {
                self.custom_token_colors.push(TokenColorRule { name: None, scope: vec![(*scope).to_string()], settings: settings.clone() });
            }
        }
        if let Some(Value::Array(rules)) = customs.get("textMateRules") {
            for rule in rules.iter().filter_map(token_rule) {
                if !rule.scope.is_empty() {
                    self.custom_token_colors.push(rule);
                }
            }
        }
        if let Some(Value::Bool(b)) = customs.get("semanticHighlighting") {
            self.custom_semantic_highlighting_deprecated = Some(*b);
        }
    }

    fn set_custom_semantic_token_colors(&mut self, customs: &Value) {
        self.custom_semantic_rules.clear();
        self.custom_semantic_highlighting = None;
        let Some(obj) = customs.as_object() else { return };
        self.custom_semantic_highlighting = obj.get("enabled").and_then(Value::as_bool);
        if let Some(Value::Object(rules)) = obj.get("rules") {
            self.read_semantic_rules(rules);
        }
        if let Some(specific) = self.theme_specific(obj) {
            if let Some(e) = specific.get("enabled").and_then(Value::as_bool) {
                self.custom_semantic_highlighting = Some(e);
            }
            if let Some(Value::Object(rules)) = specific.get("rules") {
                self.read_semantic_rules(rules);
            }
        }
    }

    fn read_semantic_rules(&mut self, rules: &Map<String, Value>) {
        for (selector, v) in rules {
            if is_theme_scope(selector) {
                continue;
            }
            if let Some(rule) = semantic_rule(selector, v) {
                self.custom_semantic_rules.push(rule);
            }
        }
    }

    /// `isThemeScopeMatch`: does `theme_id` (from `[theme id]`, may use `*` at its ends) select this theme?
    pub fn is_theme_scope_match(&self, theme_id: &str) -> bool {
        theme_scope_matches(theme_id, &self.settings_id)
    }

    /// `getThemeSpecificColors`: the merged contents of the `[theme]` blocks of `colors` that apply to this theme.
    fn theme_specific(&self, colors: &Map<String, Value>) -> Option<Map<String, Value>> {
        let mut result: Option<Map<String, Value>> = None;
        for (key, scoped) in colors {
            let Value::Object(scoped) = scoped else { continue };
            if !is_theme_scope(key) {
                continue;
            }
            for theme_id in theme_scopes(key) {
                if !self.is_theme_scope_match(theme_id) {
                    continue;
                }
                let target = result.get_or_insert_with(Map::new);
                for (subkey, over) in scoped {
                    match (target.get_mut(subkey), over) {
                        (Some(Value::Array(orig)), Value::Array(extra)) => orig.extend(extra.iter().cloned()),
                        (_, over) if truthy(over) => {
                            target.insert(subkey.clone(), over.clone());
                        }
                        _ => {}
                    }
                }
            }
        }
        result
    }

    fn rebuild_token_colors(&mut self) {
        let mut result = Vec::new();
        // The default rule (no scope) is always first.
        let foreground = self.color("editor.foreground");
        let background = self.color("editor.background");
        result.push(TokenColorRule {
            name: None,
            scope: Vec::new(),
            settings: TokenSettings {
                foreground: foreground.and_then(|c| normalize_color(&c.format_hex_a(true))),
                background: background.and_then(|c| normalize_color(&c.format_hex_a(true))),
                font_style: None,
            },
        });
        let has_default_tokens = self
            .theme
            .token_colors
            .iter()
            .chain(self.custom_token_colors.iter())
            .any(|r| r.scope.len() == 1 && r.scope[0] == "token.info-token");
        let add = |result: &mut Vec<TokenColorRule>, rule: &TokenColorRule| {
            if rule.scope.is_empty() {
                return;
            }
            result.push(TokenColorRule {
                name: rule.name.clone(),
                scope: rule.scope.clone(),
                settings: TokenSettings {
                    foreground: rule.settings.foreground.as_deref().and_then(normalize_color),
                    background: rule.settings.background.as_deref().and_then(normalize_color),
                    font_style: rule.settings.font_style.clone(),
                },
            });
        };
        for rule in self.theme.token_colors.iter().chain(self.custom_token_colors.iter()) {
            add(&mut result, rule);
        }
        if !has_default_tokens {
            for (scope, fg) in default_token_colors(self.scheme) {
                add(
                    &mut result,
                    &TokenColorRule {
                        name: None,
                        scope: vec![(*scope).to_string()],
                        settings: TokenSettings { foreground: Some((*fg).to_string()), ..Default::default() },
                    },
                );
            }
        }
        self.token_colors = result;
    }
}

impl ColorLookup for ColorTheme {
    fn color(&self, key: &str) -> Option<Color> {
        ColorTheme::color(self, key)
    }
    fn defines(&self, key: &str) -> bool {
        ColorTheme::defines(self, key)
    }
}

/// `defaultThemeColors`: the colours of `token.info-token` and friends when no rule sets them.
fn default_token_colors(scheme: ColorScheme) -> &'static [(&'static str, &'static str)] {
    match scheme {
        ColorScheme::Light | ColorScheme::HighContrastLight => &[
            ("token.info-token", "#316bcd"),
            ("token.warn-token", "#cd9731"),
            ("token.error-token", "#cd3131"),
            ("token.debug-token", "#800080"),
        ],
        ColorScheme::Dark => &[
            ("token.info-token", "#6796e6"),
            ("token.warn-token", "#cd9731"),
            ("token.error-token", "#f44747"),
            ("token.debug-token", "#b267e6"),
        ],
        ColorScheme::HighContrastDark => &[
            ("token.info-token", "#6796e6"),
            ("token.warn-token", "#008000"),
            ("token.error-token", "#FF0000"),
            ("token.debug-token", "#b267e6"),
        ],
    }
}

/// `normalizeColor`: upper case `#RRGGBB` / `#RRGGBBAA` (short forms expanded, an opaque alpha of
/// `FF` dropped); `None` for anything that is not a hex colour.
pub fn normalize_color(color: &str) -> Option<String> {
    let b = color.as_bytes();
    if b.first() != Some(&b'#') || !matches!(b.len(), 4 | 5 | 7 | 9) || !color.is_ascii() {
        return None;
    }
    let mut out = String::from("#");
    for &c in &b[1..] {
        if !c.is_ascii_hexdigit() {
            return None;
        }
        let up = c.to_ascii_uppercase() as char;
        out.push(up);
        if b.len() == 4 || b.len() == 5 {
            out.push(up);
        }
    }
    if out.len() == 9 && out.ends_with("FF") {
        out.truncate(7);
    }
    Some(out)
}

// ---- theme scopes (`"[Theme Name]": { ... }`) ----------------------------------------------------

/// `isThemeScope`.
pub fn is_theme_scope(key: &str) -> bool {
    key.starts_with('[') && key.ends_with(']')
}

/// The theme ids of a `[a][b]` key (`themeScopeRegex`: `\[(.+?)\]`).
pub fn theme_scopes(key: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = key;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        // `.+?` needs at least one character before the closing bracket.
        let Some(close) = after.char_indices().skip(1).find(|(_, c)| *c == ']').map(|(i, _)| i) else { break };
        out.push(&after[..close]);
        rest = &after[close + 1..];
    }
    out
}

/// `isThemeScopeMatch` for a theme whose settings id is `settings_id`.
pub fn theme_scope_matches(theme_id: &str, settings_id: &str) -> bool {
    let first = theme_id.chars().next();
    let last = theme_id.chars().last();
    let chars: Vec<char> = theme_id.chars().collect();
    let sub = |from: usize, to: usize| -> String { chars.get(from..to.max(from)).map(|s| s.iter().collect()).unwrap_or_default() };
    let n = chars.len();
    let prefix = sub(0, n.saturating_sub(1));
    let infix = sub(1, n.saturating_sub(1));
    let suffix = sub(1, n);
    theme_id == settings_id
        || (settings_id.contains(&infix) && first == Some('*') && last == Some('*'))
        || (settings_id.starts_with(&prefix) && last == Some('*'))
        || (settings_id.ends_with(&suffix) && first == Some('*'))
}

/// JS truthiness of a JSON value (what `else if (overrideColors)` tests).
fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().is_some_and(|f| f != 0.0),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

// ---- loading -------------------------------------------------------------------------------------

/// Joins `dir/include` and resolves `.` and `..`.
fn join_path(dir: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = dir.split('/').filter(|p| !p.is_empty()).collect();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn dirname(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(d, _)| d)
}

/// Include chains deeper than this are cut (a theme that includes itself).
const MAX_INCLUDE_DEPTH: u32 = 16;

fn load_theme_file(source: &dyn ThemeSource, path: &str, out: &mut Loaded, depth: u32) -> Result<(), ThemeError> {
    if !path.ends_with(".json") {
        return Err(ThemeError::UnsupportedTextMateTheme(path.to_string()));
    }
    let text = source.read(path).ok_or_else(|| ThemeError::Missing(path.to_string()))?;
    let value: Value = serde_json::from_str(&strip_jsonc(&text)).map_err(|e| ThemeError::Parse { path: path.to_string(), message: e.to_string() })?;
    let Some(obj) = value.as_object() else {
        return Err(ThemeError::Invalid { path: path.to_string(), message: "Object expected.".to_string() });
    };
    let invalid = |message: &str| ThemeError::Invalid { path: path.to_string(), message: message.to_string() };
    if let Some(include) = obj.get("include").and_then(Value::as_str) {
        if depth >= MAX_INCLUDE_DEPTH {
            return Err(invalid("The include chain is too deep."));
        }
        load_theme_file(source, &join_path(dirname(path), include), out, depth + 1)?;
    }
    if let Some(Value::Array(settings)) = obj.get("settings") {
        convert_settings(settings, out);
        return Ok(());
    }
    out.semantic_highlighting = out.semantic_highlighting || obj.get("semanticHighlighting").is_some_and(truthy);
    match obj.get("colors") {
        None | Some(Value::Null) => {}
        Some(Value::Object(colors)) => {
            for (id, val) in colors {
                match val.as_str() {
                    Some(DEFAULT_COLOR_CONFIG_VALUE) => {
                        out.colors.remove(id);
                    }
                    Some(hex) => {
                        out.colors.insert(id.clone(), Color::from_hex(hex));
                    }
                    None => {}
                }
            }
        }
        Some(_) => return Err(invalid("Property 'colors' is not of type 'object'.")),
    }
    match obj.get("tokenColors") {
        None | Some(Value::Null) => {}
        Some(Value::Array(rules)) => out.token_colors.extend(rules.iter().filter_map(token_rule)),
        Some(Value::String(p)) => return Err(ThemeError::UnsupportedTextMateTheme(join_path(dirname(path), p))),
        Some(_) => {
            return Err(invalid("Property 'tokenColors' should be either an array specifying colors or a path to a TextMate theme file"));
        }
    }
    if let Some(Value::Object(rules)) = obj.get("semanticTokenColors") {
        for (selector, v) in rules {
            if let Some(rule) = semantic_rule(selector, v) {
                out.semantic_rules.push(rule);
            }
        }
    }
    Ok(())
}

/// `convertSettings` (the old `settings` array of tmTheme-like JSON): the global rule's colours
/// become workbench colours.
fn convert_settings(settings: &[Value], out: &mut Loaded) {
    for rule in settings.iter().filter_map(token_rule_raw) {
        if rule.0.scope.is_empty() {
            for (key, value) in &rule.1 {
                if let (Some(ids), Some(hex)) = (setting_to_color_ids(key), value.as_str()) {
                    for id in ids {
                        out.colors.insert((*id).to_string(), Color::from_hex(hex));
                    }
                }
            }
        }
        out.token_colors.push(rule.0);
    }
}

fn setting_to_color_ids(key: &str) -> Option<&'static [&'static str]> {
    Some(match key {
        "background" => &["editor.background"],
        "foreground" => &["editor.foreground"],
        "selection" => &["editor.selectionBackground"],
        "inactiveSelection" => &["editor.inactiveSelectionBackground"],
        "selectionHighlightColor" => &["editor.selectionHighlightBackground"],
        "findMatchHighlight" => &["editor.findMatchHighlightBackground", "peekViewResult.matchHighlightBackground"],
        "currentFindMatchHighlight" => &["editor.findMatchBackground"],
        "hoverHighlight" => &["editor.hoverHighlightBackground"],
        "wordHighlight" => &["editor.wordHighlightBackground"],
        "wordHighlightStrong" => &["editor.wordHighlightStrongBackground"],
        "findRangeHighlight" => &["editor.findRangeHighlightBackground"],
        "referenceHighlight" => &["peekViewEditor.matchHighlightBackground"],
        "lineHighlight" => &["editor.lineHighlightBackground"],
        "rangeHighlight" => &["editor.rangeHighlightBackground"],
        "caret" => &["editorCursor.foreground"],
        "invisibles" => &["editorWhitespace.foreground"],
        "guide" => &["editorIndentGuide.background1"],
        "activeGuide" => &["editorIndentGuide.activeBackground1"],
        "ansiBlack" => &["terminal.ansiBlack"],
        "ansiRed" => &["terminal.ansiRed"],
        "ansiGreen" => &["terminal.ansiGreen"],
        "ansiYellow" => &["terminal.ansiYellow"],
        "ansiBlue" => &["terminal.ansiBlue"],
        "ansiMagenta" => &["terminal.ansiMagenta"],
        "ansiCyan" => &["terminal.ansiCyan"],
        "ansiWhite" => &["terminal.ansiWhite"],
        "ansiBrightBlack" => &["terminal.ansiBrightBlack"],
        "ansiBrightRed" => &["terminal.ansiBrightRed"],
        "ansiBrightGreen" => &["terminal.ansiBrightGreen"],
        "ansiBrightYellow" => &["terminal.ansiBrightYellow"],
        "ansiBrightBlue" => &["terminal.ansiBrightBlue"],
        "ansiBrightMagenta" => &["terminal.ansiBrightMagenta"],
        "ansiBrightCyan" => &["terminal.ansiBrightCyan"],
        "ansiBrightWhite" => &["terminal.ansiBrightWhite"],
        _ => return None,
    })
}

fn string_of(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn token_settings(v: &Value) -> TokenSettings {
    TokenSettings { foreground: string_of(v, "foreground"), background: string_of(v, "background"), font_style: string_of(v, "fontStyle") }
}

/// One entry of `tokenColors` / `textMateRules`. Entries that are not objects are skipped.
fn token_rule(v: &Value) -> Option<TokenColorRule> {
    token_rule_raw(v).map(|(r, _)| r)
}

/// The rule and its raw `settings` object (for the legacy `settings` conversion).
fn token_rule_raw(v: &Value) -> Option<(TokenColorRule, Map<String, Value>)> {
    let obj = v.as_object()?;
    let scope = match obj.get("scope") {
        Some(Value::String(s)) if !s.is_empty() => vec![s.clone()],
        Some(Value::Array(a)) => a.iter().filter_map(|s| s.as_str().map(str::to_string)).collect(),
        _ => Vec::new(),
    };
    let raw = obj.get("settings").and_then(Value::as_object).cloned().unwrap_or_default();
    let settings = obj.get("settings").map(token_settings).unwrap_or_default();
    Some((TokenColorRule { name: string_of(v, "name"), scope, settings }, raw))
}

/// `readSemanticTokenRule`: a string is a foreground, an object has `foreground`, `fontStyle` and the
/// boolean flags; anything else is no rule.
fn semantic_rule(selector: &str, v: &Value) -> Option<SemanticTokenRule> {
    let mut rule = SemanticTokenRule { selector: selector.to_string(), ..Default::default() };
    match v {
        Value::String(s) => rule.foreground = Some(s.clone()),
        Value::Object(o) => {
            let flag = |k: &str| o.get(k).and_then(Value::as_bool);
            rule.foreground = string_of(v, "foreground");
            rule.font_style = string_of(v, "fontStyle");
            (rule.bold, rule.italic, rule.underline, rule.strikethrough) = (flag("bold"), flag("italic"), flag("underline"), flag("strikethrough"));
            let valid = rule.foreground.is_some() || rule.font_style.is_some() || rule.bold.is_some() || rule.italic.is_some() || rule.underline.is_some() || rule.strikethrough.is_some();
            if !valid {
                return None;
            }
        }
        _ => return None,
    }
    Some(rule)
}

/// Parses `#RGB`, `#RGBA`, `#RRGGBB` or `#RRGGBBAA` strictly (a non-hex digit is `None`, unlike
/// upstream's `parseHex`, which reads it as 0): for colours typed by the user.
pub fn hex(s: &str) -> Option<Rgba8> {
    let digits = s.strip_prefix('#')?;
    if !digits.bytes().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    parse_hex(s).map(|c| c.to_rgba8())
}
