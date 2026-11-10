//! upstream: src/vs/platform/extensions/common/extensions.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/extensions/common/extensionHostProtocol.ts @ 50e1350fc78d
//!
//! `IExtensionDescription`: the extension's `package.json` plus what the scanner adds (identifier,
//! location, flags). Built here from a folder on disk; the host takes it as JSON.

use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use vsr_rpc::handshake::file_uri;

/// `ExtensionIdentifier` on the wire. The host never revives it, and its maps key on `_lower`, so
/// both fields are required.
pub fn identifier(id: &str) -> Value {
    json!({ "value": id, "_lower": id.to_lowercase() })
}

#[derive(Debug, Clone)]
pub struct ExtensionDescription {
    /// `publisher.name`, case as written.
    pub id: String,
    /// The real path of the extension folder (the host indexes real paths).
    pub location: PathBuf,
    /// The `IExtensionDescription` JSON handed to the host.
    pub json: Value,
    pub activation_events: Vec<String>,
}

impl ExtensionDescription {
    pub fn from_dir(dir: &Path) -> Result<Self, String> {
        let location = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        let text = std::fs::read_to_string(location.join("package.json"))
            .map_err(|e| format!("{}: package.json: {e}", location.display()))?;
        let manifest: Value =
            serde_json::from_str(&text).map_err(|e| format!("package.json: {e}"))?;
        let field = |k: &str| {
            manifest
                .get(k)
                .and_then(Value::as_str)
                .ok_or(format!("package.json has no string `{k}`"))
        };
        let id = format!("{}.{}", field("publisher")?, field("name")?);
        let activation_events = manifest
            .get("activationEvents")
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        let mut json = manifest;
        let obj = json
            .as_object_mut()
            .ok_or("package.json is not an object")?;
        obj.insert("identifier".into(), identifier(&id));
        obj.insert("id".into(), json!(id));
        obj.insert("extensionLocation".into(), file_uri(&location));
        // `TargetPlatform.UNDEFINED` is the string "undefined".
        obj.insert("targetPlatform".into(), json!("undefined"));
        for flag in [
            "isBuiltin",
            "isUserBuiltin",
            "isUnderDevelopment",
            "preRelease",
        ] {
            obj.insert(flag.into(), json!(false));
        }
        Ok(ExtensionDescription {
            id,
            location,
            json,
            activation_events,
        })
    }

    /// The `contributes.configuration` defaults as `(key, default)` pairs: the default layer of
    /// the configuration the host sees (the registry of upstream's `ConfigurationRegistry`).
    pub fn configuration_defaults(&self) -> Vec<(String, Value)> {
        let mut out = Vec::new();
        let conf = self.json.pointer("/contributes/configuration");
        let sections: Vec<&Value> = match conf {
            Some(Value::Array(a)) => a.iter().collect(),
            Some(v) => vec![v],
            None => vec![],
        };
        for section in sections {
            if let Some(props) = section.get("properties").and_then(Value::as_object) {
                for (key, schema) in props {
                    if let Some(d) = schema.get("default") {
                        out.push((key.clone(), d.clone()));
                    }
                }
            }
        }
        out
    }
}

/// `IExtensionDescriptionDelta` adding `extensions` (all of them to this host).
pub fn delta_adding(extensions: &[ExtensionDescription]) -> Value {
    let mut events = serde_json::Map::new();
    for e in extensions {
        events.insert(e.id.clone(), json!(e.activation_events));
    }
    json!({
        "versionId": 1,
        "toRemove": [],
        "toAdd": extensions.iter().map(|e| e.json.clone()).collect::<Vec<_>>(),
        // The host reads activation events from here, not from the manifest.
        "addActivationEvents": events,
        "myToRemove": [],
        "myToAdd": extensions.iter().map(|e| identifier(&e.id)).collect::<Vec<_>>(),
    })
}

/// Nest dotted keys: `[("a.b", 1)]` -> `{"a": {"b": 1}}` (the `contents` of a configuration model).
pub fn nest(pairs: impl IntoIterator<Item = (String, Value)>) -> Value {
    let mut root = serde_json::Map::new();
    for (key, value) in pairs {
        let mut parts: Vec<&str> = key.split('.').collect();
        let last = parts.pop().unwrap_or("");
        let mut cur = &mut root;
        for p in parts {
            let entry = cur
                .entry(p.to_string())
                .or_insert_with(|| Value::Object(Default::default()));
            if !entry.is_object() {
                *entry = Value::Object(Default::default());
            }
            cur = entry.as_object_mut().unwrap();
        }
        cur.insert(last.to_string(), value);
    }
    Value::Object(root)
}
