//! File name and first-line language associations with upstream's precedence rules.
//!
//! upstream: src/vs/editor/common/services/languagesAssociations.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/services/languagesAssociations.test.ts @ 50e1350fc78d
//!
//! Upstream keeps the registered associations in module-level arrays; here they are an
//! `Associations` value owned by the registry. Same rules: user configured associations
//! (`files.associations`) beat registered ones, inside a tier the last registered association
//! wins, an exact file name beats a pattern, the longest matching pattern beats an extension, the
//! longest extension wins, and a first line match is the last resort.

use globset::{GlobBuilder, GlobMatcher};
use onig::Regex;

pub const PLAINTEXT_LANGUAGE_ID: &str = "plaintext";
pub const MIME_TEXT: &str = "text/plain";
pub const MIME_UNKNOWN: &str = "application/unknown";

#[derive(Debug, Clone, Default)]
pub struct LanguageAssociation {
    pub id: String,
    pub mime: String,
    pub filename: Option<String>,
    pub extension: Option<String>,
    pub filepattern: Option<String>,
    pub firstline: Option<std::sync::Arc<Regex>>,
}

#[derive(Debug)]
struct Item {
    assoc: LanguageAssociation,
    filename_lower: Option<String>,
    extension_lower: Option<String>,
    pattern: Option<GlobMatcher>,
    pattern_on_path: bool,
}

impl Item {
    fn new(assoc: LanguageAssociation) -> Self {
        let pattern = assoc.filepattern.as_deref().and_then(|p| {
            GlobBuilder::new(p).case_insensitive(true).literal_separator(true).build().ok().map(|g| g.compile_matcher())
        });
        Self {
            filename_lower: assoc.filename.as_ref().map(|s| s.to_lowercase()),
            extension_lower: assoc.extension.as_ref().filter(|s| !s.is_empty()).map(|s| s.to_lowercase()),
            pattern_on_path: assoc.filepattern.as_deref().is_some_and(|p| p.contains('/')),
            pattern,
            assoc,
        }
    }
}

/// The result of a guess: a language id and its mime type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdAndMime {
    pub id: String,
    pub mime: String,
}

impl IdAndMime {
    fn unknown() -> Self {
        Self { id: "unknown".into(), mime: MIME_UNKNOWN.into() }
    }
    fn plaintext() -> Self {
        Self { id: PLAINTEXT_LANGUAGE_ID.into(), mime: MIME_TEXT.into() }
    }
}

#[derive(Debug, Default)]
pub struct Associations {
    /// Registered (platform) associations, in registration order.
    platform: Vec<Item>,
    /// User configured associations (`files.associations`).
    configured: Vec<Item>,
}

impl Associations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Associate a language (platform). Loses over `register_configured`.
    pub fn register_platform(&mut self, association: LanguageAssociation) {
        self.platform.push(Item::new(association));
    }

    /// Associate a language (user configured). Wins over `register_platform`.
    pub fn register_configured(&mut self, association: LanguageAssociation) {
        self.configured.push(Item::new(association));
    }

    pub fn clear_platform(&mut self) {
        self.platform.clear();
    }

    pub fn clear_configured(&mut self) {
        self.configured.clear();
    }

    /// The best matching mime types of a file (`getMimeTypes`).
    pub fn mime_types(&self, path: Option<&str>, first_line: Option<&str>) -> Vec<String> {
        self.guess(path, first_line).into_iter().map(|i| i.mime).collect()
    }

    /// The best matching language ids of a file (`getLanguageIds`).
    pub fn language_ids(&self, path: Option<&str>, first_line: Option<&str>) -> Vec<String> {
        self.guess(path, first_line).into_iter().map(|i| i.id).collect()
    }

    /// `path` is the file system path (or `None` for a resource without one).
    pub fn guess(&self, path: Option<&str>, first_line: Option<&str>) -> Vec<IdAndMime> {
        let Some(path) = path.filter(|p| !p.is_empty()) else {
            return vec![IdAndMime::unknown()];
        };
        let path = path.to_lowercase();
        let filename = basename(&path);

        // 1.) User configured mappings have the highest priority.
        if let Some(a) = association_by_path(&path, filename, &self.configured) {
            return vec![a, IdAndMime::plaintext()];
        }
        // 2.) Registered mappings have middle priority.
        if let Some(a) = association_by_path(&path, filename, &self.platform) {
            return vec![a, IdAndMime::plaintext()];
        }
        // 3.) First line has the lowest priority.
        if let Some(line) = first_line.filter(|l| !l.is_empty())
            && let Some(a) = self.association_by_first_line(line)
        {
            return vec![a, IdAndMime::plaintext()];
        }
        vec![IdAndMime::unknown()]
    }

    fn association_by_first_line(&self, first_line: &str) -> Option<IdAndMime> {
        let line = first_line.strip_prefix('\u{feff}').unwrap_or(first_line);
        if line.is_empty() {
            return None;
        }
        // Registered in order, the last registered association wins (upstream issue 20074). The
        // registered list upstream holds platform and configured items in registration order; the
        // configured ones never carry a first line.
        self.platform
            .iter()
            .rev()
            .filter_map(|i| i.assoc.firstline.as_ref().map(|re| (i, re)))
            .find(|(_, re)| re.find(line).is_some())
            .map(|(i, _)| IdAndMime { id: i.assoc.id.clone(), mime: i.assoc.mime.clone() })
    }
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn association_by_path(path: &str, filename: &str, items: &[Item]) -> Option<IdAndMime> {
    let mut filename_match: Option<&Item> = None;
    let mut pattern_match: Option<&Item> = None;
    let mut extension_match: Option<&Item> = None;

    // The last registered association wins over all others (upstream issue 20074).
    for item in items.iter().rev() {
        if item.filename_lower.as_deref() == Some(filename) {
            filename_match = Some(item);
            break;
        }
        if let (Some(p), Some(matcher)) = (item.assoc.filepattern.as_deref(), item.pattern.as_ref()) {
            let longer = pattern_match.is_none_or(|m| p.len() > m.assoc.filepattern.as_deref().map_or(0, str::len));
            if longer {
                let target = if item.pattern_on_path { path } else { filename };
                if matcher.is_match(target) {
                    pattern_match = Some(item);
                }
            }
        }
        if let Some(ext) = item.extension_lower.as_deref() {
            let longer = extension_match.is_none_or(|m| ext.len() > m.extension_lower.as_deref().map_or(0, str::len));
            if longer && filename.ends_with(ext) {
                extension_match = Some(item);
            }
        }
    }
    filename_match
        .or(pattern_match)
        .or(extension_match)
        .map(|i| IdAndMime { id: i.assoc.id.clone(), mime: i.assoc.mime.clone() })
}
