//! The outline model beyond the upstream ports: ids, compaction, the symbol at a position, the
//! flattened list, sorting and kind filters, and the natural name order. New tests (no upstream
//! counterpart), so they say what they pin in plain words.

use std::cmp::Ordering;
use vsr_exthost::language_features::dto::{DocumentSymbol, Position, Range};
use vsr_exthost::language_features::outline::{
    KindFilter, OutlineModel, OutlineService, ProviderSymbols, Request, SortOrder, natural_cmp,
};

const URI: &str = "file:///ws/a.rs";

fn sym(name: &str, kind: u32, range: Range, children: Vec<DocumentSymbol>) -> DocumentSymbol {
    DocumentSymbol {
        name: name.into(),
        detail: String::new(),
        kind,
        tags: vec![],
        container_name: None,
        range,
        selection_range: range,
        children,
    }
}

fn r(sl: u32, sc: u32, el: u32, ec: u32) -> Range {
    Range::new(sl, sc, el, ec)
}

/// struct Point (1-6) { fn new (2-3), fn len (4-5) }, fn main (8-10)
fn sample() -> Vec<ProviderSymbols> {
    let point = sym(
        "Point",
        4,
        r(1, 1, 6, 2),
        vec![
            sym("new", 5, r(2, 5, 3, 6), vec![]),
            sym("len", 5, r(4, 5, 5, 6), vec![]),
        ],
    );
    let main = sym("main", 11, r(8, 1, 10, 2), vec![]);
    vec![ProviderSymbols::new(
        Some("rust-analyzer"),
        vec![point, main],
    )]
}

#[test]
fn single_provider_is_compacted_into_the_top_level() {
    let m = OutlineModel::create(URI, sample());
    let top: Vec<String> = m
        .top_level()
        .iter()
        .map(|&i| m.node(i).id.clone())
        .collect();
    assert_eq!(top, ["root/provider_0/Point", "root/provider_0/main"]);
    assert_eq!(m.groups().len(), 1);
}

#[test]
fn several_providers_keep_one_group_each() {
    let m = OutlineModel::create(
        URI,
        vec![
            ProviderSymbols::new(Some("a"), vec![sym("x", 11, r(1, 1, 1, 5), vec![])]),
            ProviderSymbols::new(None, vec![sym("y", 11, r(2, 1, 2, 5), vec![])]),
            ProviderSymbols::new(Some("empty"), vec![]),
        ],
    );
    let labels: Vec<String> = m
        .top_level()
        .iter()
        .map(|&i| match &m.node(i).item {
            vsr_exthost::language_features::outline::Item::Group { label, .. } => label.clone(),
            _ => panic!("groups at the top level"),
        })
        .collect();
    assert_eq!(
        labels,
        ["a", "Unknown Outline Provider"],
        "the empty provider is left out"
    );
}

#[test]
fn duplicate_names_get_position_then_index_suffixes() {
    let m = OutlineModel::create(
        URI,
        vec![ProviderSymbols::new(
            Some("p"),
            vec![
                sym("f", 11, r(1, 1, 1, 2), vec![]),
                sym("f", 11, r(3, 1, 3, 2), vec![]),
                sym("f", 11, r(5, 1, 5, 2), vec![]),
            ],
        )],
    );
    let ids: Vec<String> = m
        .top_level()
        .iter()
        .map(|&i| m.node(i).id.clone())
        .collect();
    assert_eq!(
        ids,
        [
            "root/provider_0/f",
            "root/provider_0/f_3_1",
            "root/provider_0/f_5_1"
        ]
    );
}

#[test]
fn enclosing_position_is_the_innermost_symbol_containing_it() {
    let m = OutlineModel::create(URI, sample());
    let at = |line, col| {
        m.item_enclosing(Position::new(line, col), None)
            .map(|i| m.node(i).id.clone())
    };
    assert_eq!(at(4, 6).as_deref(), Some("root/provider_0/Point/len"));
    assert_eq!(
        at(2, 1).as_deref(),
        Some("root/provider_0/Point"),
        "outside the methods"
    );
    assert_eq!(at(7, 1), None, "between symbols");
    assert_eq!(at(9, 3).as_deref(), Some("root/provider_0/main"));
    // The end column is inclusive, as Range.containsPosition is.
    assert_eq!(at(10, 2).as_deref(), Some("root/provider_0/main"));
}

#[test]
fn path_to_lists_the_symbols_from_the_top() {
    let m = OutlineModel::create(URI, sample());
    let len = m.item_by_id("root/provider_0/Point/len").unwrap();
    let names: Vec<&str> = m
        .path_to(len)
        .iter()
        .map(|&i| m.symbol(i).unwrap().name.as_str())
        .collect();
    assert_eq!(names, ["Point", "len"]);
}

#[test]
fn list_of_symbols_is_flat_with_container_names_in_start_order() {
    let m = OutlineModel::create(URI, sample());
    let list = m.list_of_symbols();
    let names: Vec<&str> = list.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["Point", "new", "len", "main"]);
    assert_eq!(list[1].container_name.as_deref(), Some("Point"));
    assert_eq!(list[0].container_name, None);
    assert!(list.iter().all(|s| s.children.is_empty()));
}

#[test]
fn list_of_symbols_puts_the_longer_range_first_on_equal_starts() {
    let m = OutlineModel::create(
        URI,
        vec![ProviderSymbols::new(
            Some("p"),
            vec![
                sym("short", 11, r(1, 1, 1, 4), vec![]),
                sym("long", 11, r(1, 1, 3, 1), vec![]),
            ],
        )],
    );
    let names: Vec<String> = m.list_of_symbols().into_iter().map(|s| s.name).collect();
    assert_eq!(names, ["long", "short"]);
}

#[test]
fn sort_orders_siblings_by_position_name_or_type() {
    let m = OutlineModel::create(
        URI,
        vec![ProviderSymbols::new(
            Some("p"),
            vec![
                sym("beta", 5, r(1, 1, 1, 2), vec![]),
                sym("Alpha10", 12, r(2, 1, 2, 2), vec![]),
                sym("alpha2", 5, r(3, 1, 3, 2), vec![]),
            ],
        )],
    );
    let names = |order| -> Vec<String> {
        m.visible_children(
            vsr_exthost::language_features::outline::ROOT,
            order,
            &KindFilter::default(),
        )
        .iter()
        .map(|&i| m.symbol(i).unwrap().name.clone())
        .collect()
    };
    assert_eq!(names(SortOrder::Position), ["beta", "Alpha10", "alpha2"]);
    assert_eq!(names(SortOrder::Name), ["alpha2", "Alpha10", "beta"]);
    assert_eq!(names(SortOrder::Type), ["alpha2", "beta", "Alpha10"]);
}

#[test]
fn kind_filter_hides_a_kind_and_the_subtree_under_it() {
    let m = OutlineModel::create(URI, sample());
    let mut filter = KindFilter::default();
    filter.set(4, false); // Class: Point and everything under it
    let top = m.visible_children(
        vsr_exthost::language_features::outline::ROOT,
        SortOrder::Position,
        &filter,
    );
    let names: Vec<&str> = top
        .iter()
        .map(|&i| m.symbol(i).unwrap().name.as_str())
        .collect();
    assert_eq!(names, ["main"]);
}

#[test]
fn kind_filter_reads_show_settings_and_defaults_to_shown() {
    let filter = KindFilter::from_settings("outline", |key| match key {
        "outline.showClasses" => Some(false),
        "outline.showFunctions" => Some(true),
        _ => None,
    });
    assert!(!filter.allows(4));
    assert!(filter.allows(11));
    assert!(filter.allows(5), "a missing key shows the kind");
    assert!(filter.allows(999), "an unknown kind is shown");
}

#[test]
fn natural_order_compares_digit_runs_by_value() {
    assert_eq!(natural_cmp("item2", "item10"), Ordering::Less);
    assert_eq!(
        natural_cmp("Beta", "alpha"),
        Ordering::Greater,
        "case-insensitive first"
    );
    assert_eq!(natural_cmp("a", "a"), Ordering::Equal);
    assert_eq!(
        natural_cmp("a01", "a1"),
        Ordering::Less,
        "ties break on the exact text"
    );
}

#[test]
fn service_drops_a_superseded_version_and_its_providers_are_cancelled() {
    let mut s = OutlineService::new();
    let Request::Started(old) = s.request(URI, 1) else {
        panic!()
    };
    let Request::Started(_) = s.request(URI, 2) else {
        panic!("a new version starts")
    };
    assert_eq!(s.take_cancelled(), vec![URI.to_string()]);
    // The old answer arrives late: ignored.
    assert!(s.deliver(URI, 1, vec![]).is_none());
    s.cancel(old);
    assert!(s.deliver(URI, 2, sample()).is_some());
    assert!(matches!(s.request(URI, 2), Request::Cached));
    assert_eq!(s.cached(URI).unwrap().list_of_symbols().len(), 4);
    s.forget(URI);
    assert!(s.cached(URI).is_none());
}

#[test]
fn every_show_setting_hides_exactly_its_symbol_kind() {
    use vsr_exthost::language_features::outline::KIND_SETTINGS;
    for (kind, name) in KIND_SETTINGS.iter().enumerate() {
        let key = format!("breadcrumbs.{name}");
        let filter = KindFilter::from_settings("breadcrumbs", |k| (k == key).then_some(false));
        for other in 0..26u32 {
            assert_eq!(
                filter.allows(other),
                other as usize != kind,
                "{name} hides kind {kind} only"
            );
        }
    }
}
