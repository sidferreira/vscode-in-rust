//! The command registry of rscode (ADR-0030), free of any UI framework.
//!
//! * [`registry`]: `CommandsRegistry` + the command palette metadata of `Action2`
//!   (`title`, `category`, `f1`, `precondition`), generic over the handler type so the app can
//!   store gpui actions and, later, the extension host can store RPC stubs;
//! * [`when`]: the small context-key expression language used by preconditions;
//! * [`history`]: `CommandsHistory`, the "recently used" list of the Command Palette.

pub mod history;
pub mod registry;
pub mod when;

pub use history::CommandsHistory;
pub use registry::{CommandDescriptor, CommandsRegistry};
pub use when::{ContextKeys, When};
