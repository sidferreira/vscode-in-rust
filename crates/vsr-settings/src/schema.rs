//! One registered setting, read from the bundled `default-configuration.json`.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTreeModels.ts @ 50e1350fc78d

use serde::Deserialize;
use serde_json::Value;

use crate::title::setting_title;

/// The editor control a setting gets. Mirrors `SettingValueType` in upstream's `initSettingValueType`.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingKind {
    Bool,
    Number { min: Option<f64>, max: Option<f64>, integer: bool },
    String,
    Enum { values: Vec<Value>, labels: Vec<String>, descriptions: Vec<String> },
    StringArray,
    Object,
    /// Shown as "Edit in settings.json".
    Complex,
}

/// A setting as the settings editor shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingSchema {
    pub key: String,
    /// e.g. "Editor: Font Size"
    pub title: String,
    /// e.g. "Editor"; empty when the key has no dot
    pub category_label: String,
    /// e.g. "Font Size"
    pub name_label: String,
    /// Markdown.
    pub description: String,
    pub kind: SettingKind,
    pub default: Value,
    pub order: Option<i64>,
    pub tags: Vec<String>,
    pub deprecated: Option<String>,
}

/// One entry of `default-configuration.json`, as exported from VS Code.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RawSetting {
    pub key: String,
    #[serde(rename = "type")]
    pub ty: Option<Value>,
    pub default: Option<Value>,
    pub description: Option<String>,
    pub markdown_description: Option<String>,
    #[serde(rename = "enum")]
    pub enum_values: Option<Vec<Value>>,
    pub enum_descriptions: Option<Vec<String>>,
    pub markdown_enum_descriptions: Option<Vec<String>>,
    /// Entries may be null, meaning "use the value".
    pub enum_item_labels: Option<Vec<Option<String>>>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub order: Option<i64>,
    pub tags: Option<Vec<String>>,
    pub deprecation_message: Option<String>,
    pub markdown_deprecation_message: Option<String>,
    pub items: Option<Value>,
    pub additional_properties: Option<Value>,
    pub properties: Option<Value>,
    /// "core" or the id of the built-in extension that contributes the setting.
    pub source: Option<String>,
    pub extension_display_name: Option<String>,
    pub extension_title: Option<String>,
}

impl SettingSchema {
    pub(crate) fn from_raw(raw: RawSetting) -> Self {
        let title = setting_title(&raw.key);
        let format = crate::title::setting_key_to_display_format(&raw.key, "");
        let kind = kind_of(&raw);
        let description = raw.markdown_description.clone().or(raw.description.clone()).unwrap_or_default();
        let deprecated = raw.markdown_deprecation_message.clone().or(raw.deprecation_message.clone());
        Self {
            key: raw.key.clone(),
            title,
            category_label: format.category,
            name_label: format.label,
            description,
            kind,
            default: raw.default.clone().unwrap_or(Value::Null),
            order: raw.order,
            tags: raw.tags.clone().unwrap_or_default(),
            deprecated,
        }
    }
}

/// The single JSON type of a property, if it has exactly one (`"type": ["string", "null"]` has none).
fn single_type(ty: &Option<Value>) -> Option<&str> {
    match ty {
        Some(Value::String(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn kind_of(raw: &RawSetting) -> SettingKind {
    let ty = single_type(&raw.ty);
    if let Some(values) = raw.enum_values.as_ref().filter(|_| matches!(ty, None | Some("string" | "number" | "integer"))) {
        return enum_kind(raw, values);
    }
    match ty {
        Some("boolean") => SettingKind::Bool,
        Some("number") => SettingKind::Number { min: raw.minimum, max: raw.maximum, integer: false },
        Some("integer") => SettingKind::Number { min: raw.minimum, max: raw.maximum, integer: true },
        Some("string") => SettingKind::String,
        Some("array") => {
            let item_is_string = raw.items.as_ref().and_then(|i| i.get("type")).and_then(Value::as_str) == Some("string");
            if item_is_string { SettingKind::StringArray } else { SettingKind::Complex }
        }
        Some("object") => {
            let has_shape = raw.properties.is_some() || raw.additional_properties.as_ref().is_some_and(Value::is_object);
            if has_shape { SettingKind::Complex } else { SettingKind::Object }
        }
        _ => SettingKind::Complex,
    }
}

fn enum_kind(raw: &RawSetting, values: &[Value]) -> SettingKind {
    let value_label = |v: &Value| v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string());
    let labels = (0..values.len())
        .map(|i| {
            raw.enum_item_labels
                .as_ref()
                .and_then(|labels| labels.get(i).cloned().flatten())
                .unwrap_or_else(|| value_label(&values[i]))
        })
        .collect();
    let descriptions = raw
        .markdown_enum_descriptions
        .clone()
        .or(raw.enum_descriptions.clone())
        .unwrap_or_default();
    let descriptions = (0..values.len()).map(|i| descriptions.get(i).cloned().unwrap_or_default()).collect();
    SettingKind::Enum { values: values.to_vec(), labels, descriptions }
}
