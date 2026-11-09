//! Theme scope matching: rule selectors, specificity, and per-scope style resolution.
//!
//! upstream: src/vs/workbench/services/textMate/common/TMHelper.ts @ 50e1350fc78d
//! upstream: vscode-textmate src/theme.ts (microsoft/vscode-textmate, main, 2026-11-09)
//! upstream: vscode-textmate src/tests/themes.test.ts
//!
//! A theme is a list of `tokenColors` rules (a scope selector, optionally with parent scopes,
//! plus foreground / background / fontStyle). `Theme::match_scopes` finds the most specific rule
//! for a scope stack; `ThemeStyles` merges those along a stack the way vscode-textmate's
//! `AttributedScopeStack` does (each scope's rule overrides only what it sets).

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;

use crate::scope::ScopeId;

/// Font style flags (upstream `FontStyle`). `NOT_SET` is -1.
pub mod font_style {
    pub const NOT_SET: i32 = -1;
    pub const NONE: i32 = 0;
    pub const ITALIC: i32 = 1;
    pub const BOLD: i32 = 2;
    pub const UNDERLINE: i32 = 4;
    pub const STRIKETHROUGH: i32 = 8;
}

/// A `tokenColors` entry as a theme source provides it (upstream `IRawThemeSetting`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawThemeRule {
    /// A scope selector string (comma separated alternatives) or a list of them; empty for a
    /// rule without `scope` (the theme defaults).
    pub scope: Vec<String>,
    pub scope_is_string: bool,
    pub font_style: Option<String>,
    pub foreground: Option<String>,
    pub background: Option<String>,
}

impl RawThemeRule {
    pub fn new(scope: &str, foreground: Option<&str>, font_style: Option<&str>) -> Self {
        Self {
            scope: vec![scope.to_string()],
            scope_is_string: true,
            font_style: font_style.map(str::to_string),
            foreground: foreground.map(str::to_string),
            background: None,
        }
    }
}

/// Where a theme's `tokenColors` come from. The app implements this on the active colour theme
/// (`vsr-app`'s theme service, ADR-0037); `vsr-syntax::BundledTheme` implements it for tests.
pub trait TokenColorsProvider {
    fn token_colors(&self) -> Vec<RawThemeRule>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    /// `#RGB`, `#RGBA`, `#RRGGBB`, `#RRGGBBAA` (upstream `isValidHexColor`).
    pub fn parse_hex(s: &str) -> Option<Rgba> {
        let h = s.strip_prefix('#')?;
        if !h.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let nib = |i: usize| u8::from_str_radix(&h[i..i + 1], 16).ok();
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        match h.len() {
            3 => Some(Rgba([nib(0)? * 17, nib(1)? * 17, nib(2)? * 17, 255])),
            4 => Some(Rgba([nib(0)? * 17, nib(1)? * 17, nib(2)? * 17, nib(3)? * 17])),
            6 => Some(Rgba([byte(0)?, byte(2)?, byte(4)?, 255])),
            8 => Some(Rgba([byte(0)?, byte(2)?, byte(4)?, byte(6)?])),
            _ => None,
        }
    }
}

/// The resolved style of a token. `None` colors mean "the editor's default".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TokenStyle {
    pub foreground: Option<Rgba>,
    pub background: Option<Rgba>,
    /// `font_style` flags; `0` for none.
    pub font_style: u8,
}

impl TokenStyle {
    pub fn bold(&self) -> bool {
        self.font_style & font_style::BOLD as u8 != 0
    }
    pub fn italic(&self) -> bool {
        self.font_style & font_style::ITALIC as u8 != 0
    }
    pub fn underline(&self) -> bool {
        self.font_style & font_style::UNDERLINE as u8 != 0
    }
    pub fn strikethrough(&self) -> bool {
        self.font_style & font_style::STRIKETHROUGH as u8 != 0
    }
}

/// upstream `ParsedThemeRule`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedThemeRule {
    pub scope: String,
    pub parent_scopes: Option<Vec<String>>,
    pub index: usize,
    pub font_style: i32,
    pub foreground: Option<String>,
    pub background: Option<String>,
}

fn is_valid_hex_color(s: &str) -> bool {
    Rgba::parse_hex(s).is_some()
}

/// upstream `parseTheme`.
pub fn parse_theme(rules: &[RawThemeRule]) -> Vec<ParsedThemeRule> {
    let mut result = Vec::new();
    for (i, entry) in rules.iter().enumerate() {
        let scopes: Vec<String> = if entry.scope.is_empty() {
            vec![String::new()]
        } else if entry.scope_is_string {
            let s = entry.scope[0].trim_start_matches(',').trim_end_matches(',');
            s.split(',').map(str::to_string).collect()
        } else {
            entry.scope.clone()
        };
        let mut fs = font_style::NOT_SET;
        if let Some(f) = &entry.font_style {
            fs = font_style::NONE;
            for seg in f.split(' ') {
                match seg {
                    "italic" => fs |= font_style::ITALIC,
                    "bold" => fs |= font_style::BOLD,
                    "underline" => fs |= font_style::UNDERLINE,
                    "strikethrough" => fs |= font_style::STRIKETHROUGH,
                    _ => {}
                }
            }
        }
        let fg = entry.foreground.clone().filter(|c| is_valid_hex_color(c));
        let bg = entry.background.clone().filter(|c| is_valid_hex_color(c));
        for s in scopes {
            let trimmed = s.trim();
            let segments: Vec<&str> = trimmed.split(' ').collect();
            let scope = segments[segments.len() - 1].to_string();
            let parent_scopes = (segments.len() > 1).then(|| {
                let mut p: Vec<String> = segments[..segments.len() - 1].iter().map(|s| s.to_string()).collect();
                p.reverse();
                p
            });
            result.push(ParsedThemeRule { scope, parent_scopes, index: i, font_style: fs, foreground: fg.clone(), background: bg.clone() });
        }
    }
    result
}

pub fn str_arr_cmp(a: Option<&[String]>, b: Option<&[String]>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(a), Some(b)) => {
            let len = a.len().min(b.len());
            for i in 0..len {
                let c = a[i].cmp(&b[i]);
                if c != Ordering::Equal {
                    return c;
                }
            }
            a.len().cmp(&b.len())
        }
    }
}

#[derive(Debug, Clone)]
struct TrieRule {
    scope_depth: usize,
    parent_scopes: Vec<String>,
    font_style: i32,
    /// Color ids (0 = not set).
    foreground: u32,
    background: u32,
}

impl TrieRule {
    fn accept_overwrite(&mut self, scope_depth: usize, font_style: i32, foreground: u32, background: u32) {
        if self.scope_depth <= scope_depth {
            self.scope_depth = scope_depth;
        }
        if font_style != font_style::NOT_SET {
            self.font_style = font_style;
        }
        if foreground != 0 {
            self.foreground = foreground;
        }
        if background != 0 {
            self.background = background;
        }
    }
}

#[derive(Debug, Clone)]
struct TrieElement {
    main_rule: TrieRule,
    rules_with_parent_scopes: Vec<TrieRule>,
    children: HashMap<String, TrieElement>,
}

fn cmp_by_specificity(a: &TrieRule, b: &TrieRule) -> Ordering {
    if a.scope_depth != b.scope_depth {
        return b.scope_depth.cmp(&a.scope_depth);
    }
    let (mut ai, mut bi) = (0usize, 0usize);
    loop {
        if a.parent_scopes.get(ai).map(String::as_str) == Some(">") {
            ai += 1;
        }
        if b.parent_scopes.get(bi).map(String::as_str) == Some(">") {
            bi += 1;
        }
        if ai >= a.parent_scopes.len() || bi >= b.parent_scopes.len() {
            break;
        }
        let diff = b.parent_scopes[bi].len().cmp(&a.parent_scopes[ai].len());
        if diff != Ordering::Equal {
            return diff;
        }
        ai += 1;
        bi += 1;
    }
    b.parent_scopes.len().cmp(&a.parent_scopes.len())
}

impl TrieElement {
    fn new(main_rule: TrieRule, rules: Vec<TrieRule>) -> Self {
        Self { main_rule, rules_with_parent_scopes: rules, children: HashMap::new() }
    }

    fn matches(&self, scope: &str) -> Vec<TrieRule> {
        if !scope.is_empty() {
            let (head, tail) = scope.split_once('.').unwrap_or((scope, ""));
            if let Some(child) = self.children.get(head) {
                return child.matches(tail);
            }
        }
        let mut rules: Vec<TrieRule> = self.rules_with_parent_scopes.clone();
        rules.push(self.main_rule.clone());
        // A stable sort: upstream's Array.sort is stable too.
        rules.sort_by(cmp_by_specificity);
        rules
    }

    fn insert(&mut self, scope_depth: usize, scope: &str, parent_scopes: Option<&[String]>, font_style: i32, foreground: u32, background: u32) {
        if scope.is_empty() {
            self.insert_here(scope_depth, parent_scopes, font_style, foreground, background);
            return;
        }
        let (head, tail) = scope.split_once('.').unwrap_or((scope, ""));
        let main = self.main_rule.clone();
        let rules = self.rules_with_parent_scopes.clone();
        let child = self.children.entry(head.to_string()).or_insert_with(|| TrieElement::new(main, rules));
        child.insert(scope_depth + 1, tail, parent_scopes, font_style, foreground, background);
    }

    fn insert_here(&mut self, scope_depth: usize, parent_scopes: Option<&[String]>, mut font_style: i32, mut foreground: u32, mut background: u32) {
        let Some(parent_scopes) = parent_scopes else {
            self.main_rule.accept_overwrite(scope_depth, font_style, foreground, background);
            return;
        };
        for rule in &mut self.rules_with_parent_scopes {
            if str_arr_cmp(Some(&rule.parent_scopes), Some(parent_scopes)) == Ordering::Equal {
                rule.accept_overwrite(scope_depth, font_style, foreground, background);
                return;
            }
        }
        // A new rule inherits from the main rule.
        if font_style == font_style::NOT_SET {
            font_style = self.main_rule.font_style;
        }
        if foreground == 0 {
            foreground = self.main_rule.foreground;
        }
        if background == 0 {
            background = self.main_rule.background;
        }
        self.rules_with_parent_scopes.push(TrieRule { scope_depth, parent_scopes: parent_scopes.to_vec(), font_style, foreground, background });
    }
}

/// The result of matching one scope: flags and color ids (0 = not set).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StyleAttributes {
    pub font_style: i32,
    pub foreground: u32,
    pub background: u32,
}

/// upstream `Theme`.
pub struct Theme {
    colors: Vec<String>,
    defaults: StyleAttributes,
    root: TrieElement,
    cache: std::sync::Mutex<HashMap<String, Arc<Vec<TrieRule>>>>,
}

impl Theme {
    pub fn from_rules(rules: &[RawThemeRule]) -> Theme {
        Self::from_parsed(parse_theme(rules))
    }

    pub fn from_parsed(mut rules: Vec<ParsedThemeRule>) -> Theme {
        rules.sort_by(|a, b| {
            a.scope
                .cmp(&b.scope)
                .then_with(|| str_arr_cmp(a.parent_scopes.as_deref(), b.parent_scopes.as_deref()))
                .then_with(|| a.index.cmp(&b.index))
        });
        let mut default_font_style = font_style::NONE;
        let mut default_fg = "#000000".to_string();
        let mut default_bg = "#ffffff".to_string();
        while rules.first().is_some_and(|r| r.scope.is_empty()) {
            let d = rules.remove(0);
            if d.font_style != font_style::NOT_SET {
                default_font_style = d.font_style;
            }
            if let Some(f) = d.foreground {
                default_fg = f;
            }
            if let Some(b) = d.background {
                default_bg = b;
            }
        }
        // Color ids start at 1; 0 means "not set".
        let mut colors: Vec<String> = vec![String::new()];
        let mut ids: HashMap<String, u32> = HashMap::new();
        let mut id_of = |c: Option<&str>, colors: &mut Vec<String>| -> u32 {
            let Some(c) = c else { return 0 };
            let up = c.to_uppercase();
            *ids.entry(up.clone()).or_insert_with(|| {
                colors.push(up);
                (colors.len() - 1) as u32
            })
        };
        let defaults = StyleAttributes {
            font_style: default_font_style,
            foreground: id_of(Some(&default_fg), &mut colors),
            background: id_of(Some(&default_bg), &mut colors),
        };
        let mut root = TrieElement::new(TrieRule { scope_depth: 0, parent_scopes: Vec::new(), font_style: font_style::NOT_SET, foreground: 0, background: 0 }, Vec::new());
        for r in &rules {
            let fg = id_of(r.foreground.as_deref(), &mut colors);
            let bg = id_of(r.background.as_deref(), &mut colors);
            root.insert(0, &r.scope, r.parent_scopes.as_deref(), r.font_style, fg, bg);
        }
        Theme { colors, defaults, root, cache: std::sync::Mutex::new(HashMap::new()) }
    }

    pub fn defaults(&self) -> StyleAttributes {
        self.defaults
    }

    /// The color for a color id (uppercase `#RRGGBB[AA]`).
    pub fn color(&self, id: u32) -> Option<&str> {
        (id != 0).then(|| self.colors.get(id as usize).map(String::as_str)).flatten()
    }

    /// upstream `Theme.match`: the best rule for the last scope of `scopes`, given its parents.
    pub fn match_scopes(&self, scopes: &[Arc<str>]) -> Option<StyleAttributes> {
        let leaf = scopes.last()?;
        let candidates = {
            let mut cache = self.cache.lock().expect("theme cache lock");
            cache.entry(leaf.to_string()).or_insert_with(|| Arc::new(self.root.matches(leaf))).clone()
        };
        let parents = &scopes[..scopes.len() - 1];
        candidates
            .iter()
            .find(|rule| scope_path_matches_parent_scopes(parents, &rule.parent_scopes))
            .map(|r| StyleAttributes { font_style: r.font_style, foreground: r.foreground, background: r.background })
    }
}

/// `scope_path` is the stack below the matched scope, outermost first.
fn scope_path_matches_parent_scopes(scope_path: &[Arc<str>], parent_scopes: &[String]) -> bool {
    if parent_scopes.is_empty() {
        return true;
    }
    // Walk from the innermost scope outwards.
    let mut pos = scope_path.len(); // exclusive upper bound of what is still unmatched
    let mut index = 0;
    while index < parent_scopes.len() {
        let mut pattern = parent_scopes[index].as_str();
        let mut must_match = false;
        if pattern == ">" {
            if index == parent_scopes.len() - 1 {
                return false;
            }
            index += 1;
            pattern = parent_scopes[index].as_str();
            must_match = true;
        }
        loop {
            if pos == 0 {
                return false;
            }
            if matches_scope(&scope_path[pos - 1], pattern) {
                break;
            }
            if must_match {
                return false;
            }
            pos -= 1;
        }
        pos -= 1;
        index += 1;
    }
    true
}

fn matches_scope(scope_name: &str, pattern: &str) -> bool {
    scope_name == pattern || (scope_name.starts_with(pattern) && scope_name.as_bytes().get(pattern.len()) == Some(&b'.'))
}

/// Resolves scope ids to styles for one theme, caching per id. The merge matches
/// `AttributedScopeStack`: a scope's matching rule overrides only the attributes it sets.
pub struct ThemeStyles {
    theme: Theme,
    cache: std::sync::Mutex<Vec<Option<TokenStyle>>>,
}

impl ThemeStyles {
    pub fn new(theme: Theme) -> Self {
        Self { theme, cache: std::sync::Mutex::new(Vec::new()) }
    }

    pub fn theme(&self) -> &Theme {
        &self.theme
    }

    /// The style of the stack `id`. `names_of(id)` gives the scope names, outermost first, and
    /// `parent_of(id)` the id of the stack without its last scope.
    pub fn style_of(&self, id: ScopeId, names_of: &dyn Fn(ScopeId) -> Arc<Vec<Arc<str>>>, parent_of: &dyn Fn(ScopeId) -> ScopeId) -> TokenStyle {
        if id == 0 {
            return TokenStyle::default();
        }
        {
            let cache = self.cache.lock().expect("style cache lock");
            if let Some(Some(s)) = cache.get(id as usize) {
                return *s;
            }
        }
        let parent_style = self.style_of(parent_of(id), names_of, parent_of);
        let names = names_of(id);
        let mut style = parent_style;
        if let Some(m) = self.theme.match_scopes(&names) {
            if m.font_style != font_style::NOT_SET {
                style.font_style = m.font_style as u8;
            }
            if let Some(c) = self.theme.color(m.foreground) {
                style.foreground = Rgba::parse_hex(c);
            }
            if let Some(c) = self.theme.color(m.background) {
                style.background = Rgba::parse_hex(c);
            }
        }
        let mut cache = self.cache.lock().expect("style cache lock");
        if cache.len() <= id as usize {
            cache.resize(id as usize + 1, None);
        }
        cache[id as usize] = Some(style);
        style
    }
}
