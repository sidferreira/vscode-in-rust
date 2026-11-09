//! The window title (`window.title`, `window.titleSeparator`): the template engine and the
//! variables the workbench fills in. UI free, so the rules are unit tested here and the workbench
//! only supplies the values.
//!
//! upstream: src/vs/base/common/labels.ts @ 50e1350fc78d (`template`)
//! upstream: src/vs/base/test/common/labels.test.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/parts/titlebar/windowTitle.ts @ 50e1350fc78d (`getWindowTitle`)
//!
//! Not ported: `${remoteName}`, `${profileName}` (always empty: one profile, no remotes),
//! `${focusedView}`, `${activeEditorState}` and the screen reader variant (no view focus or
//! decorations service yet). They expand to nothing, as they do upstream without those services.

use std::collections::HashMap;

/// macOS default (the export's default for `window.title` on desktop macOS).
pub const DEFAULT_TITLE: &str = "${activeEditorShort}${separator}${rootName}${separator}${profileName}";
/// macOS default separator (`window.titleSeparator`).
pub const DEFAULT_SEPARATOR: &str = " \u{2014} ";
/// Upstream's dirty marker, put in front of `${dirty}` (`WindowTitle.TITLE_DIRTY`).
pub const TITLE_DIRTY: &str = "\u{25cf} ";

/// What a template variable resolves to: text (a variable) or a separator with its label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value<'a> {
    Text(&'a str),
    Separator(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Text,
    Variable,
    Separator,
}

/// Port of upstream `template(template, values)`. Variables that are unknown or empty expand to
/// nothing; a separator is kept only between two non-empty pieces, and never twice in a row.
///
/// `$` (or `${`) opens a variable and `}` closes it. A `$` with no closing `}` expands to nothing.
pub fn template(template: &str, values: &HashMap<&str, Value<'_>>) -> String {
    let mut segments: Vec<(Kind, String)> = Vec::new();
    let mut in_variable = false;
    let mut cur = String::new();
    for ch in template.chars() {
        if ch == '$' || (in_variable && ch == '{') {
            if !cur.is_empty() {
                segments.push((Kind::Text, std::mem::take(&mut cur)));
            }
            in_variable = true;
        } else if ch == '}' && in_variable {
            match values.get(cur.as_str()) {
                Some(Value::Text(text)) if !text.is_empty() => segments.push((Kind::Variable, text.to_string())),
                // Never two separators in a row.
                Some(Value::Separator(label)) if segments.last().map(|s| s.0) != Some(Kind::Separator) => {
                    segments.push((Kind::Separator, label.to_string()));
                }
                _ => {}
            }
            cur.clear();
            in_variable = false;
        } else {
            cur.push(ch);
        }
    }
    if !cur.is_empty() && !in_variable {
        segments.push((Kind::Text, cur));
    }

    let filled = |s: Option<&(Kind, String)>| s.is_some_and(|(kind, text)| *kind != Kind::Separator && !text.is_empty());
    segments
        .iter()
        .enumerate()
        .filter(|(i, (kind, _))| *kind != Kind::Separator || (*i > 0 && filled(segments.get(i - 1)) && filled(segments.get(i + 1))))
        .map(|(_, (_, text))| text.as_str())
        .collect()
}

/// The values the workbench knows about, named as upstream names its variables.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Variables {
    /// The active editor's file name, `Settings` for the Settings editor, or "".
    pub active_editor_short: String,
    /// The path of the active file relative to the folder, or its file name.
    pub active_editor_medium: String,
    /// The full path of the active file.
    pub active_editor_long: String,
    pub active_editor_language_id: String,
    pub active_folder_short: String,
    pub active_folder_medium: String,
    pub active_folder_long: String,
    pub root_name: String,
    pub root_path: String,
    pub folder_name: String,
    pub folder_path: String,
    /// True when the active editor has unsaved changes (expands to `TITLE_DIRTY`).
    pub dirty: bool,
    pub app_name: String,
}

impl Variables {
    /// Expands `template` with these variables and `separator`. `template` and `separator` are the
    /// configured values; `None` (a non-string setting) means the default, as upstream does.
    pub fn expand(&self, template_text: Option<&str>, separator: Option<&str>) -> String {
        let separator = separator.unwrap_or(DEFAULT_SEPARATOR);
        let dirty = if self.dirty { TITLE_DIRTY } else { "" };
        let mut values: HashMap<&str, Value<'_>> = HashMap::new();
        let texts: [(&str, &str); 14] = [
            ("activeEditorShort", &self.active_editor_short),
            ("activeEditorMedium", &self.active_editor_medium),
            ("activeEditorLong", &self.active_editor_long),
            ("activeEditorLanguageId", &self.active_editor_language_id),
            ("activeFolderShort", &self.active_folder_short),
            ("activeFolderMedium", &self.active_folder_medium),
            ("activeFolderLong", &self.active_folder_long),
            ("rootName", &self.root_name),
            ("rootNameShort", &self.root_name),
            ("rootPath", &self.root_path),
            ("folderName", &self.folder_name),
            ("folderPath", &self.folder_path),
            ("appName", &self.app_name),
            ("profileName", ""),
        ];
        for (name, text) in texts {
            values.insert(name, Value::Text(text));
        }
        values.insert("dirty", Value::Text(dirty));
        values.insert("remoteName", Value::Text(""));
        values.insert("focusedView", Value::Text(""));
        values.insert("activeEditorState", Value::Text(""));
        values.insert("separator", Value::Separator(separator));
        template(template_text.unwrap_or(DEFAULT_TITLE), &values)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values<'a>(pairs: &[(&'a str, Value<'a>)]) -> HashMap<&'a str, Value<'a>> {
        pairs.iter().copied().collect()
    }

    // upstream: Labels > template
    #[test]
    fn template_matches_upstream_labels() {
        let sep = Value::Separator(" - ");
        let none: HashMap<&str, Value<'_>> = HashMap::new();
        // simple
        assert_eq!(template("Foo Bar", &none), "Foo Bar");
        assert_eq!(template("Foo${}Bar", &none), "FooBar");
        assert_eq!(template("$FooBar", &none), "");
        assert_eq!(template("}FooBar", &none), "}FooBar");
        assert_eq!(template("Foo ${one} Bar", &values(&[("one", Value::Text("value"))])), "Foo value Bar");
        assert_eq!(
            template("Foo ${one} Bar ${two}", &values(&[("one", Value::Text("value")), ("two", Value::Text("other value"))])),
            "Foo value Bar other value"
        );

        // conditional separator
        assert_eq!(template("Foo${separator}Bar", &none), "FooBar");
        assert_eq!(template("Foo${separator}Bar", &values(&[("separator", sep)])), "Foo - Bar");
        assert_eq!(
            template("${separator}Foo${separator}Bar", &values(&[("value", Value::Text("something")), ("separator", sep)])),
            "Foo - Bar"
        );
        assert_eq!(
            template("${value} Foo${separator}Bar", &values(&[("value", Value::Text("something")), ("separator", sep)])),
            "something Foo - Bar"
        );

        // real world example (macOS)
        let t = "${activeEditorShort}${separator}${rootName}";
        let v = |short: &'static str, root: &'static str| {
            values(&[("activeEditorShort", Value::Text(short)), ("rootName", Value::Text(root)), ("separator", sep)])
        };
        assert_eq!(template(t, &v("", "")), "");
        assert_eq!(template(t, &v("", "root")), "root");
        assert_eq!(template(t, &v("markdown.txt", "root")), "markdown.txt - root");

        // real world example (other)
        let t = "${dirty}${activeEditorShort}${separator}${rootName}${separator}${appName}";
        let v = |dirty: &'static str, short: &'static str, root: &'static str, app: &'static str| {
            values(&[
                ("dirty", Value::Text(dirty)),
                ("activeEditorShort", Value::Text(short)),
                ("rootName", Value::Text(root)),
                ("appName", Value::Text(app)),
                ("separator", sep),
            ])
        };
        assert_eq!(template(t, &v("", "", "", "")), "");
        assert_eq!(template(t, &v("", "", "", "Visual Studio Code")), "Visual Studio Code");
        assert_eq!(template(t, &v("", "Untitled-1", "", "Visual Studio Code")), "Untitled-1 - Visual Studio Code");
        assert_eq!(template(t, &v("", "", "monaco", "Visual Studio Code")), "monaco - Visual Studio Code");
        assert_eq!(template(t, &v("", "somefile.txt", "monaco", "Visual Studio Code")), "somefile.txt - monaco - Visual Studio Code");
        assert_eq!(template(t, &v("* ", "somefile.txt", "monaco", "Visual Studio Code")), "* somefile.txt - monaco - Visual Studio Code");

        let t = "${dirty}${activeEditorShort}${separator}${rootNameShort}${separator}${appName}";
        let v = values(&[
            ("dirty", Value::Text("")),
            ("activeEditorShort", Value::Text("")),
            ("rootName", Value::Text("monaco (Workspace)")),
            ("rootNameShort", Value::Text("monaco")),
            ("appName", Value::Text("Visual Studio Code")),
            ("separator", sep),
        ]);
        assert_eq!(template(t, &v), "monaco - Visual Studio Code");
    }

    #[test]
    fn default_title_is_the_macos_template() {
        let vars = Variables {
            active_editor_short: "README.md".into(),
            root_name: "rscode".into(),
            app_name: "rscode".into(),
            ..Variables::default()
        };
        assert_eq!(vars.expand(None, None), "README.md \u{2014} rscode");
        // Non-string settings fall back to the defaults.
        assert_eq!(vars.expand(None, None), vars.expand(Some(DEFAULT_TITLE), Some(DEFAULT_SEPARATOR)));
    }

    #[test]
    fn dirty_marker_and_no_editor() {
        let mut vars = Variables { root_name: "proj".into(), ..Variables::default() };
        assert_eq!(vars.expand(None, None), "proj");
        vars.active_editor_short = "a.rs".into();
        vars.dirty = true;
        assert_eq!(vars.expand(Some("${dirty}${activeEditorShort}${separator}${rootName}"), Some(" - ")), "\u{25cf} a.rs - proj");
    }

    #[test]
    fn custom_template_and_separator_are_used() {
        let vars = Variables {
            active_editor_short: "a.rs".into(),
            active_editor_medium: "src/a.rs".into(),
            root_name: "proj".into(),
            ..Variables::default()
        };
        assert_eq!(vars.expand(Some("${rootName}: ${activeEditorMedium}"), None), "proj: src/a.rs");
        assert_eq!(vars.expand(Some("${activeEditorShort}${separator}${rootName}"), Some(" | ")), "a.rs | proj");
        // Unknown variables expand to nothing.
        assert_eq!(vars.expand(Some("${nope}${activeEditorShort}"), None), "a.rs");
    }
}
