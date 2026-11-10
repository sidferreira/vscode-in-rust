//! upstream: src/vs/workbench/services/textfile/test/browser/textFileService.test.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//!
//! Ports of the `Filename Suggestion` tests of `Files - TextFileService`. Upstream calls the service
//! method `suggestFilename(languageId, untitledName)`; rscode's rule is `vsr_editor::untitled::
//! suggest_file_name`, which takes the language's extensions and file names (`None` for a language
//! that is not registered, which upstream checks with `getLanguageName`). ADR-0074.

use vsr_editor::untitled::suggest_file_name;

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

// upstream: Files - TextFileService > Filename Suggestion - Suggest prefix only when there are no relevant extensions
#[test]
fn suggest_prefix_only_when_there_are_no_relevant_extensions() {
    // The language `shleem` is not registered: the name stays.
    assert_eq!(suggest_file_name("Untitled-1", None), "Untitled-1");
}

// upstream: Files - TextFileService > Filename Suggestion - Suggest prefix with first extension
#[test]
fn suggest_prefix_with_first_extension() {
    let (exts, names) = (strings(&[".shleem", ".gazorpazorp"]), strings(&["plumbus"]));
    assert_eq!(suggest_file_name("Untitled-1", Some((&exts, &names))), "Untitled-1.shleem");
}

// upstream: Files - TextFileService > Filename Suggestion - Preserve extension if it matchers
#[test]
fn preserve_extension_if_it_matches() {
    let (exts, names) = (strings(&[".shleem", ".gazorpazorp"]), strings(&[]));
    assert_eq!(suggest_file_name("Untitled-1.gazorpazorp", Some((&exts, &names))), "Untitled-1.gazorpazorp");
}

// upstream: Files - TextFileService > Filename Suggestion - Rewrite extension according to language
#[test]
fn rewrite_extension_according_to_language() {
    let (exts, names) = (strings(&[".shleem", ".gazorpazorp"]), strings(&[]));
    assert_eq!(suggest_file_name("Untitled-1.foobar", Some((&exts, &names))), "Untitled-1.shleem");
}

// upstream: Files - TextFileService > Filename Suggestion - Suggest filename if there are no extensions
#[test]
fn suggest_filename_if_there_are_no_extensions() {
    let names = strings(&["plumbus", "shleem", "gazorpazorp"]);
    assert_eq!(suggest_file_name("Untitled-1", Some((&[], &names))), "plumbus");
}

// upstream: Files - TextFileService > Filename Suggestion - Preserve filename if it matches
#[test]
fn preserve_filename_if_it_matches() {
    let names = strings(&["plumbus", "shleem", "gazorpazorp"]);
    assert_eq!(suggest_file_name("gazorpazorp", Some((&[], &names))), "gazorpazorp");
}

// upstream: Files - TextFileService > Filename Suggestion - Rewrites filename according to language
#[test]
fn rewrites_filename_according_to_language() {
    let names = strings(&["plumbus", "shleem", "gazorpazorp"]);
    assert_eq!(suggest_file_name("foobar", Some((&[], &names))), "plumbus");
}
