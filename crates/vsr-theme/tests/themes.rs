//! Tests of the theme loader, the registry defaults, customisations and theme selection that have
//! no upstream counterpart (the upstream ones are in `upstream_*.rs`).

use std::collections::HashMap;

use serde_json::json;
use vsr_theme::{
    BUILTIN_THEMES, ColorScheme, ColorTheme, Customizations, DEFAULT_DARK, HostColorScheme, ThemeCatalog, ThemeError, ThemeSettings, ThemeSource,
    is_theme_scope, normalize_color, registry, theme_scope_matches, theme_scopes,
};

struct Files(HashMap<&'static str, &'static str>);

impl ThemeSource for Files {
    fn read(&self, path: &str) -> Option<String> {
        self.0.get(path).map(|s| (*s).to_string())
    }
}

fn load(files: &[(&'static str, &'static str)], entry: &str, scheme: ColorScheme) -> Result<ColorTheme, ThemeError> {
    ColorTheme::load("t", "T", "T", scheme, entry, &Files(files.iter().copied().collect()))
}

fn catalog_theme(settings_id: &str) -> ColorTheme {
    let catalog = ThemeCatalog::builtin();
    catalog.load(catalog.find(settings_id, None).unwrap()).unwrap()
}

fn hex(theme: &ColorTheme, key: &str) -> String {
    theme.color(key).unwrap_or_else(|| panic!("{key} has no colour")).format_hex_a(true).to_uppercase()
}

#[test]
fn every_bundled_theme_loads() {
    let catalog = ThemeCatalog::builtin();
    assert_eq!(catalog.themes().len(), 10);
    for t in BUILTIN_THEMES {
        let theme = catalog.load(t).unwrap_or_else(|e| panic!("{}: {e}", t.settings_id));
        assert_eq!(theme.scheme(), t.scheme());
        assert!(theme.color("editor.background").is_some(), "{}", t.settings_id);
        assert!(theme.token_colors().len() > 10, "{}", t.settings_id);
    }
}

#[test]
fn include_chain_outer_theme_wins() {
    // dark_vs sets editor.background to #1E1E1E, dark_modern (via dark_plus) overrides it.
    assert_eq!(hex(&catalog_theme("Visual Studio Dark"), "editor.background"), "#1E1E1E");
    assert_eq!(hex(&catalog_theme("Dark+"), "editor.background"), "#1E1E1E");
    assert_eq!(hex(&catalog_theme("Dark Modern"), "editor.background"), "#1F1F1F");
    // a colour only the innermost theme sets is inherited all the way
    assert!(catalog_theme("Dark Modern").defines("editorIndentGuide.background1"));
}

#[test]
fn token_colors_of_included_themes_come_first() {
    let t = catalog_theme("Dark Modern");
    let rules = t.token_colors();
    // the default rule is first and has no scope: the editor colours
    assert!(rules[0].scope.is_empty());
    assert_eq!(rules[0].settings.foreground.as_deref(), Some("#CCCCCC"));
    assert_eq!(rules[0].settings.background.as_deref(), Some("#1F1F1F"));
    // dark_vs rules come before dark_plus rules: `comment` (dark_vs) before `meta.preprocessor.string` (dark_plus)
    let pos = |scope: &str| rules.iter().position(|r| r.scope.iter().any(|s| s.contains(scope)));
    assert!(pos("comment").unwrap() < pos("entity.name.function").unwrap());
    // and the default token rules are appended
    assert!(rules.iter().any(|r| r.scope == ["token.info-token"]));
}

#[test]
fn include_with_dotdot_and_cycle_guard() {
    let files = [
        ("a/b.json", r##"{ "include": "../base.json", "colors": { "editor.background": "#111111" } }"##),
        ("base.json", r##"{ "colors": { "editor.background": "#222222", "editor.foreground": "#333333" } }"##),
        ("loop.json", r##"{ "include": "./loop.json" }"##),
    ];
    let t = load(&files, "a/b.json", ColorScheme::Dark).unwrap();
    assert_eq!(hex(&t, "editor.background"), "#111111");
    assert_eq!(hex(&t, "editor.foreground"), "#333333");
    assert!(matches!(load(&files, "loop.json", ColorScheme::Dark), Err(ThemeError::Invalid { .. })));
}

#[test]
fn jsonc_comments_and_trailing_commas() {
    let files = [("t.json", "{\n // comment\n \"colors\": { \"foreground\": \"#abcdef\", /* c */ },\n}\n")];
    let t = load(&files, "t.json", ColorScheme::Dark).unwrap();
    assert_eq!(hex(&t, "foreground"), "#ABCDEF");
}

#[test]
fn default_value_removes_an_included_colour() {
    let files = [
        ("base.json", r##"{ "colors": { "editor.background": "#222222" } }"##),
        ("t.json", r##"{ "include": "./base.json", "colors": { "editor.background": "default" } }"##),
    ];
    let t = load(&files, "t.json", ColorScheme::Dark).unwrap();
    assert!(!t.defines("editor.background"));
    assert_eq!(hex(&t, "editor.background"), "#1E1E1E", "the registry default for dark themes");
}

#[test]
fn errors_name_the_problem() {
    assert!(matches!(load(&[("t.json", "{ nope")], "t.json", ColorScheme::Dark), Err(ThemeError::Parse { .. })));
    assert!(matches!(load(&[("t.json", "[1]")], "t.json", ColorScheme::Dark), Err(ThemeError::Invalid { .. })));
    assert!(matches!(load(&[("t.json", r##"{ "colors": 3 }"##)], "t.json", ColorScheme::Dark), Err(ThemeError::Invalid { .. })));
    assert!(matches!(load(&[], "missing.json", ColorScheme::Dark), Err(ThemeError::Missing(_))));
    assert!(matches!(
        load(&[("t.json", r##"{ "tokenColors": "./x.tmTheme" }"##)], "t.json", ColorScheme::Dark),
        Err(ThemeError::UnsupportedTextMateTheme(_))
    ));
}

#[test]
fn legacy_settings_array_maps_to_workbench_colors() {
    let files = [("t.json", r##"{ "settings": [ { "settings": { "background": "#101010", "foreground": "#eeeeee", "caret": "#ff0000" } }, { "scope": "string", "settings": { "foreground": "#00ff00" } } ] }"##)];
    let t = load(&files, "t.json", ColorScheme::Dark).unwrap();
    assert_eq!(hex(&t, "editor.background"), "#101010");
    assert_eq!(hex(&t, "editorCursor.foreground"), "#FF0000");
    assert!(t.token_colors().iter().any(|r| r.scope == ["string"] && r.settings.foreground.as_deref() == Some("#00FF00")));
}

#[test]
fn registry_defaults_follow_the_theme_type() {
    let dark = ColorTheme::empty("d", "D", "D", ColorScheme::Dark);
    let light = ColorTheme::empty("l", "L", "L", ColorScheme::Light);
    let hc = ColorTheme::empty("h", "H", "H", ColorScheme::HighContrastDark);
    assert_eq!(hex(&dark, "foreground"), "#CCCCCC");
    assert_eq!(hex(&light, "foreground"), "#616161");
    assert_eq!(hex(&hc, "foreground"), "#FFFFFF");
    // a derived default: descriptionForeground = transparent(foreground, 0.7) for dark
    assert_eq!(hex(&dark, "descriptionForeground"), "#CCCCCCB3");
    assert_eq!(hex(&light, "descriptionForeground"), "#717171");
    // derived defaults follow the theme's own colours
    let t = ColorTheme::from_colors("x", ColorScheme::Dark, &[("foreground", "#FF0000")]);
    assert_eq!(hex(&t, "descriptionForeground"), "#FF0000B3");
}

#[test]
fn registry_has_the_keys_the_app_paints_with() {
    let r = registry();
    assert!(r.keys().count() > 500);
    for key in ["editor.background", "list.hoverBackground", "settings.focusedRowBackground", "keybindingLabel.bottomBorder", "tab.activeModifiedBorder"] {
        assert!(r.contains(key), "{key}");
    }
}

#[test]
fn color_customizations_global_and_per_theme() {
    let customs = Customizations {
        colors: json!({
            "editor.background": "#112233",
            "foreground": "#445566",
            "list.hoverBackground": "#102030",
            "[Dark Modern]": { "editor.background": "#AABBCC" },
            "[Light Modern]": { "editor.background": "#000000" },
        }),
        ..Default::default()
    };
    let t = catalog_theme("Dark Modern").with_customizations(&customs);
    assert_eq!(hex(&t, "editor.background"), "#AABBCC", "the theme block wins over the global value");
    assert_eq!(hex(&t, "foreground"), "#445566");
    assert!(t.defines("editor.background"));
    assert_eq!(t.color_customization("foreground").unwrap().format_hex(), "#445566");
    // other themes see only the global part
    let light = catalog_theme("Light Modern").with_customizations(&customs);
    assert_eq!(hex(&light, "editor.background"), "#000000");
    // a customisation of a colour another default derives from changes the derived colour
    assert_eq!(hex(&t, "settings.rowHoverBackground"), "#1020304D");
}

#[test]
fn customizing_to_default_restores_the_registry_default() {
    let customs = Customizations { colors: json!({ "editor.background": "default" }), ..Default::default() };
    let t = catalog_theme("Dark Modern").with_customizations(&customs);
    assert_eq!(hex(&t, "editor.background"), "#1E1E1E");
    assert!(!t.defines("editor.background"));
}

#[test]
fn theme_scope_keys_and_matching() {
    assert!(is_theme_scope("[Dark Modern]") && !is_theme_scope("editor.background"));
    assert_eq!(theme_scopes("[A][B C]"), ["A", "B C"]);
    assert_eq!(theme_scopes("[]]"), ["]"]);
    assert!(theme_scope_matches("Dark Modern", "Dark Modern"));
    assert!(theme_scope_matches("Dark*", "Dark Modern"));
    assert!(theme_scope_matches("*Modern", "Dark Modern"));
    assert!(theme_scope_matches("*rk Mo*", "Dark Modern"));
    assert!(!theme_scope_matches("Light*", "Dark Modern"));
    assert!(!theme_scope_matches("Dark", "Dark Modern"));
}

#[test]
fn wildcard_theme_scope_in_customizations() {
    let customs = Customizations { colors: json!({ "[*Modern]": { "foreground": "#010203" } }), ..Default::default() };
    assert_eq!(hex(&catalog_theme("Dark Modern").with_customizations(&customs), "foreground"), "#010203");
    assert_eq!(hex(&catalog_theme("Light Modern").with_customizations(&customs), "foreground"), "#010203");
    assert_eq!(hex(&catalog_theme("Dark+").with_customizations(&customs), "foreground"), "#CCCCCC");
}

#[test]
fn token_color_customizations_groups_then_rules_then_theme_specific() {
    let customs = Customizations {
        token_colors: json!({
            "comments": "#112233",
            "strings": { "foreground": "#445566", "fontStyle": "italic" },
            "textMateRules": [ { "scope": "keyword.control", "settings": { "foreground": "#aabbcc" } }, { "settings": { "foreground": "#000000" } } ],
            "[Dark Modern]": { "comments": "#FFFFFF" }
        }),
        ..Default::default()
    };
    let t = catalog_theme("Dark Modern").with_customizations(&customs);
    let rules = t.token_colors();
    let last_for = |scope: &str| rules.iter().rev().find(|r| r.scope == [scope]).unwrap();
    // group rules expand to their scopes; the theme specific block comes last so it wins
    assert_eq!(last_for("comment").settings.foreground.as_deref(), Some("#FFFFFF"));
    assert_eq!(last_for("punctuation.definition.comment").settings.foreground.as_deref(), Some("#FFFFFF"));
    assert_eq!(last_for("string").settings.foreground.as_deref(), Some("#445566"));
    assert_eq!(last_for("string").settings.font_style.as_deref(), Some("italic"));
    assert_eq!(last_for("keyword.control").settings.foreground.as_deref(), Some("#AABBCC"));
}

#[test]
fn token_scope_strings_are_handed_through_untouched() {
    let files = [("t.json", r##"{ "tokenColors": [ { "name": "x", "scope": "a.b, c.d - e", "settings": { "foreground": "#fff", "fontStyle": "bold underline" } }, { "scope": ["f", "g"], "settings": { "foreground": "#0f08" } } ] }"##)];
    let t = load(&files, "t.json", ColorScheme::Dark).unwrap();
    let rules = t.token_colors();
    assert_eq!(rules[1].scope, ["a.b, c.d - e"]);
    assert_eq!(rules[1].name.as_deref(), Some("x"));
    assert_eq!(rules[1].settings.foreground.as_deref(), Some("#FFFFFF"), "#RGB is expanded and upper-cased");
    assert_eq!(rules[1].settings.font_style.as_deref(), Some("bold underline"));
    assert_eq!(rules[2].scope, ["f", "g"]);
    assert_eq!(rules[2].settings.foreground.as_deref(), Some("#00FF0088"));
}

#[test]
fn semantic_token_rules_and_highlighting() {
    let t = catalog_theme("Dark Modern");
    assert!(t.semantic_highlighting());
    assert!(t.semantic_token_rules().count() > 0);
    let off = t.clone().with_customizations(&Customizations { semantic_token_colors: json!({ "enabled": false, "rules": { "variable": "#ff0000" } }), ..Default::default() });
    assert!(!off.semantic_highlighting());
    let last = off.semantic_token_rules().last().unwrap();
    assert_eq!((last.selector.as_str(), last.foreground.as_deref()), ("variable", Some("#ff0000")));
}

#[test]
fn normalize_color_matches_upstream() {
    assert_eq!(normalize_color("#abc").as_deref(), Some("#AABBCC"));
    assert_eq!(normalize_color("#abcd").as_deref(), Some("#AABBCCDD"));
    assert_eq!(normalize_color("#abcdef").as_deref(), Some("#ABCDEF"));
    assert_eq!(normalize_color("#abcdefff").as_deref(), Some("#ABCDEF"));
    assert_eq!(normalize_color("#abcdef12").as_deref(), Some("#ABCDEF12"));
    assert_eq!(normalize_color("abc"), None);
    assert_eq!(normalize_color("#abcde"), None);
    assert_eq!(normalize_color("#ggg"), None);
}

#[test]
fn selection_follows_the_system_only_when_asked() {
    let catalog = ThemeCatalog::builtin();
    let dark = HostColorScheme { dark: true, high_contrast: false };
    let light = HostColorScheme { dark: false, high_contrast: false };
    let mut s = ThemeSettings::default();
    assert_eq!(s.resolve(&catalog, light).unwrap().settings_id, DEFAULT_DARK, "the default theme is Dark 2026");
    s.color_theme = Some("Light Modern".into());
    assert_eq!(s.resolve(&catalog, dark).unwrap().settings_id, "Light Modern");
    s.detect_color_scheme = true;
    s.preferred_light = Some("Light+".into());
    assert_eq!(s.resolve(&catalog, light).unwrap().settings_id, "Light+");
    assert_eq!(s.color_theme_setting_id(light), "workbench.preferredLightColorTheme");
    assert_eq!(s.resolve(&catalog, dark).unwrap().settings_id, "Dark 2026", "the dark default");
    // high contrast wins over the colour scheme
    let hc = HostColorScheme { dark: true, high_contrast: true };
    assert_eq!(s.resolve(&catalog, hc).unwrap().settings_id, "Default High Contrast");
    s.detect_high_contrast = false;
    assert_eq!(s.resolve(&catalog, hc).unwrap().settings_id, "Dark 2026");
}

#[test]
fn an_unknown_theme_falls_back_to_the_default_of_the_scheme() {
    let catalog = ThemeCatalog::builtin();
    let s = ThemeSettings { color_theme: Some("Monokai".into()), ..ThemeSettings::default() };
    assert_eq!(s.resolve(&catalog, HostColorScheme::default()).unwrap().settings_id, "Dark 2026");
    let s = ThemeSettings { detect_color_scheme: true, preferred_light: Some("Abyss".into()), ..ThemeSettings::default() };
    assert_eq!(s.resolve(&catalog, HostColorScheme { dark: false, high_contrast: false }).unwrap().settings_id, "Light 2026");
    // no preferred light theme at all: the light default, as upstream
    let s = ThemeSettings { detect_color_scheme: true, ..ThemeSettings::default() };
    assert_eq!(s.resolve(&catalog, HostColorScheme { dark: false, high_contrast: false }).unwrap().settings_id, "Light 2026");
    // legacy ids are migrated
    let s = ThemeSettings { color_theme: Some("Default Light+".into()), ..ThemeSettings::default() };
    assert_eq!(s.resolve(&catalog, HostColorScheme::default()).unwrap().settings_id, "Light+");
}
