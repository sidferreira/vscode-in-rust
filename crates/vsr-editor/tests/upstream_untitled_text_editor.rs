//! upstream: src/vs/workbench/services/untitled/test/browser/untitledTextEditor.test.ts @ 50e1350fc78df67b11049b903d96b530f28d26a5
//!
//! Ports of the dirty-state and language tests of `Untitled text editors`. The tests here check the
//! pure rules in `vsr_editor::untitled`; the upstream tests that drive the service, the working copy
//! and the editor input (`basics`, the backup and revert events, `service#*`, `model#*`,
//! `canDispose`, `can change language afterwards`, `remembers that language was set explicitly`)
//! are covered by the workbench's headless tests (ADR-0058), not here. The
//! `UntitledTextEditorWorkingCopyEditorHandler` suite is not ported: rscode has no working copy service.

use vsr_editor::untitled::{ACTIVE_EDITOR_LANGUAGE, is_dirty, new_language};

// upstream: Untitled text editors > initial content is dirty
#[test]
fn initial_content_is_dirty() {
    assert!(is_dirty("Hello World"));
    assert!(!is_dirty(""));
}

// upstream: Untitled text editors > no longer dirty when content gets empty (not with associated resource)
#[test]
fn no_longer_dirty_when_content_gets_empty() {
    assert!(is_dirty("foo bar"));
    assert!(!is_dirty(""));
}

// upstream: Untitled text editors > created with files.defaultLanguage setting
#[test]
fn created_with_files_default_language_setting() {
    assert_eq!(new_language(None, Some("javascript"), None), "javascript");
}

// upstream: Untitled text editors > created with files.defaultLanguage setting (${activeEditorLanguage})
#[test]
fn created_with_files_default_language_setting_active_editor_language() {
    assert_eq!(ACTIVE_EDITOR_LANGUAGE, "${activeEditorLanguage}");
    assert_eq!(
        new_language(None, Some(ACTIVE_EDITOR_LANGUAGE), Some("typescript")),
        "typescript"
    );
}

// upstream: Untitled text editors > created with language overrides files.defaultLanguage setting
#[test]
fn created_with_language_overrides_files_default_language_setting() {
    assert_eq!(
        new_language(Some("typescript"), Some("javascript"), None),
        "typescript"
    );
}
