//! The settings of session restore, hot exit and the Welcome page.
//!
//! upstream: src/vs/workbench/contrib/files/browser/files.contribution.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//! upstream: src/vs/platform/window/common/window.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5

use serde::Deserialize;
use vsr_settings::Configuration;

/// `files.hotExit`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotExit {
    Off,
    OnExit,
    OnExitAndWindowClose,
}

/// `window.restoreWindows`. rscode has one window, so the values differ only by `none` (start
/// without restoring) against the rest (restore the window that was open).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RestoreWindows {
    Preserve,
    All,
    Folders,
    One,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionSettings {
    pub hot_exit: HotExit,
    pub restore_windows: RestoreWindows,
    /// `workbench.editor.restoreEditors`.
    pub restore_editors: bool,
    /// `workbench.editor.restoreViewState`.
    pub restore_view_state: bool,
}

impl SessionSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            hot_exit: c.get_enum("files.hotExit").unwrap_or(HotExit::OnExit),
            restore_windows: c.get_enum("window.restoreWindows").unwrap_or(RestoreWindows::All),
            restore_editors: c.get_bool("workbench.editor.restoreEditors").unwrap_or(true),
            restore_view_state: c.get_bool("workbench.editor.restoreViewState").unwrap_or(true),
        }
    }

    /// Whether the open editors of the last run come back.
    pub fn restores_editors(&self) -> bool {
        self.restore_editors && self.restore_windows != RestoreWindows::None
    }

    /// Whether quitting with unsaved changes asks first instead of keeping them. `window_close` is
    /// the red button; Cmd+Q and `workbench.action.quit` are an exit. Upstream: `off` always asks,
    /// `onExit` keeps on quit and asks when a window closes, `onExitAndWindowClose` never asks.
    pub fn asks_before_closing(&self, window_close: bool) -> bool {
        match self.hot_exit {
            HotExit::Off => true,
            HotExit::OnExit => window_close,
            HotExit::OnExitAndWindowClose => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use vsr_settings::{SettingsRegistry, UserSettings};

    use super::*;

    fn config(user: &str) -> Configuration {
        Configuration::new(&SettingsRegistry::load_default(), UserSettings::parse(user).unwrap())
    }

    #[test]
    fn defaults_match_upstream() {
        let s = SessionSettings::from_configuration(&config(""));
        assert_eq!(s.hot_exit, HotExit::OnExit);
        assert_eq!(s.restore_windows, RestoreWindows::All);
        assert!(s.restore_editors && s.restore_view_state && s.restores_editors());
    }

    #[test]
    fn values_are_read_and_bad_values_fall_back() {
        let s = SessionSettings::from_configuration(&config(
            r#"{ "files.hotExit": "off", "window.restoreWindows": "none", "workbench.editor.restoreViewState": false }"#,
        ));
        assert_eq!((s.hot_exit, s.restore_windows, s.restore_view_state), (HotExit::Off, RestoreWindows::None, false));
        assert!(!s.restores_editors());
        let s = SessionSettings::from_configuration(&config(r#"{ "files.hotExit": "bogus", "window.restoreWindows": "folders" }"#));
        assert_eq!((s.hot_exit, s.restore_windows), (HotExit::OnExit, RestoreWindows::Folders));
        assert!(s.restores_editors());
        assert!(!SessionSettings::from_configuration(&config(r#"{ "workbench.editor.restoreEditors": false }"#)).restores_editors());
    }

    #[test]
    fn the_quit_prompt_follows_hot_exit() {
        let ask = |mode: &str, window_close| {
            SessionSettings::from_configuration(&config(&format!(r#"{{ "files.hotExit": "{mode}" }}"#))).asks_before_closing(window_close)
        };
        assert!(ask("off", false) && ask("off", true));
        assert!(!ask("onExit", false) && ask("onExit", true));
        assert!(!ask("onExitAndWindowClose", false) && !ask("onExitAndWindowClose", true));
    }
}
