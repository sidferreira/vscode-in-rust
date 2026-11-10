//! Upstream source: src/vs/platform/extensionManagement/node/extensionsScannerService.ts
//! Upstream source: src/vs/platform/extensionManagement/common/extensionsProfileScannerService.ts
//!
//! Finding extensions on disk, read-only: a folder of extensions (`~/.vscode/extensions`: one
//! folder per `publisher.name-version`, `.obsolete` marking the superseded ones) or one extension
//! folder (a development path). Builds the [`ExtensionDescription`]s the host takes, with
//! `%key%` strings localised from `package.nls.json` and the implicit activation events
//! (`onCommand:` for contributed commands, `onLanguage:` for contributed languages) the scanner
//! adds. Nothing here writes.

use crate::description::ExtensionDescription;
use serde_json::Value;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

/// What a scan found.
#[derive(Debug, Default)]
pub struct Discovered {
    pub extensions: Vec<ExtensionDescription>,
    /// Folders that were skipped, with the reason (an obsolete version, a bad manifest, a newer
    /// engine requirement, another platform).
    pub skipped: Vec<(PathBuf, String)>,
}

/// A command an extension contributes (`contributes.commands`), for the Command Palette.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContributedCommand {
    pub extension: String,
    pub id: String,
    pub title: String,
    pub category: Option<String>,
    /// Not hidden by a `commandPalette` menu entry with `"when": "false"`.
    pub in_palette: bool,
}

/// The target platform of this build in VS Code's naming, for folder suffixes.
pub fn current_platform() -> &'static str {
    if cfg!(target_arch = "aarch64") {
        "darwin-arm64"
    } else {
        "darwin-x64"
    }
}

pub fn is_extension_dir(dir: &Path) -> bool {
    dir.join("package.json").is_file()
}

/// Scan `roots`, in order: a root with a `package.json` is one extension, any other is a folder
/// of extensions. When an id is found twice the later root wins (so a development path overrides
/// the installed copy), and within a root the highest version does.
pub fn scan(roots: &[PathBuf], host_version: &str, platform: &str) -> Discovered {
    let mut by_id: BTreeMap<String, (usize, ExtensionDescription)> = BTreeMap::new();
    let mut out = Discovered::default();
    for (rank, root) in roots.iter().enumerate() {
        let mut candidates: Vec<PathBuf> = Vec::new();
        if is_extension_dir(root) {
            candidates.push(root.clone());
        } else {
            let obsolete = read_obsolete(root);
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            let mut dirs: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for d in dirs {
                let name = d.file_name().map(|n| n.to_string_lossy().into_owned());
                let Some(name) = name else { continue };
                if !d.is_dir() || name.starts_with('.') || !is_extension_dir(&d) {
                    continue;
                }
                if obsolete.contains(&name) {
                    out.skipped.push((d, "obsolete".into()));
                } else if !platform_ok(&name, platform) {
                    out.skipped.push((d, format!("built for another platform than {platform}")));
                } else {
                    candidates.push(d);
                }
            }
        }
        for dir in candidates {
            match load(&dir) {
                Ok(ext) => {
                    if let Some(req) = ext.json.pointer("/engines/vscode").and_then(Value::as_str)
                        && !engine_compatible(req, host_version)
                    {
                        out.skipped
                            .push((dir, format!("needs VS Code {req}, this is {host_version}")));
                        continue;
                    }
                    let key = ext.id.to_lowercase();
                    let better = match by_id.get(&key) {
                        None => true,
                        Some((r, old)) => {
                            rank > *r || (rank == *r && newer(&ext, old) == Ordering::Greater)
                        }
                    };
                    if better {
                        if let Some((_, old)) = by_id.insert(key, (rank, ext)) {
                            out.skipped.push((old.location, "superseded by another version".into()));
                        }
                    } else {
                        out.skipped.push((ext.location, "superseded by another version".into()));
                    }
                }
                Err(e) => out.skipped.push((dir, e)),
            }
        }
    }
    out.extensions = by_id.into_values().map(|(_, e)| e).collect();
    out
}

fn newer(a: &ExtensionDescription, b: &ExtensionDescription) -> Ordering {
    let v = |e: &ExtensionDescription| {
        e.json.get("version").and_then(Value::as_str).unwrap_or("0").to_string()
    };
    compare_versions(&v(a), &v(b))
}

/// `.obsolete`: `{ "publisher.name-1.2.3": true }`, the folders a newer install superseded.
fn read_obsolete(root: &Path) -> HashSet<String> {
    let Ok(text) = std::fs::read_to_string(root.join(".obsolete")) else {
        return HashSet::new();
    };
    let Ok(Value::Object(map)) = serde_json::from_str::<Value>(&text) else {
        return HashSet::new();
    };
    map.into_iter()
        .filter(|(_, v)| v.as_bool().unwrap_or(false))
        .map(|(k, _)| k)
        .collect()
}

/// Folders of platform-specific builds end in `-<platform>` (`-darwin-arm64`, `-linux-x64`,
/// `-universal`): only this machine's (or no suffix) is usable.
pub fn platform_ok(folder: &str, platform: &str) -> bool {
    const KNOWN: [&str; 13] = [
        "win32-x64", "win32-arm64", "win32-ia32", "linux-x64", "linux-arm64", "linux-armhf",
        "alpine-x64", "alpine-arm64", "darwin-x64", "darwin-arm64", "web", "universal", "undefined",
    ];
    match KNOWN.iter().find(|k| folder.ends_with(&format!("-{k}"))) {
        Some(k) => *k == platform || *k == "universal" || *k == "undefined",
        None => true,
    }
}

/// `engines.vscode` against the host version: `^1.80.0`, `>=1.60`, `~1.50.0`, `1.x`, `*`.
pub fn engine_compatible(required: &str, host: &str) -> bool {
    let required = required.trim();
    if required == "*" || required.is_empty() {
        return true;
    }
    let digits = required.trim_start_matches(|c: char| !c.is_ascii_digit());
    let digits: String = digits
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if digits.is_empty() {
        return true;
    }
    // `<`, `<=` and exact ranges are rare for engines.vscode; treat every form as a minimum.
    compare_versions(host, &digits) != Ordering::Less
}

/// Compare dotted numeric versions (`1.2.10` > `1.2.9`; a pre-release suffix is ignored).
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let parts = |s: &str| -> Vec<u64> {
        s.split(['-', '+']).next().unwrap_or("").split('.').map(|p| p.parse().unwrap_or(0)).collect()
    };
    let (a, b) = (parts(a), parts(b));
    for i in 0..a.len().max(b.len()) {
        let o = a.get(i).unwrap_or(&0).cmp(b.get(i).unwrap_or(&0));
        if o != Ordering::Equal {
            return o;
        }
    }
    Ordering::Equal
}

/// Load one extension folder: localised, with the implicit activation events.
pub fn load(dir: &Path) -> Result<ExtensionDescription, String> {
    let mut ext = ExtensionDescription::from_dir(dir)?;
    if let Ok(text) = std::fs::read_to_string(ext.location.join("package.nls.json"))
        && let Ok(nls) = serde_json::from_str::<Value>(&text)
    {
        localize(&mut ext.json, &nls);
    }
    let mut events = ext.activation_events.clone();
    for c in contributed_commands(&ext) {
        events.push(format!("onCommand:{}", c.id));
    }
    for l in ext
        .json
        .pointer("/contributes/languages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(id) = l.get("id").and_then(Value::as_str) {
            events.push(format!("onLanguage:{id}"));
        }
    }
    let mut seen = HashSet::new();
    events.retain(|e| seen.insert(e.clone()));
    ext.activation_events = events;
    Ok(ext)
}

/// Replace every string that is exactly `%key%` with the value of `key` in `nls` (a string, or
/// `{ "message": ... }`), as the scanner's `localizeManifest` does.
pub fn localize(manifest: &mut Value, nls: &Value) {
    match manifest {
        Value::String(s) => {
            if s.len() > 2 && s.starts_with('%') && s.ends_with('%') {
                let key = &s[1..s.len() - 1];
                let found = nls.get(key).and_then(|v| match v {
                    Value::String(t) => Some(t.clone()),
                    Value::Object(o) => o.get("message").and_then(Value::as_str).map(String::from),
                    _ => None,
                });
                if let Some(t) = found {
                    *s = t;
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|v| localize(v, nls)),
        Value::Object(o) => o.values_mut().for_each(|v| localize(v, nls)),
        _ => {}
    }
}

/// The commands the extension contributes. A command whose `commandPalette` menu entry says
/// `"when": "false"` is not listed in the palette (it is still runnable).
pub fn contributed_commands(ext: &ExtensionDescription) -> Vec<ContributedCommand> {
    let hidden: HashSet<&str> = ext
        .json
        .pointer("/contributes/menus/commandPalette")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|m| m.get("when").and_then(Value::as_str).map(str::trim) == Some("false"))
        .filter_map(|m| m.get("command").and_then(Value::as_str))
        .collect();
    let list = match ext.json.pointer("/contributes/commands") {
        Some(Value::Array(a)) => a.iter().collect(),
        Some(v @ Value::Object(_)) => vec![v],
        _ => vec![],
    };
    list.into_iter()
        .filter_map(|c| {
            let id = c.get("command")?.as_str()?;
            let title = c.get("title").and_then(|t| match t {
                Value::String(s) => Some(s.as_str()),
                // `{ value, original }`
                Value::Object(o) => o.get("value").and_then(Value::as_str),
                _ => None,
            })?;
            let category = c.get("category").and_then(|t| match t {
                Value::String(s) => Some(s.clone()),
                Value::Object(o) => o.get("value").and_then(Value::as_str).map(String::from),
                _ => None,
            });
            Some(ContributedCommand {
                extension: ext.id.clone(),
                id: id.to_string(),
                title: title.to_string(),
                category,
                in_palette: !hidden.contains(id),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rscode-disc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir(&d); // only an empty leftover
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn write_ext(root: &Path, folder: &str, manifest: &str) -> PathBuf {
        let d = root.join(folder);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("package.json"), manifest).unwrap();
        d
    }

    fn manifest(name: &str, version: &str, extra: &str) -> String {
        format!(
            r#"{{"name":"{name}","publisher":"pub","version":"{version}","engines":{{"vscode":"^1.80.0"}}{extra}}}"#
        )
    }

    #[test]
    fn versions_and_engines() {
        assert_eq!(compare_versions("1.2.10", "1.2.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("2.0.0-beta", "2.0.0"), Ordering::Equal);
        assert!(engine_compatible("^1.80.0", "1.147.0"));
        assert!(engine_compatible(">=1.60", "1.60.0"));
        assert!(engine_compatible("*", "1.0.0"));
        assert!(!engine_compatible("^1.200.0", "1.147.0"));
    }

    #[test]
    fn platform_suffixes() {
        assert!(platform_ok("pub.name-1.0.0", "darwin-arm64"));
        assert!(platform_ok("pub.name-1.0.0-darwin-arm64", "darwin-arm64"));
        assert!(!platform_ok("pub.name-1.0.0-linux-x64", "darwin-arm64"));
        assert!(!platform_ok("pub.name-1.0.0-darwin-x64", "darwin-arm64"));
        assert!(platform_ok("pub.name-1.0.0-universal", "darwin-arm64"));
    }

    #[test]
    fn scans_a_folder_of_extensions_skipping_obsolete_and_old_versions() {
        let root = scratch("scan");
        write_ext(&root, "pub.a-1.0.0", &manifest("a", "1.0.0", ""));
        write_ext(&root, "pub.a-1.2.0", &manifest("a", "1.2.0", ""));
        write_ext(&root, "pub.b-0.1.0", &manifest("b", "0.1.0", ""));
        write_ext(&root, "pub.c-3.0.0", &manifest("c", "3.0.0", ""));
        write_ext(&root, "pub.d-1.0.0-linux-x64", &manifest("d", "1.0.0", ""));
        write_ext(&root, "pub.new-1.0.0", &manifest("new", "1.0.0", &r#","engines2":1"#.replace("engines2", "x")));
        fs::write(root.join(".obsolete"), r#"{"pub.c-3.0.0": true}"#).unwrap();
        fs::write(root.join("extensions.json"), "[]").unwrap(); // ignored: a file, not a folder
        let found = scan(std::slice::from_ref(&root), "1.147.0", "darwin-arm64");
        let ids: Vec<(&str, &str)> = found
            .extensions
            .iter()
            .map(|e| (e.id.as_str(), e.json["version"].as_str().unwrap()))
            .collect();
        assert_eq!(ids, [("pub.a", "1.2.0"), ("pub.b", "0.1.0"), ("pub.new", "1.0.0")]);
        let reasons: Vec<&str> = found.skipped.iter().map(|(_, r)| r.as_str()).collect();
        assert!(reasons.contains(&"obsolete"));
        assert!(reasons.iter().any(|r| r.contains("another platform")));
        assert!(reasons.iter().any(|r| r.contains("superseded")));
    }

    #[test]
    fn engine_too_new_is_skipped_and_a_later_root_wins() {
        let user = scratch("roots-user");
        let dev = scratch("roots-dev");
        write_ext(&user, "pub.a-1.0.0", &manifest("a", "1.0.0", ""));
        write_ext(&user, "pub.future-1.0.0", &manifest("future", "1.0.0", "").replace("^1.80.0", "^1.999.0"));
        let dev_ext = write_ext(&dev, "checkout", &manifest("a", "0.0.1", ""));
        let found = scan(&[user, dev_ext], "1.147.0", "darwin-arm64");
        assert_eq!(found.extensions.len(), 1);
        assert_eq!(found.extensions[0].json["version"], "0.0.1", "the development path overrides the installed copy");
        assert!(found.skipped.iter().any(|(_, r)| r.contains("needs VS Code")));
    }

    #[test]
    fn localizes_titles_and_adds_implicit_events() {
        let root = scratch("nls");
        let d = write_ext(
            &root,
            "pub.n-1.0.0",
            &manifest(
                "n",
                "1.0.0",
                r#","activationEvents":["onStartupFinished"],"contributes":{"commands":[{"command":"n.run","title":"%run.title%","category":"N"},{"command":"n.hidden","title":"Hidden"}],"languages":[{"id":"nlang"}],"menus":{"commandPalette":[{"command":"n.hidden","when":"false"}]}}"#,
            ),
        );
        fs::write(d.join("package.nls.json"), r#"{"run.title":"Run it"}"#).unwrap();
        let ext = load(&d).unwrap();
        assert_eq!(
            ext.activation_events,
            ["onStartupFinished", "onCommand:n.run", "onCommand:n.hidden", "onLanguage:nlang"]
        );
        let cmds = contributed_commands(&ext);
        assert_eq!(cmds[0].title, "Run it");
        assert_eq!(cmds[0].category.as_deref(), Some("N"));
        assert!(cmds[0].in_palette && !cmds[1].in_palette);
    }
}
