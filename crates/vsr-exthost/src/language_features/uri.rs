//! upstream: src/vs/base/common/uri.ts @ 50e1350fc78d
//!
//! The part of `URI` the language feature registry needs: components, parsing and `fsPath`.

use serde_json::Value;

/// A resource: `scheme://authority/path?query#fragment`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Uri {
    pub scheme: String,
    pub authority: String,
    pub path: String,
    pub query: String,
    pub fragment: String,
}

impl Uri {
    /// `URI.file(path)`.
    pub fn file(path: &str) -> Uri {
        let path = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{path}")
        };
        Uri {
            scheme: "file".into(),
            path,
            ..Uri::default()
        }
    }

    /// `URI.revive(UriComponents)`.
    pub fn from_components(v: &Value) -> Option<Uri> {
        let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        v.get("scheme")?.as_str()?;
        Some(Uri {
            scheme: s("scheme"),
            authority: s("authority"),
            path: s("path"),
            query: s("query"),
            fragment: s("fragment"),
        })
    }

    /// `URI.parse(value)` for the forms the registry meets: `scheme:path`, `scheme://authority/path`
    /// with an optional query and fragment (no percent-decoding).
    pub fn parse(value: &str) -> Uri {
        let (scheme, rest) = match value.split_once(':') {
            Some((s, r))
                if !s.is_empty()
                    && s.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c)) =>
            {
                (s, r)
            }
            _ => ("file", value),
        };
        let (rest, fragment) = rest.split_once('#').unwrap_or((rest, ""));
        let (rest, query) = rest.split_once('?').unwrap_or((rest, ""));
        let (authority, path) = match rest.strip_prefix("//") {
            Some(after) => match after.find('/') {
                Some(i) => (&after[..i], &after[i..]),
                None => (after, ""),
            },
            None => ("", rest),
        };
        let mut path = path.to_string();
        if matches!(scheme, "file" | "http" | "https") && !path.starts_with('/') {
            path.insert(0, '/');
        }
        Uri {
            scheme: scheme.into(),
            authority: authority.into(),
            path,
            query: query.into(),
            fragment: fragment.into(),
        }
    }

    /// `uri.fsPath` (the POSIX form: separators are left alone).
    pub fn fs_path(&self) -> String {
        let b = self.path.as_bytes();
        if !self.authority.is_empty() && self.path.len() > 1 && self.scheme == "file" {
            format!("//{}{}", self.authority, self.path)
        } else if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
            format!("{}{}", (b[1] as char).to_ascii_lowercase(), &self.path[2..])
        } else {
            self.path.clone()
        }
    }

    /// `uri.toString()` without percent-encoding.
    pub fn to_uri_string(&self) -> String {
        let mut s = format!("{}:", self.scheme);
        if !self.authority.is_empty() || self.scheme == "file" {
            s.push_str("//");
            s.push_str(&self.authority);
        }
        s.push_str(&self.path);
        if !self.query.is_empty() {
            s.push('?');
            s.push_str(&self.query);
        }
        if !self.fragment.is_empty() {
            s.push('#');
            s.push_str(&self.fragment);
        }
        s
    }
}
