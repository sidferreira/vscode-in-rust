//! The app's configuration service (ADR-0022): a gpui `Global` holding the effective
//! `Configuration` (registry defaults + the user's settings.json), the file it lives in, and the
//! live reload of that file.
//!
//! upstream: src/vs/platform/configuration/common/configuration.ts @ 50e1350fc78d
//! upstream: src/vs/platform/configuration/common/configurationModels.ts @ 50e1350fc78d
//!
//! Writers (the Settings editor) go through `AppConfig::set_value`, which saves the file and
//! updates the configuration in one step, so changes apply immediately. Readers (the workbench,
//! the editor) call `cx.observe_global::<AppConfig>(..)` and re-read what they need; `last_change`
//! says which keys the latest update touched.
//!
//! External edits are picked up by polling the file (every `POLL_INTERVAL`): a changed text must be
//! seen on two consecutive polls before it is applied (debounce: editors write in several steps),
//! and the text we wrote ourselves is remembered so our own writes are not re-applied. A file with
//! a syntax error keeps the previous configuration and is reported through `error()`; writes are
//! refused until the file parses again (upstream refuses to write into an invalid settings file too).

#[cfg(test)]
mod e2e;
#[cfg(test)]
mod e2e_files;
#[cfg(test)]
mod e2e_sash;
#[cfg(test)]
mod e2e_syntax;
#[cfg(test)]
mod e2e_typing;
pub mod files;
pub mod folding;
pub mod glob;
pub mod minimap;
pub mod scrollbar;
pub mod settings;
pub mod wired;

use std::cell::RefCell;
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use gpui_kit::{App, BorrowAppContext, Global};
use serde_json::Value;
use vsr_settings::{Configuration, ConfigurationChange, SettingsRegistry, UserSettings};

/// How often the settings file is checked for external edits.
pub const POLL_INTERVAL: Duration = Duration::from_millis(300);

pub struct AppConfig {
    registry: Rc<SettingsRegistry>,
    configuration: Configuration,
    /// `None`: in memory only (nothing is read or written).
    path: Option<PathBuf>,
    last_change: ConfigurationChange,
    /// The file's text as we last read or wrote it ("" when missing).
    known_text: RefCell<String>,
    /// A differing text seen once; applied when seen again.
    pending: RefCell<Option<String>>,
    error: Option<String>,
    file_invalid: bool,
    generation: u64,
}

impl Global for AppConfig {}

impl AppConfig {
    /// Loads `path` (or starts empty and in memory for `None`) and starts watching it.
    /// Replaces any existing configuration.
    pub fn install(cx: &mut App, path: Option<PathBuf>) {
        let generation = cx.try_global::<AppConfig>().map_or(0, |c| c.generation + 1);
        let registry = cx.try_global::<AppConfig>().map(|c| c.registry.clone()).unwrap_or_else(|| Rc::new(SettingsRegistry::load_default()));
        let text = path.as_deref().and_then(|p| fs::read_to_string(p).ok()).unwrap_or_default();
        let (user, error) = match UserSettings::parse(&text) {
            Ok(user) => (user, None),
            Err(e) => (UserSettings::default(), Some(describe(path.as_deref(), &e))),
        };
        let watch = path.is_some();
        cx.set_global(AppConfig {
            configuration: Configuration::new(&registry, user),
            registry,
            path,
            last_change: ConfigurationChange::default(),
            known_text: RefCell::new(text),
            pending: RefCell::new(None),
            file_invalid: error.is_some(),
            error,
            generation,
        });
        if watch {
            cx.spawn(async move |cx| {
                loop {
                    cx.background_executor().timer(POLL_INTERVAL).await;
                    if !cx.update(|cx| Self::poll(cx, generation)) {
                        break;
                    }
                }
            })
            .detach();
        }
    }

    pub fn get(cx: &App) -> &AppConfig {
        cx.global::<AppConfig>()
    }

    pub fn configuration(&self) -> &Configuration {
        &self.configuration
    }

    pub fn registry(&self) -> Rc<SettingsRegistry> {
        self.registry.clone()
    }

    pub fn user(&self) -> &UserSettings {
        self.configuration.user()
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// The keys the latest update changed (upstream `IConfigurationChangeEvent`).
    pub fn last_change(&self) -> &ConfigurationChange {
        &self.last_change
    }

    /// A problem with the settings file: a syntax error found on load or reload, or a failed write.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    /// Sets (`Some`) or resets (`None`) a user setting, saves the file and notifies observers.
    pub fn set_value(cx: &mut App, key: &str, value: Option<Value>) -> Result<(), String> {
        cx.update_global::<AppConfig, _>(|c, _| c.write(key, value))
    }

    fn write(&mut self, key: &str, value: Option<Value>) -> Result<(), String> {
        if self.file_invalid {
            let msg = format!(
                "Unable to write into user settings: fix the errors in {} first.",
                self.path.as_deref().map_or_else(|| "settings.json".to_string(), |p| p.display().to_string())
            );
            self.error = Some(msg.clone());
            self.last_change = ConfigurationChange::default();
            return Err(msg);
        }
        let mut user = self.configuration.user().clone();
        match value {
            Some(v) => user.set(key, v),
            None => user.reset(key),
        }
        if let Some(path) = &self.path {
            let text = user.to_text();
            let written = path
                .parent()
                .map_or(Ok(()), fs::create_dir_all)
                .and_then(|()| fs::write(path, &text));
            if let Err(e) = written {
                let msg = describe(Some(path), &e);
                self.error = Some(msg.clone());
                self.last_change = ConfigurationChange::default();
                return Err(msg);
            }
            *self.known_text.borrow_mut() = text;
            *self.pending.borrow_mut() = None;
        }
        self.last_change = self.configuration.update_user(user);
        self.error = None;
        Ok(())
    }

    /// One watcher tick. Returns false when this watcher is outdated (the configuration was
    /// reinstalled) and should stop.
    pub fn poll(cx: &mut App, generation: u64) -> bool {
        let Some(c) = cx.try_global::<AppConfig>() else { return false };
        if c.generation != generation {
            return false;
        }
        if let Some(text) = c.changed_text() {
            cx.update_global::<AppConfig, _>(|c, _| c.apply_external(text));
        }
        true
    }

    /// The file's text when it differs from what we know and was already seen once before.
    fn changed_text(&self) -> Option<String> {
        let now = fs::read_to_string(self.path.as_ref()?).unwrap_or_default();
        if now == *self.known_text.borrow() {
            *self.pending.borrow_mut() = None;
            return None;
        }
        let mut pending = self.pending.borrow_mut();
        if pending.as_ref() == Some(&now) {
            *pending = None;
            Some(now)
        } else {
            *pending = Some(now);
            None
        }
    }

    fn apply_external(&mut self, text: String) {
        *self.known_text.borrow_mut() = text.clone();
        match UserSettings::parse(&text) {
            Ok(user) => {
                self.last_change = self.configuration.update_user(user);
                self.error = None;
                self.file_invalid = false;
            }
            Err(e) => {
                self.last_change = ConfigurationChange::default();
                self.error = Some(describe(self.path.as_deref(), &e));
                self.file_invalid = true;
            }
        }
    }
}

fn describe(path: Option<&Path>, e: &dyn std::fmt::Display) -> String {
    match path {
        Some(p) => format!("{}: {e}", p.display()),
        None => e.to_string(),
    }
}
