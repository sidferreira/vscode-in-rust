//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/api/browser/mainThreadExtensionService.ts @ 50e1350fc78d
//!
//! The UI-free main-thread side of the extension host (ADR-0020 Option A, ADR-0064): what
//! upstream's `MainThread*` classes do for the host process, without a window. `vsr-rpc` is the
//! wire; this crate is the meaning.
//!
//! * [`description`]: `IExtensionDescription` from an extension folder (the scanner's output) and the
//!   `IExtensionDescriptionDelta` for `$startExtensionHost`;
//! * [`main_thread`]: the `MainThread*` shapes as plain models ([`main_thread::MainThread`] routes a
//!   request to the shape and records what the extensions asked for);
//! * [`host`]: [`host::ExtensionHost`] boots a host (spawn, handshake, workspace and configuration
//!   init, `$startExtensionHost`), pumps its events into the models and offers the typed calls the
//!   app makes (`$activateByEvent`, `$executeContributedCommand`, ...).
//!
//! No UI crate is used here: the app reads the models (status bar entries, decorations,
//! diagnostics, registered commands) and paints them.

pub mod commands;
pub mod configuration;
pub mod description;
pub mod diagnostics;
pub mod documents;
pub mod editors;
pub mod host;
pub mod language_features;
pub mod main_thread;
pub mod status_bar;
