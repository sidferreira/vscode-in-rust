//! The Settings editor's search query: `@modified`, `@tag:`, `@ext:`, `@feature:`, `@id:`, `@lang:`
//! filters plus free text, and the filter menu's toggle helpers.
//!
//! Upstream source: src/vs/workbench/contrib/preferences/browser/settingsTreeModels.ts (`parseQuery`)
//! and settingsSearchMenu.ts (the filter actions). The upstream regexes allow a filter anywhere in
//! the text; here a filter starts at a word boundary, which is what people type.

/// A query split into filters and the remaining free text.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ParsedQuery {
    /// `@modified`, `@stable`, `@tag:x`: a setting must carry every tag.
    pub tags: Vec<String>,
    /// `@ext:publisher.name[,...]`: contributed by one of these extensions.
    pub extensions: Vec<String>,
    /// `@feature:x[,...]` (parsed, not applied: rscode has no feature map yet).
    pub features: Vec<String>,
    /// `@id:key[,...]`, a trailing `.*` matches a prefix.
    pub ids: Vec<String>,
    /// `@lang:id` (first one wins; parsed, not applied: no language-override settings yet).
    pub language: Option<String>,
    /// What is left, whitespace-trimmed.
    pub text: String,
}

impl ParsedQuery {
    pub fn has_filters(&self) -> bool {
        !(self.tags.is_empty()
            && self.extensions.is_empty()
            && self.features.is_empty()
            && self.ids.is_empty()
            && self.language.is_none())
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t == tag)
    }
}

pub fn parse(query: &str) -> ParsedQuery {
    let mut out = ParsedQuery::default();
    let mut text: Vec<String> = Vec::new();
    let mut rest = query.trim_start();
    while !rest.is_empty() {
        let end = word_end(rest);
        let (word, tail) = rest.split_at(end);
        if let Some((name, value)) = filter_word(rest, word) {
            let (value, consumed) = value;
            let parts = || value.split(',').map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
            match name {
                "modified" | "stable" => out.tags.push(name.to_string()),
                "tag" => out.tags.extend(parts()),
                "ext" => out.extensions.extend(parts()),
                "feature" => out.features.extend(parts()),
                "id" => out.ids.extend(parts()),
                "lang" => {
                    if out.language.is_none() {
                        out.language = parts().next();
                    }
                }
                _ => unreachable!(),
            }
            rest = rest[consumed..].trim_start();
        } else {
            text.push(word.to_string());
            rest = tail.trim_start();
        }
    }
    out.text = text.join(" ");
    out
}

fn word_end(s: &str) -> usize {
    s.find(char::is_whitespace).unwrap_or(s.len())
}

/// If `word` (the first word of `rest`) is a filter: its name, the value and the bytes consumed.
fn filter_word<'a>(rest: &'a str, word: &'a str) -> Option<(&'static str, (&'a str, usize))> {
    for name in ["modified", "stable"] {
        if word.eq_ignore_ascii_case(&format!("@{name}")) {
            return Some((name, ("", word.len())));
        }
    }
    for name in ["tag", "ext", "feature", "id", "lang"] {
        let prefix = format!("@{name}:");
        if word.get(..prefix.len()).is_some_and(|p| p.eq_ignore_ascii_case(&prefix)) {
            let after = &rest[prefix.len()..];
            return Some(match after.strip_prefix('"') {
                Some(quoted) => match quoted.find('"') {
                    Some(close) => (name, (&quoted[..close], prefix.len() + 1 + close + 1)),
                    None => (name, (quoted, rest.len())),
                },
                None => {
                    let end = word_end(after);
                    (name, (&after[..end], prefix.len() + end))
                }
            });
        }
    }
    None
}

/// Whether `key` passes an `@id:` filter (exact, or a `prefix.*` wildcard).
pub fn id_matches(filter: &str, key: &str) -> bool {
    match filter.strip_suffix('*') {
        Some(prefix) if prefix.ends_with('.') => key.starts_with(prefix),
        _ => filter == key,
    }
}

/// Filter menu "toggle" entries: add `filter` to the query, or remove it when present.
pub fn toggle(query: &str, filter: &str) -> String {
    let words: Vec<&str> = query.split(' ').collect();
    if words.contains(&filter) {
        words.into_iter().filter(|w| *w != filter).collect::<Vec<_>>().join(" ")
    } else {
        append(query, filter)
    }
}

/// Like `toggle`, but turning `filter` on removes the `exclusive` ones (Stable / Preview / Experimental).
pub fn toggle_exclusive(query: &str, filter: &str, exclusive: &[&str]) -> String {
    if query.split(' ').any(|w| w == filter) {
        return toggle(query, filter);
    }
    let kept: Vec<&str> = query.split(' ').filter(|w| !exclusive.contains(w)).collect();
    append(&kept.join(" "), filter)
}

/// Append `filter` after the current text (upstream `doSearchWidgetAction`).
pub fn append(query: &str, filter: &str) -> String {
    let q = query.trim_end();
    if q.is_empty() { filter.to_string() } else { format!("{q} {filter}") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_filters_from_text() {
        let q = parse("font @modified size @id:editor.fontSize,editor.tabSize @ext:vscode.git");
        assert_eq!(q.text, "font size");
        assert_eq!(q.tags, ["modified"]);
        assert_eq!(q.ids, ["editor.fontSize", "editor.tabSize"]);
        assert_eq!(q.extensions, ["vscode.git"]);
        assert!(q.has_filters());
    }

    #[test]
    fn tags_quotes_and_empty_values() {
        let q = parse("@tag:\"a b\" @tag:preview @ext: @lang:typescript @lang:css @stable");
        assert_eq!(q.tags, ["a b", "preview", "stable"]);
        assert!(q.extensions.is_empty());
        assert_eq!(q.language.as_deref(), Some("typescript"));
        assert_eq!(q.text, "");
    }

    #[test]
    fn plain_text_has_no_filters() {
        let q = parse("  font   size ");
        assert_eq!(q.text, "font size");
        assert!(!q.has_filters());
        assert_eq!(parse("a@modified").text, "a@modified");
    }

    #[test]
    fn id_wildcards() {
        assert!(id_matches("editor.*", "editor.fontSize"));
        assert!(!id_matches("editor.*", "editorx.a"));
        assert!(id_matches("editor.fontSize", "editor.fontSize"));
        assert!(!id_matches("editor.font", "editor.fontSize"));
    }

    #[test]
    fn menu_toggles() {
        assert_eq!(toggle("", "@modified"), "@modified");
        assert_eq!(toggle("font", "@modified"), "font @modified");
        assert_eq!(toggle("font @modified", "@modified"), "font");
        assert_eq!(toggle_exclusive("a @tag:preview", "@stable", &["@tag:preview", "@tag:experimental"]), "a @stable");
        assert_eq!(toggle_exclusive("a @stable", "@stable", &["@tag:preview"]), "a");
    }
}
