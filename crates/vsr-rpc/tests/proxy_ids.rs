//! upstream: src/vs/workbench/api/common/extHost.protocol.ts @ 50e1350fc78d
//!
//! The generated id table (`scripts/gen-rpc-ids`). The numbers were cross-checked once against
//! the live `ProxyIdentifier.nid` values of the transpiled upstream module (ADR-0060): the 171
//! `(sid, nid)` pairs were identical. These pins catch a regenerated table that shifts.

use vsr_rpc::proxy_ids::{self, ext_host, main};

#[test]
fn ids_follow_construction_order_main_first() {
    assert_eq!(main::MAIN_THREAD_AUTHENTICATION, 1);
    assert_eq!(proxy_ids::name_of(1), Some("MainThreadAuthentication"));
    assert_eq!(main::MAIN_THREAD_COMMANDS, 10);
    assert_eq!(main::MAIN_THREAD_CONSOLE, 13);
    assert_eq!(main::MAIN_THREAD_WINDOW, 59);
    // ExtHostContext continues where MainContext stops.
    assert_eq!(main::ALL.len(), 89);
    assert_eq!(ext_host::ALL.len(), 82);
    assert_eq!(ext_host::ALL[0].1, 90);
    assert_eq!(ext_host::EXT_HOST_EXTENSION_SERVICE, 112);
    assert_eq!(proxy_ids::name_of(171), Some("ExtHostBrowsers"));
}

#[test]
fn lookups_work_both_ways_and_ids_are_unique() {
    assert_eq!(proxy_ids::id_of("MainThreadCommands"), Some(10));
    assert_eq!(proxy_ids::id_of("NoSuchProxy"), None);
    assert_eq!(proxy_ids::name_of(0), None);
    assert_eq!(proxy_ids::name_of(172), None);
    let mut ids: Vec<u8> = main::ALL
        .iter()
        .chain(ext_host::ALL.iter())
        .map(|(_, i)| *i)
        .collect();
    let before = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), before);
    assert!(
        ids.windows(2).all(|w| w[1] == w[0] + 1),
        "ids are consecutive from 1"
    );
}

#[test]
fn is_main_splits_the_two_directions() {
    assert!(proxy_ids::is_main(1));
    assert!(proxy_ids::is_main(89));
    assert!(!proxy_ids::is_main(90));
    assert!(!proxy_ids::is_main(0));
}
