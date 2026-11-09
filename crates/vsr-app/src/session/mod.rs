//! Session restore and hot exit (ADR-0040): where state lives (`store`), what is saved (`state`),
//! and the backups of unsaved text (`backup`).

pub mod backup;
pub mod state;
pub mod store;
