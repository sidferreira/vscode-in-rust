//! Ported from upstream's settingsTreeModels.test.ts (`settingKeyToDisplayFormat` cases).
//! The sandbox-specific label cases are not ported: their label overrides are not implemented.
//!
//! upstream: src/vs/workbench/contrib/preferences/test/browser/settingsTreeModels.test.ts @ 50e1350fc78d

use vsr_settings::{DisplayFormat, setting_key_to_display_format, wordify_key};

fn fmt(key: &str) -> DisplayFormat {
    setting_key_to_display_format(key, "")
}

fn df(category: &str, label: &str) -> DisplayFormat {
    DisplayFormat { category: category.into(), label: label.into() }
}

#[test]
fn settings_key_to_display_format() {
    assert_eq!(fmt("foo.bar"), df("Foo", "Bar"));
    assert_eq!(fmt("foo.bar.etc"), df("Foo \u{203A} Bar", "Etc"));
    assert_eq!(fmt("fooBar.etcSomething"), df("Foo Bar", "Etc Something"));
    assert_eq!(fmt("foo"), df("", "Foo"));
    assert_eq!(fmt("foo.1leading.number"), df("Foo \u{203A} 1leading", "Number"));
    assert_eq!(fmt("foo.1Leading.number"), df("Foo \u{203A} 1 Leading", "Number"));
}

#[test]
fn category_is_trimmed_when_the_group_already_shows_it() {
    let f = setting_key_to_display_format("editor.fontSize", "editor");
    assert_eq!(f, df("", "Font Size"));
    let f = setting_key_to_display_format("chat.agent.sandbox.network.allowX", "chat.agent.sandbox.network");
    assert_eq!(f, df("", "Allow X"));
    let f = setting_key_to_display_format("chat.agent.sandbox.allowX", "chat");
    assert_eq!(f.category, "Agent \u{203A} Sandbox");
}

#[test]
fn acronyms_and_camel_case_split() {
    assert_eq!(wordify_key("AISearch"), "AI Search");
    assert_eq!(wordify_key("jsonSchema"), "Json Schema".replace("Json", "JSON"));
    assert_eq!(fmt("editor.tsserver.css.enable"), df("Editor \u{203A} Tsserver \u{203A} CSS", "Enable"));
}

#[test]
fn known_terms_are_mapped_whole_words_only() {
    assert_eq!(fmt("github.copilot.enable"), df("GitHub \u{203A} Copilot", "Enable"));
    assert_eq!(wordify_key("typescript"), "TypeScript");
    assert_eq!(wordify_key("githubs"), "Githubs");
}
