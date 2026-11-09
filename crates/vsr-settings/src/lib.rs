//! Settings data for rscode's Settings editor: the registered settings (as VS Code 1.144.1
//! exports them), the table of contents, and the user's settings.json.
//!
//! Provenance of the bundled data is in `README.md`; `scripts/export-default-settings`
//! regenerates the settings asset.

mod configuration;
pub mod jsonc;
mod registry;
mod schema;
mod title;
mod toc;
mod user_settings;

pub use configuration::{Configuration, ConfigurationChange};
pub use registry::SettingsRegistry;
pub use schema::{SettingKind, SettingSchema};
pub use title::{DisplayFormat, setting_key_to_display_format, wordify_key};
pub use toc::TocNode;
pub use user_settings::{UserSettings, default_user_settings_path};
