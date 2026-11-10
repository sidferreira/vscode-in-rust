//! upstream: src/vs/workbench/api/browser/mainThreadLanguageFeatures.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostLanguageFeatures.ts @ 50e1350fc78d
//!
//! Every provider kind round-tripped through upstream's real extension host: a fixture extension
//! (`fixtures/lang-features-fixture`, plain JS, written in this repo) registers providers for a
//! fake language, the main side queries them. Needs Node and a Code-OSS build (`RSCODE_CODE_OSS`);
//! otherwise each test prints why and passes.

mod common;

use common::T;
use serde_json::json;
use std::path::PathBuf;
use vsr_exthost::description::ExtensionDescription;
use vsr_exthost::host::{BootParams, ExtensionHost};
use vsr_exthost::language_features::dto::*;
use vsr_exthost::language_features::main_thread::Kind;
use vsr_exthost::language_features::query::{
    CodeActionTrigger, CompletionTrigger, SignatureTrigger,
};
use vsr_rpc::handshake::Launch;

const TEXT: &str =
    "fn alpha(a, b) {\n\talpha(1, 2) \n}\nfn beta() {}\n// see https://example.com/x\nstruct Point";

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/lang-features-fixture")
}

struct Fixture {
    host: ExtensionHost,
    file: PathBuf,
}

/// Boot a host with the fixture extension and an open `a.fk` document, or print why not.
fn boot(name: &str) -> Option<Fixture> {
    let launch = match Launch::from_env() {
        Ok(l) => l,
        Err(why) => {
            eprintln!("SKIP {name}: {why}");
            return None;
        }
    };
    let base = common::scratch(name);
    let workspace = base.join("ws");
    std::fs::create_dir_all(&workspace).unwrap();
    let mut host = ExtensionHost::boot(&BootParams {
        launch,
        version: "1.147.0".into(),
        storage_root: base.join("storage"),
        workspace: Some(("ws".into(), workspace.clone())),
        extensions: vec![ExtensionDescription::from_dir(&fixture_dir()).unwrap()],
        default_settings: json!({}),
        user_settings: json!({}),
        env: vec![],
    })
    .unwrap_or_else(|e| panic!("boot: {e}"));
    host.wait_started(T)
        .unwrap_or_else(|e| panic!("start: {e}"));
    assert!(
        host.main_thread.extensions.activation_errors.is_empty(),
        "{:?}",
        host.main_thread.extensions.activation_errors
    );
    let file = workspace.join("a.fk");
    host.open_document(&file, TEXT, "fakelang", T).unwrap();
    // The registrations arrive as the extension activates; wait for the last one.
    common::wait_until(&mut host, |h| {
        h.main_thread.language_features.registrations.len() >= 22
    });
    Some(Fixture { host, file })
}

fn p(line: u32, col: u32) -> Position {
    Position::new(line, col)
}

#[test]
fn registrations_match_the_documents_language_and_scheme() {
    let Some(mut f) = boot("lang-registrations") else {
        return;
    };
    let lf = &mut f.host.main_thread.language_features;
    // Three hover providers, only the one for fakelang + file applies.
    assert_eq!(lf.all(Kind::Hover).len(), 3);
    let doc = f
        .host
        .main_thread
        .documents
        .open
        .values()
        .next()
        .unwrap()
        .clone();
    let model = vsr_exthost::language_features::registry::ModelInfo::new(
        vsr_exthost::language_features::uri::Uri::from_components(&doc.uri).unwrap(),
        &doc.language,
    );
    let lf = &mut f.host.main_thread.language_features;
    assert_eq!(lf.ordered(Kind::Hover, &model).len(), 1);
    // The failing '*' definition provider ranks after the fakelang one.
    let defs = lf.ordered(Kind::Definition, &model);
    assert_eq!(defs.len(), 2);
    assert_eq!(lf.registration(defs[0]).unwrap().meta, json!({}));
    let c = lf.all(Kind::Completion)[0];
    let reg = lf.registration(c).unwrap();
    assert_eq!(reg.meta_strings("triggerCharacters"), ["."]);
    assert!(reg.meta_bool("supportsResolveDetails"));
    assert_eq!(reg.extension_id(), "rscode-test.lang-features-fixture");
    let sig = lf.all(Kind::SignatureHelp)[0];
    assert_eq!(
        lf.registration(sig).unwrap().meta["metadata"]["triggerCharacters"],
        json!(["(", ","])
    );
    let fmt = lf.all(Kind::DocumentFormatting)[0];
    assert_eq!(
        lf.registration(fmt).unwrap().extension_id(),
        "rscode-test.lang-features-fixture"
    );
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn completion_items_and_resolve() {
    let Some(mut f) = boot("lang-completion") else {
        return;
    };
    let got = f
        .host
        .provide_completion_items(&f.file, p(2, 5), CompletionTrigger::Invoke, T)
        .unwrap();
    assert!(got.errors.is_empty(), "{:?}", got.errors);
    let (handle, list) = got.results.into_iter().next().unwrap();
    assert!(list.is_incomplete);
    assert!(list.cache_id.is_some());
    let labels: Vec<_> = list.items.iter().map(|i| i.label.as_str()).collect();
    assert_eq!(labels, ["alpha", "beta", "trigger:"]);

    let alpha = &list.items[0];
    assert_eq!(completion_item_kind_name(alpha.kind.unwrap()), "Function");
    assert!(alpha.is_snippet());
    assert_eq!(alpha.insert_text, "alpha(${1:a}, ${2:b})");
    assert_eq!(alpha.sort_text(), "0");
    assert_eq!(alpha.commit_characters, ['(']);
    // No range of its own: the list's default (insert and replace the word before the cursor).
    assert!(matches!(alpha.range, CompletionRange::InsertReplace { .. }));

    let beta = &list.items[1];
    assert_eq!(beta.label_detail.as_deref(), Some(" (fn)"));
    assert_eq!(beta.label_description.as_deref(), Some("fixture"));
    assert_eq!(beta.filter_text(), "bet");
    assert_eq!(beta.insert_text, "beta");
    assert!(!beta.is_snippet());
    assert_eq!(beta.range, CompletionRange::Single(Range::new(2, 1, 2, 5)));
    assert_eq!(beta.additional_text_edits.len(), 1);
    assert_eq!(beta.additional_text_edits[0].text, "// imported\n");
    let cmd = beta.command.as_ref().unwrap();
    // With arguments the command stays in the host (see the code lens test).
    assert!(
        cmd.ident
            .as_deref()
            .is_some_and(|i| i.starts_with("fixture.noop"))
    );

    // Resolve fills in detail and documentation (and keeps the rest).
    assert_eq!(alpha.detail, None);
    let resolved = f.host.resolve_completion_item(handle, alpha, T).unwrap();
    assert_eq!(resolved.detail.as_deref(), Some("resolved: alpha"));
    assert!(
        resolved
            .documentation
            .as_ref()
            .unwrap()
            .as_str()
            .contains("**docs**")
    );
    assert_eq!(resolved.insert_text, alpha.insert_text);
    assert_eq!(resolved.label, "alpha");

    f.host
        .release(Kind::Completion, handle, list.cache_id.unwrap());
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn completion_trigger_characters() {
    let Some(mut f) = boot("lang-completion-trigger") else {
        return;
    };
    let got = f
        .host
        .provide_completion_items(&f.file, p(2, 5), CompletionTrigger::Character('.'), T)
        .unwrap();
    let list = &got.results[0].1;
    assert_eq!(list.items[2].label, "trigger:.");
    // Nobody registered ';': nobody is asked.
    let got = f
        .host
        .provide_completion_items(&f.file, p(2, 5), CompletionTrigger::Character(';'), T)
        .unwrap();
    assert!(got.results.is_empty() && got.errors.is_empty());
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn hover_and_a_query_on_a_document_that_is_not_open() {
    let Some(mut f) = boot("lang-hover") else {
        return;
    };
    let got = f.host.provide_hover(&f.file, p(1, 5), T).unwrap();
    assert_eq!(got.results.len(), 1, "{got:?}");
    let hover = &got.results[0].1;
    assert_eq!(hover.contents[0].value, "hover for `alpha`");
    assert_eq!(hover.range, Some(Range::new(1, 4, 1, 9)));
    assert!(hover.id.is_some());
    f.host
        .release(Kind::Hover, got.results[0].0, hover.id.unwrap());
    // On the leading tab there is no word: the provider answers undefined.
    assert!(
        f.host
            .provide_hover(&f.file, p(2, 1), T)
            .unwrap()
            .results
            .is_empty()
    );
    let other = f.file.with_file_name("not-open.fk");
    assert!(f.host.provide_hover(&other, p(1, 1), T).is_err());
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn symbols_definitions_references_and_highlights() {
    let Some(mut f) = boot("lang-navigation") else {
        return;
    };

    let syms = f
        .host
        .provide_document_symbols(&f.file, T)
        .unwrap()
        .flatten();
    let names: Vec<_> = syms
        .iter()
        .map(|s| (s.name.as_str(), symbol_kind_name(s.kind)))
        .collect();
    assert_eq!(
        names,
        [
            ("alpha", "Function"),
            ("beta", "Function"),
            ("Point", "Struct")
        ]
    );
    assert_eq!(syms[0].range, Range::new(1, 1, 1, 17));
    assert_eq!(syms[0].selection_range, Range::new(1, 4, 1, 9));
    assert_eq!(syms[2].children.len(), 1);
    assert_eq!(syms[2].flatten().len(), 2);
    assert_eq!(syms[2].children[0].name, "x");

    // Definition: one provider answers, the other throws; the answer survives.
    let got = f.host.provide_definition(&f.file, p(2, 4), T).unwrap();
    assert_eq!(got.results.len(), 1, "{got:?}");
    assert_eq!(got.errors.len(), 1);
    assert!(got.errors[0].1.contains("boom"), "{:?}", got.errors);
    let link = &got.results[0].1[0];
    assert_eq!(link.origin_selection_range, Some(Range::new(2, 2, 2, 7)));
    assert_eq!(link.target_selection_range, Some(Range::new(1, 4, 1, 9)));
    assert_eq!(link.uri.path, f.file.to_string_lossy());
    // No `fn zzz`: nothing to go to.
    assert!(
        f.host
            .provide_definition(&f.file, p(3, 4), T)
            .unwrap()
            .flatten()
            .is_empty()
    );

    let decl = f
        .host
        .provide_declaration(&f.file, p(2, 4), T)
        .unwrap()
        .flatten();
    assert_eq!(decl[0].range, Range::new(1, 1, 1, 1));
    let imp = f
        .host
        .provide_implementation(&f.file, p(2, 4), T)
        .unwrap()
        .flatten();
    assert_eq!(imp[0].range.start_line, 4);
    let ty = f
        .host
        .provide_type_definition(&f.file, p(2, 4), T)
        .unwrap()
        .flatten();
    assert_eq!(ty[0].range.start_line, 6);

    let refs = f
        .host
        .provide_references(&f.file, p(1, 5), true, T)
        .unwrap()
        .flatten();
    assert_eq!(
        refs.iter().map(|l| l.range).collect::<Vec<_>>(),
        [Range::new(1, 4, 1, 9), Range::new(2, 2, 2, 7)]
    );
    let refs = f
        .host
        .provide_references(&f.file, p(1, 5), false, T)
        .unwrap()
        .flatten();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].range.start_line, 2);

    let hl = f
        .host
        .provide_document_highlights(&f.file, p(2, 4), T)
        .unwrap()
        .flatten();
    assert_eq!(
        hl.iter().map(|h| h.kind).collect::<Vec<_>>(),
        [Some(2), Some(1)]
    );
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn formatting_document_range_and_on_type() {
    let Some(mut f) = boot("lang-formatting") else {
        return;
    };
    let spaces = FormattingOptions {
        tab_size: 4,
        insert_spaces: true,
    };
    let got = f
        .host
        .provide_document_formatting_edits(&f.file, &spaces, T)
        .unwrap();
    assert!(got.errors.is_empty(), "{:?}", got.errors);
    let edits = got.flatten();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].range, Range::new(2, 1, 2, 14));
    assert_eq!(edits[0].text, "    alpha(1, 2)");
    // The options reach the provider.
    let tabs = FormattingOptions {
        tab_size: 4,
        insert_spaces: false,
    };
    let edits = f
        .host
        .provide_document_formatting_edits(&f.file, &tabs, T)
        .unwrap()
        .flatten();
    assert_eq!(edits[0].text, "\talpha(1, 2)");

    let r = f
        .host
        .provide_range_formatting_edits(&f.file, Range::new(1, 1, 1, 3), &spaces, T)
        .unwrap()
        .flatten();
    assert_eq!(r[0].text, "FN");
    assert_eq!(r[0].range, Range::new(1, 1, 1, 3));

    let on_type = f
        .host
        .provide_on_type_formatting_edits(&f.file, p(1, 5), ';', &spaces, T)
        .unwrap()
        .flatten();
    assert_eq!(on_type[0].text, "/*;*/");
    // Only the registered trigger character asks anyone.
    let none = f
        .host
        .provide_on_type_formatting_edits(&f.file, p(1, 5), 'x', &spaces, T)
        .unwrap();
    assert!(none.results.is_empty());
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn code_lens_inlay_hints_folding_and_links() {
    let Some(mut f) = boot("lang-decorations") else {
        return;
    };

    let got = f.host.provide_code_lenses(&f.file, T).unwrap();
    let (handle, list) = &got.results[0];
    assert_eq!(list.lenses.len(), 2);
    assert_eq!(list.lenses[0].range, Range::new(1, 1, 1, 3));
    assert!(list.lenses[0].command.is_none());
    let resolved = f
        .host
        .resolve_code_lens(*handle, &list.lenses[1], T)
        .unwrap();
    let cmd = resolved.command.unwrap();
    assert_eq!(cmd.title, "lens@3");
    // A command with arguments stays in the host: the lens carries a delegating command and the
    // key of the real one (`$ident`), to be run through `$executeContributedCommand`.
    let ident = cmd.ident.unwrap();
    assert!(ident.starts_with("fixture.noop"), "{ident}");
    assert_eq!(cmd.arguments, [json!(ident)]);
    f.host
        .release(Kind::CodeLens, *handle, list.cache_id.unwrap());

    let got = f
        .host
        .provide_inlay_hints(&f.file, Range::new(1, 1, 6, 20), T)
        .unwrap();
    let (handle, list) = &got.results[0];
    assert_eq!(list.hints.len(), 1);
    let hint = &list.hints[0];
    assert_eq!(hint.position, p(2, 8));
    assert_eq!(hint.text(), "a:");
    assert_eq!(hint.kind, Some(2));
    assert!(hint.padding_right && !hint.padding_left);
    assert!(hint.tooltip.is_none());
    let resolved = f.host.resolve_inlay_hint(*handle, hint, T).unwrap();
    assert_eq!(
        resolved.tooltip,
        Some(Documentation::Text("resolved hint".into()))
    );
    // A range with nothing in it.
    let got = f
        .host
        .provide_inlay_hints(&f.file, Range::new(4, 1, 6, 1), T)
        .unwrap();
    assert!(got.results.is_empty() || got.results[0].1.hints.is_empty());

    let ranges = f.host.provide_folding_ranges(&f.file, T).unwrap().flatten();
    assert_eq!(
        ranges,
        [
            FoldingRange {
                start: 1,
                end: 3,
                kind: Some("region".into())
            },
            FoldingRange {
                start: 5,
                end: 6,
                kind: Some("comment".into())
            },
        ]
    );

    let got = f.host.provide_document_links(&f.file, T).unwrap();
    let (handle, links) = &got.results[0];
    assert_eq!(links.links.len(), 2);
    let web = links.links.iter().find(|l| l.url.is_some()).unwrap();
    assert_eq!(web.url.as_deref(), Some("https://example.com/x"));
    assert_eq!(web.range.start_line, 5);
    let lazy = links.links.iter().find(|l| l.url.is_none()).unwrap();
    let resolved = f.host.resolve_document_link(*handle, lazy, T).unwrap();
    assert_eq!(
        resolved.url.as_deref(),
        Some("https://example.com/resolved")
    );
    assert_eq!(resolved.tooltip.as_deref(), Some("resolved link"));
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn signature_help_code_actions_and_rename() {
    let Some(mut f) = boot("lang-refactor") else {
        return;
    };

    let got = f
        .host
        .provide_signature_help(&f.file, p(2, 8), SignatureTrigger::Invoke, None, false, T)
        .unwrap();
    let help = got.first().unwrap();
    assert_eq!(help.signatures.len(), 1);
    assert_eq!(help.active_parameter, 0);
    let sig = &help.signatures[0];
    assert_eq!(sig.label, "alpha(a, b)");
    assert_eq!(sig.documentation.as_ref().unwrap().as_str(), "Calls alpha");
    assert_eq!(sig.parameters[0].label, ParameterLabel::Text("a".into()));
    assert_eq!(sig.parameters[1].label, ParameterLabel::Offsets(9, 10));
    let got = f
        .host
        .provide_signature_help(
            &f.file,
            p(2, 11),
            SignatureTrigger::Character,
            Some(','),
            false,
            T,
        )
        .unwrap();
    assert_eq!(got.first().unwrap().active_parameter, 1);
    f.host.release(
        Kind::SignatureHelp,
        got.results[0].0,
        got.first().unwrap().id.unwrap(),
    );

    let got = f
        .host
        .provide_code_actions(
            &f.file,
            Range::new(1, 1, 1, 1),
            None,
            CodeActionTrigger::Invoke,
            T,
        )
        .unwrap();
    assert!(got.errors.is_empty(), "{:?}", got.errors);
    let (handle, list) = &got.results[0];
    let titles: Vec<_> = list.actions.iter().map(|a| a.title.as_str()).collect();
    assert_eq!(titles, ["Fix it", "Extract lazily", "Not now"]);
    let fix = &list.actions[0];
    assert_eq!(fix.kind.as_deref(), Some("quickfix"));
    assert!(fix.is_preferred);
    match &fix.edit.as_ref().unwrap().edits[0] {
        WorkspaceEditEntry::Text {
            resource,
            edit,
            version_id,
        } => {
            assert_eq!(resource.path, f.file.to_string_lossy());
            assert_eq!(edit.text, "omega");
            assert_eq!(edit.range, Range::new(1, 4, 1, 9));
            // The host only reports a version when the edit asked for one.
            assert_eq!(*version_id, None);
        }
        other => panic!("{other:?}"),
    }
    let lazy = &list.actions[1];
    assert_eq!(lazy.kind.as_deref(), Some("refactor.extract"));
    assert!(lazy.edit.is_none());
    assert_eq!(
        list.actions[2].disabled.as_deref(),
        Some("nothing selected")
    );
    let resolved = f.host.resolve_code_action(*handle, lazy, T).unwrap();
    match &resolved.edit.unwrap().edits[0] {
        WorkspaceEditEntry::Text { resource, edit, .. } => {
            assert_eq!(resource.path, "/elsewhere.fk");
            assert_eq!(edit.text, "lazy edit");
        }
        other => panic!("{other:?}"),
    }
    f.host
        .release(Kind::CodeAction, *handle, list.cache_id.unwrap());

    let loc = f.host.resolve_rename_location(&f.file, p(1, 5), T).unwrap();
    let loc = loc.first().unwrap();
    assert_eq!(
        (loc.range, loc.text.as_str()),
        (Range::new(1, 4, 1, 9), "alpha")
    );
    let got = f
        .host
        .provide_rename_edits(&f.file, p(1, 5), "omega", T)
        .unwrap();
    let edits = &got.first().unwrap().edit.edits;
    assert_eq!(edits.len(), 2);
    assert!(
        edits
            .iter()
            .all(|e| matches!(e, WorkspaceEditEntry::Text { edit, .. } if edit.text == "omega"))
    );
    assert!(got.first().unwrap().reject_reason.is_none());
    f.host.shutdown(std::time::Duration::from_secs(5));
}

#[test]
fn a_polled_query_does_not_block_and_can_be_cancelled() {
    let Some(mut f) = boot("lang-poll") else {
        return;
    };
    let mut q = f
        .host
        .begin_provide(
            Kind::DocumentSymbol,
            &f.file,
            "$provideDocumentSymbols",
            &[],
            Box::new(|v| Some(v.as_array()?.len())),
        )
        .unwrap();
    assert_eq!(q.handles().len(), 1);
    let deadline = std::time::Instant::now() + T;
    while !f.host.poll_query(&mut q) {
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(q.take().results[0].1, 3);

    let mut q = f
        .host
        .begin_provide(
            Kind::Hover,
            &f.file,
            "$provideHover",
            &[p(1, 5).to_json(), serde_json::Value::Null],
            Box::new(|_| Some(())),
        )
        .unwrap();
    f.host.cancel_query(&mut q);
    assert!(q.is_finished());
    f.host.shutdown(std::time::Duration::from_secs(5));
}
