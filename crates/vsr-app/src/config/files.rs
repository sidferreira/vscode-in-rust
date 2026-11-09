//! The `files.*` settings that decide when a file is saved and whether it can be edited (ADR-0034).
//! Pure: no gpui. The timers and the save triggers live in `autosave.rs`.
//!
//! upstream: src/vs/workbench/services/filesConfiguration/common/filesConfigurationService.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/parts/editor/editorAutoSave.ts @ 50e1350fc78d

use std::path::{Path, PathBuf};

use vsr_settings::Configuration;

use super::glob::ExcludeMatcher;

/// `files.autoSave`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoSaveMode {
    #[default]
    Off,
    /// `afterDelay`: a dirty file is saved `files.autoSaveDelay` ms after its last edit.
    AfterDelay,
    /// `onFocusChange`: the previously active editor is saved when another editor becomes active,
    /// and every dirty editor when the window loses focus.
    OnFocusChange,
    /// `onWindowChange`: every dirty editor is saved when the window loses focus.
    OnWindowChange,
}

impl AutoSaveMode {
    pub fn parse(value: &str) -> Self {
        match value {
            "afterDelay" => Self::AfterDelay,
            "onFocusChange" => Self::OnFocusChange,
            "onWindowChange" => Self::OnWindowChange,
            _ => Self::Off,
        }
    }
}

/// `files.readonlyInclude`, `files.readonlyExclude` and `files.readonlyFromPermissions`.
#[derive(Debug, Clone, Default)]
pub struct ReadonlyRules {
    /// The opened folder: the globs match paths relative to it.
    root: PathBuf,
    include: ExcludeMatcher,
    exclude: ExcludeMatcher,
    from_permissions: bool,
}

impl ReadonlyRules {
    pub fn from_configuration(c: &Configuration, root: &Path) -> Self {
        let matcher = |key: &str| {
            ExcludeMatcher::from_value(c.get_value(key).unwrap_or(&serde_json::Value::Null))
        };
        Self {
            root: root.to_path_buf(),
            include: matcher("files.readonlyInclude"),
            exclude: matcher("files.readonlyExclude"),
            from_permissions: c.get_bool("files.readonlyFromPermissions").unwrap_or(false),
        }
    }

    /// Whether `path` opens read-only from the settings alone (upstream `isReadonly`, without the
    /// session override). A file without owner write permission is read-only when
    /// `readonlyFromPermissions` is on (upstream's "file is write locked" check; the mode is read).
    pub fn is_read_only(&self, path: &Path) -> bool {
        let rel = path
            .strip_prefix(&self.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let no_siblings = |_: &str| false;
        if self.include.is_excluded(&rel, &no_siblings) {
            return !self.exclude.is_excluded(&rel, &no_siblings);
        }
        self.from_permissions && owner_cannot_write(path)
    }
}

/// True when the file exists and its owner write bit is off.
fn owner_cannot_write(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o200 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn rules(include: serde_json::Value, exclude: serde_json::Value, perms: bool) -> ReadonlyRules {
        ReadonlyRules {
            root: PathBuf::from("/ws"),
            include: ExcludeMatcher::from_value(&include),
            exclude: ExcludeMatcher::from_value(&exclude),
            from_permissions: perms,
        }
    }

    #[test]
    fn auto_save_mode_parses_the_four_values_and_falls_back_to_off() {
        assert_eq!(AutoSaveMode::parse("afterDelay"), AutoSaveMode::AfterDelay);
        assert_eq!(
            AutoSaveMode::parse("onFocusChange"),
            AutoSaveMode::OnFocusChange
        );
        assert_eq!(
            AutoSaveMode::parse("onWindowChange"),
            AutoSaveMode::OnWindowChange
        );
        assert_eq!(AutoSaveMode::parse("off"), AutoSaveMode::Off);
        assert_eq!(AutoSaveMode::parse("nonsense"), AutoSaveMode::Off);
    }

    #[test]
    fn include_makes_matching_files_read_only_relative_to_the_root() {
        let r = rules(json!({"**/*.log": true}), json!({}), false);
        assert!(r.is_read_only(Path::new("/ws/logs/a.log")));
        assert!(!r.is_read_only(Path::new("/ws/a.txt")));
    }

    #[test]
    fn exclude_wins_over_include() {
        let r = rules(json!({"**/*.log": true}), json!({"keep.log": true}), false);
        assert!(r.is_read_only(Path::new("/ws/a.log")));
        assert!(!r.is_read_only(Path::new("/ws/keep.log")));
    }

    #[test]
    fn false_values_do_not_match() {
        let r = rules(json!({"**/*.log": false}), json!({}), false);
        assert!(!r.is_read_only(Path::new("/ws/a.log")));
    }

    #[test]
    fn permissions_only_count_when_the_setting_is_on() {
        let dir = std::env::temp_dir().join(format!("rscode-readonly-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("locked.txt");
        std::fs::write(&file, "x").unwrap();
        std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o444))
            .unwrap();
        let off = ReadonlyRules {
            root: dir.clone(),
            ..rules(json!({}), json!({}), false)
        };
        let on = ReadonlyRules {
            root: dir.clone(),
            ..rules(json!({}), json!({}), true)
        };
        assert!(!off.is_read_only(&file));
        assert!(on.is_read_only(&file));
        // Restore the owner write bit so the temp file can be removed by the OS later.
        std::fs::set_permissions(&file, std::os::unix::fs::PermissionsExt::from_mode(0o644))
            .unwrap();
        std::fs::remove_file(&file).unwrap();
    }
}
