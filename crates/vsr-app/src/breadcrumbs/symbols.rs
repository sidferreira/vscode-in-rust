//! The symbol part of the breadcrumbs (ADR-0087): the symbols that contain the cursor, after the
//! file path. UI-free: the crumbs and the dropdown rows are computed here, the bar draws them.
//!
//! Upstream source: src/vs/workbench/browser/parts/editor/breadcrumbsModel.ts (`symbolPath`)
//! Upstream source: src/vs/workbench/browser/parts/editor/breadcrumbsPicker.ts (siblings, sorting)
//!
//! Settings: `breadcrumbs.symbolPath` (`on` default, `off`, `last` keeps only the innermost
//! symbol), `breadcrumbs.symbolSortOrder` and the `breadcrumbs.show<Kind>` filter, which applies
//! to the dropdown only (upstream's path is not filtered).

use vsr_exthost::language_features::dto::Position;
use vsr_exthost::language_features::outline::{KindFilter, NodeId, OutlineModel, SortOrder};
use vsr_settings::Configuration;

/// `breadcrumbs.symbolPath`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolPath {
    On,
    Off,
    Last,
}

impl SymbolPath {
    /// Unknown values fall back to `on`, the default.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("off") => Self::Off,
            Some("last") => Self::Last,
            _ => Self::On,
        }
    }
}

/// The settings the symbol crumbs and their dropdown read.
#[derive(Clone, Debug, PartialEq)]
pub struct SymbolSettings {
    pub path: SymbolPath,
    pub order: SortOrder,
    pub filter: KindFilter,
}

impl SymbolSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            path: SymbolPath::parse(c.get_str("breadcrumbs.symbolPath")),
            order: SortOrder::parse(c.get_str("breadcrumbs.symbolSortOrder")),
            filter: KindFilter::from_settings("breadcrumbs", |key| c.get_bool(key)),
        }
    }
}

/// One crumb of the symbol path: the symbol, and its node in the model (for its siblings).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolCrumb {
    pub name: String,
    pub kind: u32,
    pub node: NodeId,
}

/// The symbols containing `cursor` (a 1-based editor position, as `Selection` reports), from the outermost. Empty when
/// no symbol contains it.
pub fn crumbs(model: &OutlineModel, cursor: (usize, usize), mode: SymbolPath) -> Vec<SymbolCrumb> {
    if mode == SymbolPath::Off {
        return Vec::new();
    }
    let pos = Position::new(cursor.0 as u32, cursor.1 as u32);
    let Some(inner) = model.item_enclosing(pos, None) else {
        return Vec::new();
    };
    let mut out: Vec<SymbolCrumb> = model
        .path_to(inner)
        .into_iter()
        .filter_map(|node| {
            let s = model.symbol(node)?;
            Some(SymbolCrumb {
                name: s.name.clone(),
                kind: s.kind,
                node,
            })
        })
        .collect();
    if mode == SymbolPath::Last {
        out = out.split_off(out.len().saturating_sub(1));
    }
    out
}

/// The dropdown of a symbol crumb: the siblings of `node` (its parent's children), as the
/// outline orders and filters them. Returns the node ids, in list order.
pub fn siblings(model: &OutlineModel, node: NodeId, settings: &SymbolSettings) -> Vec<NodeId> {
    let parent = model.node(node).parent.unwrap_or(0);
    model.visible_children(parent, settings.order, &settings.filter)
}
