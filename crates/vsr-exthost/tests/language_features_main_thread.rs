//! Upstream source: src/vs/workbench/api/browser/mainThreadLanguageFeatures.ts @ 50e1350fc78d
//!
//! `$register*Support` / `$unregister` as the host sends them, applied to `MainThread` (always
//! run, no host process).

use serde_json::{Value, json};
use vsr_exthost::language_features::main_thread::Kind;
use vsr_exthost::language_features::registry::ModelInfo;
use vsr_exthost::language_features::uri::Uri;
use vsr_exthost::main_thread::MainThread;
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::proxy_ids::main;
use vsr_rpc::rpc::Call;

fn send(mt: &mut MainThread, method: &str, args: Vec<Value>) -> Reply {
    mt.handle(&Call {
        req: 1,
        rpc_id: main::MAIN_THREAD_LANGUAGE_FEATURES,
        method: method.into(),
        args: args.into_iter().map(Arg::Json).collect(),
        cancellable: false,
    })
    .unwrap()
}

fn sel(language: &str) -> Value {
    json!([{ "$serialized": true, "language": language, "isBuiltin": false }])
}

fn model(lang: &str) -> ModelInfo {
    ModelInfo::new(Uri::parse("file:///w/a.fb"), lang)
}

#[test]
fn registers_with_metadata_and_unregisters() {
    let mut mt = MainThread::new();
    send(
        &mut mt,
        "$registerCompletionsProvider",
        vec![
            json!(3),
            sel("fb"),
            json!([".", ":"]),
            json!(true),
            json!({ "value": "pub.ext", "_lower": "pub.ext" }),
        ],
    );
    send(&mut mt, "$registerHoverProvider", vec![json!(4), sel("fb")]);
    let lf = &mut mt.language_features;
    assert_eq!(lf.ordered(Kind::Completion, &model("fb")), [3]);
    assert_eq!(lf.ordered(Kind::Hover, &model("fb")), [4]);
    assert!(lf.ordered(Kind::Hover, &model("other")).is_empty());
    let r = lf.registration(3).unwrap();
    assert_eq!(r.meta_strings("triggerCharacters"), [".", ":"]);
    assert!(r.meta_bool("supportsResolveDetails"));
    assert_eq!(r.extension_id(), "pub.ext");
    assert_eq!(lf.registered_language_ids(Kind::Completion).len(), 1);

    send(&mut mt, "$unregister", vec![json!(3)]);
    let lf = &mut mt.language_features;
    assert!(lf.ordered(Kind::Completion, &model("fb")).is_empty());
    assert!(lf.registration(3).is_none());
    assert_eq!(lf.all(Kind::Hover), [4]);
    assert!(mt.unhandled.is_empty());
}

#[test]
fn every_register_method_lands_in_its_registry() {
    let cases: &[(&str, Kind, Vec<Value>)] = &[
        (
            "$registerDocumentSymbolProvider",
            Kind::DocumentSymbol,
            vec![json!("Fb")],
        ),
        ("$registerCodeLensSupport", Kind::CodeLens, vec![json!(7)]),
        ("$registerDefinitionSupport", Kind::Definition, vec![]),
        ("$registerDeclarationSupport", Kind::Declaration, vec![]),
        (
            "$registerImplementationSupport",
            Kind::Implementation,
            vec![],
        ),
        (
            "$registerTypeDefinitionSupport",
            Kind::TypeDefinition,
            vec![],
        ),
        (
            "$registerDocumentHighlightProvider",
            Kind::DocumentHighlight,
            vec![],
        ),
        ("$registerReferenceSupport", Kind::Reference, vec![]),
        (
            "$registerCodeActionSupport",
            Kind::CodeAction,
            vec![json!({}), json!("Fb"), json!("a.b"), json!(true)],
        ),
        (
            "$registerDocumentFormattingSupport",
            Kind::DocumentFormatting,
            vec![json!({"value":"a.b"}), json!("Fb")],
        ),
        (
            "$registerRangeFormattingSupport",
            Kind::RangeFormatting,
            vec![json!({"value":"a.b"}), json!("Fb"), json!(true)],
        ),
        (
            "$registerOnTypeFormattingSupport",
            Kind::OnTypeFormatting,
            vec![json!([";"]), json!({"value":"a.b"})],
        ),
        ("$registerRenameSupport", Kind::Rename, vec![json!(true)]),
        (
            "$registerSignatureHelpProvider",
            Kind::SignatureHelp,
            vec![json!({"triggerCharacters":["("],"retriggerCharacters":[","]})],
        ),
        (
            "$registerInlayHintsProvider",
            Kind::InlayHints,
            vec![json!(true), json!(9), json!("Fb")],
        ),
        (
            "$registerDocumentLinkProvider",
            Kind::DocumentLink,
            vec![json!(true)],
        ),
        (
            "$registerFoldingRangeProvider",
            Kind::FoldingRange,
            vec![json!({"value":"a.b"}), Value::Null],
        ),
        (
            "$registerSelectionRangeProvider",
            Kind::SelectionRange,
            vec![],
        ),
        (
            "$registerDocumentColorProvider",
            Kind::DocumentColor,
            vec![],
        ),
    ];
    let mut mt = MainThread::new();
    for (i, (method, kind, rest)) in cases.iter().enumerate() {
        let mut args = vec![json!(i), sel("fb")];
        args.extend(rest.clone());
        send(&mut mt, method, args);
        assert_eq!(
            mt.language_features.ordered(*kind, &model("fb")),
            [i as u32],
            "{method}"
        );
    }
    assert!(mt.unhandled.is_empty(), "{:?}", mt.unhandled);
}

#[test]
fn events_are_attributed_to_their_provider_and_navigate_type_has_no_selector() {
    let mut mt = MainThread::new();
    send(
        &mut mt,
        "$registerCodeLensSupport",
        vec![json!(1), sel("fb"), json!(42)],
    );
    send(&mut mt, "$emitCodeLensEvent", vec![json!(42)]);
    send(&mut mt, "$emitCodeLensEvent", vec![json!(43)]);
    let lf = &mt.language_features;
    assert_eq!(lf.events.len(), 2);
    assert_eq!(lf.events[0].handle, Some(1));
    assert_eq!(lf.events[1].handle, None);

    send(
        &mut mt,
        "$registerNavigateTypeSupport",
        vec![json!(5), json!(true)],
    );
    let lf = &mut mt.language_features;
    assert_eq!(lf.all(Kind::NavigateType), [5]);
    assert!(lf.ordered(Kind::NavigateType, &model("fb")).is_empty());
}

#[test]
fn reregistering_a_handle_replaces_it_and_language_configuration_is_kept() {
    let mut mt = MainThread::new();
    send(&mut mt, "$registerHoverProvider", vec![json!(1), sel("a")]);
    send(&mut mt, "$registerHoverProvider", vec![json!(1), sel("b")]);
    assert_eq!(mt.language_features.all(Kind::Hover), [1]);
    assert!(
        mt.language_features
            .ordered(Kind::Hover, &model("a"))
            .is_empty()
    );
    assert_eq!(mt.language_features.ordered(Kind::Hover, &model("b")), [1]);

    send(
        &mut mt,
        "$setLanguageConfiguration",
        vec![
            json!(2),
            json!("b"),
            json!({"comments":{"lineComment":"//"}}),
        ],
    );
    assert_eq!(mt.language_features.language_configurations[&2].0, "b");
    send(&mut mt, "$unregister", vec![json!(2)]);
    assert!(mt.language_features.language_configurations.is_empty());
}
