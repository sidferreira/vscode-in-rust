//! Setting key to display label/category, ported from upstream's `settingKeyToDisplayFormat`.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTreeModels.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/common/preferences.ts @ 50e1350fc78d

const KNOWN_ACRONYMS: &[&str] = &[
    "css", "html", "scss", "less", "json", "js", "ts", "ie", "id", "php", "scm",
];

const KNOWN_TERM_MAPPINGS: &[(&str, &str)] = &[
    ("power shell", "PowerShell"),
    ("powershell", "PowerShell"),
    ("javascript", "JavaScript"),
    ("typescript", "TypeScript"),
    ("github", "GitHub"),
    ("ocaml", "OCaml"),
    ("jet brains", "JetBrains"),
    ("jetbrains", "JetBrains"),
    ("re sharper", "ReSharper"),
    ("resharper", "ReSharper"),
];

/// The category (the key prefix, trimmed against the group) and the label (the last key segment).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayFormat {
    pub category: String,
    pub label: String,
}

/// Port of `settingKeyToDisplayFormat(key, groupId)`. Sandbox-specific label overrides are not ported.
pub fn setting_key_to_display_format(key: &str, group_id: &str) -> DisplayFormat {
    let (category, last) = match key.rfind('.') {
        Some(i) => (&key[..i], &key[i + 1..]),
        None => ("", key),
    };
    let group_id = group_id.replace('/', ".");
    let category = wordify_key(&trim_category_for_group(category, &group_id));
    DisplayFormat {
        category,
        label: wordify_key(last),
    }
}

/// The title shown for a setting: `Category: Label`, or just the label without a category.
pub fn setting_title(key: &str) -> String {
    let format = setting_key_to_display_format(key, "");
    if format.category.is_empty() {
        format.label
    } else {
        format!("{}: {}", format.category, format.label)
    }
}

/// Port of `trimCategoryForGroup`: removes the part of the category already shown by the group.
fn trim_category_for_group(category: &str, group_id: &str) -> String {
    let do_trim = |forward: bool| -> Option<String> {
        let mut group = group_id.to_string();
        // Remove the Insiders portion if the category doesn't use it.
        if !category.to_lowercase().ends_with("insiders") {
            group = strip_insiders(&group);
        }
        let mut parts: Vec<String> = group
            .split('.')
            .map(|part| {
                // Remove hyphens, but only if that results in a match with the category.
                let no_hyphens = part.replace('-', "");
                if no_hyphens.to_lowercase() == category.to_lowercase() {
                    no_hyphens
                } else {
                    part.to_string()
                }
            })
            .collect();
        while !parts.is_empty() {
            let prefix = parts.join(".");
            if let Some(rest) = strip_prefix_ci(category, &prefix) {
                if rest.is_empty() {
                    return Some(String::new());
                }
                if let Some(rest) = rest.strip_prefix('.') {
                    return Some(rest.to_string());
                }
            }
            if forward {
                parts.pop();
            } else {
                parts.remove(0);
            }
        }
        None
    };

    do_trim(true)
        .or_else(|| do_trim(false))
        .unwrap_or_else(|| category.to_string())
}

fn strip_insiders(group: &str) -> String {
    let lower = group.to_lowercase();
    if let Some(stripped) = lower.strip_suffix("insiders") {
        let cut = if stripped.ends_with('-') { stripped.len() - 1 } else { stripped.len() };
        group[..cut].to_string()
    } else {
        group.to_string()
    }
}

fn strip_prefix_ci<'a>(text: &'a str, prefix: &str) -> Option<&'a str> {
    if text.len() >= prefix.len() && text.is_char_boundary(prefix.len()) && text[..prefix.len()].eq_ignore_ascii_case(prefix) {
        Some(&text[prefix.len()..])
    } else {
        None
    }
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Port of `wordifyKey`: `fooBar.etcSomething` becomes `Foo Bar › Etc Something`.
pub fn wordify_key(key: &str) -> String {
    // .replace(/\.([a-z0-9])/g, ' › X')
    let chars: Vec<char> = key.chars().collect();
    let mut s = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '.' && i + 1 < chars.len() && (chars[i + 1].is_ascii_lowercase() || chars[i + 1].is_ascii_digit()) {
            s.push_str(" \u{203A} ");
            s.push(chars[i + 1].to_ascii_uppercase());
            i += 2;
        } else {
            s.push(chars[i]);
            i += 1;
        }
    }

    // .replace(/([a-z0-9])([A-Z])/g, '$1 $2'): camel case to spacing, fooBar => foo Bar
    let chars: Vec<char> = s.chars().collect();
    let mut s = String::new();
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len()
            && (chars[i].is_ascii_lowercase() || chars[i].is_ascii_digit())
            && chars[i + 1].is_ascii_uppercase()
        {
            s.push(chars[i]);
            s.push(' ');
            s.push(chars[i + 1]);
            i += 2;
        } else {
            s.push(chars[i]);
            i += 1;
        }
    }

    // .replace(/([A-Z]{1,})([A-Z][a-z])/g, '$1 $2'): AISearch => AI Search
    let chars: Vec<char> = s.chars().collect();
    let mut s = String::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_ascii_uppercase() {
            let mut run_end = i;
            while run_end < chars.len() && chars[run_end].is_ascii_uppercase() {
                run_end += 1;
            }
            if run_end - i >= 2 && run_end < chars.len() && chars[run_end].is_ascii_lowercase() {
                s.extend(&chars[i..run_end - 1]);
                s.push(' ');
                s.push(chars[run_end - 1]);
                s.push(chars[run_end]);
                i = run_end + 1;
                continue;
            }
        }
        s.push(chars[i]);
        i += 1;
    }

    // .replace(/^[a-z]/g, upper): upper-case the first letter
    let mut s: String = match s.chars().next() {
        Some(first) if first.is_ascii_lowercase() => {
            let mut out = first.to_ascii_uppercase().to_string();
            out.push_str(&s[first.len_utf8()..]);
            out
        }
        _ => s,
    };

    // .replace(/\b\w+\b/g, known acronyms are upper-cased)
    let mut out = String::with_capacity(s.len());
    let mut word = String::new();
    for c in s.chars() {
        if is_word_char(c) {
            word.push(c);
        } else {
            flush_word(&mut word, &mut out);
            out.push(c);
        }
    }
    flush_word(&mut word, &mut out);
    s = out;

    // Known term mappings, whole words, case-insensitive.
    for (term, replacement) in KNOWN_TERM_MAPPINGS {
        s = replace_whole_word_ci(&s, term, replacement);
    }
    s
}

fn flush_word(word: &mut String, out: &mut String) {
    if !word.is_empty() {
        if KNOWN_ACRONYMS.contains(&word.to_lowercase().as_str()) {
            out.push_str(&word.to_uppercase());
        } else {
            out.push_str(word);
        }
        word.clear();
    }
}

/// `text.replace(new RegExp('\\b' + term + '\\b', 'gi'), replacement)` for an ASCII term.
fn replace_whole_word_ci(text: &str, term: &str, replacement: &str) -> String {
    let lower = text.to_ascii_lowercase();
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    while let Some(found) = lower[pos..].find(term) {
        let start = pos + found;
        let end = start + term.len();
        let before_ok = text[..start].chars().next_back().is_none_or(|c| !is_word_char(c));
        let after_ok = text[end..].chars().next().is_none_or(|c| !is_word_char(c));
        out.push_str(&text[pos..start]);
        if before_ok && after_ok {
            out.push_str(replacement);
        } else {
            out.push_str(&text[start..end]);
        }
        pos = end;
    }
    out.push_str(&text[pos..]);
    out
}
