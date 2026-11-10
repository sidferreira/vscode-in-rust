//! File and folder names in the explorer: validation of a typed name (upstream `validateFileName`)
//! and the name suggested for a copy (upstream `incrementFileName`, `explorer.incrementalNaming`).
//!
//! upstream: src/vs/workbench/contrib/files/browser/fileActions.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/test/browser/fileActions.test.ts @ 50e1350fc78d

use std::sync::OnceLock;

use regex::Regex;

/// `MAX_SAFE_SMALL_INTEGER` in upstream `base/common/constants.ts`: numbers at or above it are not
/// incremented; a `copy` suffix is appended instead.
const MAX_SAFE_SMALL_INTEGER: u64 = 1 << 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NameMessage {
    pub severity: Severity,
    pub text: String,
}

/// The `explorer.incrementalNaming` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncrementalNaming {
    Simple,
    Smart,
}

impl IncrementalNaming {
    pub fn parse(value: &str) -> Self {
        match value {
            "smart" => Self::Smart,
            _ => Self::Simple,
        }
    }
}

/// Upstream `validateFileName`: why `name` cannot be used for the item being renamed or created in
/// a folder, or `None` if it can. `sibling_exists(name)` says whether the folder already has an
/// entry called `name` (other than the item itself, which the caller excludes).
///
/// Unlike upstream, the messages are plain text: the explorer shows them without Markdown.
pub fn validate_file_name(name: &str, sibling_exists: impl Fn(&str) -> bool) -> Option<NameMessage> {
    let name = well_formed(name);
    if name.is_empty() || name.trim().is_empty() {
        return Some(error("A file or folder name must be provided."));
    }
    if name.starts_with('/') || name.starts_with('\\') {
        return Some(error("A file or folder name cannot start with a slash."));
    }
    let names: Vec<&str> = name.split(['\\', '/']).filter(|s| !s.is_empty()).collect();
    if sibling_exists(&name) {
        return Some(error(&format!("A file or folder {name} already exists at this location. Please choose a different name.")));
    }
    if names.iter().any(|n| !is_valid_basename(n)) {
        let shown: String = name.chars().take(255).collect();
        let shown = if name.chars().count() > 255 { format!("{shown}...") } else { shown };
        return Some(error(&format!("The name {shown} is not valid as a file or folder name. Please choose a different name.")));
    }
    if names.iter().any(|n| n.starts_with(char::is_whitespace) || n.ends_with(char::is_whitespace)) {
        return Some(NameMessage {
            severity: Severity::Warning,
            text: "Leading or trailing whitespace detected in file or folder name.".into(),
        });
    }
    None
}

fn error(text: &str) -> NameMessage {
    NameMessage { severity: Severity::Error, text: text.to_string() }
}

/// Upstream `getWellFormedFileName`: trailing separators are dropped.
fn well_formed(name: &str) -> String {
    name.trim_end_matches(['/', '\\']).to_string()
}

/// `isValidBasename` for macOS (`UNIX_INVALID_FILE_CHARS` is `/` only): not blank, not `.` or `..`,
/// no `/`.
pub fn is_valid_basename(name: &str) -> bool {
    !name.is_empty() && !name.trim().is_empty() && !name.contains('/') && name != "." && name != ".."
}

/// Upstream `incrementFileName`: the name for a copy of `name`.
pub fn increment_file_name(name: &str, is_folder: bool, naming: IncrementalNaming) -> String {
    match naming {
        IncrementalNaming::Simple => simple(name, is_folder),
        IncrementalNaming::Smart => smart(name, is_folder),
    }
}

/// Node's `path.extname`: the text from the last dot, unless that dot is the first character or
/// there is none.
fn ext_of(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[i..],
        _ => "",
    }
}

fn simple(name: &str, is_folder: bool) -> String {
    let ext = if is_folder { "" } else { ext_of(name) };
    let prefix = &name[..name.len() - ext.len()];
    if let Some((g1, g2)) = match_copy_suffix(prefix) {
        let number: u64 = g2.map(|d| d.trim_start_matches(' ').parse::<u64>().unwrap_or(u64::MAX)).unwrap_or(1);
        let renamed = if number == 0 {
            g1.to_string()
        } else if number < MAX_SAFE_SMALL_INTEGER {
            format!("{g1} {}", number + 1)
        } else {
            format!("{g1}{} copy", g2.unwrap_or(""))
        };
        return renamed + ext;
    }
    format!("{prefix} copy{ext}")
}

/// `^(.+ copy)( \d+)?$` by hand: the stem (`g1`, which ends in ` copy`) and the optional number
/// (`g2`, with its leading space).
fn match_copy_suffix(prefix: &str) -> Option<(&str, Option<&str>)> {
    if let Some(stem) = prefix.strip_suffix(" copy")
        && !stem.is_empty()
    {
        return Some((prefix, None));
    }
    let idx = prefix.rfind(" copy ")?;
    let (g1, rest) = prefix.split_at(idx + " copy".len());
    let digits = rest.strip_prefix(' ')?;
    let stem = &g1[..g1.len() - " copy".len()];
    (!stem.is_empty() && !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())).then_some((g1, Some(rest)))
}

fn smart(name: &str, is_folder: bool) -> String {
    static FILE_SUFFIX: OnceLock<Regex> = OnceLock::new();
    static FILE_PREFIX: OnceLock<Regex> = OnceLock::new();
    static NO_NAME: OnceLock<Regex> = OnceLock::new();
    static NO_EXT: OnceLock<Regex> = OnceLock::new();
    static TRAILING: OnceLock<Regex> = OnceLock::new();
    let re = |cell: &'static OnceLock<Regex>, pat: &str| cell.get_or_init(|| Regex::new(pat).expect("valid pattern")).clone();
    let max = MAX_SAFE_SMALL_INTEGER;

    // `file.1.txt` => `file.2.txt`
    if !is_folder {
        let suffix_file = re(&FILE_SUFFIX, r"^(.*[\.\-_])(\d+)(\..*)$");
        if let Some(c) = suffix_file.captures(name) {
            let digits = &c[2];
            let n: u64 = digits.parse().unwrap_or(0);
            return if n < max {
                format!("{}{:0width$}{}", &c[1], n + 1, &c[3], width = digits.len())
            } else {
                format!("{}{}.1{}", &c[1], digits, &c[3])
            };
        }
        // `1.file.txt` => `2.file.txt`
        let prefix_file = re(&FILE_PREFIX, r"^(\d+)([\.\-_].*)(\..*)$");
        if let Some(c) = prefix_file.captures(name) {
            let digits = &c[1];
            let n: u64 = digits.parse().unwrap_or(0);
            return if n < max {
                format!("{:0width$}{}{}", n + 1, &c[2], &c[3], width = digits.len())
            } else {
                format!("{}{}.1{}", digits, &c[2], &c[3])
            };
        }
        // `1.txt` => `2.txt`
        let no_name = re(&NO_NAME, r"^(\d+)(\..*)$");
        if let Some(c) = no_name.captures(name) {
            let digits = &c[1];
            let n: u64 = digits.parse().unwrap_or(0);
            return if n < max {
                format!("{:0width$}{}", n + 1, &c[2], width = digits.len())
            } else {
                format!("{}.1{}", digits, &c[2])
            };
        }
        // `file.txt` => `file.1.txt`, unless the dot is the first character.
        if let Some(dot) = name.rfind('.') {
            return format!("{}.1{}", &name[..dot], &name[dot..]);
        }
        // `123` => `124`
        let trailing = re(&TRAILING, r"(\d+)$");
        if let Some(c) = trailing.captures(name) {
            let m = c.get(1).expect("group 1");
            let digits = m.as_str();
            let n: u64 = digits.parse().unwrap_or(0);
            return if n < max {
                format!("{}{:0width$}", &name[..m.start()], n + 1, width = digits.len())
            } else {
                format!("{name}.1")
            };
        }
        // `file` => `file1`, `file1` => `file2`
        let no_ext = re(&NO_EXT, r"^(.*?)(\d*)$");
        let c = no_ext.captures(name).expect("always matches");
        let digits = &c[2];
        let n: u64 = if digits.is_empty() { 0 } else { digits.parse().unwrap_or(0) };
        return if n < max {
            format!("{}{:0width$}", &c[1], n + 1, width = digits.len())
        } else {
            format!("{}{}.1", &c[1], digits)
        };
    }
    // `folder.1` => `folder.2`, `folder.001` => `folder.002`
    let trailing = re(&TRAILING, r"(\d+)$");
    if let Some(c) = trailing.captures(name) {
        let m = c.get(1).expect("group 1");
        let digits = m.as_str();
        let n: u64 = digits.parse().unwrap_or(0);
        return if n < max {
            format!("{}{:0width$}", &name[..m.start()], n + 1, width = digits.len())
        } else {
            format!("{name}.1")
        };
    }
    format!("{name}.1")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> bool {
        false
    }

    fn simple_of(name: &str) -> String {
        increment_file_name(name, false, IncrementalNaming::Simple)
    }

    fn folder_simple(name: &str) -> String {
        increment_file_name(name, true, IncrementalNaming::Simple)
    }

    fn smart_of(name: &str) -> String {
        increment_file_name(name, false, IncrementalNaming::Smart)
    }

    fn smart_folder(name: &str) -> String {
        increment_file_name(name, true, IncrementalNaming::Smart)
    }

    // upstream: Files - Increment file name simple > Increment file name without any version
    #[test]
    fn increment_file_name_without_any_version() {
        assert_eq!(simple_of("test.js"), "test copy.js");
    }

    // upstream: Files - Increment file name simple > Increment file name with suffix version
    #[test]
    fn increment_file_name_with_suffix_version() {
        assert_eq!(simple_of("test copy.js"), "test copy 2.js");
    }

    // upstream: Files - Increment file name simple > Increment file name with suffix version with leading zeros
    #[test]
    fn increment_file_name_with_suffix_version_with_leading_zeros() {
        assert_eq!(simple_of("test copy 005.js"), "test copy 6.js");
    }

    // upstream: Files - Increment file name simple > Increment file name with suffix version, too big number
    #[test]
    fn increment_file_name_with_suffix_version_too_big_number() {
        assert_eq!(simple_of("test copy 9007199254740992.js"), "test copy 9007199254740992 copy.js");
    }

    // upstream: Files - Increment file name simple > Increment file name with just version in name
    #[test]
    fn increment_file_name_with_just_version_in_name() {
        assert_eq!(simple_of("copy.js"), "copy copy.js");
        assert_eq!(simple_of("copy 2.js"), "copy 2 copy.js");
    }

    // upstream: Files - Increment file name simple > Increment file name without any extension or version
    #[test]
    fn increment_file_name_without_any_extension_or_version() {
        assert_eq!(simple_of("test"), "test copy");
        assert_eq!(simple_of("test."), "test copy.");
        assert_eq!(simple_of(".test"), ".test copy");
        assert_eq!(simple_of("..test"), ". copy.test");
    }

    // upstream: Files - Increment file name simple > Increment file name without any extension but with suffix version
    #[test]
    fn increment_file_name_without_any_extension_but_with_suffix_version() {
        assert_eq!(simple_of("test copy 5"), "test copy 6");
    }

    // upstream: Files - Increment file name simple > Increment folder name without any version
    #[test]
    fn increment_folder_name_without_any_version() {
        assert_eq!(folder_simple("test"), "test copy");
    }

    // upstream: Files - Increment file name simple > Increment folder name with suffix version
    #[test]
    fn increment_folder_name_with_suffix_version() {
        assert_eq!(folder_simple("test copy 1"), "test copy 2");
    }

    // upstream: Files - Increment file name simple > Increment file/folder name with suffix version, special case 1
    #[test]
    fn copy_zero_becomes_plain_copy() {
        assert_eq!(folder_simple("test copy 0"), "test copy");
    }

    // upstream: Files - Increment file name smart > Increment file name without any version
    #[test]
    fn smart_file_without_version_gets_dot_one() {
        assert_eq!(smart_of("test.js"), "test.1.js");
        assert_eq!(smart_of("test"), "test1");
    }

    // upstream: Files - Increment file name smart > Increment file name with suffix version
    #[test]
    fn smart_file_with_suffix_version() {
        assert_eq!(smart_of("test.1.js"), "test.2.js");
        assert_eq!(smart_of("test.001.js"), "test.002.js");
    }

    // upstream: Files - Increment file name smart > Increment folder name with suffix version
    #[test]
    fn smart_folder_with_suffix_version() {
        assert_eq!(smart_folder("test.1"), "test.2");
        assert_eq!(smart_folder("test.001"), "test.002");
        assert_eq!(smart_folder("test-1"), "test-2");
        assert_eq!(smart_folder("test_1"), "test_2");
        assert_eq!(smart_folder("test"), "test.1");
    }

    #[test]
    fn validation_messages_match_upstream_wording() {
        assert_eq!(validate_file_name("", none).unwrap().text, "A file or folder name must be provided.");
        assert_eq!(validate_file_name("   ", none).unwrap().text, "A file or folder name must be provided.");
        assert_eq!(validate_file_name("/abs", none).unwrap().text, "A file or folder name cannot start with a slash.");
        assert_eq!(
            validate_file_name("a.txt", |n| n == "a.txt").unwrap().text,
            "A file or folder a.txt already exists at this location. Please choose a different name."
        );
        assert_eq!(
            validate_file_name("..", none).unwrap().text,
            "The name .. is not valid as a file or folder name. Please choose a different name."
        );
        let ws = validate_file_name(" a.txt", none).unwrap();
        assert_eq!(ws.severity, Severity::Warning);
        assert_eq!(ws.text, "Leading or trailing whitespace detected in file or folder name.");
        assert_eq!(validate_file_name("ok.txt", none), None);
    }

    #[test]
    fn a_name_with_a_separator_is_checked_part_by_part() {
        // `a/..` splits into `a` and `..`; the `..` part is invalid.
        assert!(validate_file_name("a/..", none).is_some());
        assert_eq!(validate_file_name("dir/file.txt", none), None);
    }
}
