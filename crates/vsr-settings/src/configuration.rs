//! The effective configuration: the registered defaults layered with the user's settings.json.
//!
//! upstream: src/vs/platform/configuration/common/configuration.ts @ 50e1350fc78d
//! upstream: src/vs/platform/configuration/common/configurationModels.ts @ 50e1350fc78d
//!
//! Layering mirrors upstream's `Configuration` for the default and user layers (the workspace
//! layer is not ported yet): a user value wins over the registered default.
//!
//! Object-valued settings (`files.exclude`) deep-merge the default and the user value, like
//! upstream's `mergeContents`: the user adds entries or overrides one (`"**/.git": false`).
//!
//! Differences from upstream, on purpose:
//! - Upstream stores user values as written and leaves validation to each consumer (for example
//!   `EditorIntOption.clampedInt` in `editorOptions.ts`). Here validation is done once, against
//!   the registered schema, with the same outcomes: a value of the wrong type, or not one of the
//!   enum values, falls back to the default; a number outside `minimum`/`maximum` is clamped to
//!   the bound (like `clampedInt`/`clampedFloat`, not a fallback); a non-integer for an integer
//!   setting falls back to the default. Settings whose schema is `Complex` (several types) are not
//!   validated, their consumer must.
//! - `ConfigurationChange.affected_keys` is the diff of the raw user documents, like upstream's
//!   `compare(...)` of the user models: setting a key to its default value still counts as a
//!   change of that key.

use std::collections::{BTreeSet, HashMap};

use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::registry::SettingsRegistry;
use crate::schema::SettingKind;
use crate::user_settings::UserSettings;

/// What `update_user` changed (upstream `IConfigurationChangeEvent`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigurationChange {
    pub affected_keys: BTreeSet<String>,
}

impl ConfigurationChange {
    pub fn is_empty(&self) -> bool {
        self.affected_keys.is_empty()
    }

    /// Upstream `affectsConfiguration(section)`: true when a changed key is `section` or lies
    /// below it. `"editor"` affects `"editor.fontSize"`, but `"editor.font"` does not.
    pub fn affects(&self, section: &str) -> bool {
        self.affected_keys.iter().any(|key| {
            key == section || key.strip_prefix(section).is_some_and(|rest| rest.starts_with('.'))
        })
    }
}

struct Registered {
    default: Value,
    kind: SettingKind,
}

/// Defaults layered with user overrides.
pub struct Configuration {
    registered: HashMap<String, Registered>,
    user: UserSettings,
    /// User values that passed validation (clamped where needed), by key.
    effective: HashMap<String, Value>,
}

impl Configuration {
    pub fn new(registry: &SettingsRegistry, user: UserSettings) -> Self {
        let mut registered = HashMap::with_capacity(registry.len());
        for key in registry.keys() {
            if let Some(s) = registry.get(key) {
                registered.insert(key.to_string(), Registered { default: s.default.clone(), kind: s.kind.clone() });
            }
        }
        let mut config = Self { registered, user: UserSettings::default(), effective: HashMap::new() };
        config.set_user(user);
        config
    }

    /// The user layer as written.
    pub fn user(&self) -> &UserSettings {
        &self.user
    }

    /// The value in effect: the user's value when set and valid, else the default. A key that is
    /// not registered returns the user's value as is.
    pub fn get_value(&self, key: &str) -> Option<&Value> {
        if let Some(v) = self.effective.get(key) {
            return Some(v);
        }
        self.registered.get(key).map(|r| &r.default)
    }

    /// The registered default (`None` for an unregistered key).
    pub fn default_value(&self, key: &str) -> Option<&Value> {
        self.registered.get(key).map(|r| &r.default)
    }

    pub fn get_bool(&self, key: &str) -> Option<bool> {
        self.get_value(key)?.as_bool()
    }

    pub fn get_f64(&self, key: &str) -> Option<f64> {
        self.get_value(key)?.as_f64()
    }

    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.get_value(key)?.as_str()
    }

    /// Deserializes the value, for enums (`#[serde(rename_all = "kebab-case")]`) and structured
    /// values. `None` when the value does not fit `T`.
    pub fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_value(self.get_value(key)?.clone()).ok()
    }

    /// Like `get`, but falls back to the registered default when the value does not fit `T`.
    pub fn get_enum<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.get(key).or_else(|| serde_json::from_value(self.default_value(key)?.clone()).ok())
    }

    /// Replaces the user layer and reports which keys changed.
    pub fn update_user(&mut self, user: UserSettings) -> ConfigurationChange {
        let mut affected: BTreeSet<String> = BTreeSet::new();
        for (key, value) in user.iter() {
            if self.user.get(key) != Some(value) {
                affected.insert(key.to_string());
            }
        }
        for (key, _) in self.user.iter() {
            if user.get(key).is_none() {
                affected.insert(key.to_string());
            }
        }
        self.set_user(user);
        ConfigurationChange { affected_keys: affected }
    }

    fn set_user(&mut self, user: UserSettings) {
        self.effective.clear();
        for (key, value) in user.iter() {
            match self.registered.get(key) {
                None => {
                    self.effective.insert(key.to_string(), value.clone());
                }
                Some(r) => {
                    if let Some(valid) = validate(&r.kind, value) {
                        // Upstream `mergeContents`: objects from both layers merge recursively.
                        let merged = match (&r.default, valid) {
                            (Value::Object(d), Value::Object(u)) => {
                                let mut base = d.clone();
                                merge_objects(&mut base, u);
                                Value::Object(base)
                            }
                            (_, valid) => valid,
                        };
                        self.effective.insert(key.to_string(), merged);
                    }
                }
            }
        }
        self.user = user;
    }
}

fn merge_objects(base: &mut serde_json::Map<String, Value>, over: serde_json::Map<String, Value>) {
    for (k, v) in over {
        match (base.get_mut(&k), v) {
            (Some(Value::Object(b)), Value::Object(o)) => merge_objects(b, o),
            (_, v) => {
                base.insert(k, v);
            }
        }
    }
}

/// The value as it takes effect, or `None` when it is invalid (the default applies).
fn validate(kind: &SettingKind, value: &Value) -> Option<Value> {
    match kind {
        SettingKind::Bool => value.is_boolean().then(|| value.clone()),
        SettingKind::Number { min, max, integer } => {
            let n = value.as_f64()?;
            if !n.is_finite() || (*integer && n.fract() != 0.0) {
                return None;
            }
            let clamped = n.max(min.unwrap_or(f64::NEG_INFINITY)).min(max.unwrap_or(f64::INFINITY));
            if clamped == n {
                return Some(value.clone());
            }
            Some(if *integer || clamped.fract() == 0.0 { Value::from(clamped as i64) } else { Value::from(clamped) })
        }
        SettingKind::String => value.is_string().then(|| value.clone()),
        SettingKind::Enum { values, .. } => values.contains(value).then(|| value.clone()),
        SettingKind::StringArray => value.as_array().filter(|a| a.iter().all(Value::is_string)).map(|_| value.clone()),
        SettingKind::Object => value.is_object().then(|| value.clone()),
        SettingKind::Complex => Some(value.clone()),
    }
}
