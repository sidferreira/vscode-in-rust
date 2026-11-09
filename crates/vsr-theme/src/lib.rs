//! The theme service's data model, free of any UI: VS Code colour themes loaded from their JSON,
//! the colour registry defaults, user colour customisations and the choice of the active theme.
//! The app (`vsr-app`) turns the result into painted colours; the syntax highlighter reads
//! [`ColorTheme::token_colors`]. ADR-0037.

pub mod catalog;
pub mod color;
pub mod registry;
pub mod selection;
pub mod theme;

pub use catalog::{
    BUILTIN_THEMES, BundledSource, DEFAULT_DARK, DEFAULT_HC_DARK, DEFAULT_HC_LIGHT, DEFAULT_LIGHT, ThemeCatalog, ThemeContribution,
    bundled_theme_text, migrate_theme_settings_id, scheme_of_ui_theme,
};
pub use color::{Color, Hsla, Rgba, Rgba8, parse_css, parse_hex};
pub use registry::{ColorLookup, ColorRegistry, ColorScheme, ColorTransform, ColorValue, execute_transform, registry, resolve_color_value};
pub use selection::{HostColorScheme, ThemeSettings};
pub use theme::{
    ColorTheme, Customizations, DEFAULT_COLOR_CONFIG_VALUE, SemanticTokenRule, ThemeError, ThemeSource, TokenColorRule, TokenSettings, hex,
    is_theme_scope, normalize_color, theme_scope_matches, theme_scopes,
};
