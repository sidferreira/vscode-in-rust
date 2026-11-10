//! upstream: src/vs/editor/contrib/documentSymbols/test/browser/outlineModel.test.ts @ 50e1350fc78d
//!
//! Ports of outlineModel.test.ts. The two `OutlineModelService` tests run against
//! `OutlineService` with a counting fake provider instead of a language registry: the request
//! starts a provider call, `deliver` is the provider's answer, and cancelling every requester
//! is what the upstream token observes. The marker tests use `Marker` for `IMarker`.

use vsr_exthost::language_features::dto::{DocumentSymbol, Range};
use vsr_exthost::language_features::outline::{
    Marker, OutlineModel, OutlineService, ProviderSymbols, ROOT, Request, severity,
};

const URI: &str = "file:///fome/path.foo";

/// `fakeSymbolInformation`: kind Boolean (16), selection range equal to the range.
fn fake_symbol(range: Range, name: &str) -> DocumentSymbol {
    DocumentSymbol {
        name: name.to_string(),
        detail: "fake".into(),
        kind: 16,
        tags: vec![],
        container_name: None,
        range,
        selection_range: range,
        children: vec![],
    }
}

/// `fakeMarker`: an error at the range.
fn fake_marker(range: Range) -> Marker {
    Marker {
        range,
        severity: severity::ERROR,
    }
}

fn r(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
    Range::new(sl, sc, el, ec)
}

// upstream: OutlineModel > OutlineModel#create, cached
#[test]
fn outline_model_create_cached() {
    let mut service = OutlineService::new();
    let mut count = 0;
    // The fake provider answers with no symbols, once per request that starts.
    let ask = |service: &mut OutlineService, version: u64, count: &mut u32| {
        if let Request::Started(_) = service.request(URI, version) {
            *count += 1;
            service.deliver(URI, version, vec![]);
        }
    };

    ask(&mut service, 1, &mut count);
    assert_eq!(count, 1);

    // cached
    ask(&mut service, 1, &mut count);
    assert_eq!(count, 1);

    // new version
    ask(&mut service, 2, &mut count);
    assert_eq!(count, 2);
}

// upstream: OutlineModel > OutlineModel#create, cached/cancel
#[test]
fn outline_model_create_cached_cancel() {
    let mut service = OutlineService::new();

    assert!(service.take_cancelled().is_empty());
    let Request::Started(s1) = service.request(URI, 1) else {
        panic!("first request starts")
    };
    let Request::Joined(s2) = service.request(URI, 1) else {
        panic!("second joins")
    };

    service.cancel(s1);
    assert!(
        service.take_cancelled().is_empty(),
        "one requester left: not cancelled"
    );

    service.cancel(s2);
    assert_eq!(
        service.take_cancelled(),
        vec![URI.to_string()],
        "every requester cancelled"
    );
}

// upstream: OutlineElement > OutlineElement - updateMarker
#[test]
fn outline_element_update_marker() {
    let symbols = vec![
        fake_symbol(r(1, 1, 1, 10), "foo1"),
        fake_symbol(r(2, 1, 5, 1), "foo2"),
        fake_symbol(r(6, 1, 10, 10), "foo3"),
    ];
    let mut model = OutlineModel::create(URI, vec![ProviderSymbols::new(Some("group"), symbols)]);
    let (e0, e1, e2) = (
        model.item_by_id("root/provider_0/foo1").unwrap(),
        model.item_by_id("root/provider_0/foo2").unwrap(),
        model.item_by_id("root/provider_0/foo3").unwrap(),
    );

    let mut data = vec![
        fake_marker(r(6, 1, 6, 7)),
        fake_marker(r(1, 1, 1, 4)),
        fake_marker(r(10, 2, 14, 1)),
    ];
    data.sort_by(|a, b| {
        vsr_exthost::language_features::outline::compare_ranges_using_starts(&a.range, &b.range)
    });

    model.update_markers_in(ROOT, &mut data);
    assert_eq!(data.len(), 0, "all 'stolen'");
    assert_eq!(model.marker(e0).unwrap().count, 1);
    assert_eq!(model.marker(e1), None);
    assert_eq!(model.marker(e2).unwrap().count, 2);

    model.update_markers_in(ROOT, &mut vec![]);
    assert_eq!(model.marker(e0), None);
    assert_eq!(model.marker(e1), None);
    assert_eq!(model.marker(e2), None);
}

// upstream: OutlineElement > OutlineElement - updateMarker, 2
#[test]
fn outline_element_update_marker_2() {
    let p = fake_symbol(r(1, 1, 11, 1), "A");
    let mut p = p;
    p.children = vec![
        fake_symbol(r(2, 4, 5, 4), "A/B"),
        fake_symbol(r(6, 4, 9, 4), "A/C"),
    ];
    let mut model = OutlineModel::create(URI, vec![ProviderSymbols::new(Some("group"), vec![p])]);
    let (pa, c1, c2) = (
        model.item_by_id("root/provider_0/A").unwrap(),
        model.item_by_id("root/provider_0/A/A/B").unwrap(),
        model.item_by_id("root/provider_0/A/A/C").unwrap(),
    );

    let mut data = vec![fake_marker(r(2, 4, 5, 4))];
    model.update_markers_in(ROOT, &mut data);
    assert_eq!(model.marker(pa).unwrap().count, 0);
    assert_eq!(model.marker(c1).unwrap().count, 1);
    assert_eq!(model.marker(c2), None);

    let mut data = vec![
        fake_marker(r(2, 4, 5, 4)),
        fake_marker(r(2, 6, 2, 8)),
        fake_marker(r(7, 6, 7, 8)),
    ];
    model.update_markers_in(ROOT, &mut data);
    assert_eq!(model.marker(pa).unwrap().count, 0);
    assert_eq!(model.marker(c1).unwrap().count, 2);
    assert_eq!(model.marker(c2).unwrap().count, 1);

    let mut data = vec![fake_marker(r(1, 4, 1, 11)), fake_marker(r(7, 6, 7, 8))];
    model.update_markers_in(ROOT, &mut data);
    assert_eq!(model.marker(pa).unwrap().count, 1);
    assert_eq!(model.marker(c1), None);
    assert_eq!(model.marker(c2).unwrap().count, 1);
}

// upstream: OutlineElement > OutlineElement - updateMarker/multiple groups
#[test]
fn outline_element_update_marker_multiple_groups() {
    let mut c2 = fake_symbol(r(1, 1, 7, 1), "c2");
    c2.children = vec![
        fake_symbol(r(1, 3, 2, 19), "c2.1"),
        fake_symbol(r(4, 1, 6, 10), "c2.2"),
    ];
    let mut model = OutlineModel::create(
        URI,
        vec![
            ProviderSymbols::new(Some("g1"), vec![fake_symbol(r(1, 1, 11, 1), "c1")]),
            ProviderSymbols::new(Some("g2"), vec![c2]),
        ],
    );
    assert_eq!(model.groups().len(), 2);

    let mut data = vec![fake_marker(r(1, 1, 2, 8)), fake_marker(r(6, 1, 6, 98))];
    model.update_marker(&mut data);

    let c1 = model.item_by_id("root/provider_0/c1").unwrap();
    let c21 = model.item_by_id("root/provider_1/c2/c2.1").unwrap();
    let c22 = model.item_by_id("root/provider_1/c2/c2.2").unwrap();
    assert_eq!(model.marker(c1).unwrap().count, 2);
    assert_eq!(model.marker(c21).unwrap().count, 1);
    assert_eq!(model.marker(c22).unwrap().count, 1);
}
