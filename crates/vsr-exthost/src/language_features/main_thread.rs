//! upstream: src/vs/workbench/api/browser/mainThreadLanguageFeatures.ts @ 50e1350fc78d
//!
//! `MainThreadLanguageFeatures`: the host's `$register*Support` calls put a provider (a handle
//! the host knows it by, a selector, and per-kind metadata) into one [`LanguageFeatureRegistry`]
//! per [`Kind`]; `$unregister(handle)` removes it. Upstream wraps each handle in an object that
//! calls back through `$provide*`; here the handle itself is the registry's value and
//! [`crate::language_features::query`] makes the calls.
//!
//! The metadata keeps what the main side needs later (trigger characters, `supportsResolve`,
//! display names, event handles, ...) under the parameter names of the protocol.

use super::registry::{LanguageFeatureRegistry, ModelInfo, RegistrationId};
use super::selector::LanguageSelector;
use crate::main_thread::{arg, str_arg};
use serde_json::{Map, Value};
use std::collections::BTreeMap;
use vsr_rpc::codec::Reply;
use vsr_rpc::rpc::{Call, HandlerError};

/// A kind of language feature (one registry each).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    DocumentSymbol,
    CodeLens,
    Definition,
    Declaration,
    Implementation,
    TypeDefinition,
    Hover,
    EvaluatableExpression,
    InlineValues,
    DocumentHighlight,
    MultiDocumentHighlight,
    LinkedEditingRange,
    Reference,
    CodeAction,
    PasteEdit,
    DocumentFormatting,
    RangeFormatting,
    OnTypeFormatting,
    NavigateType,
    Rename,
    NewSymbolNames,
    DocumentSemanticTokens,
    DocumentRangeSemanticTokens,
    Completion,
    InlineCompletions,
    SignatureHelp,
    InlayHints,
    DocumentLink,
    DocumentColor,
    FoldingRange,
    SelectionRange,
    CallHierarchy,
    TypeHierarchy,
    DocumentOnDropEdit,
}

use Kind::*;

/// `$register*` method, its kind, and the names of the arguments after `(handle, selector)`.
/// `$registerNavigateTypeSupport` has no selector and is handled apart.
const REGISTER: &[(&str, Kind, &[&str])] = &[
    (
        "$registerDocumentSymbolProvider",
        DocumentSymbol,
        &["label"],
    ),
    ("$registerCodeLensSupport", CodeLens, &["eventHandle"]),
    ("$registerDefinitionSupport", Definition, &[]),
    ("$registerDeclarationSupport", Declaration, &[]),
    ("$registerImplementationSupport", Implementation, &[]),
    ("$registerTypeDefinitionSupport", TypeDefinition, &[]),
    ("$registerHoverProvider", Hover, &[]),
    (
        "$registerEvaluatableExpressionProvider",
        EvaluatableExpression,
        &[],
    ),
    (
        "$registerInlineValuesProvider",
        InlineValues,
        &["eventHandle"],
    ),
    ("$registerDocumentHighlightProvider", DocumentHighlight, &[]),
    (
        "$registerMultiDocumentHighlightProvider",
        MultiDocumentHighlight,
        &[],
    ),
    (
        "$registerLinkedEditingRangeProvider",
        LinkedEditingRange,
        &[],
    ),
    ("$registerReferenceSupport", Reference, &[]),
    (
        "$registerCodeActionSupport",
        CodeAction,
        &["metadata", "displayName", "extensionID", "supportsResolve"],
    ),
    ("$registerPasteEditProvider", PasteEdit, &["metadata"]),
    (
        "$registerDocumentFormattingSupport",
        DocumentFormatting,
        &["extensionId", "displayName"],
    ),
    (
        "$registerRangeFormattingSupport",
        RangeFormatting,
        &["extensionId", "displayName", "supportRanges"],
    ),
    (
        "$registerOnTypeFormattingSupport",
        OnTypeFormatting,
        &["autoFormatTriggerCharacters", "extensionId"],
    ),
    (
        "$registerRenameSupport",
        Rename,
        &["supportsResolveInitialValues"],
    ),
    ("$registerNewSymbolNamesProvider", NewSymbolNames, &[]),
    (
        "$registerDocumentSemanticTokensProvider",
        DocumentSemanticTokens,
        &["legend", "eventHandle"],
    ),
    (
        "$registerDocumentRangeSemanticTokensProvider",
        DocumentRangeSemanticTokens,
        &["legend", "eventHandle"],
    ),
    (
        "$registerCompletionsProvider",
        Completion,
        &["triggerCharacters", "supportsResolveDetails", "extensionId"],
    ),
    (
        "$registerInlineCompletionsSupport",
        InlineCompletions,
        &[
            "supportsHandleEvents",
            "extensionId",
            "extensionVersion",
            "groupId",
            "yieldsToExtensionIds",
            "displayName",
            "debounceDelayMs",
            "excludesExtensionIds",
        ],
    ),
    (
        "$registerSignatureHelpProvider",
        SignatureHelp,
        &["metadata"],
    ),
    (
        "$registerInlayHintsProvider",
        InlayHints,
        &["supportsResolve", "eventHandle", "displayName"],
    ),
    (
        "$registerDocumentLinkProvider",
        DocumentLink,
        &["supportsResolve"],
    ),
    ("$registerDocumentColorProvider", DocumentColor, &[]),
    (
        "$registerFoldingRangeProvider",
        FoldingRange,
        &["extensionId", "eventHandle"],
    ),
    ("$registerSelectionRangeProvider", SelectionRange, &[]),
    ("$registerCallHierarchyProvider", CallHierarchy, &[]),
    ("$registerTypeHierarchyProvider", TypeHierarchy, &[]),
    (
        "$registerDocumentOnDropEditProvider",
        DocumentOnDropEdit,
        &["metadata"],
    ),
];

/// A provider the host registered.
#[derive(Debug, Clone, PartialEq)]
pub struct Registration {
    pub handle: u32,
    pub kind: Kind,
    pub selector: LanguageSelector,
    /// The arguments after `(handle, selector)`, by their protocol names (`triggerCharacters`,
    /// `supportsResolveDetails`, `extensionId`, ...).
    pub meta: Value,
    id: RegistrationId,
}

impl Registration {
    pub fn meta_bool(&self, key: &str) -> bool {
        self.meta.get(key).and_then(Value::as_bool).unwrap_or(false)
    }

    pub fn meta_str(&self, key: &str) -> Option<&str> {
        self.meta.get(key).and_then(Value::as_str)
    }

    /// The characters of a string-array entry (`triggerCharacters`, ...).
    pub fn meta_strings(&self, key: &str) -> Vec<String> {
        self.meta
            .get(key)
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect()
    }

    /// The id of an `ExtensionIdentifier` (`extensionId`) or a plain string (`extensionID`).
    pub fn extension_id(&self) -> String {
        let v = self
            .meta
            .get("extensionId")
            .or_else(|| self.meta.get("extensionID"))
            .unwrap_or(&Value::Null);
        crate::main_thread::extension_id(v)
    }

    pub fn event_handle(&self) -> Option<u64> {
        self.meta.get("eventHandle").and_then(Value::as_u64)
    }
}

/// A `$emit*Event` from the host: the provider's results may have changed.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureEvent {
    pub method: String,
    /// The registration whose `eventHandle` it was (`None` when no provider has that handle).
    pub handle: Option<u32>,
    pub event_handle: u64,
    pub event: Value,
}

#[derive(Debug, Default)]
pub struct LanguageFeatures {
    pub registrations: BTreeMap<u32, Registration>,
    registries: BTreeMap<Kind, LanguageFeatureRegistry<u32>>,
    /// Change events in arrival order; the app drains them to refresh code lenses, hints, ...
    pub events: Vec<FeatureEvent>,
    /// `$setLanguageConfiguration`: handle -> (language id, configuration JSON).
    pub language_configurations: BTreeMap<u32, (String, Value)>,
}

impl LanguageFeatures {
    pub fn registration(&self, handle: u32) -> Option<&Registration> {
        self.registrations.get(&handle)
    }

    /// Providers of `kind` for a model, best first (the registry's `ordered`).
    pub fn ordered(&mut self, kind: Kind, model: &ModelInfo) -> Vec<u32> {
        self.registries
            .get_mut(&kind)
            .map(|r| r.ordered(model, false))
            .unwrap_or_default()
    }

    /// Providers of `kind` for a model, in buckets of equal score.
    pub fn ordered_groups(&mut self, kind: Kind, model: &ModelInfo) -> Vec<Vec<u32>> {
        self.registries
            .get_mut(&kind)
            .map(|r| r.ordered_groups(model))
            .unwrap_or_default()
    }

    /// Whether any provider of `kind` applies to the model.
    pub fn has(&mut self, kind: Kind, model: &ModelInfo) -> bool {
        !self.ordered(kind, model).is_empty()
    }

    /// Every provider of `kind`, whatever it is registered for (`allNoModel`).
    pub fn all(&self, kind: Kind) -> Vec<u32> {
        self.registries
            .get(&kind)
            .map(|r| r.all_no_model())
            .unwrap_or_default()
    }

    /// Language ids some provider of `kind` is registered for.
    pub fn registered_language_ids(&self, kind: Kind) -> std::collections::BTreeSet<String> {
        self.registries
            .get(&kind)
            .map(|r| r.registered_language_ids())
            .unwrap_or_default()
    }

    /// The registry of one kind (its `version` changes with every register and unregister).
    pub fn registry(&mut self, kind: Kind) -> &mut LanguageFeatureRegistry<u32> {
        self.registries.entry(kind).or_default()
    }

    fn register(&mut self, handle: u32, kind: Kind, selector: LanguageSelector, meta: Value) {
        // A repeated handle replaces the old registration.
        self.unregister(handle);
        let id = self.registry(kind).register(selector.clone(), handle);
        self.registrations.insert(
            handle,
            Registration {
                handle,
                kind,
                selector,
                meta,
                id,
            },
        );
    }

    fn unregister(&mut self, handle: u32) {
        if let Some(r) = self.registrations.remove(&handle) {
            self.registry(r.kind).unregister(r.id);
        }
        self.language_configurations.remove(&handle);
    }

    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        let method = call.method.as_str();
        let handle = arg(call, 0).as_u64().unwrap_or(0) as u32;
        if let Some((_, kind, names)) = REGISTER.iter().find(|(m, _, _)| *m == method) {
            let selector = LanguageSelector::from_dtos(arg(call, 1));
            let mut meta = Map::new();
            for (i, name) in names.iter().enumerate() {
                meta.insert((*name).into(), arg(call, 2 + i).clone());
            }
            self.register(handle, *kind, selector, Value::Object(meta));
            return Some(Ok(Reply::Empty));
        }
        match method {
            "$registerNavigateTypeSupport" => {
                let mut meta = Map::new();
                meta.insert("supportsResolve".into(), arg(call, 1).clone());
                // No selector: workspace symbols are not tied to a model.
                self.register(
                    handle,
                    NavigateType,
                    LanguageSelector::Many(vec![]),
                    Value::Object(meta),
                );
                Some(Ok(Reply::Empty))
            }
            "$unregister" => {
                self.unregister(handle);
                Some(Ok(Reply::Empty))
            }
            "$setLanguageConfiguration" => {
                self.language_configurations
                    .insert(handle, (str_arg(call, 1).to_string(), arg(call, 2).clone()));
                Some(Ok(Reply::Empty))
            }
            m if m.starts_with("$emit") && m.ends_with("Event") => {
                let event_handle = handle as u64;
                let owner = self
                    .registrations
                    .values()
                    .find(|r| r.event_handle() == Some(event_handle))
                    .map(|r| r.handle);
                self.events.push(FeatureEvent {
                    method: m.to_string(),
                    handle: owner,
                    event_handle,
                    event: arg(call, 1).clone(),
                });
                Some(Ok(Reply::Empty))
            }
            _ => None,
        }
    }
}
