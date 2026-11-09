//! Session restore and hot exit (ADR-0040): where state lives (`store`), what is saved (`state`),
//! the backups of unsaved text (`backup`), the settings (`settings`) and the workbench side
//! (`runtime`: restore, debounced saving, the quit path).

pub mod backup;
pub(crate) mod close; // a dirty tab asks before it closes (ADR-0040)
#[cfg(test)]
mod e2e_tests;
pub mod runtime;
pub mod settings;
pub mod state;
pub mod store;

pub use runtime::Runtime;
