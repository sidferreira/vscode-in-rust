//! The bundled registry guesses languages like VS Code does for common files, and
//! `files.associations` (configured associations) beat the built-in ones.

use vsr_languages::bundled::{builtin_registry, grammar_json, scope_name_for_language};

fn guess(r: &vsr_languages::LanguagesRegistry, path: &str, first: Option<&str>) -> String {
    r.language_for_file(Some(path), first)
}

#[test]
fn common_files() {
    let r = builtin_registry();
    for (path, id) in [
        ("/p/main.rs", "rust"),
        ("/p/a.ts", "typescript"),
        ("/p/a.tsx", "typescriptreact"),
        ("/p/a.js", "javascript"),
        ("/p/a.json", "json"),
        ("/p/tsconfig.json", "jsonc"),
        ("/p/.eslintrc.json", "jsonc"),
        ("/p/README.md", "markdown"),
        ("/p/a.py", "python"),
        ("/p/a.sh", "shellscript"),
        ("/p/.bashrc", "shellscript"),
        ("/p/a.css", "css"),
        ("/p/index.html", "html"),
        ("/p/a.yml", "yaml"),
        ("/p/a.xml", "xml"),
        ("/p/a.c", "c"),
        ("/p/a.cpp", "cpp"),
        ("/p/a.go", "go"),
        ("/p/A.java", "java"),
        ("/p/Makefile", "makefile"),
        ("/p/Dockerfile", "dockerfile"),
        ("/p/a.txt", "plaintext"),
        ("/p/unknown.zzz", "plaintext"),
    ] {
        assert_eq!(guess(&r, path, None), id, "{path}");
    }
    // Case insensitive extension, and shebangs for files without a known name.
    assert_eq!(guess(&r, "/p/MAIN.RS", None), "rust");
    assert_eq!(guess(&r, "/p/run", Some("#!/usr/bin/env python3")), "python");
    assert_eq!(guess(&r, "/p/run", Some("#!/bin/bash")), "shellscript");
}

#[test]
fn names_come_from_aliases() {
    let r = builtin_registry();
    assert_eq!(r.language_name("rust"), Some("Rust"));
    assert_eq!(r.language_name("typescript"), Some("TypeScript"));
    assert_eq!(r.language_name("jsonc"), Some("JSON with Comments"));
    assert_eq!(r.language_name("plaintext"), Some("Plain Text"));
    assert_eq!(r.language_name("shellscript"), Some("Shell Script"));
    assert_eq!(r.language_id_by_language_name("Plain Text"), Some("plaintext"));
    // Not built-in upstream: there is no TOML or Kotlin extension in VS Code.
    assert!(!r.is_registered_language_id("toml"));
    assert!(!r.is_registered_language_id("kotlin"));
}

#[test]
fn configured_associations_win() {
    let mut r = builtin_registry();
    r.set_configured_associations(&[("*.foo".into(), "json".into()), ("*.rs".into(), "python".into())]);
    assert_eq!(guess(&r, "/p/a.foo", None), "json");
    assert_eq!(guess(&r, "/p/main.rs", None), "python");
    r.set_configured_associations(&[]);
    assert_eq!(guess(&r, "/p/main.rs", None), "rust");
    assert_eq!(guess(&r, "/p/a.foo", None), "plaintext");
}

#[test]
fn bundled_grammars_resolve_for_the_first_language_set() {
    for lang in [
        "rust", "javascript", "typescript", "typescriptreact", "json", "jsonc", "markdown", "python", "shellscript",
        "css", "html", "yaml", "xml", "c", "cpp", "go", "java",
    ] {
        let scope = scope_name_for_language(lang).unwrap_or_else(|| panic!("no grammar for {lang}"));
        assert!(grammar_json(scope).is_some(), "{scope}");
    }
    assert!(scope_name_for_language("plaintext").is_none());
}
