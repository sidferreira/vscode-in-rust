//! Upstream source: src/vs/workbench/api/common/extHostTypeConverters.ts @ 50e1350fc78d
//!
//! DTO -> Rust conversion of provider results, from JSON as the host sends it (always run).

use serde_json::{Value, json};
use vsr_exthost::language_features::dto::*;
use vsr_exthost::language_features::selector::{LanguageFilter, LanguageSelector, Pattern};
use vsr_exthost::language_features::uri::Uri;

fn r(a: u32, b: u32, c: u32, d: u32) -> Value {
    json!({ "startLineNumber": a, "startColumn": b, "endLineNumber": c, "endColumn": d })
}

fn uri(path: &str) -> Value {
    json!({ "$mid": 1, "scheme": "file", "authority": "", "path": path })
}

#[test]
fn completion_list_with_default_ranges_and_single_letter_keys() {
    let list = CompletionList::from_json(&json!({
        "a": { "insert": r(1, 1, 1, 3), "replace": r(1, 1, 1, 6) },
        "b": [
            { "a": "plain", "h": "plain()", "i": 4, "x": [7, 0] },
            { "a": { "label": "fancy", "detail": " (d)", "description": "mod" }, "b": 2, "c": "det",
              "d": { "value": "**md**", "isTrusted": { "enabledCommands": ["x"] } }, "e": "zz", "f": "ff",
              "g": true, "j": r(2, 2, 2, 4), "k": ".;", "l": [{ "range": r(1, 1, 1, 1), "text": "import\n" }],
              "m": [1], "o": "cmd.id", "p": [1, "a"], "x": [7, 1] },
            { "a": "own-ranges", "j": { "insert": r(3, 1, 3, 2), "replace": r(3, 1, 3, 4) } },
            { "b": 1 }
        ],
        "c": true, "d": 12.5, "x": 7
    }))
    .unwrap();
    assert!(list.is_incomplete);
    assert_eq!(list.cache_id, Some(7));
    // The item without a label is skipped.
    assert_eq!(list.items.len(), 3);
    let plain = &list.items[0];
    assert_eq!(plain.insert_text, "plain()");
    assert!(plain.is_snippet());
    assert_eq!(plain.cache_id, Some((7, 0)));
    assert_eq!(
        plain.range,
        CompletionRange::InsertReplace {
            insert: Range::new(1, 1, 1, 3),
            replace: Range::new(1, 1, 1, 6)
        }
    );
    let fancy = &list.items[1];
    assert_eq!(fancy.insert_text, "fancy", "no insert text: the label");
    assert_eq!(fancy.label_detail.as_deref(), Some(" (d)"));
    assert_eq!(fancy.kind, Some(2));
    assert_eq!(completion_item_kind_name(2), "Constructor");
    assert_eq!(fancy.sort_text(), "zz");
    assert_eq!(fancy.filter_text(), "ff");
    assert!(fancy.preselect);
    assert_eq!(fancy.commit_characters, ['.', ';']);
    assert_eq!(fancy.range, CompletionRange::Single(Range::new(2, 2, 2, 4)));
    assert_eq!(fancy.additional_text_edits[0].text, "import\n");
    let doc = match fancy.documentation.as_ref().unwrap() {
        Documentation::Markdown(m) => m,
        other => panic!("{other:?}"),
    };
    assert!(doc.is_trusted);
    let cmd = fancy.command.as_ref().unwrap();
    assert_eq!((cmd.id.as_str(), cmd.arguments.len()), ("cmd.id", 2));
    assert!(matches!(
        list.items[2].range,
        CompletionRange::InsertReplace { .. }
    ));
    assert_eq!(completion_item_kind_name(99), "Unknown");
}

#[test]
fn an_item_without_any_range_is_dropped() {
    // No default ranges in the list and none on the item.
    let list = CompletionList::from_json(&json!({ "b": [{ "a": "x" }] })).unwrap();
    assert!(list.items.is_empty());
}

#[test]
fn symbols_hover_highlight_and_folding() {
    let s = DocumentSymbol::from_json(&json!({
        "name": "S", "detail": "d", "kind": 4, "tags": [1], "range": r(1, 1, 9, 1),
        "selectionRange": r(1, 7, 1, 8),
        "children": [{ "name": "m", "kind": 5, "range": r(2, 1, 3, 1), "selectionRange": r(2, 1, 2, 2) }, { "kind": 1 }]
    }))
    .unwrap();
    assert_eq!(symbol_kind_name(s.kind), "Class");
    assert_eq!(s.tags, [1]);
    assert_eq!(s.children.len(), 1);
    assert_eq!(s.flatten().len(), 2);

    let h = Hover::from_json(
        &json!({ "id": 3, "contents": [{ "value": "a" }, "b"], "range": r(1, 1, 1, 2) }),
    )
    .unwrap();
    assert_eq!(
        h.contents
            .iter()
            .map(|c| c.value.as_str())
            .collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(h.id, Some(3));
    assert!(Hover::from_json(&json!({ "contents": [] })).is_none());

    assert_eq!(
        DocumentHighlight::from_json(&json!({ "range": r(1, 1, 1, 2), "kind": 2 }))
            .unwrap()
            .kind,
        Some(2)
    );
    assert_eq!(
        FoldingRange::from_json(&json!({ "start": 1, "end": 4, "kind": { "value": "region" } }))
            .unwrap()
            .kind
            .as_deref(),
        Some("region")
    );
    assert_eq!(
        FoldingRange::from_json(&json!({ "start": 1, "end": 4 }))
            .unwrap()
            .kind,
        None
    );
}

#[test]
fn locations_links_and_edits() {
    let l = LocationLink::from_json(&json!({ "uri": uri("/a/b.fk"), "range": r(1, 1, 2, 1), "targetSelectionRange": r(1, 4, 1, 5) })).unwrap();
    assert_eq!(l.uri.path, "/a/b.fk");
    assert_eq!(l.origin_selection_range, None);
    assert_eq!(l.target_selection_range, Some(Range::new(1, 4, 1, 5)));
    assert!(LocationLink::from_json(&json!({ "range": r(1, 1, 2, 1) })).is_none());
    assert_eq!(
        Location::from_json(&json!({ "uri": uri("/x"), "range": r(1, 1, 1, 1) }))
            .unwrap()
            .range,
        Range::new(1, 1, 1, 1)
    );

    let edits = TextEdit::list_from_json(
        &json!([{ "range": r(1, 1, 1, 2), "text": "x", "eol": 1 }, { "text": "bad" }]),
    );
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].eol, Some(1));
    assert!(TextEdit::list_from_json(&Value::Null).is_empty());
}

#[test]
fn workspace_edits_keep_text_file_and_other_entries() {
    let e = WorkspaceEdit::from_json(&json!({ "edits": [
        { "resource": uri("/a"), "textEdit": { "range": r(1, 1, 1, 2), "text": "t" }, "versionId": 3 },
        { "oldResource": uri("/a"), "newResource": uri("/b") },
        { "newResource": uri("/c") },
        { "resource": uri("/nb"), "cellEdit": {} }
    ]}));
    assert_eq!(e.edits.len(), 4);
    assert!(matches!(
        &e.edits[0],
        WorkspaceEditEntry::Text {
            version_id: Some(3),
            ..
        }
    ));
    assert!(matches!(
        &e.edits[1],
        WorkspaceEditEntry::File {
            old: Some(_),
            new: Some(_)
        }
    ));
    assert!(matches!(
        &e.edits[2],
        WorkspaceEditEntry::File {
            old: None,
            new: Some(_)
        }
    ));
    assert!(matches!(&e.edits[3], WorkspaceEditEntry::Other(_)));
    let rename = RenameEdits::from_json(&json!({ "edits": [], "rejectReason": "no" })).unwrap();
    assert_eq!(rename.reject_reason.as_deref(), Some("no"));
}

#[test]
fn lenses_hints_links_signature_and_actions() {
    let lenses = CodeLensList::from_json(&json!({ "cacheId": 2, "lenses": [
        { "range": r(1, 1, 1, 2), "cacheId": [2, 0] },
        { "range": r(2, 1, 2, 2), "command": { "id": "c", "title": "t", "arguments": [1] } }] }))
    .unwrap();
    assert_eq!(lenses.lenses[0].cache_id, Some((2, 0)));
    assert!(lenses.lenses[0].command.is_none());
    assert_eq!(
        lenses.lenses[1].command.as_ref().unwrap().arguments,
        [json!(1)]
    );

    let hints = InlayHintList::from_json(&json!({ "cacheId": 1, "hints": [
        { "position": { "lineNumber": 1, "column": 5 }, "label": [{ "label": "a" }, { "label": ": ", "tooltip": { "value": "t" } }],
          "kind": 1, "paddingLeft": true, "cacheId": [1, 0] },
        { "position": { "lineNumber": 1, "column": 9 }, "label": "x" }] })).unwrap();
    assert_eq!(hints.hints[0].text(), "a: ");
    assert!(hints.hints[0].padding_left);
    assert_eq!(hints.hints[1].text(), "x");

    let links = DocumentLinkList::from_json(&json!({ "links": [
        { "range": r(1, 1, 1, 5), "url": "https://a/b" },
        { "range": r(2, 1, 2, 5), "url": { "scheme": "file", "authority": "", "path": "/f" } },
        { "range": r(3, 1, 3, 5) }] }))
    .unwrap();
    let urls: Vec<_> = links.links.iter().map(|l| l.url.as_deref()).collect();
    assert_eq!(urls, [Some("https://a/b"), Some("file:///f"), None]);

    let sig = SignatureHelp::from_json(&json!({ "id": 5, "activeSignature": 1, "activeParameter": 2, "signatures": [
        { "label": "f(a)", "parameters": [{ "label": [2, 3] }, { "label": "b", "documentation": "d" }], "activeParameter": 1 }] })).unwrap();
    assert_eq!(sig.active_signature, 1);
    assert_eq!(
        sig.signatures[0].parameters[0].label,
        ParameterLabel::Offsets(2, 3)
    );
    assert_eq!(sig.signatures[0].active_parameter, Some(1));

    let acts = CodeActionList::from_json(&json!({ "cacheId": 9, "actions": [
        { "title": "A", "kind": "quickfix", "isPreferred": true, "cacheId": [9, 0],
          "diagnostics": [{ "message": "m" }], "command": { "id": "c", "title": "t" } },
        { "title": "B", "disabled": "why" }, { "kind": "x" }] }))
    .unwrap();
    assert_eq!(acts.actions.len(), 2);
    assert!(acts.actions[0].is_preferred);
    assert_eq!(acts.actions[0].diagnostics.len(), 1);
    assert_eq!(acts.actions[1].disabled.as_deref(), Some("why"));
}

#[test]
fn positions_and_ranges() {
    let rg = Range::from_json(&r(2, 3, 4, 5)).unwrap();
    assert_eq!(Range::from_json(&rg.to_json()), Some(rg));
    assert!(rg.contains(Position::new(3, 1)));
    assert!(rg.contains(Position::new(4, 5)));
    assert!(!rg.contains(Position::new(4, 6)));
    assert_eq!(
        Position::from_json(&Position::new(1, 2).to_json()),
        Some(Position::new(1, 2))
    );
    assert!(Range::from_json(&json!({ "startLineNumber": 1 })).is_none());
}

#[test]
fn selectors_from_document_filter_dtos() {
    let s = LanguageSelector::from_dtos(&json!([
        { "$serialized": true, "language": "fk", "scheme": "file", "exclusive": true },
        { "$serialized": true, "pattern": "**/*.fk" },
        { "$serialized": true, "pattern": { "baseUri": uri("/ws"), "base": "/ws", "pattern": "src/*.fk" }, "isBuiltin": true },
        "plain"
    ]));
    let LanguageSelector::Many(items) = s else {
        panic!()
    };
    let f = |i: usize| match &items[i] {
        LanguageSelector::Filter(f) => f.clone(),
        other => panic!("{other:?}"),
    };
    assert_eq!(
        f(0),
        LanguageFilter {
            language: Some("fk".into()),
            scheme: Some("file".into()),
            exclusive: true,
            ..Default::default()
        }
    );
    assert_eq!(f(1).pattern, Some(Pattern::Glob("**/*.fk".into())));
    assert_eq!(
        f(2).pattern,
        Some(Pattern::Relative {
            base: "/ws".into(),
            pattern: "src/*.fk".into()
        })
    );
    assert!(f(2).is_builtin);
    assert_eq!(items[3], LanguageSelector::Language("plain".into()));
}

#[test]
fn uri_parse_fs_path_and_string() {
    let u = Uri::parse("file:///C:/Users/a/b.txt");
    assert_eq!(
        (u.scheme.as_str(), u.path.as_str()),
        ("file", "/C:/Users/a/b.txt")
    );
    assert_eq!(u.fs_path(), "c:/Users/a/b.txt");
    let u = Uri::parse("git:foo/file.txt?x#frag");
    assert_eq!(
        (
            u.authority.as_str(),
            u.path.as_str(),
            u.query.as_str(),
            u.fragment.as_str()
        ),
        ("", "foo/file.txt", "x", "frag")
    );
    assert_eq!(
        Uri::parse("https://host/p?q").to_uri_string(),
        "https://host/p?q"
    );
    assert_eq!(Uri::parse("file://srv/share/f").fs_path(), "//srv/share/f");
    assert_eq!(Uri::file("rel/x").path, "/rel/x");
    assert_eq!(
        Uri::from_components(&uri("/p")).unwrap().to_uri_string(),
        "file:///p"
    );
    assert!(Uri::from_components(&json!({ "path": "/x" })).is_none());
}
