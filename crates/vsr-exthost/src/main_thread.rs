//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadExtensionService.ts @ 50e1350fc78d
//!
//! The `MainThread*` shapes as models. [`MainThread::handle`] routes a request from the host by
//! proxy id (the generated table of `vsr-rpc`) and method name to the model that owns it; anything
//! no model implements yet is recorded in [`MainThread::unhandled`] and answered with `undefined`
//! (the two values the host needs at startup excepted), so a missing shape shows up in a test
//! instead of breaking the host.

use serde_json::{Value, json};
use vsr_rpc::codec::{Arg, Reply};
use vsr_rpc::proxy_ids::{self, main};
use vsr_rpc::rpc::{Call, HandlerError};

/// The JSON of argument `i`, `Null` when absent or not JSON.
pub fn arg(call: &Call, i: usize) -> &Value {
    match call.args.get(i) {
        Some(Arg::Json(v)) => v,
        Some(Arg::ObjectWithBuffers { json, .. }) => json,
        _ => &Value::Null,
    }
}

/// `args[i]` as a string (empty when it is not one).
pub fn str_arg(call: &Call, i: usize) -> &str {
    arg(call, i).as_str().unwrap_or("")
}

/// The id of an `ExtensionIdentifier` argument (`{ value, _lower }`) or a plain string.
pub fn extension_id(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        other => other
            .get("value")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
    }
}

/// A successful activation (`$onDidActivateExtension`).
#[derive(Debug, Clone, PartialEq)]
pub struct Activation {
    pub id: String,
    pub code_loading_time: f64,
    pub activate_call_time: f64,
    pub activate_resolved_time: f64,
    /// `ExtensionActivationReason`: `{ startup, extensionId, activationEvent }`.
    pub reason: Value,
}

/// A failed activation (`$onExtensionActivationError`): surfaced, never swallowed.
#[derive(Debug, Clone, PartialEq)]
pub struct ActivationError {
    pub id: String,
    /// The `SerializedError`: `{ $isError, name, message, stack }`.
    pub error: Value,
    pub missing_dependency: Value,
}

impl ActivationError {
    pub fn message(&self) -> &str {
        self.error
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("")
    }
}

/// `MainThreadExtensionService`: what the host told us about activation.
#[derive(Debug, Default)]
pub struct ExtensionService {
    /// The descriptions the host was given (`$getExtension` answers from here).
    pub known: Vec<Value>,
    pub will_activate: Vec<String>,
    pub activated: Vec<Activation>,
    pub activation_errors: Vec<ActivationError>,
    /// `$onExtensionRuntimeError`: an error thrown outside activation, attributed to an extension.
    pub runtime_errors: Vec<(String, Value)>,
}

impl ExtensionService {
    pub fn is_activated(&self, id: &str) -> bool {
        self.activated.iter().any(|a| a.id.eq_ignore_ascii_case(id))
    }

    fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        let id = extension_id(arg(call, 0));
        Some(Ok(match call.method.as_str() {
            "$getExtension" => {
                let found = self.known.iter().find(|d| {
                    d.pointer("/identifier/value")
                        .and_then(Value::as_str)
                        .is_some_and(|v| v.eq_ignore_ascii_case(&id))
                });
                found.map_or(Reply::Empty, |d| Reply::Json(d.clone()))
            }
            "$onWillActivateExtension" => {
                self.will_activate.push(id);
                Reply::Empty
            }
            "$onDidActivateExtension" => {
                let n = |i: usize| arg(call, i).as_f64().unwrap_or(0.0);
                self.activated.push(Activation {
                    id,
                    code_loading_time: n(1),
                    activate_call_time: n(2),
                    activate_resolved_time: n(3),
                    reason: arg(call, 4).clone(),
                });
                Reply::Empty
            }
            "$onExtensionActivationError" => {
                self.activation_errors.push(ActivationError {
                    id,
                    error: arg(call, 1).clone(),
                    missing_dependency: arg(call, 2).clone(),
                });
                Reply::Empty
            }
            "$onExtensionRuntimeError" => {
                self.runtime_errors.push((id, arg(call, 1).clone()));
                Reply::Empty
            }
            // Resources are served from disk, not through a browser: the identity mapping.
            "$asBrowserUri" => Reply::Json(arg(call, 0).clone()),
            "$setPerformanceMarks" => Reply::Empty,
            _ => return None,
        }))
    }
}

/// A request nothing implements yet.
#[derive(Debug, Clone, PartialEq)]
pub struct Unhandled {
    pub proxy: &'static str,
    pub method: String,
}

#[derive(Debug, Default)]
pub struct MainThread {
    pub extensions: ExtensionService,
    pub commands: crate::commands::Commands,
    pub configuration: crate::configuration::ConfigurationService,
    pub status_bar: crate::status_bar::StatusBar,
    pub editors: crate::editors::Editors,
    pub diagnostics: crate::diagnostics::Diagnostics,
    pub documents: crate::documents::Documents,
    pub language_features: crate::language_features::main_thread::LanguageFeatures,
    pub file_system: crate::file_system::FileSystem,
    /// `MainThreadConsole.$logExtensionHostMessage` payloads (the host's own warnings land here,
    /// for example `Could not identify extension for 'vscode' require`).
    pub console: Vec<Value>,
    /// Requests no model handled, in order (each answered with `undefined`).
    pub unhandled: Vec<Unhandled>,
}

impl MainThread {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn handle(&mut self, call: &Call) -> Result<Reply, HandlerError> {
        let handled = match call.rpc_id {
            main::MAIN_THREAD_EXTENSION_SERVICE => self.extensions.handle(call),
            main::MAIN_THREAD_COMMANDS => self.commands.handle(call),
            main::MAIN_THREAD_CONFIGURATION => self.configuration.handle(call),
            main::MAIN_THREAD_STATUS_BAR => self.status_bar.handle(call),
            main::MAIN_THREAD_TEXT_EDITORS => self.editors.handle(call),
            main::MAIN_THREAD_DIAGNOSTICS => self.diagnostics.handle(call),
            main::MAIN_THREAD_DOCUMENTS => self.documents.handle(call),
            main::MAIN_THREAD_LANGUAGE_FEATURES => self.language_features.handle(call),
            main::MAIN_THREAD_FILE_SYSTEM => self.file_system.handle(call),
            main::MAIN_THREAD_CONSOLE if call.method == "$logExtensionHostMessage" => {
                self.console.push(arg(call, 0).clone());
                Some(Ok(Reply::Empty))
            }
            _ => None,
        };
        // No file watcher yet (ADR-0090 next step): the changes this call made are dropped.
        self.file_system.changes.clear();
        if let Some(r) = handled {
            return r;
        }
        self.unhandled.push(Unhandled {
            proxy: proxy_ids::name_of(call.rpc_id).unwrap_or("?"),
            method: call.method.clone(),
        });
        Ok(match call.method.as_str() {
            // The host logs an unhandled rejection when these two come back `undefined`.
            "$getInitialState" => Reply::Json(json!({ "isFocused": false, "isActive": false })),
            "$getTools" => Reply::Json(json!([])),
            _ => Reply::Empty,
        })
    }
}
