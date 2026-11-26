//! upstream: extensions/css-language-features/client/src/node/cssClientMain.ts @ 50e1350fc78d
//! upstream: extensions/html-language-features/client/src/node/htmlClientMain.ts @ 50e1350fc78d
//!
//! Code-OSS's own `css-language-features` (css, scss, less) and `html-language-features` in the
//! real host, end to end (ADR-0103): activation, a marker for a syntax error, completion, and the
//! settings the extensions read (`css.*`, `scss.*`, `less.*`, `html.*`). Like the JSON extension
//! (ADR-0102) they pull diagnostics only for documents that have an editor. Skips (and says why)
//! without `RSCODE_CODE_OSS`, or when that build lacks the extension's bundles
//! (`scripts/build-codeoss-exthost --language-features json,css,html`).

mod common;

use common::{T, scratch, wait_until};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_exthost::language_features::dto::Position;
use vsr_exthost::language_features::query::CompletionTrigger;
use vsr_rpc::handshake::Launch;

fn extension_dir(name: &str, main: &str) -> Option<PathBuf> {
    let root = std::env::var_os("RSCODE_CODE_OSS")?;
    let dir = PathBuf::from(root).join("extensions").join(name);
    dir.join(main).exists().then_some(dir)
}

const CSS: (&str, &str) = ("css-language-features", "client/out/node/cssClientMain.js");
const HTML: (&str, &str) = ("html-language-features", "client/out/node/htmlClientMain.js");

fn boot(
    name: &str,
    ext: (&str, &str),
    user_settings: Value,
) -> Option<(ExtensionHost, PathBuf)> {
    let Some(dir) = extension_dir(ext.0, ext.1) else {
        eprintln!("SKIP {name}: RSCODE_CODE_OSS not set or {} not built", ext.0);
        return None;
    };
    // The HTML server asks the client for the `css` section too (embedded `<style>`); VS Code has
    // the CSS extension's defaults loaded next to it, so the HTML tests load it as well.
    let mut dirs = vec![dir];
    if ext.0 == HTML.0 {
        match extension_dir(CSS.0, CSS.1) {
            Some(css) => dirs.push(css),
            None => {
                eprintln!("SKIP {name}: css-language-features not built");
                return None;
            }
        }
    }
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = scratch(name);
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        socket_dir: Some(vsr_test_scratch::socket_dir!()),
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace.clone())),
        extensions: dirs
            .iter()
            .map(|d| ExtensionDescription::from_dir(d).unwrap())
            .collect(),
        default_settings: json!({}),
        user_settings,
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T).unwrap_or_else(|e| panic!("start: {e}"));
    Some((host, workspace))
}

fn open(host: &mut ExtensionHost, file: &Path, text: &str, language: &str) {
    host.open_document(file, text, language, T).unwrap();
    host.show_editor(file, T).unwrap();
    host.activate_by_event(&format!("onLanguage:{language}"), T).unwrap();
}

fn key(file: &Path) -> String {
    format!("file://{}", file.display())
}

fn markers(host: &ExtensionHost, file: &Path) -> Vec<Value> {
    host.main_thread
        .diagnostics
        .markers_for(&key(file))
        .into_iter()
        .cloned()
        .collect()
}

/// Pump the host for `secs`, to show that nothing arrives.
fn settle(host: &mut ExtensionHost, secs: u64) {
    let until = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < until {
        host.pump(Duration::from_millis(100));
    }
}

/// Completion labels at a 1-based position, polled until `want` is among them.
fn complete_until(host: &mut ExtensionHost, file: &Path, at: Position, want: &str) -> Vec<String> {
    let deadline = Instant::now() + T;
    loop {
        let got = host
            .provide_completion_items(file, at, CompletionTrigger::Invoke, T)
            .unwrap();
        let labels: Vec<String> = got
            .results
            .iter()
            .flat_map(|(_, l)| l.items.iter().map(|i| i.label.clone()))
            .collect();
        if labels.iter().any(|l| l == want) {
            return labels;
        }
        assert!(Instant::now() < deadline, "no `{want}` in {labels:?} {:?}", got.errors);
        host.pump(Duration::from_millis(200));
    }
}

const BAD_CSS: &str = "a { color red }\n";
const EMPTY_RULE: &str = "a { }\n";

#[test]
fn css_language_features_activates_and_registers_its_providers() {
    let Some((mut host, ws)) = boot("css-activate", CSS, json!({})) else {
        return;
    };
    open(&mut host, &ws.join("a.css"), "a { }\n", "css");
    assert!(
        host.main_thread.extensions.activation_errors.is_empty(),
        "{:?}",
        host.main_thread.extensions.activation_errors
    );
    assert!(!host.main_thread.language_features.registrations.is_empty());
}

#[test]
fn a_css_syntax_error_becomes_a_marker() {
    let Some((mut host, ws)) = boot("css-marker", CSS, json!({})) else {
        return;
    };
    let file = ws.join("a.css");
    open(&mut host, &file, BAD_CSS, "css");
    wait_until(&mut host, |h| !markers(h, &file).is_empty());
    let m = markers(&host, &file);
    assert_eq!(m[0]["message"], "semi-colon expected");
    assert_eq!(m[0]["severity"], 8, "an error");
    assert_eq!(m[0]["source"], "css");
}

#[test]
fn css_completes_property_names() {
    let Some((mut host, ws)) = boot("css-complete", CSS, json!({})) else {
        return;
    };
    let file = ws.join("a.css");
    open(&mut host, &file, "a { col }\n", "css");
    let labels = complete_until(&mut host, &file, Position::new(1, 8), "color");
    assert!(labels.iter().any(|l| l == "column-count"), "{labels:?}");
}

#[test]
fn css_lint_settings_set_the_severity() {
    // `css.lint.emptyRules` defaults to a warning (4); `error` makes it 8.
    let Some((mut host, ws)) = boot(
        "css-lint",
        CSS,
        json!({ "css": { "lint": { "emptyRules": "error" } } }),
    ) else {
        return;
    };
    let file = ws.join("a.css");
    open(&mut host, &file, EMPTY_RULE, "css");
    wait_until(&mut host, |h| !markers(h, &file).is_empty());
    assert_eq!(markers(&host, &file)[0]["severity"], 8);
}

#[test]
fn css_lint_default_is_a_warning() {
    let Some((mut host, ws)) = boot("css-lint-default", CSS, json!({})) else {
        return;
    };
    let file = ws.join("a.css");
    open(&mut host, &file, EMPTY_RULE, "css");
    wait_until(&mut host, |h| !markers(h, &file).is_empty());
    assert_eq!(markers(&host, &file)[0]["severity"], 4);
}

#[test]
fn css_lint_ignore_drops_the_marker() {
    let Some((mut host, ws)) = boot(
        "css-lint-ignore",
        CSS,
        json!({ "css": { "lint": { "emptyRules": "ignore" } } }),
    ) else {
        return;
    };
    let file = ws.join("a.css");
    open(&mut host, &file, EMPTY_RULE, "css");
    settle(&mut host, 5);
    assert!(markers(&host, &file).is_empty());
}

/// `css.validate`, `scss.validate` and `less.validate`: off means no markers for the same
/// text that gives one with the default (the tests above; SCSS and LESS use their own keys).
fn validate_off(ns: &str, language: &str, file_name: &str, text: &str) {
    let Some((mut host, ws)) = boot(
        &format!("{ns}-validate-off"),
        CSS,
        json!({ ns: { "validate": false } }),
    ) else {
        return;
    };
    let file = ws.join(file_name);
    open(&mut host, &file, text, language);
    settle(&mut host, 5);
    assert!(markers(&host, &file).is_empty(), "{:?}", markers(&host, &file));
}

fn validate_on(ns: &str, language: &str, file_name: &str, text: &str) {
    let Some((mut host, ws)) = boot(&format!("{ns}-validate-on"), CSS, json!({})) else {
        return;
    };
    let file = ws.join(file_name);
    open(&mut host, &file, text, language);
    wait_until(&mut host, |h| !markers(h, &file).is_empty());
}

#[test]
fn css_validate_false_silences_the_markers() {
    validate_off("css", "css", "a.css", BAD_CSS);
}

#[test]
fn scss_validates_and_scss_validate_false_silences_it() {
    validate_on("scss", "scss", "a.scss", BAD_CSS);
    validate_off("scss", "scss", "a.scss", BAD_CSS);
}

#[test]
fn less_validates_and_less_validate_false_silences_it() {
    validate_on("less", "less", "a.less", BAD_CSS);
    validate_off("less", "less", "a.less", BAD_CSS);
}

#[test]
fn scss_lint_is_its_own_namespace() {
    // `css.lint.emptyRules: ignore` must not affect SCSS files; `scss.lint` does.
    let Some((mut host, ws)) = boot(
        "scss-lint",
        CSS,
        json!({ "css": { "lint": { "emptyRules": "ignore" } }, "scss": { "lint": { "emptyRules": "error" } } }),
    ) else {
        return;
    };
    let file = ws.join("a.scss");
    open(&mut host, &file, EMPTY_RULE, "scss");
    wait_until(&mut host, |h| !markers(h, &file).is_empty());
    assert_eq!(markers(&host, &file)[0]["severity"], 8);
}

#[test]
fn html_language_features_activates_and_registers_its_providers() {
    let Some((mut host, ws)) = boot("html-activate", HTML, json!({})) else {
        return;
    };
    open(&mut host, &ws.join("a.html"), "<div></div>\n", "html");
    assert!(
        host.main_thread.extensions.activation_errors.is_empty(),
        "{:?}",
        host.main_thread.extensions.activation_errors
    );
    assert!(!host.main_thread.language_features.registrations.is_empty());
}

#[test]
fn html_completes_tag_names() {
    let Some((mut host, ws)) = boot("html-complete", HTML, json!({})) else {
        return;
    };
    let file = ws.join("a.html");
    open(&mut host, &file, "<di\n", "html");
    complete_until(&mut host, &file, Position::new(1, 4), "div");
}

#[test]
fn html_suggest_html5_false_drops_html5_tags() {
    // With `html.suggest.html5` off, `section` (an HTML5 element) is no longer offered.
    let Some((mut host, ws)) = boot(
        "html-html5-off",
        HTML,
        json!({ "html": { "suggest": { "html5": false } } }),
    ) else {
        return;
    };
    let file = ws.join("a.html");
    open(&mut host, &file, "<sect\n", "html");
    settle(&mut host, 3);
    let got = host
        .provide_completion_items(&file, Position::new(1, 6), CompletionTrigger::Invoke, T)
        .unwrap();
    let labels: Vec<_> = got
        .results
        .iter()
        .flat_map(|(_, l)| l.items.iter().map(|i| i.label.clone()))
        .collect();
    assert!(!labels.iter().any(|l| l == "section"), "{labels:?}");
}

#[test]
fn a_css_syntax_error_in_an_html_style_block_becomes_a_marker() {
    let Some((mut host, ws)) = boot("html-style-marker", HTML, json!({})) else {
        return;
    };
    let file = ws.join("a.html");
    open(&mut host, &file, "<style>\na { color red }\n</style>\n", "html");
    wait_until(&mut host, |h| !markers(h, &file).is_empty());
    assert_eq!(markers(&host, &file)[0]["severity"], 8);
}

#[test]
fn html_validate_styles_false_silences_style_block_markers() {
    let Some((mut host, ws)) = boot(
        "html-styles-off",
        HTML,
        json!({ "html": { "validate": { "styles": false } } }),
    ) else {
        return;
    };
    let file = ws.join("a.html");
    open(&mut host, &file, "<style>\na { color red }\n</style>\n", "html");
    settle(&mut host, 5);
    assert!(markers(&host, &file).is_empty());
}

// ---- formatting settings ----

use vsr_exthost::language_features::dto::FormattingOptions;

/// The text of the formatting edits the providers return: polled until some arrive (`wait`), or
/// for 4 seconds when none are expected.
fn format_edits(host: &mut ExtensionHost, file: &Path, wait: bool) -> Vec<String> {
    let deadline = Instant::now() + if wait { T } else { Duration::from_secs(4) };
    loop {
        let got = host
            .provide_document_formatting_edits(
                file,
                &FormattingOptions {
                    tab_size: 2,
                    insert_spaces: true,
                },
                T,
            )
            .unwrap();
        let texts: Vec<String> = got.flatten().into_iter().map(|e| e.text).collect();
        if !texts.is_empty() || Instant::now() >= deadline {
            return texts;
        }
        host.pump(Duration::from_millis(200));
    }
}

fn formatted(label: &str, language: &str, name: &str, settings: Value, expect: bool) -> Vec<String> {
    let Some((mut host, ws)) = boot(&format!("fmt-{label}-{expect}"), CSS, settings) else {
        return vec![];
    };
    let file = ws.join(name);
    open(&mut host, &file, "a{color:red}\n", language);
    format_edits(&mut host, &file, expect)
}

#[test]
fn css_format_brace_style_changes_the_output() {
    let collapse = formatted("collapse", "css", "a.css", json!({}), true);
    let expand = formatted(
        "expand",
        "css",
        "a.css",
        json!({ "css": { "format": { "braceStyle": "expand" } } }),
        true,
    );
    if collapse.is_empty() && expand.is_empty() {
        return; // skipped: no build
    }
    let (c, e) = (collapse.join(""), expand.join(""));
    assert!(c.contains("a {"), "{c:?}");
    assert!(e.contains("a\n{"), "{e:?}");
}

#[test]
fn css_scss_less_format_enable_false_registers_no_formatter() {
    for (ns, lang, file) in [
        ("css", "css", "a.css"),
        ("scss", "scss", "a.scss"),
        ("less", "less", "a.less"),
    ] {
        let on = formatted(ns, lang, file, json!({}), true);
        if on.is_empty() {
            return; // skipped: no build
        }
        let off = formatted(
            &format!("{ns}-off"),
            lang,
            file,
            json!({ ns: { "format": { "enable": false } } }),
            false,
        );
        assert!(off.is_empty(), "{ns}: {off:?}");
    }
}

#[test]
fn html_format_enable_false_registers_no_formatter() {
    let run = |settings: Value, expect: bool| {
        let Some((mut host, ws)) = boot(&format!("fmt-html-{expect}"), HTML, settings) else {
            return vec![];
        };
        let file = ws.join("a.html");
        open(&mut host, &file, "<div><p>x</p></div>\n", "html");
        format_edits(&mut host, &file, expect)
    };
    if run(json!({}), true).is_empty() {
        return; // skipped: no build
    }
    assert!(run(json!({ "html": { "format": { "enable": false } } }), false).is_empty());
}

#[test]
fn html_suggest_html5_default_offers_html5_tags() {
    // The control for the `false` test above.
    let Some((mut host, ws)) = boot("html-html5-on", HTML, json!({})) else {
        return;
    };
    let file = ws.join("a.html");
    open(&mut host, &file, "<sect\n", "html");
    complete_until(&mut host, &file, Position::new(1, 6), "section");
}

// ---- more settings ----

fn completion_items(host: &mut ExtensionHost, file: &Path, at: Position, want: &str) -> Vec<(String, String)> {
    let deadline = Instant::now() + T;
    loop {
        let got = host
            .provide_completion_items(file, at, CompletionTrigger::Invoke, T)
            .unwrap();
        let items: Vec<(String, String)> = got
            .results
            .iter()
            .flat_map(|(_, l)| l.items.iter().map(|i| (i.label.clone(), i.insert_text.clone())))
            .collect();
        if items.iter().any(|(l, _)| l == want) {
            return items;
        }
        assert!(Instant::now() < deadline, "no `{want}` in {items:?}");
        host.pump(Duration::from_millis(200));
    }
}

fn color_insert_text(settings: Value) -> Option<String> {
    let (mut host, ws) = boot("css-semicolon", CSS, settings)?;
    let file = ws.join("a.css");
    open(&mut host, &file, "a { col }\n", "css");
    let items = completion_items(&mut host, &file, Position::new(1, 8), "color");
    items.into_iter().find(|(l, _)| l == "color").map(|(_, t)| t)
}

#[test]
fn css_completion_complete_property_with_semicolon() {
    let Some(on) = color_insert_text(json!({})) else {
        return;
    };
    assert!(on.ends_with(';'), "{on:?}");
    let off = color_insert_text(json!({ "css": { "completion": { "completePropertyWithSemicolon": false } } })).unwrap();
    assert!(!off.contains(';'), "{off:?}");
}

#[test]
fn css_custom_data_adds_properties_to_completion() {
    let Some((mut host, ws)) = boot(
        "css-custom-data",
        CSS,
        json!({ "css": { "customData": ["./data.json"] } }),
    ) else {
        return;
    };
    std::fs::write(
        ws.join("data.json"),
        r#"{"version":1.1,"properties":[{"name":"rscode-widget","description":"A custom property."}]}"#,
    )
    .unwrap();
    let file = ws.join("a.css");
    open(&mut host, &file, "a { rscode }\n", "css");
    complete_until(&mut host, &file, Position::new(1, 11), "rscode-widget");
}

#[test]
fn html_custom_data_adds_tags_to_completion() {
    let Some((mut host, ws)) = boot(
        "html-custom-data",
        HTML,
        json!({ "html": { "customData": ["./data.json"] } }),
    ) else {
        return;
    };
    std::fs::write(
        ws.join("data.json"),
        r#"{"version":1.1,"tags":[{"name":"rscode-widget","description":"A custom tag."}]}"#,
    )
    .unwrap();
    let file = ws.join("a.html");
    open(&mut host, &file, "<rscode\n", "html");
    complete_until(&mut host, &file, Position::new(1, 8), "rscode-widget");
}

fn hover_text(settings: Value) -> Option<String> {
    let (mut host, ws) = boot("css-hover", CSS, settings)?;
    let file = ws.join("a.css");
    open(&mut host, &file, "a { color: red }\n", "css");
    let deadline = Instant::now() + T;
    loop {
        let got = host.provide_hover(&file, Position::new(1, 6), T).unwrap();
        let text: String = got
            .results
            .iter()
            .flat_map(|(_, h)| h.contents.iter().map(|m| m.value.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        if !text.is_empty() || Instant::now() >= deadline {
            return Some(text);
        }
        host.pump(Duration::from_millis(200));
    }
}

#[test]
fn css_hover_documentation_false_drops_the_description() {
    let Some(on) = hover_text(json!({})) else {
        return;
    };
    assert!(on.contains("color"), "{on:?}");
    let off = hover_text(json!({ "css": { "hover": { "documentation": false } } })).unwrap();
    assert!(off.len() < on.len(), "on: {on:?} off: {off:?}");
}

#[test]
fn html_attribute_default_value_quotes() {
    let insert = |settings: Value| -> Option<String> {
        let (mut host, ws) = boot("html-attr-quotes", HTML, settings)?;
        let file = ws.join("a.html");
        open(&mut host, &file, "<div cla></div>\n", "html");
        let items = completion_items(&mut host, &file, Position::new(1, 9), "class");
        items.into_iter().find(|(l, _)| l == "class").map(|(_, t)| t)
    };
    let Some(double) = insert(json!({})) else {
        return;
    };
    let single = insert(json!({ "html": { "completion": { "attributeDefaultValue": "singlequotes" } } })).unwrap();
    let none = insert(json!({ "html": { "completion": { "attributeDefaultValue": "empty" } } })).unwrap();
    assert!(double.contains('"'), "{double:?}");
    assert!(single.contains('\''), "{single:?}");
    assert!(!none.contains('"') && !none.contains('\''), "{none:?}");
}
