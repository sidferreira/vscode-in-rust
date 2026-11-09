//! Which theme is active: `workbench.colorTheme`, the `workbench.preferred*ColorTheme` settings and
//! the system appearance (`window.autoDetectColorScheme`, `window.autoDetectHighContrast`).
//!
//! upstream: src/vs/workbench/services/themes/common/themeConfiguration.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/themes/common/hostColorSchemeService.ts @ 50e1350fc78d
//!
//! Pure: the app reads the settings and the window appearance, builds a [`ThemeSettings`] and a
//! [`HostColorScheme`], and asks which theme to load.

use crate::catalog::{DEFAULT_DARK, DEFAULT_HC_DARK, DEFAULT_HC_LIGHT, DEFAULT_LIGHT, ThemeCatalog, ThemeContribution, migrate_theme_settings_id};
use crate::registry::ColorScheme;
use crate::theme::Customizations;

pub const COLOR_THEME: &str = "workbench.colorTheme";
pub const PREFERRED_DARK_THEME: &str = "workbench.preferredDarkColorTheme";
pub const PREFERRED_LIGHT_THEME: &str = "workbench.preferredLightColorTheme";
pub const PREFERRED_HC_DARK_THEME: &str = "workbench.preferredHighContrastColorTheme";
pub const PREFERRED_HC_LIGHT_THEME: &str = "workbench.preferredHighContrastLightColorTheme";
pub const DETECT_COLOR_SCHEME: &str = "window.autoDetectColorScheme";
pub const DETECT_HC: &str = "window.autoDetectHighContrast";
pub const COLOR_CUSTOMIZATIONS: &str = "workbench.colorCustomizations";
pub const TOKEN_COLOR_CUSTOMIZATIONS: &str = "editor.tokenColorCustomizations";
pub const SEMANTIC_TOKEN_COLOR_CUSTOMIZATIONS: &str = "editor.semanticTokenColorCustomizations";

/// `IColorScheme` of the host: what the system says (`IHostColorSchemeService`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HostColorScheme {
    pub dark: bool,
    pub high_contrast: bool,
}

/// The theme settings, as the user set them (`None`: not set, the default applies).
#[derive(Clone, Debug, PartialEq)]
pub struct ThemeSettings {
    pub color_theme: Option<String>,
    pub preferred_dark: Option<String>,
    pub preferred_light: Option<String>,
    pub preferred_hc_dark: Option<String>,
    pub preferred_hc_light: Option<String>,
    /// `window.autoDetectColorScheme` (default false).
    pub detect_color_scheme: bool,
    /// `window.autoDetectHighContrast` (default true).
    pub detect_high_contrast: bool,
    pub customizations: Customizations,
}

impl Default for ThemeSettings {
    fn default() -> Self {
        Self {
            color_theme: None,
            preferred_dark: None,
            preferred_light: None,
            preferred_hc_dark: None,
            preferred_hc_light: None,
            detect_color_scheme: false,
            detect_high_contrast: true,
            customizations: Customizations::default(),
        }
    }
}

impl ThemeSettings {
    /// `getPreferredColorScheme`: the scheme the system asks for, `None` when not following it.
    pub fn preferred_color_scheme(&self, host: HostColorScheme) -> Option<ColorScheme> {
        if self.detect_high_contrast && host.high_contrast {
            return Some(if host.dark { ColorScheme::HighContrastDark } else { ColorScheme::HighContrastLight });
        }
        if self.detect_color_scheme {
            return Some(if host.dark { ColorScheme::Dark } else { ColorScheme::Light });
        }
        None
    }

    /// `getColorThemeSettingId`: the setting that names the active theme.
    pub fn color_theme_setting_id(&self, host: HostColorScheme) -> &'static str {
        match self.preferred_color_scheme(host) {
            Some(ColorScheme::Dark) => PREFERRED_DARK_THEME,
            Some(ColorScheme::Light) => PREFERRED_LIGHT_THEME,
            Some(ColorScheme::HighContrastDark) => PREFERRED_HC_DARK_THEME,
            Some(ColorScheme::HighContrastLight) => PREFERRED_HC_LIGHT_THEME,
            None => COLOR_THEME,
        }
    }

    /// The settings id of the theme to show: the value of [`Self::color_theme_setting_id`], or
    /// that setting's default.
    pub fn theme_settings_id(&self, host: HostColorScheme) -> String {
        let key = self.color_theme_setting_id(host);
        let (value, default) = match key {
            PREFERRED_DARK_THEME => (&self.preferred_dark, DEFAULT_DARK),
            PREFERRED_LIGHT_THEME => (&self.preferred_light, DEFAULT_LIGHT),
            PREFERRED_HC_DARK_THEME => (&self.preferred_hc_dark, DEFAULT_HC_DARK),
            PREFERRED_HC_LIGHT_THEME => (&self.preferred_hc_light, DEFAULT_HC_LIGHT),
            _ => (&self.color_theme, DEFAULT_DARK),
        };
        migrate_theme_settings_id(value.as_deref().unwrap_or(default))
    }

    /// The bundled theme to load: the named one, or (when it is unknown or not installed) the
    /// default of the scheme the system asks for, Dark Modern when not following the system.
    pub fn resolve<'a>(&self, catalog: &'a ThemeCatalog, host: HostColorScheme) -> Option<&'a ThemeContribution> {
        let id = self.theme_settings_id(host);
        let fallback = match self.preferred_color_scheme(host) {
            Some(ColorScheme::Light) => DEFAULT_LIGHT,
            Some(ColorScheme::HighContrastDark) => DEFAULT_HC_DARK,
            Some(ColorScheme::HighContrastLight) => DEFAULT_HC_LIGHT,
            _ => DEFAULT_DARK,
        };
        catalog.find(&id, Some(fallback))
    }

    /// `isPreferredColorSchemeChange`: did the system change in a way that selects another theme?
    pub fn is_preferred_color_scheme_change(&self, previous: HostColorScheme, now: HostColorScheme) -> bool {
        let dark_changed = previous.dark != now.dark;
        if self.detect_color_scheme && dark_changed {
            return true;
        }
        if self.detect_high_contrast {
            return previous.high_contrast != now.high_contrast || (now.high_contrast && dark_changed);
        }
        false
    }
}
