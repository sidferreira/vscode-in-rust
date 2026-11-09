//! The theme service: the active colour theme as a gpui `Global`.
//!
//! upstream: src/vs/workbench/services/themes/browser/workbenchThemeService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/themes/common/themeConfiguration.ts @ 50e1350fc78d
//!
//! [`ThemeService`] owns which theme is active (`workbench.colorTheme` and the preferred-theme
//! settings, the system appearance for `window.autoDetectColorScheme`, a live preview from the
//! theme picker), the user's colour customisations on top of it, and the [`Theme`] the views paint
//! with. Every change goes through `update_global`, so observers (the Workbench) re-render at once.
//! Views read the colours with [`ThemeService::theme`] in their `render`.
//!
//! The pure parts (loading, defaults, which theme the settings select) are `vsr-theme`.
//!
//! Differences from VS Code: themes are the ten bundled ones (no extensions), a theme that fails
//! to load keeps the previous one and reports the error, the OS high contrast mode is not
//! detected (gpui has no API for it; `set_host_color_scheme` can inject it).

use std::collections::HashMap;

use gpui_kit::component::theme::{Theme as ComponentTheme, ThemeMode, ThemeRegistry};
use gpui_kit::*;
use serde_json::Value;
use vsr_settings::Configuration;
use vsr_theme::selection::{
    COLOR_CUSTOMIZATIONS, COLOR_THEME, DETECT_COLOR_SCHEME, DETECT_HC, PREFERRED_DARK_THEME, PREFERRED_HC_DARK_THEME, PREFERRED_HC_LIGHT_THEME,
    PREFERRED_LIGHT_THEME, SEMANTIC_TOKEN_COLOR_CUSTOMIZATIONS, TOKEN_COLOR_CUSTOMIZATIONS,
};
use vsr_theme::{ColorScheme, ColorTheme, Customizations, HostColorScheme, ThemeCatalog, ThemeContribution, ThemeSettings};

use crate::config::AppConfig;
use crate::ui::to_gpui;
use crate::theme::Theme;

/// `ThemeSettings` as the configuration has it: the values the user set explicitly for the theme
/// choice (an unset one means rscode's default), the effective values for the rest.
pub fn theme_settings(c: &Configuration) -> ThemeSettings {
    let explicit = |key: &str| c.user().get(key).and_then(Value::as_str).map(str::to_string);
    let object = |key: &str| c.get_value(key).cloned().unwrap_or(Value::Null);
    ThemeSettings {
        color_theme: explicit(COLOR_THEME),
        preferred_dark: explicit(PREFERRED_DARK_THEME),
        preferred_light: explicit(PREFERRED_LIGHT_THEME),
        preferred_hc_dark: explicit(PREFERRED_HC_DARK_THEME),
        preferred_hc_light: explicit(PREFERRED_HC_LIGHT_THEME),
        detect_color_scheme: c.get_bool(DETECT_COLOR_SCHEME).unwrap_or(false),
        detect_high_contrast: c.get_bool(DETECT_HC).unwrap_or(true),
        customizations: Customizations {
            colors: object(COLOR_CUSTOMIZATIONS),
            token_colors: object(TOKEN_COLOR_CUSTOMIZATIONS),
            semantic_token_colors: object(SEMANTIC_TOKEN_COLOR_CUSTOMIZATIONS),
        },
    }
}

/// `IHostColorSchemeService` from the window appearance.
pub fn host_color_scheme(appearance: WindowAppearance, high_contrast: bool) -> HostColorScheme {
    HostColorScheme { dark: matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark), high_contrast }
}

pub struct ThemeService {
    catalog: ThemeCatalog,
    settings: ThemeSettings,
    host: HostColorScheme,
    /// The settings id of a theme shown while the picker navigates (not saved).
    preview: Option<String>,
    /// Loaded bundled themes, by settings id (before customisations).
    loaded: HashMap<String, ColorTheme>,
    active: ColorTheme,
    theme: Theme,
    error: Option<String>,
    generation: u64,
    _config_sub: Subscription,
}

impl Global for ThemeService {}

impl ThemeService {
    /// Installs the service when it is not there yet (idempotent: tests and embedders call it).
    pub fn ensure_installed(cx: &mut App) {
        if cx.has_global::<ThemeService>() {
            return;
        }
        Self::install(cx);
    }

    /// Installs (or replaces) the service. Reads the settings of `AppConfig` when it is installed
    /// and follows its changes; the system appearance is `cx.window_appearance()` until a window
    /// reports its own.
    pub fn install(cx: &mut App) {
        let settings = cx.try_global::<AppConfig>().map(|c| theme_settings(c.configuration())).unwrap_or_default();
        let host = host_color_scheme(cx.window_appearance(), false);
        let catalog = ThemeCatalog::builtin();
        let sub = cx.observe_global::<AppConfig>(|cx| {
            let settings = theme_settings(AppConfig::get(cx).configuration());
            cx.update_global::<ThemeService, _>(|svc, cx| svc.set_settings(settings, cx));
        });
        let mut svc = Self {
            catalog,
            settings,
            host,
            preview: None,
            loaded: HashMap::new(),
            active: ColorTheme::empty("", "", "", ColorScheme::Dark),
            theme: Theme::dark_modern(),
            error: None,
            generation: 0,
            _config_sub: sub,
        };
        svc.recompute();
        sync_component_theme(&svc.theme, svc.active.scheme(), cx);
        cx.set_global(svc);
    }

    // ---- reading ----------------------------------------------------------------------------

    /// The colours to paint with. Before the service is installed: the default theme.
    pub fn theme(cx: &App) -> Theme {
        cx.try_global::<ThemeService>().map_or_else(Theme::dark_modern, |s| s.theme.clone())
    }

    /// The active colour theme with the user's customisations: token colours for the highlighter,
    /// any colour key by name.
    #[allow(dead_code)] // API for the syntax highlighter and tests
    pub fn color_theme(cx: &App) -> Option<&ColorTheme> {
        cx.try_global::<ThemeService>().map(|s| &s.active)
    }

    /// The settings id of the theme that is showing (a preview counts).
    pub fn active_settings_id(cx: &App) -> Option<String> {
        cx.try_global::<ThemeService>().map(|s| s.active.settings_id().to_string())
    }

    /// The themes the picker lists, sorted by label.
    pub fn themes(cx: &App) -> Vec<ThemeContribution> {
        cx.try_global::<ThemeService>().map_or_else(Vec::new, |s| s.catalog.sorted_by_label())
    }

    /// The setting that holds the theme name now (`workbench.colorTheme` or a preferred-theme
    /// setting when the system appearance picks the theme).
    pub fn theme_setting_key(cx: &App) -> &'static str {
        cx.try_global::<ThemeService>().map_or(COLOR_THEME, |s| s.settings.color_theme_setting_id(s.host))
    }

    /// The colour mode the system asks for (`None` unless `window.autoDetectColorScheme` or a high contrast OS mode is on).
    pub fn preferred_scheme(cx: &App) -> Option<ColorScheme> {
        cx.try_global::<ThemeService>().and_then(|s| s.settings.preferred_color_scheme(s.host))
    }

    /// Changes whenever the painted theme changes.
    #[allow(dead_code)] // API for the syntax highlighter and tests
    pub fn generation(cx: &App) -> u64 {
        cx.try_global::<ThemeService>().map_or(0, |s| s.generation)
    }

    /// The last problem loading a theme (the previous theme stays).
    #[allow(dead_code)] // API for the syntax highlighter and tests
    pub fn error(cx: &App) -> Option<String> {
        cx.try_global::<ThemeService>().and_then(|s| s.error.clone())
    }

    // ---- changing ---------------------------------------------------------------------------

    /// The system appearance changed (a window reports it; tests inject it).
    pub fn set_system_appearance(cx: &mut App, appearance: WindowAppearance) {
        Self::ensure_installed(cx);
        cx.update_global::<ThemeService, _>(|svc, cx| {
            let host = HostColorScheme { dark: host_color_scheme(appearance, false).dark, high_contrast: svc.host.high_contrast };
            svc.set_host(host, cx);
        });
    }

    /// The system colour scheme including high contrast (not detected on macOS; for tests).
    #[allow(dead_code)] // API for the syntax highlighter and tests
    pub fn set_host_color_scheme(cx: &mut App, host: HostColorScheme) {
        Self::ensure_installed(cx);
        cx.update_global::<ThemeService, _>(|svc, cx| svc.set_host(host, cx));
    }

    /// Shows `settings_id` without saving it (`None`: back to the configured theme). The picker
    /// uses this to preview while navigating.
    pub fn preview(cx: &mut App, settings_id: Option<&str>) {
        Self::ensure_installed(cx);
        let id = settings_id.map(str::to_string);
        cx.update_global::<ThemeService, _>(|svc, cx| {
            if svc.preview != id {
                svc.preview = id;
                svc.refresh(cx);
            }
        });
    }

    fn set_settings(&mut self, settings: ThemeSettings, cx: &mut App) {
        if settings != self.settings {
            self.settings = settings;
            self.refresh(cx);
        }
    }

    fn set_host(&mut self, host: HostColorScheme, cx: &mut App) {
        if host != self.host {
            self.host = host;
            self.refresh(cx);
        }
    }

    // ---- computing --------------------------------------------------------------------------

    /// The theme the configuration selects, when it is one of the bundled.
    fn configured(&self) -> Option<&ThemeContribution> {
        self.settings.resolve(&self.catalog, self.host)
    }

    fn refresh(&mut self, cx: &mut App) {
        let before = self.theme.clone();
        let scheme_before = self.active.scheme();
        self.recompute();
        if self.theme != before {
            self.generation += 1;
        }
        if self.theme != before || self.active.scheme() != scheme_before {
            sync_component_theme(&self.theme, self.active.scheme(), cx);
        }
    }

    /// Loads the theme to show, applies the customisations and builds the paint colours.
    fn recompute(&mut self) {
        let contribution = match self.preview.as_deref() {
            Some(id) => self.catalog.find(id, None).cloned(),
            None => self.configured().cloned(),
        };
        let Some(contribution) = contribution else { return };
        let id = contribution.settings_id.to_string();
        if !self.loaded.contains_key(&id) {
            match self.catalog.load(&contribution) {
                Ok(t) => {
                    self.loaded.insert(id.clone(), t);
                }
                Err(e) => {
                    self.error = Some(format!("{}: {e}", contribution.settings_id));
                    return;
                }
            }
        }
        self.error = None;
        let base = self.loaded.get(&id).expect("just loaded").clone();
        self.active = base.with_customizations(&self.settings.customizations);
        self.theme = Theme::from_color_theme(&self.active);
    }
}

/// Aligns the component library's theme (used by its text input) with the active theme.
pub fn sync_component_theme(t: &Theme, scheme: ColorScheme, cx: &mut App) {
    if !cx.has_global::<ThemeRegistry>() {
        return; // gpui_kit::init has not run (some headless tests): nothing to align
    }
    ComponentTheme::change(if scheme.is_dark() { ThemeMode::Dark } else { ThemeMode::Light }, None, cx);
    let theme = ComponentTheme::global_mut(cx);
    theme.foreground = to_gpui(t.input_fg);
    theme.muted_foreground = to_gpui(t.input_placeholder_fg);
    theme.caret = to_gpui(t.foreground);
    theme.selection = to_gpui(t.selection_bg);
}

/// Follows the window's appearance for `window.autoDetectColorScheme`: reads it now and on every
/// change. Keep the returned subscription as long as the window lives.
pub fn watch_window_appearance(window: &mut Window, cx: &mut App) -> Subscription {
    ThemeService::set_system_appearance(cx, window.appearance());
    window.observe_window_appearance(|window, cx| ThemeService::set_system_appearance(cx, window.appearance()))
}
