//! upstream: src/vs/editor/test/common/services/languagesAssociations.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/services/languagesRegistry.test.ts @ 50e1350fc78d
//!
//! Ports of the association and registry tests. Upstream's associations are module-level state
//! shared by the tests; here each test has its own `Associations`.

use onig::Regex;
use std::sync::Arc;
use vsr_languages::{Associations, LanguageAssociation, LanguageExtensionPoint, LanguageNameIdPair, LanguagesRegistry};

fn assoc(id: &str, mime: &str) -> LanguageAssociation {
    LanguageAssociation { id: id.into(), mime: mime.into(), ..Default::default() }
}
fn ext(id: &str, e: &str, mime: &str) -> LanguageAssociation {
    LanguageAssociation { extension: Some(e.into()), ..assoc(id, mime) }
}
fn filename(id: &str, f: &str, mime: &str) -> LanguageAssociation {
    LanguageAssociation { filename: Some(f.into()), ..assoc(id, mime) }
}
fn pattern(id: &str, p: &str, mime: &str) -> LanguageAssociation {
    LanguageAssociation { filepattern: Some(p.into()), ..assoc(id, mime) }
}
fn first(id: &str, re: &str, mime: &str) -> LanguageAssociation {
    LanguageAssociation { firstline: Some(Arc::new(Regex::new(re).unwrap())), ..assoc(id, mime) }
}
fn mimes(a: &Associations, path: &str, first_line: Option<&str>) -> Vec<String> {
    a.mime_types(Some(path), first_line)
}

#[test]
fn dynamically_register_text_mime() {
    let mut a = Associations::new();
    assert_eq!(mimes(&a, "foo.monaco", None), ["application/unknown"]);

    a.register_platform(ext("monaco", ".monaco", "text/monaco"));
    assert_eq!(mimes(&a, "foo.monaco", None), ["text/monaco", "text/plain"]);
    assert_eq!(mimes(&a, ".monaco", None), ["text/monaco", "text/plain"]);

    a.register_platform(filename("codefile", "Codefile", "text/code"));
    assert_eq!(mimes(&a, "Codefile", None), ["text/code", "text/plain"]);
    assert_eq!(mimes(&a, "foo.Codefile", None), ["application/unknown"]);

    a.register_platform(pattern("docker", "Docker*", "text/docker"));
    assert_eq!(mimes(&a, "Docker-debug", None), ["text/docker", "text/plain"]);
    assert_eq!(mimes(&a, "docker-PROD", None), ["text/docker", "text/plain"]);

    a.register_platform(first("niceregex", "RegexesAreNice", "text/nice-regex"));
    assert_eq!(mimes(&a, "Randomfile.noregistration", Some("RegexesAreNice")), ["text/nice-regex", "text/plain"]);
    assert_eq!(mimes(&a, "Randomfile.noregistration", Some("RegexesAreNotNice")), ["application/unknown"]);
    assert_eq!(mimes(&a, "Codefile", Some("RegexesAreNice")), ["text/code", "text/plain"]);
}

#[test]
fn mimes_priority() {
    let mut a = Associations::new();
    a.register_platform(ext("monaco", ".monaco", "text/monaco"));
    a.register_platform(first("foobar", "foobar", "text/foobar"));
    assert_eq!(mimes(&a, "foo.monaco", None), ["text/monaco", "text/plain"]);
    assert_eq!(mimes(&a, "foo.monaco", Some("foobar")), ["text/monaco", "text/plain"]);

    a.register_platform(filename("docker", "dockerfile", "text/winner"));
    a.register_platform(pattern("docker", "dockerfile*", "text/looser"));
    assert_eq!(mimes(&a, "dockerfile", None), ["text/winner", "text/plain"]);

    a.register_platform(first("azure-looser", "azure", "text/azure-looser"));
    a.register_platform(first("azure-winner", "azure", "text/azure-winner"));
    assert_eq!(mimes(&a, "azure", Some("azure")), ["text/azure-winner", "text/plain"]);
}

#[test]
fn specificity_priority_1() {
    let mut a = Associations::new();
    a.register_platform(ext("monaco2", ".monaco2", "text/monaco2"));
    a.register_platform(filename("monaco2", "specific.monaco2", "text/specific-monaco2"));
    assert_eq!(mimes(&a, "specific.monaco2", None), ["text/specific-monaco2", "text/plain"]);
    assert_eq!(mimes(&a, "foo.monaco2", None), ["text/monaco2", "text/plain"]);
}

#[test]
fn specificity_priority_2() {
    let mut a = Associations::new();
    a.register_platform(filename("monaco3", "specific.monaco3", "text/specific-monaco3"));
    a.register_platform(ext("monaco3", ".monaco3", "text/monaco3"));
    assert_eq!(mimes(&a, "specific.monaco3", None), ["text/specific-monaco3", "text/plain"]);
    assert_eq!(mimes(&a, "foo.monaco3", None), ["text/monaco3", "text/plain"]);
}

#[test]
fn longest_extension_wins() {
    let mut a = Associations::new();
    a.register_platform(ext("monaco", ".monaco", "text/monaco"));
    a.register_platform(ext("monaco", ".monaco.xml", "text/monaco-xml"));
    a.register_platform(ext("monaco", ".monaco.xml.build", "text/monaco-xml-build"));
    assert_eq!(mimes(&a, "foo.monaco", None), ["text/monaco", "text/plain"]);
    assert_eq!(mimes(&a, "foo.monaco.xml", None), ["text/monaco-xml", "text/plain"]);
    assert_eq!(mimes(&a, "foo.monaco.xml.build", None), ["text/monaco-xml-build", "text/plain"]);
}

#[test]
fn user_configured_wins() {
    let mut a = Associations::new();
    a.register_configured(ext("monaco", ".monaco.xnl", "text/monaco"));
    a.register_platform(ext("monaco", ".monaco.xml", "text/monaco-xml"));
    assert_eq!(mimes(&a, "foo.monaco.xnl", None), ["text/monaco", "text/plain"]);
}

#[test]
fn pattern_matches_on_path_if_specified() {
    let mut a = Associations::new();
    a.register_platform(pattern("monaco", "**/dot.monaco.xml", "text/monaco"));
    a.register_platform(pattern("other", "*ot.other.xml", "text/other"));
    assert_eq!(mimes(&a, "/some/path/dot.monaco.xml", None), ["text/monaco", "text/plain"]);
}

#[test]
fn last_registered_mime_wins() {
    let mut a = Associations::new();
    a.register_platform(pattern("monaco", "**/dot.monaco.xml", "text/monaco"));
    a.register_platform(pattern("other", "**/dot.monaco.xml", "text/other"));
    assert_eq!(mimes(&a, "/some/path/dot.monaco.xml", None), ["text/other", "text/plain"]);
}

#[test]
fn shebang_detection_for_typescript_runtimes() {
    let mut a = Associations::new();
    a.register_platform(first("typescript", r"^#!.*\b(deno|bun|ts-node)\b", "text/typescript"));
    for line in [
        "#!/usr/bin/env deno",
        "#!/usr/bin/env -S deno -A",
        "#!/usr/bin/deno",
        "#!/usr/bin/env bun",
        "#!/usr/bin/env -S bun run",
        "#!/usr/bin/env ts-node",
        "#!/usr/bin/env -S ts-node --esm",
    ] {
        assert_eq!(mimes(&a, "script", Some(line)), ["text/typescript", "text/plain"], "{line}");
    }
    for line in ["#!/usr/bin/env node", "#!/usr/bin/env python", "#!/bin/bash"] {
        assert_eq!(mimes(&a, "script", Some(line)), ["application/unknown"], "{line}");
    }
}

// --- LanguagesRegistry ---

fn lang(id: &str) -> LanguageExtensionPoint {
    LanguageExtensionPoint::new(id)
}
fn s(v: &[&str]) -> Option<Vec<String>> {
    Some(v.iter().map(|x| x.to_string()).collect())
}
fn pair(n: &str, id: &str) -> LanguageNameIdPair {
    LanguageNameIdPair { language_name: n.into(), language_id: id.into() }
}

#[test]
fn output_language_does_not_have_a_name() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint {
        extensions: s(&[]),
        aliases: s(&[]),
        mimetypes: s(&["outputLanguageMimeType"]),
        ..lang("outputLangId")
    }]);
    assert_eq!(r.sorted_registered_language_names(), []);
}

#[test]
fn language_with_alias_has_a_name() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { aliases: s(&["LangName"]), mimetypes: s(&["bla"]), ..lang("langId") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("LangName", "langId")]);
    assert_eq!(r.language_name("langId"), Some("LangName"));
}

#[test]
fn language_without_alias_gets_a_name() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { mimetypes: s(&["bla"]), ..lang("langId") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("langId", "langId")]);
    assert_eq!(r.language_name("langId"), Some("langId"));
}

#[test]
fn bug_4360_fsharp_not_shown_in_status_bar() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&[".ext1"]), aliases: s(&["LangName"]), mimetypes: s(&["bla"]), ..lang("langId") }]);
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&[".ext2"]), aliases: s(&[]), mimetypes: s(&["bla"]), ..lang("langId") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("LangName", "langId")]);
    assert_eq!(r.language_name("langId"), Some("LangName"));
}

#[test]
fn issue_5278_extension_can_override_language_name() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&[".ext1"]), aliases: s(&["LangName"]), mimetypes: s(&["bla"]), ..lang("langId") }]);
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&[".ext2"]), aliases: s(&["BetterLanguageName"]), mimetypes: s(&["bla"]), ..lang("langId") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("BetterLanguageName", "langId")]);
    assert_eq!(r.language_name("langId"), Some("BetterLanguageName"));
}

#[test]
fn mimetypes_are_generated_if_necessary() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[lang("langId")]);
    assert_eq!(r.mime_type("langId"), Some("text/x-langId"));
}

#[test]
fn first_mimetype_wins() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { mimetypes: s(&["text/langId", "text/langId2"]), ..lang("langId") }]);
    assert_eq!(r.mime_type("langId"), Some("text/langId"));
}

#[test]
fn first_mimetype_wins_2() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[lang("langId")]);
    r.register_languages(&[LanguageExtensionPoint { mimetypes: s(&["text/langId"]), ..lang("langId") }]);
    assert_eq!(r.mime_type("langId"), Some("text/x-langId"));
}

#[test]
fn aliases() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[lang("a")]);
    assert_eq!(r.sorted_registered_language_names(), [pair("a", "a")]);
    assert_eq!(r.language_id_by_language_name("a"), Some("a"));
    assert_eq!(r.language_name("a"), Some("a"));

    r.register_languages(&[LanguageExtensionPoint { aliases: s(&["A1", "A2"]), ..lang("a") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("A1", "a")]);
    assert_eq!(r.language_id_by_language_name("a"), Some("a"));
    assert_eq!(r.language_id_by_language_name("a1"), Some("a"));
    assert_eq!(r.language_id_by_language_name("a2"), Some("a"));
    assert_eq!(r.language_name("a"), Some("A1"));

    r.register_languages(&[LanguageExtensionPoint { aliases: s(&["A3", "A4"]), ..lang("a") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("A3", "a")]);
    for n in ["a", "a1", "a2", "a3", "a4"] {
        assert_eq!(r.language_id_by_language_name(n), Some("a"), "{n}");
    }
    assert_eq!(r.language_name("a"), Some("A3"));
}

#[test]
fn empty_aliases_array_means_no_alias() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[lang("a")]);
    assert_eq!(r.sorted_registered_language_names(), [pair("a", "a")]);
    r.register_languages(&[LanguageExtensionPoint { aliases: s(&[]), ..lang("b") }]);
    assert_eq!(r.sorted_registered_language_names(), [pair("a", "a")]);
    assert_eq!(r.language_id_by_language_name("a"), Some("a"));
    assert_eq!(r.language_id_by_language_name("b"), Some("b"));
    assert_eq!(r.language_name("a"), Some("a"));
    assert_eq!(r.language_name("b"), None);
}

#[test]
fn extensions() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { aliases: s(&["aName"]), extensions: s(&["aExt"]), ..lang("a") }]);
    assert_eq!(r.extensions("a"), ["aExt"]);
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&["aExt2"]), ..lang("a") }]);
    assert_eq!(r.extensions("a"), ["aExt", "aExt2"]);
}

#[test]
fn extensions_of_primary_language_registration_come_first() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&["aExt3"]), ..lang("a") }]);
    assert_eq!(r.extensions("a")[0], "aExt3");
    r.register_languages(&[LanguageExtensionPoint { configuration: Some("conf.json".into()), extensions: s(&["aExt"]), ..lang("a") }]);
    assert_eq!(r.extensions("a")[0], "aExt");
    r.register_languages(&[LanguageExtensionPoint { extensions: s(&["aExt2"]), ..lang("a") }]);
    assert_eq!(r.extensions("a")[0], "aExt");
}

#[test]
fn filenames() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { aliases: s(&["aName"]), filenames: s(&["aFilename"]), ..lang("a") }]);
    assert_eq!(r.filenames("a"), ["aFilename"]);
    r.register_languages(&[LanguageExtensionPoint { filenames: s(&["aFilename2"]), ..lang("a") }]);
    assert_eq!(r.filenames("a"), ["aFilename", "aFilename2"]);
}

#[test]
fn configuration() {
    let mut r = LanguagesRegistry::new();
    r.register_languages(&[LanguageExtensionPoint { aliases: s(&["aName"]), configuration: Some("/path/to/aFilename".into()), ..lang("a") }]);
    assert_eq!(r.configuration_files("a"), ["/path/to/aFilename"]);
    assert!(r.configuration_files("aname").is_empty());
    assert!(r.configuration_files("aName").is_empty());
    r.register_languages(&[LanguageExtensionPoint { configuration: Some("/path/to/aFilename2".into()), ..lang("a") }]);
    assert_eq!(r.configuration_files("a"), ["/path/to/aFilename", "/path/to/aFilename2"]);
}
