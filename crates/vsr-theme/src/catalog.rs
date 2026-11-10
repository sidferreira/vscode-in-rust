//! The themes rscode ships: the built-in `theme-defaults` extension of VS Code, bundled as
//! JSON files (see `README.md` for provenance), and the lookup by settings id.
//!
//! upstream: extensions/theme-defaults/package.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/package.nls.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/2026-dark.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/2026-light.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/dark_modern.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/dark_plus.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/dark_vs.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/hc_black.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/hc_light.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/light_modern.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/light_plus.json @ 50e1350fc78d
//! upstream: extensions/theme-defaults/themes/light_vs.json @ 50e1350fc78d
//! upstream: src/vs/workbench/services/themes/common/workbenchThemeService.ts @ 50e1350fc78d

use crate::registry::ColorScheme;
use crate::theme::{ColorTheme, ThemeError, ThemeSource};

/// `ThemeSettingDefaults`. The dark default follows upstream 1.147 (Dark 2026, ADR-0049; it was
/// Dark Modern under ADR-0037). The light default stays Light Modern: rscode's light default is
/// not yet switched to upstream's Light 2026 (ADR-0049).
pub const DEFAULT_DARK: &str = "Dark 2026";
pub const DEFAULT_LIGHT: &str = "Light Modern";
pub const DEFAULT_HC_DARK: &str = "Default High Contrast";
pub const DEFAULT_HC_LIGHT: &str = "Default High Contrast Light";

/// `IThemeExtensionPoint` of a bundled theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThemeContribution {
    /// The value of `workbench.colorTheme` (`id` in package.json).
    pub settings_id: &'static str,
    /// The label shown in pickers (`package.nls.json`).
    pub label: &'static str,
    /// `uiTheme`: `vs`, `vs-dark`, `hc-black` or `hc-light`.
    pub ui_theme: &'static str,
    /// The file under `assets/themes`.
    pub path: &'static str,
}

impl ThemeContribution {
    /// `ColorThemeData.type` for this `uiTheme`.
    pub fn scheme(&self) -> ColorScheme {
        scheme_of_ui_theme(self.ui_theme)
    }
}

/// `ThemeTypeSelector` -> `ColorScheme` (anything unknown is dark, as upstream).
pub fn scheme_of_ui_theme(ui_theme: &str) -> ColorScheme {
    match ui_theme {
        "vs" => ColorScheme::Light,
        "hc-black" => ColorScheme::HighContrastDark,
        "hc-light" => ColorScheme::HighContrastLight,
        _ => ColorScheme::Dark,
    }
}

/// The `themes` contribution of the `theme-defaults` extension, in package.json order.
pub const BUILTIN_THEMES: &[ThemeContribution] = &[
    ThemeContribution { settings_id: "Light 2026", label: "Light 2026", ui_theme: "vs", path: "2026-light.json" },
    ThemeContribution { settings_id: "Dark 2026", label: "Dark 2026", ui_theme: "vs-dark", path: "2026-dark.json" },
    ThemeContribution { settings_id: "Dark+", label: "Dark+", ui_theme: "vs-dark", path: "dark_plus.json" },
    ThemeContribution { settings_id: "Dark Modern", label: "Dark Modern", ui_theme: "vs-dark", path: "dark_modern.json" },
    ThemeContribution { settings_id: "Light+", label: "Light+", ui_theme: "vs", path: "light_plus.json" },
    ThemeContribution { settings_id: "Light Modern", label: "Light Modern", ui_theme: "vs", path: "light_modern.json" },
    ThemeContribution { settings_id: "Visual Studio Dark", label: "Dark (Visual Studio)", ui_theme: "vs-dark", path: "dark_vs.json" },
    ThemeContribution { settings_id: "Visual Studio Light", label: "Light (Visual Studio)", ui_theme: "vs", path: "light_vs.json" },
    ThemeContribution { settings_id: "Default High Contrast", label: "Dark High Contrast", ui_theme: "hc-black", path: "hc_black.json" },
    ThemeContribution { settings_id: "Default High Contrast Light", label: "Light High Contrast", ui_theme: "hc-light", path: "hc_light.json" },
];

/// The bundled theme files.
pub struct BundledSource;

impl ThemeSource for BundledSource {
    fn read(&self, path: &str) -> Option<String> {
        let text = match path {
            "2026-dark.json" => include_str!("../assets/themes/2026-dark.json"),
            "2026-light.json" => include_str!("../assets/themes/2026-light.json"),
            "dark_modern.json" => include_str!("../assets/themes/dark_modern.json"),
            "dark_plus.json" => include_str!("../assets/themes/dark_plus.json"),
            "dark_vs.json" => include_str!("../assets/themes/dark_vs.json"),
            "hc_black.json" => include_str!("../assets/themes/hc_black.json"),
            "hc_light.json" => include_str!("../assets/themes/hc_light.json"),
            "light_modern.json" => include_str!("../assets/themes/light_modern.json"),
            "light_plus.json" => include_str!("../assets/themes/light_plus.json"),
            "light_vs.json" => include_str!("../assets/themes/light_vs.json"),
            _ => return None,
        };
        Some(text.to_string())
    }
}

/// The text of a bundled theme file (tests compare loaded colours against it).
pub fn bundled_theme_text(path: &str) -> Option<String> {
    BundledSource.read(path)
}

/// `ColorThemeRegistry`: the themes that can be selected.
#[derive(Clone, Debug)]
pub struct ThemeCatalog {
    themes: Vec<ThemeContribution>,
}

impl Default for ThemeCatalog {
    fn default() -> Self {
        Self::builtin()
    }
}

impl ThemeCatalog {
    pub fn builtin() -> Self {
        Self { themes: BUILTIN_THEMES.to_vec() }
    }

    pub fn themes(&self) -> &[ThemeContribution] {
        &self.themes
    }

    /// `findThemeBySettingsId(settingsId, defaultSettingsId)`: the theme with that settings id,
    /// else the one with the fallback id.
    pub fn find(&self, settings_id: &str, fallback: Option<&str>) -> Option<&ThemeContribution> {
        let by_id = |id: &str| self.themes.iter().find(|t| t.settings_id == id);
        by_id(settings_id).or_else(|| fallback.and_then(by_id))
    }

    /// Loads a bundled theme.
    pub fn load(&self, theme: &ThemeContribution) -> Result<ColorTheme, ThemeError> {
        ColorTheme::load(theme.settings_id, theme.label, theme.settings_id, theme.scheme(), theme.path, &BundledSource)
    }

    /// `ColorThemeRegistry.getThemes()` sorted by label, as the settings enum and the picker list them.
    pub fn sorted_by_label(&self) -> Vec<ThemeContribution> {
        let mut v = self.themes.clone();
        v.sort_by(|a, b| a.label.to_lowercase().cmp(&b.label.to_lowercase()).then(a.label.cmp(b.label)));
        v
    }
}

/// `migrateThemeSettingsId`: legacy settings ids and their current equivalents.
pub fn migrate_theme_settings_id(settings_id: &str) -> String {
    match settings_id {
        "Default Dark Modern" => "Dark Modern",
        "Default Light Modern" => "Light Modern",
        "Default Dark+" => "Dark+",
        "Default Light+" => "Light+",
        // Upstream's `ThemeSettingDefaults.COLOR_THEME_DARK` / `_LIGHT` (both are bundled).
        "Experimental Dark" | "VS Code Dark" => "Dark 2026",
        "Experimental Light" | "VS Code Light" => "Light 2026",
        other => return other.to_string(),
    }
    .to_string()
}
