//! The app owns the extension host on a worker thread (ADR-0066), so everything it holds must be
//! `Send`. A non-`Send` field added here (e.g. a boxed closure without `+ Send`) breaks the app
//! build; this test catches it in this crate.

use vsr_exthost::host::ExtensionHost;
use vsr_exthost::language_features::registry::NotebookInfoResolver;

fn is_send<T: Send>() {}

#[test]
fn the_extension_host_can_move_to_a_worker_thread() {
    is_send::<ExtensionHost>();
    is_send::<NotebookInfoResolver>();
}
