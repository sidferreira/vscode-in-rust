//! upstream: src/vs/editor/test/common/modes/languageSelector.test.ts @ 50e1350fc78d
//!
//! `LanguageSelector` scoring, ported test by test. `undefined`, `null` and `{}` selectors are
//! `None` or an empty filter; JS has one `''`-vs-undefined, Rust has both.

use std::collections::BTreeSet;
use vsr_exthost::language_features::selector::{
    LanguageFilter, LanguageSelector as S, Pattern, score, select_language_ids,
};
use vsr_exthost::language_features::uri::Uri;

fn lang(s: &str) -> S {
    S::Language(s.into())
}

fn f(language: Option<&str>, scheme: Option<&str>) -> LanguageFilter {
    LanguageFilter {
        // JS treats '' as unset.
        language: language.filter(|s| !s.is_empty()).map(Into::into),
        scheme: scheme.filter(|s| !s.is_empty()).map(Into::into),
        ..Default::default()
    }
}

fn filter(language: Option<&str>, scheme: Option<&str>) -> S {
    S::Filter(f(language, scheme))
}

fn sc(selector: &S, uri: &Uri, language: &str) -> u32 {
    score(Some(selector), uri, language, true, None, None)
}

fn model() -> (Uri, &'static str) {
    (Uri::parse("file:///testbed/file.fb"), "farboo")
}

// upstream: LanguageSelector > score, invalid selector
#[test]
fn score_invalid_selector() {
    let (uri, l) = model();
    assert_eq!(sc(&S::Filter(LanguageFilter::default()), &uri, l), 0);
    assert_eq!(score(None, &uri, l, true, None, None), 0);
    assert_eq!(sc(&lang(""), &uri, l), 0);
}

// upstream: LanguageSelector > score, any language
#[test]
fn score_any_language() {
    let (uri, l) = model();
    assert_eq!(sc(&filter(Some("*"), None), &uri, l), 5);
    assert_eq!(sc(&lang("*"), &uri, l), 5);
    assert_eq!(sc(&lang("*"), &Uri::parse("foo:bar"), l), 5);
    assert_eq!(sc(&lang("farboo"), &Uri::parse("foo:bar"), l), 10);
}

// upstream: LanguageSelector > score, default schemes
#[test]
fn score_default_schemes() {
    let uri = Uri::parse("git:foo/file.txt");
    let l = "farboo";
    assert_eq!(sc(&lang("*"), &uri, l), 5);
    assert_eq!(sc(&lang("farboo"), &uri, l), 10);
    // An empty scheme is "no scheme".
    assert_eq!(sc(&filter(Some("farboo"), Some("")), &uri, l), 10);
    assert_eq!(sc(&filter(Some("farboo"), Some("git")), &uri, l), 10);
    assert_eq!(sc(&filter(Some("farboo"), Some("*")), &uri, l), 10);
    assert_eq!(sc(&filter(Some("farboo"), None), &uri, l), 10);
    assert_eq!(sc(&filter(Some("*"), None), &uri, l), 5);

    assert_eq!(sc(&filter(None, Some("*")), &uri, l), 5);
    assert_eq!(sc(&filter(None, Some("git")), &uri, l), 10);
}

// upstream: LanguageSelector > score, filter
#[test]
fn score_filter() {
    let (uri, l) = model();
    assert_eq!(sc(&lang("farboo"), &uri, l), 10);
    assert_eq!(sc(&filter(Some("farboo"), None), &uri, l), 10);
    assert_eq!(sc(&filter(Some("farboo"), Some("file")), &uri, l), 10);
    assert_eq!(sc(&filter(Some("farboo"), Some("http")), &uri, l), 0);

    let pat = |p: &str, scheme: Option<&str>| {
        S::Filter(LanguageFilter {
            pattern: Some(Pattern::Glob(p.into())),
            scheme: scheme.map(Into::into),
            ..Default::default()
        })
    };
    assert_eq!(sc(&pat("**/*.fb", None), &uri, l), 10);
    assert_eq!(sc(&pat("**/*.fb", Some("file")), &uri, l), 10);
    assert_eq!(sc(&pat("**/*.fb", None), &Uri::parse("foo:bar"), l), 0);
    assert_eq!(
        sc(&pat("**/*.fb", Some("foo")), &Uri::parse("foo:bar"), l),
        0
    );

    let doc = Uri::parse("git:/my/file.js");
    let js = "javascript";
    assert_eq!(sc(&lang("javascript"), &doc, js), 10);
    assert_eq!(sc(&filter(Some("javascript"), Some("git")), &doc, js), 10);
    assert_eq!(sc(&lang("*"), &doc, js), 5);
    assert_eq!(sc(&lang("fooLang"), &doc, js), 0);
    assert_eq!(sc(&S::Many(vec![lang("fooLang"), lang("*")]), &doc, js), 5);
}

// upstream: LanguageSelector > score, max(filters)
#[test]
fn score_max_filters() {
    let (uri, l) = model();
    let m = || filter(Some("farboo"), Some("file"));
    let fail = || filter(Some("farboo"), Some("http"));
    assert_eq!(sc(&m(), &uri, l), 10);
    assert_eq!(sc(&fail(), &uri, l), 0);
    assert_eq!(sc(&S::Many(vec![m(), fail()]), &uri, l), 10);
    assert_eq!(sc(&S::Many(vec![fail(), fail()]), &uri, l), 0);
    assert_eq!(sc(&S::Many(vec![lang("farboo"), lang("*")]), &uri, l), 10);
    assert_eq!(sc(&S::Many(vec![lang("*"), lang("farboo")]), &uri, l), 10);
}

// upstream: LanguageSelector > score hasAccessToAllModels
#[test]
fn score_has_access_to_all_models() {
    let doc = Uri::parse("file:/my/file.js");
    let js = "javascript";
    let unsynced = |s: &S| score(Some(s), &doc, js, false, None, None);
    assert_eq!(unsynced(&lang("javascript")), 0);
    assert_eq!(unsynced(&filter(Some("javascript"), Some("file"))), 0);
    assert_eq!(unsynced(&lang("*")), 0);
    assert_eq!(unsynced(&lang("fooLang")), 0);
    assert_eq!(unsynced(&S::Many(vec![lang("fooLang"), lang("*")])), 0);

    let all = |language: &str, scheme: Option<&str>| {
        S::Filter(LanguageFilter {
            has_access_to_all_models: true,
            ..f(Some(language), scheme)
        })
    };
    assert_eq!(unsynced(&all("javascript", Some("file"))), 10);
    assert_eq!(
        unsynced(&S::Many(vec![lang("fooLang"), lang("*"), all("*", None)])),
        5
    );
}

// upstream: LanguageSelector > score, notebookType
#[test]
fn score_notebook_type() {
    let uri = Uri::parse("vscode-notebook-cell:///my/file.js#blabla");
    let l = "javascript";
    let nb_uri = Uri::parse("file:///my/file.js");
    let nb = |s: &S| score(Some(s), &uri, l, true, Some(&nb_uri), Some("fooBook"));
    let nbf = |notebook: &str, language: Option<&str>, scheme: Option<&str>| {
        S::Filter(LanguageFilter {
            notebook_type: Some(notebook.into()),
            ..f(language, scheme)
        })
    };
    assert_eq!(sc(&lang("javascript"), &uri, l), 10);
    assert_eq!(nb(&lang("javascript")), 10);
    assert_eq!(nb(&nbf("fooBook", None, None)), 10);
    assert_eq!(nb(&nbf("fooBook", Some("javascript"), Some("file"))), 10);
    assert_eq!(nb(&nbf("fooBook", Some("*"), None)), 10);
    assert_eq!(nb(&nbf("*", Some("*"), None)), 5);
    assert_eq!(nb(&nbf("*", Some("javascript"), None)), 10);
}

// upstream: LanguageSelector > Snippet choices lost #149363
#[test]
fn snippet_choices_lost_149363() {
    let selector = LanguageFilter {
        scheme: Some("vscode-notebook-cell".into()),
        pattern: Some(Pattern::Glob("/some/path/file.py".into())),
        language: Some("python".into()),
        ..Default::default()
    };
    let model_uri = Uri::parse("vscode-notebook-cell:///some/path/file.py");
    let nb_uri = Uri::parse("file:///some/path/file.py");
    assert_eq!(
        score(
            Some(&S::Filter(selector.clone())),
            &model_uri,
            "python",
            true,
            Some(&nb_uri),
            Some("jupyter")
        ),
        10
    );
    let selector2 = LanguageFilter {
        notebook_type: Some("jupyter".into()),
        ..selector
    };
    assert_eq!(
        score(
            Some(&S::Filter(selector2)),
            &model_uri,
            "python",
            true,
            Some(&nb_uri),
            Some("jupyter")
        ),
        0
    );
}

// upstream: LanguageSelector > Document selector match - unexpected result value #60232
#[test]
fn document_selector_match_unexpected_result_value_60232() {
    let selector = S::Filter(LanguageFilter {
        pattern: Some(Pattern::Glob("**/*.interface.json".into())),
        ..f(Some("json"), Some("file"))
    });
    let uri = Uri::parse("file:///C:/Users/zlhe/Desktop/test.interface.json");
    assert_eq!(sc(&selector, &uri, "json"), 10);
}

// upstream: LanguageSelector > Document selector match - platform paths #99938
#[test]
fn document_selector_match_platform_paths_99938() {
    let selector = S::Filter(LanguageFilter {
        pattern: Some(Pattern::Relative {
            base: "/home/user/Desktop".into(),
            pattern: "*.json".into(),
        }),
        ..Default::default()
    });
    let uri = Uri::file("/home/user/Desktop/test.json");
    assert_eq!(sc(&selector, &uri, "json"), 10);
}

// upstream: LanguageSelector > NotebookType without notebook
#[test]
fn notebook_type_without_notebook() {
    let uri = Uri::parse("file:///my/file.bat");
    let nbt = |t: &str| {
        S::Filter(LanguageFilter {
            notebook_type: Some(t.into()),
            ..LanguageFilter::language("bat")
        })
    };
    assert_eq!(sc(&nbt("xxx"), &uri, "bat"), 0);
    assert_eq!(sc(&nbt("*"), &uri, "bat"), 0);
}

// upstream: LanguageSelector > selectLanguageIds
#[test]
fn select_language_ids_cases() {
    let ids = |s: &S| {
        let mut r = BTreeSet::new();
        select_language_ids(s, &mut r);
        r.into_iter().collect::<Vec<_>>()
    };
    assert_eq!(ids(&lang("typescript")), ["typescript"]);
    assert_eq!(ids(&filter(Some("python"), Some("file"))), ["python"]);
    assert!(ids(&filter(None, Some("file"))).is_empty());
    assert_eq!(
        ids(&S::Many(vec![
            lang("javascript"),
            filter(Some("css"), None),
            filter(None, Some("untitled"))
        ])),
        ["css", "javascript"]
    );
    assert_eq!(ids(&lang("*")), ["*"]);
}
