//! upstream: src/vs/workbench/api/browser/mainThreadCommands.ts @ 50e1350fc78d
//!
//! `MainThreadCommands`: the commands extensions register (so the app can list them and call
//! `ExtHostCommands.$executeContributedCommand`), and the app's own commands an extension runs
//! with `vscode.commands.executeCommand`.

use crate::main_thread::{arg, str_arg};
use serde_json::Value;
use std::collections::BTreeSet;
use vsr_rpc::codec::Reply;
use vsr_rpc::rpc::{Call, HandlerError};

/// Runs a command of the app for an extension: `Ok(None)` is `undefined`, `Err` rejects.
pub type Executor = Box<dyn FnMut(&str, &[Value]) -> Result<Option<Value>, String> + Send>;

#[derive(Default)]
pub struct Commands {
    /// Ids registered by extensions (`$registerCommand`), sorted.
    pub registered: BTreeSet<String>,
    /// Commands extensions asked the app to run, with their arguments, in order.
    pub executed: Vec<(String, Vec<Value>)>,
    /// Ids whose activation event (`onCommand:`) the host fired.
    pub activation_events: Vec<String>,
    executor: Option<Executor>,
}

impl std::fmt::Debug for Commands {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Commands")
            .field("registered", &self.registered)
            .field("executed", &self.executed)
            .finish()
    }
}

impl Commands {
    /// How the app runs its own commands for extensions. Without one, `$executeCommand` of an
    /// unknown command rejects, like upstream's `CommandsRegistry` lookup does.
    pub fn set_executor(&mut self, executor: Executor) {
        self.executor = Some(executor);
    }

    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        Some(Ok(match call.method.as_str() {
            "$registerCommand" => {
                self.registered.insert(str_arg(call, 0).to_string());
                Reply::Empty
            }
            "$unregisterCommand" => {
                self.registered.remove(str_arg(call, 0));
                Reply::Empty
            }
            "$fireCommandActivationEvent" => {
                self.activation_events.push(str_arg(call, 0).to_string());
                Reply::Empty
            }
            "$getCommands" => Reply::Json(Value::Array(
                self.registered.iter().cloned().map(Value::String).collect(),
            )),
            "$executeCommand" => {
                let id = str_arg(call, 0).to_string();
                let args: Vec<Value> = arg(call, 1).as_array().cloned().unwrap_or_default();
                self.executed.push((id.clone(), args.clone()));
                let Some(exec) = self.executor.as_mut() else {
                    return Some(Err(HandlerError::error(format!(
                        "command '{id}' not found"
                    ))));
                };
                return Some(match exec(&id, &args) {
                    Ok(Some(v)) => Ok(Reply::Json(v)),
                    Ok(None) => Ok(Reply::Empty),
                    Err(e) => Err(HandlerError::error(e)),
                });
            }
            _ => return None,
        }))
    }
}
