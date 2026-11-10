//! upstream: src/vs/workbench/api/browser/mainThreadConfiguration.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/common/extHostConfiguration.ts @ 50e1350fc78d
//!
//! Configuration both ways. The host reads settings from the layers we send
//! (`$initializeConfiguration`, `$acceptConfigurationChanged`): [`layers`] builds them from
//! `vsr-settings` (the registered defaults and the user's settings.json). `MainThreadConfiguration`
//! receives `WorkspaceConfiguration.update(...)` calls: [`ConfigurationService`] records them for
//! the app to write to settings.json.

use crate::description::nest;
use crate::main_thread::{arg, str_arg};
use serde_json::Value;
use vsr_rpc::codec::Reply;
use vsr_rpc::rpc::{Call, HandlerError};
use vsr_settings::{Configuration, SettingsRegistry};

/// `(defaults, user)` as nested `contents` for the host: every registered setting's default, and
/// the user's values as written.
pub fn layers(registry: &SettingsRegistry, conf: &Configuration) -> (Value, Value) {
    let defaults = registry
        .keys()
        .filter_map(|k| conf.default_value(k).map(|v| (k.to_string(), v.clone())));
    let user = conf.user().iter().map(|(k, v)| (k.to_string(), v.clone()));
    (nest(defaults), nest(user))
}

/// One `update` / `remove` from an extension.
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigUpdate {
    /// `ConfigurationTarget` of `platform/configuration`: 1 application, 2 user, 3 user local, 4 user
    /// remote, 5 workspace, 6 workspace folder (`update(.., true)` is 2); `None` for "decide".
    pub target: Option<i64>,
    pub key: String,
    /// `None` for a removal (`update(key, undefined)`).
    pub value: Option<Value>,
}

#[derive(Debug, Default)]
pub struct ConfigurationService {
    pub updates: Vec<ConfigUpdate>,
}

impl ConfigurationService {
    pub(crate) fn handle(&mut self, call: &Call) -> Option<Result<Reply, HandlerError>> {
        let target = arg(call, 0).as_i64();
        let key = str_arg(call, 1).to_string();
        match call.method.as_str() {
            "$updateConfigurationOption" => {
                let value = Some(arg(call, 2).clone()).filter(|v| !v.is_null());
                self.updates.push(ConfigUpdate { target, key, value });
                Some(Ok(Reply::Empty))
            }
            "$removeConfigurationOption" => {
                self.updates.push(ConfigUpdate {
                    target,
                    key,
                    value: None,
                });
                Some(Ok(Reply::Empty))
            }
            _ => None,
        }
    }
}
