//! Untitled text editors (new, unsaved files): the name of a new file, when it is dirty, the tab
//! title taken from its first line, and the language a new file starts with (ADR-0057).
//!
//! upstream: src/vs/workbench/services/untitled/common/untitledTextEditorModel.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/workbench/services/untitled/common/untitledTextEditorService.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//!
//! These are pure rules over the text. The editor that owns the text (the workbench's open editor)
//! asks them on every read, so there is no cached state to keep in step with the buffer. Upstream's
//! events, encodings and working-copy registration are not modelled here.
//!
//! Simplifications: the first-line title is cut at 40 `char`s, not grapheme clusters (upstream
//! `getCharContainingOffset`); whitespace is the Unicode `White_Space` set (upstream `\s`).

use std::collections::HashSet;

use vsr_languages::PLAINTEXT_LANGUAGE_ID;

/// The resource name prefix of a new untitled text: `Untitled-1`, `Untitled-2`...
pub const NAME_PREFIX: &str = "Untitled-";

/// `workbench.editor.untitled.labelFormat` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LabelFormat {
    /// The first line of the text, when it has words (the default).
    #[default]
    Content,
    /// Always `Untitled-N`.
    Name,
}

impl LabelFormat {
    /// The setting's value; anything but `name` is `content` (upstream reads only these two).
    pub fn from_setting(value: &str) -> Self {
        if value == "name" {
            Self::Name
        } else {
            Self::Content
        }
    }
}

/// The special `files.defaultLanguage` value: the language of the editor that was active.
pub const ACTIVE_EDITOR_LANGUAGE: &str = "${activeEditorLanguage}";

/// Upstream `FIRST_LINE_NAME_MAX_LENGTH`: the most characters a title taken from the first line has.
const FIRST_LINE_NAME_MAX_LENGTH: usize = 40;
/// Upstream `FIRST_LINE_NAME_CANDIDATE_MAX_LENGTH`: only this many characters of the first line are read.
const FIRST_LINE_NAME_CANDIDATE_MAX_LENGTH: usize = FIRST_LINE_NAME_MAX_LENGTH * 10;

/// The lowest `Untitled-N` (N from 1) that no open editor uses. Upstream counts up from 1 and skips
/// the names of the models that exist, so a number is reused once its editor is gone.
pub fn next_name<'a>(in_use: impl IntoIterator<Item = &'a str>) -> String {
    let used: HashSet<&str> = in_use.into_iter().collect();
    (1..)
        .map(|n| format!("{NAME_PREFIX}{n}"))
        .find(|name| !used.contains(name.as_str()))
        .expect("an unused name exists")
}

/// An untitled text without a file is dirty when it has any text. It is clean again as soon as its
/// text is empty (upstream `onModelContentChanged`), and an initial value makes it dirty at once.
pub fn is_dirty(text: &str) -> bool {
    !text.is_empty()
}

/// The language of a new untitled text (upstream `doCreate` options and the model's constructor):
/// the language asked for when it is created, else `files.defaultLanguage` (where
/// `${activeEditorLanguage}` means the active editor's language), else plain text.
pub fn new_language(
    explicit: Option<&str>,
    files_default_language: Option<&str>,
    active_editor_language: Option<&str>,
) -> String {
    if let Some(language) = explicit.filter(|l| !l.is_empty()) {
        return language.to_string();
    }
    let configured = match files_default_language.filter(|l| !l.is_empty()) {
        Some(ACTIVE_EDITOR_LANGUAGE) => active_editor_language.filter(|l| !l.is_empty()),
        other => other,
    };
    configured.unwrap_or(PLAINTEXT_LANGUAGE_ID).to_string()
}

/// The title taken from a first line of text (upstream `updateNameFromFirstLine`): whitespace is
/// trimmed and collapsed, the right-to-left override is dropped, the title is cut to
/// [`FIRST_LINE_NAME_MAX_LENGTH`] characters, and it is used only when it has a word character
/// (the word definition: any character that is not whitespace or punctuation).
pub fn first_line_name(first_line: &str) -> Option<String> {
    let candidate: String = first_line
        .chars()
        .take(FIRST_LINE_NAME_CANDIDATE_MAX_LENGTH)
        .collect();
    let mut normalised = String::new();
    for word in candidate.split_whitespace() {
        if !normalised.is_empty() {
            normalised.push(' ');
        }
        normalised.push_str(word);
    }
    let normalised = normalised.replace('\u{202E}', "");
    let title: String = normalised
        .chars()
        .take(FIRST_LINE_NAME_MAX_LENGTH)
        .collect();
    if title.chars().any(is_word_char) {
        Some(title)
    } else {
        None
    }
}

/// The word definition upstream uses for the title: a character that is neither whitespace nor one
/// of the separators of `DEFAULT_WORD_REGEXP`.
fn is_word_char(c: char) -> bool {
    !c.is_whitespace() && !"`~!@#$%^&*()-=+[{]}\\|;:'\",.<>/?".contains(c)
}

/// The tab title of an untitled text: its first line under [`LabelFormat::Content`] when that has
/// words, else the resource name.
pub fn label(name: &str, text: &str, format: LabelFormat) -> String {
    if format == LabelFormat::Content {
        let first_line = text.split('\n').next().unwrap_or("");
        if let Some(title) = first_line_name(first_line) {
            return title;
        }
    }
    name.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_count_up_and_reuse_the_free_number() {
        assert_eq!(next_name([]), "Untitled-1");
        assert_eq!(next_name(["Untitled-1", "Untitled-3"]), "Untitled-2");
        assert_eq!(next_name(["Untitled-1", "Untitled-2"]), "Untitled-3");
        assert_eq!(next_name(["notes.md"]), "Untitled-1");
    }

    #[test]
    fn first_line_becomes_the_title_when_it_has_words() {
        assert_eq!(
            first_line_name("  fn   main()  {"),
            Some("fn main() {".to_string())
        );
        assert_eq!(
            first_line_name("hello\u{202E}world"),
            Some("helloworld".to_string())
        );
    }

    #[test]
    fn first_line_is_cut_and_needs_a_word() {
        let long = "a".repeat(60);
        assert_eq!(first_line_name(&long).map(|t| t.chars().count()), Some(40));
        assert_eq!(first_line_name("   "), None);
        assert_eq!(first_line_name("!!! --- ..."), None);
        assert_eq!(first_line_name("9"), Some("9".to_string()));
        assert_eq!(first_line_name(""), None);
    }

    #[test]
    fn label_uses_the_first_line_only_under_content() {
        assert_eq!(
            label("Untitled-1", "todo list\nmilk", LabelFormat::Content),
            "todo list"
        );
        assert_eq!(
            label("Untitled-1", "todo list\nmilk", LabelFormat::Name),
            "Untitled-1"
        );
        assert_eq!(
            label("Untitled-1", "\ntodo", LabelFormat::Content),
            "Untitled-1"
        );
        assert_eq!(label("Untitled-1", "", LabelFormat::Content), "Untitled-1");
    }

    #[test]
    fn label_format_values() {
        assert_eq!(LabelFormat::from_setting("name"), LabelFormat::Name);
        assert_eq!(LabelFormat::from_setting("content"), LabelFormat::Content);
        assert_eq!(LabelFormat::from_setting("other"), LabelFormat::Content);
    }

    #[test]
    fn language_precedence() {
        assert_eq!(new_language(None, None, None), "plaintext");
        assert_eq!(
            new_language(None, Some(ACTIVE_EDITOR_LANGUAGE), None),
            "plaintext"
        );
        assert_eq!(new_language(Some("rust"), Some("javascript"), None), "rust");
        assert_eq!(
            new_language(None, Some("javascript"), Some("rust")),
            "javascript"
        );
    }
}
