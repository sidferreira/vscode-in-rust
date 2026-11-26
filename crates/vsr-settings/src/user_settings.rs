//! A settings.json document: flat `"key": value` JSONC, read and written in place.
//!
//! Comments and trailing commas are accepted on load. Saving writes plain JSON, so comments
//! in an existing file are not kept. Keys keep the order of the file and a new key goes last, as
//! upstream's jsonc edits do (`setProperty` changes a value in place and appends a new property).

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use crate::jsonc::strip_jsonc;

/// The user's settings.json: only the keys the user has set.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UserSettings {
    values: Map<String, Value>,
}

impl UserSettings {
    /// Parses JSONC text. Empty or whitespace-only text is an empty document.
    pub fn parse(text: &str) -> io::Result<Self> {
        let stripped = strip_jsonc(text);
        if stripped.trim().is_empty() {
            return Ok(Self::default());
        }
        match serde_json::from_str::<Value>(&stripped) {
            Ok(Value::Object(values)) => Ok(Self { values }),
            Ok(_) => Err(io::Error::new(io::ErrorKind::InvalidData, "settings.json must contain an object")),
            Err(e) => Err(io::Error::new(io::ErrorKind::InvalidData, e)),
        }
    }

    /// Loads the file. A missing file is an empty document, like a fresh profile.
    pub fn load(path: &Path) -> io::Result<Self> {
        match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e),
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.values.get(key)
    }

    /// The keys and values the user has set, in the order of the file (new keys last).
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.values.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }

    /// Removes the user's value so the default applies again.
    pub fn reset(&mut self, key: &str) {
        // `remove` is a swap-remove under `preserve_order`: it would move the last key into the gap.
        self.values.shift_remove(key);
    }

    pub fn is_modified(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// The document as pretty JSON with a trailing newline, as `save` writes it.
    pub fn to_text(&self) -> String {
        let mut text = serde_json::to_string_pretty(&Value::Object(self.values.clone()))
            .expect("a JSON map always serializes");
        text.push('\n');
        text
    }

    /// Writes the document as pretty JSON, creating parent directories.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, self.to_text())
    }
}

/// Where rscode keeps the user's settings.json.
///
/// macOS: `~/Library/Application Support/rscode/User/settings.json`.
/// Elsewhere: `$XDG_CONFIG_HOME/rscode/User/settings.json`, or `~/.config/...`.
pub fn default_user_settings_path() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/rscode/User/settings.json")
    } else {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        config.join("rscode/User/settings.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_keeps_the_order_of_the_file_with_new_keys_last() {
        let mut u = UserSettings::parse(r#"{"z.last": 1, "a.first": 2, "m.mid": 3}"#).unwrap();
        u.set("a.first", Value::from(20));
        u.set("b.new", Value::from(4));
        u.reset("z.last");
        let keys: Vec<&str> = u.iter().map(|(k, _)| k).collect();
        assert_eq!(keys, ["a.first", "m.mid", "b.new"], "a changed value stays in place, a removed key leaves no reorder");
        let text = u.to_text();
        let order: Vec<usize> = ["a.first", "m.mid", "b.new"].iter().map(|k| text.find(k).unwrap()).collect();
        assert!(order.windows(2).all(|w| w[0] < w[1]), "the written text has the same order: {text}");
        assert_eq!(UserSettings::parse(&text).unwrap(), u);
    }
}
