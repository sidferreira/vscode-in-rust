//! upstream: src/vs/workbench/services/themes/test/common/workbenchThemeService.test.ts @ 50e1350fc78d
//!
//! Ports of the theme service tests that do not need the configuration service: the settings id
//! migration and the preferred colour scheme change detection (`ThemeConfiguration` is
//! `ThemeSettings` here, the host colour scheme service is `HostColorScheme`).

use vsr_theme::{HostColorScheme, ThemeSettings, migrate_theme_settings_id};

fn host(dark: bool, high_contrast: bool) -> HostColorScheme {
    HostColorScheme { dark, high_contrast }
}

#[test]
fn migrates_default_prefixed_theme_ids() {
    // upstream: WorkbenchThemeService > migrateThemeSettingsId > migrates Default-prefixed theme IDs
    let got: Vec<String> = ["Default Dark Modern", "Default Light Modern", "Default Dark+", "Default Light+"].map(migrate_theme_settings_id).to_vec();
    assert_eq!(got, ["Dark Modern", "Light Modern", "Dark+", "Light+"]);
}

#[test]
fn migrates_experimental_theme_ids_to_vs_code_themes() {
    // upstream: WorkbenchThemeService > migrateThemeSettingsId > migrates Experimental theme IDs to VS Code themes
    // (`ThemeSettingDefaults.COLOR_THEME_DARK` / `_LIGHT` upstream are "Dark 2026" / "Light 2026".)
    let got: Vec<String> = ["Experimental Dark", "Experimental Light", "VS Code Dark", "VS Code Light"].map(migrate_theme_settings_id).to_vec();
    assert_eq!(got, ["Dark 2026", "Light 2026", "Dark 2026", "Light 2026"]);
}

#[test]
fn returns_unknown_ids_unchanged() {
    // upstream: WorkbenchThemeService > migrateThemeSettingsId > returns unknown IDs unchanged
    let ids = ["Dark Modern", "Dark 2026", "Some Custom Theme", ""];
    let got: Vec<String> = ids.map(migrate_theme_settings_id).to_vec();
    assert_eq!(got, ids);
}

#[test]
fn restores_only_for_preferred_color_scheme_changes() {
    // upstream: WorkbenchThemeService > restores only for preferred color scheme changes
    let mut settings = ThemeSettings { detect_color_scheme: false, detect_high_contrast: true, ..ThemeSettings::default() };
    let mut results = Vec::new();

    // (host.dark, host.highContrast) now vs. the previous value passed in.
    results.push(settings.is_preferred_color_scheme_change(host(false, false), host(true, false)));
    results.push(settings.is_preferred_color_scheme_change(host(true, false), host(true, true)));
    results.push(settings.is_preferred_color_scheme_change(host(true, true), host(false, true)));
    results.push(settings.is_preferred_color_scheme_change(host(false, true), host(false, false)));

    settings.detect_color_scheme = true;
    results.push(settings.is_preferred_color_scheme_change(host(false, false), host(true, false)));

    assert_eq!(results, [false, true, true, true, true]);
}
