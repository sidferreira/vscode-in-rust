//! `Preferences: Color Theme` (`workbench.action.selectTheme`, Cmd+K Cmd+T): a Quick Pick of the
//! themes, grouped light / dark / high contrast, that previews the theme under the cursor while
//! you navigate, saves the chosen one to the theme setting on Enter and goes back to the
//! configured theme on Escape.
//!
//! upstream: src/vs/workbench/contrib/themes/browser/themes.contribution.ts @ 50e1350fc78d
//!
//! Differences from VS Code: the preview applies at once (upstream waits 200 ms), no marketplace
//! entries or inline buttons, and the pinned themes at the top of each group are rscode's
//! defaults (Dark Modern, Light Modern).

use gpui_kit::*;
use serde_json::Value;
use vsr_quickinput::pick::{PickItem, PickRow, filter_items};
use vsr_theme::{ColorScheme, DEFAULT_DARK, DEFAULT_LIGHT, ThemeContribution};

use crate::Workbench;
use crate::config::AppConfig;
use crate::quick_input::{PickResult, QuickPick, QuickPickEvent};
use crate::theme_service::ThemeService;

actions!(rscode, [SelectTheme]);

/// The Quick Pick's rows: the entries of each group under a separator, in the order the system
/// colour mode asks for (`preferredColorScheme`).
fn groups(themes: &[ThemeContribution], preferred: Option<ColorScheme>) -> Vec<(&'static str, Vec<(usize, ThemeContribution)>)> {
    let indexed: Vec<(usize, ThemeContribution)> = themes.iter().copied().enumerate().collect();
    let pick = |f: fn(ColorScheme) -> bool| -> Vec<(usize, ThemeContribution)> {
        let mut v: Vec<_> = indexed.iter().filter(|(_, t)| f(t.scheme())).copied().collect();
        // Pinned defaults first, then by label (the list arrives sorted by label).
        v.sort_by_key(|(_, t)| !matches!(t.settings_id, DEFAULT_DARK | DEFAULT_LIGHT));
        v
    };
    let light = ("light themes", pick(|s| s == ColorScheme::Light));
    let dark = ("dark themes", pick(|s| s == ColorScheme::Dark));
    let hc = ("high contrast themes", pick(ColorScheme::is_high_contrast));
    let mut out = match preferred {
        Some(ColorScheme::Dark) => vec![dark, light, hc],
        Some(ColorScheme::HighContrastDark | ColorScheme::HighContrastLight) => vec![hc, light, dark],
        _ => vec![light, dark, hc],
    };
    out.retain(|(_, v)| !v.is_empty());
    out
}

fn item_of(id: usize, t: &ThemeContribution) -> PickItem {
    let description = match t.settings_id {
        DEFAULT_DARK => Some("Default Dark".to_string()),
        DEFAULT_LIGHT => Some("Default Light".to_string()),
        id if id != t.label => Some(id.to_string()),
        _ => None,
    };
    let mut item = PickItem::new(id, t.label);
    item.description = description;
    item.value = Some(t.settings_id.to_string());
    item
}

impl Workbench {
    /// `workbench.action.selectTheme`.
    pub(crate) fn show_theme_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.quick.pick.is_some() {
            self.hide_quick_open(false, window, cx);
        }
        ThemeService::ensure_installed(cx);
        let themes = ThemeService::themes(cx);
        let preferred = ThemeService::preferred_scheme(cx);
        let current = ThemeService::active_settings_id(cx).unwrap_or_default();
        let grouped = groups(&themes, preferred);
        let all_items: Vec<PickItem> = grouped.iter().flat_map(|(_, v)| v.iter().map(|(i, t)| item_of(*i, t))).collect();
        let rows_grouped: Vec<PickRow> = grouped
            .iter()
            .flat_map(|(label, v)| {
                std::iter::once(PickRow::Separator((*label).to_string())).chain(v.iter().map(|(i, t)| PickRow::Item(item_of(*i, t))))
            })
            .collect();
        let source = move |query: &str, _cx: &mut App| -> PickResult {
            if query.trim().is_empty() { PickResult::Ready(rows_grouped.clone()) } else { PickResult::Ready(filter_items(&all_items, query)) }
        };
        let placeholder = match preferred {
            Some(ColorScheme::Dark) => "Select Color Theme for System Dark Mode",
            Some(ColorScheme::Light) => "Select Color Theme for System Light Mode",
            Some(ColorScheme::HighContrastDark) => "Select Color Theme for High Contrast Dark Mode",
            Some(ColorScheme::HighContrastLight) => "Select Color Theme for High Contrast Light Mode",
            None => "Select Color Theme (detect system color mode disabled)",
        };
        let close_on_focus_lost = crate::quick_open::QuickOpenSettings::from_configuration(AppConfig::get(cx).configuration()).close_on_focus_lost;
        self.quick.return_focus = window.focused(cx);
        self.quick.provider = String::from("theme");
        self.quick.theme_pick = true;
        let pick = cx.new(|cx| QuickPick::new(placeholder, "", None, close_on_focus_lost, source, window, cx));
        let current_id = themes.iter().position(|t| t.settings_id == current);
        if let Some(i) = current_id {
            pick.update(cx, |p, _| p.set_active_item(i));
        }
        self.quick._sub = Some(cx.subscribe_in(&pick, window, |this, _, ev: &QuickPickEvent, window, cx| match ev {
            QuickPickEvent::ActiveChange { item } => ThemeService::preview(cx, item.value.as_deref()),
            QuickPickEvent::Accept { item, .. } => this.accept_theme(item.value.clone(), window, cx),
            QuickPickEvent::Dismiss => this.hide_quick_open(true, window, cx),
            QuickPickEvent::Change { .. } => {}
        }));
        self.quick.pick = Some(pick);
        cx.notify();
    }

    /// Enter: save the theme to the setting that names the active theme, close.
    fn accept_theme(&mut self, settings_id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = settings_id {
            let key = ThemeService::theme_setting_key(cx);
            if let Err(e) = AppConfig::set_value(cx, key, Some(Value::String(id))) {
                self.error = Some(e);
            }
        }
        self.hide_quick_open(true, window, cx);
    }

    /// Called when the picker closes: drop the preview (after an accept the saved theme shows).
    pub(crate) fn end_theme_pick(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.quick.theme_pick) {
            ThemeService::preview(cx, None);
        }
    }
}
