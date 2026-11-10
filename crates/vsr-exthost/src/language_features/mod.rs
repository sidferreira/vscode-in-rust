//! upstream: src/vs/workbench/api/browser/mainThreadLanguageFeatures.ts @ 50e1350fc78d
//!
//! Language providers on the main side (ADR-0072, ADR-0073): the registry of what extensions
//! registered, and the typed queries that ask them.
//!
//! * [`selector`], [`registry`], [`uri`]: `LanguageSelector` scoring and `LanguageFeatureRegistry<T>`;
//! * [`main_thread`]: `MainThreadLanguageFeatures`, the `$register*Support` / `$unregister` handler;
//! * [`dto`]: the result shapes (completion items, hovers, symbols, locations, edits, ...) converted
//!   from what the host sends;
//! * [`query`]: `$provide*` calls on [`crate::host::ExtensionHost`] with typed results.

pub mod dto;
pub mod main_thread;
pub mod outline;
pub mod query;
pub mod registry;
pub mod selector;
pub mod uri;
