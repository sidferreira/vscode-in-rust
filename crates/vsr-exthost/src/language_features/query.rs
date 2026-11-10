//! upstream: src/vs/workbench/api/browser/mainThreadLanguageFeatures.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostLanguageFeatures.ts @ 50e1350fc78d
//!
//! The typed `$provide*` queries on [`ExtensionHost`] (ADR-0073). Upstream's editor features
//! (`getHover`, `provideSuggestionItems`, ...) ask every provider the registry orders for a model
//! and merge the answers; this does the asking and the DTO conversion and leaves the merging to
//! the feature.
//!
//! * A query names a document that is open ([`ExtensionHost::open_document`]); its language comes
//!   from there.
//! * Providers answer independently: a [`Provided`] holds every answer tagged with the provider's
//!   handle, and the errors of the providers that failed (one failing provider does not hide the
//!   others).
//! * Calls carry a `CancellationToken` (cancellable requests, see [`ExtensionHost::begin_call`]).
//! * Without a runtime: [`ExtensionHost::begin_provide_to`] starts the calls and returns a
//!   [`Query`]; the app polls it with [`ExtensionHost::poll_query`] from its own loop. The typed
//!   `provide_*` methods below block (pumping the host) up to a timeout.
//! * Results the host caches (completion lists, hovers, code lenses, inlay hints, links, code
//!   actions, signature help) stay cached until [`ExtensionHost::release`].

use super::dto::*;
use super::main_thread::Kind;
use super::registry::ModelInfo;
use super::uri::Uri;
use crate::diagnostics::uri_key;
use crate::host::{ExtensionHost, HostError};
use serde_json::{Value, json};
use std::time::{Duration, Instant};
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::handshake::file_uri;
use vsr_rpc::proxy_ids::ext_host::EXT_HOST_LANGUAGE_FEATURES;
use vsr_rpc::session::{Pending, WaitError};

/// What the providers of one query answered.
#[derive(Debug, Clone, PartialEq)]
pub struct Provided<T> {
    /// `(provider handle, answer)` in the registry's order (best provider first). Providers that
    /// answered `undefined` or something unreadable are left out.
    pub results: Vec<(u32, T)>,
    /// `(provider handle, message)` of providers whose call failed.
    pub errors: Vec<(u32, String)>,
}

impl<T> Default for Provided<T> {
    fn default() -> Self {
        Provided {
            results: vec![],
            errors: vec![],
        }
    }
}

impl<T> Provided<Vec<T>> {
    /// The answers of all providers in one list (definitions, symbols, edits, ...).
    pub fn flatten(self) -> Vec<T> {
        self.results.into_iter().flat_map(|(_, v)| v).collect()
    }
}

impl<T> Provided<T> {
    /// The best provider's answer.
    pub fn first(&self) -> Option<&T> {
        self.results.first().map(|(_, v)| v)
    }
}

type Convert<T> = Box<dyn Fn(&Value) -> Option<T>>;

/// Calls on their way: one per provider asked.
pub struct Query<T> {
    pending: Vec<(u32, Pending)>,
    convert: Convert<T>,
    done: Provided<T>,
    /// Answers received, in arrival order, to be sorted into the registry's order at the end.
    order: Vec<u32>,
}

impl<T> Query<T> {
    /// No call is outstanding.
    pub fn is_finished(&self) -> bool {
        self.pending.is_empty()
    }

    /// The providers asked.
    pub fn handles(&self) -> Vec<u32> {
        self.order.clone()
    }

    /// What has arrived so far (in the registry's order).
    pub fn take(&mut self) -> Provided<T> {
        let mut p = std::mem::take(&mut self.done);
        let rank = |h: &u32| self.order.iter().position(|o| o == h).unwrap_or(usize::MAX);
        p.results.sort_by_key(|(h, _)| rank(h));
        p.errors.sort_by_key(|(h, _)| rank(h));
        p
    }
}

fn error_message(e: &Option<Value>) -> String {
    e.as_ref()
        .and_then(|v| v.get("message").and_then(Value::as_str))
        .unwrap_or("the provider failed")
        .to_string()
}

fn is_cancellation(e: &Option<Value>) -> bool {
    e.as_ref()
        .and_then(|v| v.get("name").and_then(Value::as_str))
        .is_some_and(|n| n == "Canceled" || n == "CancellationError")
}

/// How a completion was triggered (`CompletionTriggerKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionTrigger {
    Invoke,
    Character(char),
    /// The previous result was incomplete and the user typed on.
    Incomplete,
}

impl CompletionTrigger {
    fn context(self) -> Value {
        match self {
            CompletionTrigger::Invoke => json!({ "triggerKind": 0 }),
            CompletionTrigger::Character(c) => {
                json!({ "triggerKind": 1, "triggerCharacter": c.to_string() })
            }
            CompletionTrigger::Incomplete => json!({ "triggerKind": 2 }),
        }
    }
}

/// `CodeActionTriggerType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeActionTrigger {
    Invoke = 1,
    Auto = 2,
}

/// `SignatureHelpContext.triggerKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureTrigger {
    Invoke = 1,
    Character = 2,
    ContentChange = 3,
}

fn list_of<T: 'static>(f: fn(&Value) -> Option<T>) -> Convert<Vec<T>> {
    Box::new(move |v| Some(v.as_array()?.iter().filter_map(f).collect()))
}

impl ExtensionHost {
    /// The resource and model info of an open document.
    fn model_of(&self, path: &std::path::Path) -> Result<(Value, ModelInfo), HostError> {
        let doc = self
            .main_thread
            .documents
            .open
            .get(&uri_key(&file_uri(path)))
            .ok_or_else(|| {
                HostError::Codec(format!("{} is not an open document", path.display()))
            })?;
        let uri = Uri::from_components(&doc.uri).unwrap_or_default();
        Ok((doc.uri.clone(), ModelInfo::new(uri, &doc.language)))
    }

    /// Start `method(handle, resource, ...rest)` on the given providers.
    pub fn begin_provide_to<T>(
        &mut self,
        handles: Vec<u32>,
        resource: &Value,
        method: &str,
        rest: &[Value],
        convert: Convert<T>,
    ) -> Result<Query<T>, HostError> {
        let mut pending = vec![];
        for &h in &handles {
            let mut args = vec![Arg::Json(json!(h)), Arg::Json(resource.clone())];
            args.extend(rest.iter().cloned().map(Arg::Json));
            pending.push((
                h,
                self.begin_call(EXT_HOST_LANGUAGE_FEATURES, method, &args, true)?,
            ));
        }
        Ok(Query {
            pending,
            convert,
            done: Provided::default(),
            order: handles,
        })
    }

    /// Start a query on the providers of `kind` for an open document, best first.
    pub fn begin_provide<T>(
        &mut self,
        kind: Kind,
        path: &std::path::Path,
        method: &str,
        rest: &[Value],
        convert: Convert<T>,
    ) -> Result<Query<T>, HostError> {
        let (resource, model) = self.model_of(path)?;
        let handles = self.main_thread.language_features.ordered(kind, &model);
        self.begin_provide_to(handles, &resource, method, rest, convert)
    }

    /// Apply what has arrived (non-blocking); true when the query is finished.
    pub fn poll_query<T>(&mut self, q: &mut Query<T>) -> bool {
        self.pump(Duration::ZERO);
        let mut still = vec![];
        for (h, p) in std::mem::take(&mut q.pending) {
            match p.wait(Duration::ZERO) {
                Ok(done) => match done.result {
                    Ok(Reply::Json(v)) => {
                        if let Some(t) = (q.convert)(&v) {
                            q.done.results.push((h, t));
                        }
                    }
                    Ok(_) => {}
                    Err(e) if is_cancellation(&e) => {}
                    Err(e) => q.done.errors.push((h, error_message(&e))),
                },
                Err(WaitError::Timeout) => still.push((h, p)),
                Err(WaitError::Closed) => q.done.errors.push((h, "the host closed".into())),
            }
        }
        q.pending = still;
        q.pending.is_empty()
    }

    /// Cancel the calls still outstanding (the providers' tokens fire).
    pub fn cancel_query<T>(&mut self, q: &mut Query<T>) {
        for (_, p) in std::mem::take(&mut q.pending) {
            self.cancel_request(p.req);
        }
    }

    /// Wait for a query (pumping the host); providers that have not answered by `timeout` are
    /// cancelled and reported as errors.
    pub fn finish_query<T>(&mut self, mut q: Query<T>, timeout: Duration) -> Provided<T> {
        let deadline = Instant::now() + timeout;
        while !self.poll_query(&mut q) {
            if Instant::now() >= deadline {
                for (h, _) in &q.pending {
                    q.done.errors.push((*h, "timed out".into()));
                }
                self.cancel_query(&mut q);
                break;
            }
            self.pump(Duration::from_millis(5));
        }
        q.take()
    }

    fn provide<T>(
        &mut self,
        kind: Kind,
        path: &std::path::Path,
        method: &str,
        rest: &[Value],
        convert: Convert<T>,
        timeout: Duration,
    ) -> Result<Provided<T>, HostError> {
        let q = self.begin_provide(kind, path, method, rest, convert)?;
        Ok(self.finish_query(q, timeout))
    }

    /// One call to one provider (`$resolve*`), answered as JSON (`None` for `undefined`).
    pub fn call_provider(
        &mut self,
        method: &str,
        args: Vec<Value>,
        timeout: Duration,
    ) -> Result<Option<Value>, HostError> {
        let args: Vec<Arg> = args.into_iter().map(Arg::Json).collect();
        let p = self.begin_call(EXT_HOST_LANGUAGE_FEATURES, method, &args, true)?;
        match self.wait(&p, timeout)?.result {
            Ok(Reply::Json(v)) => Ok(Some(v)),
            Ok(_) => Ok(None),
            Err(e) => Err(HostError::Remote(e)),
        }
    }

    /// Tell the host to drop what it cached for a result (`$releaseCompletionItems`,
    /// `$releaseHover`, `$releaseCodeLenses`, `$releaseInlayHints`, `$releaseDocumentLinks`,
    /// `$releaseCodeActions`, `$releaseSignatureHelp`). `id` is the list's `cache_id` (the hover's
    /// `id`). Does not wait.
    pub fn release(&mut self, kind: Kind, handle: u32, id: u64) {
        let method = match kind {
            Kind::Completion => "$releaseCompletionItems",
            Kind::Hover => "$releaseHover",
            Kind::CodeLens => "$releaseCodeLenses",
            Kind::InlayHints => "$releaseInlayHints",
            Kind::DocumentLink => "$releaseDocumentLinks",
            Kind::CodeAction => "$releaseCodeActions",
            Kind::SignatureHelp => "$releaseSignatureHelp",
            _ => return,
        };
        let args = [Arg::Json(json!(handle)), Arg::Json(json!(id))];
        // The reply (`undefined`) is not waited for.
        let _ = self.begin_call(EXT_HOST_LANGUAGE_FEATURES, method, &args, false);
    }

    // ---- completion ----

    /// `$provideCompletionItems` on the providers that apply. Providers are asked in groups of
    /// equal score, best group first, and the asking stops at the first group that offered items
    /// (upstream's `provideSuggestionItems`); for a character trigger only providers that
    /// registered that trigger character are asked.
    pub fn provide_completion_items(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        trigger: CompletionTrigger,
        timeout: Duration,
    ) -> Result<Provided<CompletionList>, HostError> {
        let (resource, model) = self.model_of(path)?;
        let groups = self
            .main_thread
            .language_features
            .ordered_groups(Kind::Completion, &model);
        let mut all = Provided::default();
        for group in groups {
            let handles: Vec<u32> = group
                .into_iter()
                .filter(|h| match trigger {
                    CompletionTrigger::Character(c) => self
                        .main_thread
                        .language_features
                        .registration(*h)
                        .is_some_and(|r| {
                            r.meta_strings("triggerCharacters").contains(&c.to_string())
                        }),
                    _ => true,
                })
                .collect();
            let q = self.begin_provide_to(
                handles,
                &resource,
                "$provideCompletionItems",
                &[pos.to_json(), trigger.context()],
                Box::new(CompletionList::from_json),
            )?;
            let got = self.finish_query(q, timeout);
            let any = got.results.iter().any(|(_, l)| !l.items.is_empty());
            all.results.extend(got.results);
            all.errors.extend(got.errors);
            if any {
                break;
            }
        }
        Ok(all)
    }

    /// `$resolveCompletionItem`: the provider fills in `detail`, `documentation`, additional edits
    /// (and, discouraged, insert text and command). Returns the item merged with the answer; the
    /// item itself when the provider cannot resolve or the item has no cache id.
    pub fn resolve_completion_item(
        &mut self,
        handle: u32,
        item: &CompletionItem,
        timeout: Duration,
    ) -> Result<CompletionItem, HostError> {
        let Some((a, b)) = item.cache_id else {
            return Ok(item.clone());
        };
        let can = self
            .main_thread
            .language_features
            .registration(handle)
            .is_some_and(|r| r.meta_bool("supportsResolveDetails"));
        if !can {
            return Ok(item.clone());
        }
        let got = self.call_provider(
            "$resolveCompletionItem",
            vec![json!(handle), json!([a, b])],
            timeout,
        )?;
        let Some(resolved) = got.and_then(|v| CompletionItem::from_json(&v, Some(&item.range)))
        else {
            return Ok(item.clone());
        };
        Ok(CompletionItem {
            detail: resolved.detail,
            documentation: resolved.documentation,
            additional_text_edits: resolved.additional_text_edits,
            insert_text: resolved.insert_text,
            insert_text_rules: resolved.insert_text_rules,
            command: resolved.command,
            ..item.clone()
        })
    }

    // ---- hover, navigation, symbols, highlights ----

    pub fn provide_hover(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Hover>, HostError> {
        self.provide(
            Kind::Hover,
            path,
            "$provideHover",
            &[pos.to_json(), Value::Null],
            Box::new(Hover::from_json),
            timeout,
        )
    }

    fn provide_links(
        &mut self,
        kind: Kind,
        method: &str,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Vec<LocationLink>>, HostError> {
        self.provide(
            kind,
            path,
            method,
            &[pos.to_json()],
            list_of(LocationLink::from_json),
            timeout,
        )
    }

    pub fn provide_definition(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Vec<LocationLink>>, HostError> {
        self.provide_links(Kind::Definition, "$provideDefinition", path, pos, timeout)
    }

    pub fn provide_declaration(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Vec<LocationLink>>, HostError> {
        self.provide_links(Kind::Declaration, "$provideDeclaration", path, pos, timeout)
    }

    pub fn provide_implementation(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Vec<LocationLink>>, HostError> {
        self.provide_links(
            Kind::Implementation,
            "$provideImplementation",
            path,
            pos,
            timeout,
        )
    }

    pub fn provide_type_definition(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Vec<LocationLink>>, HostError> {
        self.provide_links(
            Kind::TypeDefinition,
            "$provideTypeDefinition",
            path,
            pos,
            timeout,
        )
    }

    pub fn provide_references(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        include_declaration: bool,
        timeout: Duration,
    ) -> Result<Provided<Vec<Location>>, HostError> {
        self.provide(
            Kind::Reference,
            path,
            "$provideReferences",
            &[
                pos.to_json(),
                json!({ "includeDeclaration": include_declaration }),
            ],
            list_of(Location::from_json),
            timeout,
        )
    }

    pub fn provide_document_symbols(
        &mut self,
        path: &std::path::Path,
        timeout: Duration,
    ) -> Result<Provided<Vec<DocumentSymbol>>, HostError> {
        self.provide(
            Kind::DocumentSymbol,
            path,
            "$provideDocumentSymbols",
            &[],
            list_of(DocumentSymbol::from_json),
            timeout,
        )
    }

    pub fn provide_document_highlights(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<Vec<DocumentHighlight>>, HostError> {
        self.provide(
            Kind::DocumentHighlight,
            path,
            "$provideDocumentHighlights",
            &[pos.to_json()],
            list_of(DocumentHighlight::from_json),
            timeout,
        )
    }

    pub fn provide_folding_ranges(
        &mut self,
        path: &std::path::Path,
        timeout: Duration,
    ) -> Result<Provided<Vec<FoldingRange>>, HostError> {
        self.provide(
            Kind::FoldingRange,
            path,
            "$provideFoldingRanges",
            &[json!({})],
            list_of(FoldingRange::from_json),
            timeout,
        )
    }

    // ---- code lens, inlay hints, links ----

    pub fn provide_code_lenses(
        &mut self,
        path: &std::path::Path,
        timeout: Duration,
    ) -> Result<Provided<CodeLensList>, HostError> {
        self.provide(
            Kind::CodeLens,
            path,
            "$provideCodeLenses",
            &[],
            Box::new(CodeLensList::from_json),
            timeout,
        )
    }

    /// `$resolveCodeLens`: the lens with its command.
    pub fn resolve_code_lens(
        &mut self,
        handle: u32,
        lens: &CodeLens,
        timeout: Duration,
    ) -> Result<CodeLens, HostError> {
        let Some((a, b)) = lens.cache_id else {
            return Ok(lens.clone());
        };
        let dto = json!({ "range": lens.range.to_json(), "cacheId": [a, b] });
        let got = self.call_provider("$resolveCodeLens", vec![json!(handle), dto], timeout)?;
        Ok(got
            .and_then(|v| CodeLens::from_json(&v))
            .unwrap_or_else(|| lens.clone()))
    }

    pub fn provide_inlay_hints(
        &mut self,
        path: &std::path::Path,
        range: Range,
        timeout: Duration,
    ) -> Result<Provided<InlayHintList>, HostError> {
        self.provide(
            Kind::InlayHints,
            path,
            "$provideInlayHints",
            &[range.to_json()],
            Box::new(InlayHintList::from_json),
            timeout,
        )
    }

    pub fn resolve_inlay_hint(
        &mut self,
        handle: u32,
        hint: &InlayHint,
        timeout: Duration,
    ) -> Result<InlayHint, HostError> {
        let Some((a, b)) = hint.cache_id else {
            return Ok(hint.clone());
        };
        let got = self.call_provider(
            "$resolveInlayHint",
            vec![json!(handle), json!([a, b])],
            timeout,
        )?;
        Ok(got
            .and_then(|v| InlayHint::from_json(&v))
            .unwrap_or_else(|| hint.clone()))
    }

    pub fn provide_document_links(
        &mut self,
        path: &std::path::Path,
        timeout: Duration,
    ) -> Result<Provided<DocumentLinkList>, HostError> {
        self.provide(
            Kind::DocumentLink,
            path,
            "$provideDocumentLinks",
            &[],
            Box::new(DocumentLinkList::from_json),
            timeout,
        )
    }

    pub fn resolve_document_link(
        &mut self,
        handle: u32,
        link: &DocumentLink,
        timeout: Duration,
    ) -> Result<DocumentLink, HostError> {
        let Some((a, b)) = link.cache_id else {
            return Ok(link.clone());
        };
        let got = self.call_provider(
            "$resolveDocumentLink",
            vec![json!(handle), json!([a, b])],
            timeout,
        )?;
        Ok(got
            .and_then(|v| DocumentLink::from_json(&v))
            .unwrap_or_else(|| link.clone()))
    }

    // ---- formatting ----
    // Every provider that applies answers; the caller picks one (the default formatter's
    // `extensionId` is in the registration's metadata).

    pub fn provide_document_formatting_edits(
        &mut self,
        path: &std::path::Path,
        options: &FormattingOptions,
        timeout: Duration,
    ) -> Result<Provided<Vec<TextEdit>>, HostError> {
        self.provide(
            Kind::DocumentFormatting,
            path,
            "$provideDocumentFormattingEdits",
            &[options.to_json()],
            Box::new(|v| Some(TextEdit::list_from_json(v))),
            timeout,
        )
    }

    pub fn provide_range_formatting_edits(
        &mut self,
        path: &std::path::Path,
        range: Range,
        options: &FormattingOptions,
        timeout: Duration,
    ) -> Result<Provided<Vec<TextEdit>>, HostError> {
        self.provide(
            Kind::RangeFormatting,
            path,
            "$provideDocumentRangeFormattingEdits",
            &[range.to_json(), options.to_json()],
            Box::new(|v| Some(TextEdit::list_from_json(v))),
            timeout,
        )
    }

    /// Only providers registered for `ch` as a trigger character are asked (the registration's
    /// `autoFormatTriggerCharacters`).
    pub fn provide_on_type_formatting_edits(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        ch: char,
        options: &FormattingOptions,
        timeout: Duration,
    ) -> Result<Provided<Vec<TextEdit>>, HostError> {
        let (resource, model) = self.model_of(path)?;
        let handles: Vec<u32> = self
            .main_thread
            .language_features
            .ordered(Kind::OnTypeFormatting, &model)
            .into_iter()
            .filter(|h| {
                self.main_thread
                    .language_features
                    .registration(*h)
                    .is_some_and(|r| {
                        r.meta_strings("autoFormatTriggerCharacters")
                            .contains(&ch.to_string())
                    })
            })
            .collect();
        let q = self.begin_provide_to(
            handles,
            &resource,
            "$provideOnTypeFormattingEdits",
            &[pos.to_json(), json!(ch.to_string()), options.to_json()],
            Box::new(|v| Some(TextEdit::list_from_json(v))),
        )?;
        Ok(self.finish_query(q, timeout))
    }

    // ---- signature help, code actions, rename ----

    /// `activeSignatureHelp` of a retrigger is not passed on (`undefined`).
    pub fn provide_signature_help(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        trigger: SignatureTrigger,
        trigger_character: Option<char>,
        is_retrigger: bool,
        timeout: Duration,
    ) -> Result<Provided<SignatureHelp>, HostError> {
        let ctx = json!({
            "triggerKind": trigger as u32,
            "triggerCharacter": trigger_character.map(|c| c.to_string()),
            "isRetrigger": is_retrigger,
        });
        self.provide(
            Kind::SignatureHelp,
            path,
            "$provideSignatureHelp",
            &[pos.to_json(), ctx],
            Box::new(SignatureHelp::from_json),
            timeout,
        )
    }

    pub fn provide_code_actions(
        &mut self,
        path: &std::path::Path,
        range: Range,
        only: Option<&str>,
        trigger: CodeActionTrigger,
        timeout: Duration,
    ) -> Result<Provided<CodeActionList>, HostError> {
        let ctx = json!({ "only": only, "trigger": trigger as u32 });
        self.provide(
            Kind::CodeAction,
            path,
            "$provideCodeActions",
            &[range.to_json(), ctx],
            Box::new(CodeActionList::from_json),
            timeout,
        )
    }

    /// `$resolveCodeAction`: fills in `edit` and `command` of an action that came without them.
    pub fn resolve_code_action(
        &mut self,
        handle: u32,
        action: &CodeAction,
        timeout: Duration,
    ) -> Result<CodeAction, HostError> {
        let Some((a, b)) = action.cache_id else {
            return Ok(action.clone());
        };
        let got = self.call_provider(
            "$resolveCodeAction",
            vec![json!(handle), json!([a, b])],
            timeout,
        )?;
        let mut out = action.clone();
        if let Some(v) = got {
            out.edit = v.get("edit").map(WorkspaceEdit::from_json).or(out.edit);
            out.command = v
                .get("command")
                .and_then(Command::from_json)
                .or(out.command);
        }
        Ok(out)
    }

    pub fn resolve_rename_location(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        timeout: Duration,
    ) -> Result<Provided<RenameLocation>, HostError> {
        self.provide(
            Kind::Rename,
            path,
            "$resolveRenameLocation",
            &[pos.to_json()],
            Box::new(RenameLocation::from_json),
            timeout,
        )
    }

    pub fn provide_rename_edits(
        &mut self,
        path: &std::path::Path,
        pos: Position,
        new_name: &str,
        timeout: Duration,
    ) -> Result<Provided<RenameEdits>, HostError> {
        self.provide(
            Kind::Rename,
            path,
            "$provideRenameEdits",
            &[pos.to_json(), json!(new_name)],
            Box::new(RenameEdits::from_json),
            timeout,
        )
    }
}
