//! Upstream source: src/vs/editor/common/languageFeatureRegistry.ts @ 50e1350fc78d
//!
//! `LanguageFeatureRegistry` behaviour (upstream has no test file of its own for it).

use vsr_exthost::language_features::registry::{LanguageFeatureRegistry, ModelInfo, NotebookInfo};
use vsr_exthost::language_features::selector::{LanguageFilter, LanguageSelector as S};
use vsr_exthost::language_features::uri::Uri;

fn model(lang: &str) -> ModelInfo {
    ModelInfo::new(Uri::parse("file:///w/a.fb"), lang)
}

fn filt(f: impl FnOnce(&mut LanguageFilter)) -> S {
    let mut x = LanguageFilter::default();
    f(&mut x);
    S::Filter(x)
}

#[test]
fn ordered_by_score_then_newest_first() {
    let mut r = LanguageFeatureRegistry::<&str>::new();
    r.register("*".into(), "wild");
    r.register("fb".into(), "old");
    r.register("fb".into(), "new");
    r.register("other".into(), "never");
    assert_eq!(r.ordered(&model("fb"), false), ["new", "old", "wild"]);
    assert_eq!(r.all(&model("fb")), ["new", "old", "wild"]);
    assert_eq!(r.all_no_model().len(), 4);
}

#[test]
fn ordered_groups_bucket_equal_scores() {
    let mut r = LanguageFeatureRegistry::<&str>::new();
    r.register("fb".into(), "a");
    r.register("*".into(), "b");
    r.register("fb".into(), "c");
    assert_eq!(r.ordered_groups(&model("fb")), [vec!["c", "a"], vec!["b"]]);
}

#[test]
fn builtin_providers_come_after_user_ones_of_equal_score() {
    let mut r = LanguageFeatureRegistry::<&str>::new();
    r.register(
        filt(|f| {
            f.language = Some("fb".into());
            f.is_builtin = true;
        }),
        "builtin-old",
    );
    r.register("fb".into(), "user");
    r.register(
        filt(|f| {
            f.language = Some("fb".into());
            f.is_builtin = true;
        }),
        "builtin-new",
    );
    assert_eq!(
        r.ordered(&model("fb"), false),
        ["user", "builtin-new", "builtin-old"]
    );
}

#[test]
fn an_exclusive_selector_overrides_the_rest() {
    let mut r = LanguageFeatureRegistry::<&str>::new();
    r.register("fb".into(), "plain");
    r.register(
        filt(|f| {
            f.language = Some("fb".into());
            f.exclusive = true;
        }),
        "exclusive",
    );
    assert_eq!(r.ordered(&model("fb"), false), ["exclusive"]);
    // A recursive query leaves exclusive providers out (they were asked first).
    assert_eq!(r.ordered(&model("fb"), true), ["plain"]);
    // An exclusive selector that does not match does not hide anything.
    assert_eq!(r.ordered(&model("zz"), false), Vec::<&str>::new());
}

#[test]
fn unregister_removes_and_rescores() {
    let mut r = LanguageFeatureRegistry::<u32>::new();
    let a = r.register("fb".into(), 1);
    let b = r.register("fb".into(), 2);
    let v = r.version();
    assert_eq!(r.ordered(&model("fb"), false), [2, 1]);
    assert!(r.unregister(b));
    assert!(!r.unregister(b));
    assert!(r.version() > v);
    // The cached scores are dropped with the registration.
    assert_eq!(r.ordered(&model("fb"), false), [1]);
    assert!(r.unregister(a));
    assert!(r.is_empty());
    assert!(!r.has(&model("fb")));
}

#[test]
fn the_cache_follows_the_candidate() {
    let mut r = LanguageFeatureRegistry::<&str>::new();
    r.register("fb".into(), "fb");
    r.register("py".into(), "py");
    assert_eq!(r.ordered(&model("fb"), false), ["fb"]);
    assert_eq!(r.ordered(&model("py"), false), ["py"]);
    assert_eq!(r.ordered(&model("fb"), false), ["fb"]);
    // A register after a query is seen by the next one.
    r.register("fb".into(), "later");
    assert_eq!(r.ordered(&model("fb"), false), ["later", "fb"]);
}

#[test]
fn unsynchronized_models_only_match_filters_with_access_to_all() {
    let mut r = LanguageFeatureRegistry::<&str>::new();
    r.register("fb".into(), "string");
    r.register(
        filt(|f| {
            f.language = Some("fb".into());
            f.has_access_to_all_models = true;
        }),
        "all",
    );
    let mut m = model("fb");
    m.synchronized = false;
    assert_eq!(r.ordered(&m, false), ["all"]);
}

#[test]
fn registered_language_ids_collects_selectors() {
    let mut r = LanguageFeatureRegistry::<()>::new();
    r.register(
        S::Many(vec!["a".into(), filt(|f| f.language = Some("b".into()))]),
        (),
    );
    r.register(filt(|f| f.scheme = Some("file".into())), ());
    let ids: Vec<_> = r.registered_language_ids().into_iter().collect();
    assert_eq!(ids, ["a", "b"]);
}

#[test]
fn notebook_resolver_supplies_the_notebook_uri() {
    let mut r = LanguageFeatureRegistry::<&str>::with_notebook_resolver(Box::new(|u: &Uri| {
        (u.scheme == "vscode-notebook-cell").then(|| NotebookInfo {
            uri: Uri::parse("file:///w/book.ipynb"),
            notebook_type: "jupyter".into(),
        })
    }));
    r.register(
        filt(|f| {
            f.notebook_type = Some("jupyter".into());
            f.scheme = Some("file".into());
        }),
        "nb",
    );
    let cell = ModelInfo::new(
        Uri::parse("vscode-notebook-cell:///w/book.ipynb#c1"),
        "python",
    );
    assert_eq!(r.ordered(&cell, false), ["nb"]);
    assert!(r.ordered(&model("python"), false).is_empty());
}
