//! The outline model: the symbols of one document as a tree grouped by provider, with the
//! sorting and kind filters the Outline view and the breadcrumbs share. UI-free (ADR-0085).
//!
//! upstream: src/vs/editor/contrib/documentSymbols/browser/outlineModel.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/codeEditor/browser/outline/documentSymbolsTree.ts @ 50e1350fc78d
//!
//! * [`OutlineModel::create`] merges the answers of several providers. A provider's symbols
//!   form a group; with one non-empty group its symbols become the top level (`_compact`).
//! * Ids are the upstream `TreeElement.findId` scheme (`root/provider_0/foo`, with suffixes when
//!   names collide) so a tree keeps its expand state across refreshes.
//! * [`OutlineModel::update_markers_in`] attaches diagnostics to the innermost symbol that
//!   contains them (`outline.problems.*`), stealing each marker from the first symbol that takes it.
//! * [`OutlineModel::item_enclosing`] is the symbol at a position, the basis of "follow cursor"
//!   and the breadcrumbs path.
//! * [`OutlineService`] is the per-document cache with shared cancellation (`OutlineModelService`).
//!
//! Positions are 1-based like [`super::dto`]. Sorting uses a natural (numeric-aware) name
//! compare instead of ICU's `Intl.Collator`: the same order for ordinary names, not identical
//! for every locale.

use super::dto::{DocumentSymbol, Position, Range};
use std::cmp::Ordering;
use std::collections::HashMap;

/// The name a provider group gets when the provider has no display name (upstream).
pub const UNKNOWN_PROVIDER: &str = "Unknown Outline Provider";

/// Index of a node in the model's arena. [`ROOT`] is the model itself.
pub type NodeId = usize;
pub const ROOT: NodeId = 0;

/// Marker severities, numerically ordered as in `MarkerSeverity` (`Error` is the largest).
pub mod severity {
    pub const HINT: u8 = 1;
    pub const INFO: u8 = 2;
    pub const WARNING: u8 = 4;
    pub const ERROR: u8 = 8;
}

/// A diagnostic, reduced to what the outline needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Marker {
    pub range: Range,
    pub severity: u8,
}

/// The diagnostics under one symbol: how many, and the worst severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerCount {
    pub count: usize,
    pub top_severity: u8,
}

/// What one provider answered, with its display name.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderSymbols {
    pub label: String,
    pub symbols: Vec<DocumentSymbol>,
}

impl ProviderSymbols {
    pub fn new(label: Option<&str>, symbols: Vec<DocumentSymbol>) -> Self {
        Self {
            label: label.unwrap_or(UNKNOWN_PROVIDER).to_string(),
            symbols,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Root,
    /// One provider's answer; `order` is its position in the registry.
    Group {
        label: String,
        order: usize,
    },
    Element {
        symbol: DocumentSymbol,
        marker: Option<MarkerCount>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub item: Item,
}

/// The symbols of one document.
#[derive(Debug, Clone, PartialEq)]
pub struct OutlineModel {
    pub uri: String,
    nodes: Vec<Node>,
    /// The non-empty groups, in provider order (upstream `_groups`).
    groups: Vec<NodeId>,
}

/// `Range.compareRangesUsingStarts`: by start, then by end.
pub fn compare_ranges_using_starts(a: &Range, b: &Range) -> Ordering {
    (a.start(), a.end()).cmp(&(b.start(), b.end()))
}

/// `Range.areIntersecting`: touching ranges do not intersect.
pub fn are_intersecting(a: &Range, b: &Range) -> bool {
    a.end() > b.start() && b.end() > a.start()
}

impl OutlineModel {
    /// Builds the model from the answers of the providers, in registry order.
    pub fn create(uri: &str, providers: Vec<ProviderSymbols>) -> OutlineModel {
        let mut m = OutlineModel {
            uri: uri.to_string(),
            nodes: vec![Node {
                id: "root".into(),
                parent: None,
                children: vec![],
                item: Item::Root,
            }],
            groups: vec![],
        };
        for (index, provider) in providers.into_iter().enumerate() {
            let id = format!("root/provider_{index}");
            let group = m.push(
                id,
                ROOT,
                Item::Group {
                    label: provider.label,
                    order: index,
                },
            );
            for symbol in &provider.symbols {
                m.make_element(symbol, group);
            }
            if !m.nodes[group].children.is_empty() {
                m.groups.push(group);
            }
        }
        m.compact();
        m
    }

    fn push(&mut self, id: String, parent: NodeId, item: Item) -> NodeId {
        let node = self.nodes.len();
        self.nodes.push(Node {
            id,
            parent: Some(parent),
            children: vec![],
            item,
        });
        self.nodes[parent].children.push(node);
        node
    }

    fn make_element(&mut self, symbol: &DocumentSymbol, container: NodeId) {
        let id = self.find_id(symbol, container);
        let node = self.nodes.len();
        self.nodes.push(Node {
            id,
            parent: Some(container),
            children: vec![],
            item: Item::Element {
                symbol: symbol.clone(),
                marker: None,
            },
        });
        for child in &symbol.children {
            self.make_element(child, node);
        }
        self.nodes[container].children.push(node);
    }

    /// `TreeElement.findId`: `<container>/<name>`, then `<container>/<name>_<line>_<col>` when a
    /// sibling has that name, then `_<i>` suffixes until the id is free.
    fn find_id(&self, symbol: &DocumentSymbol, container: NodeId) -> String {
        let taken = |id: &str| {
            self.nodes[container]
                .children
                .iter()
                .any(|&c| self.nodes[c].id == id)
        };
        let parent_id = &self.nodes[container].id;
        let mut candidate = format!("{parent_id}/{}", symbol.name);
        if taken(&candidate) {
            candidate = format!(
                "{parent_id}/{}_{}_{}",
                symbol.name, symbol.range.start_line, symbol.range.start_column
            );
        }
        let mut id = candidate.clone();
        let mut i = 0;
        while taken(&id) {
            id = format!("{candidate}_{i}");
            i += 1;
        }
        id
    }

    /// `_compact`: one non-empty group's symbols become the top level; otherwise the groups do.
    fn compact(&mut self) {
        if self.groups.len() == 1 {
            let group = self.groups[0];
            let kids = self.nodes[group].children.clone();
            for &k in &kids {
                self.nodes[k].parent = Some(ROOT);
            }
            self.nodes[ROOT].children = kids;
        } else {
            self.nodes[ROOT].children = self.groups.clone();
        }
    }

    pub fn node(&self, id: NodeId) -> &Node {
        &self.nodes[id]
    }

    /// The symbol of an element node.
    pub fn symbol(&self, id: NodeId) -> Option<&DocumentSymbol> {
        match &self.nodes[id].item {
            Item::Element { symbol, .. } => Some(symbol),
            _ => None,
        }
    }

    pub fn is_group(&self, id: NodeId) -> bool {
        matches!(self.nodes[id].item, Item::Group { .. })
    }

    /// The non-empty provider groups, in provider order.
    pub fn groups(&self) -> &[NodeId] {
        &self.groups
    }

    /// The top-level nodes: the groups when several providers answered, else the symbols.
    pub fn top_level(&self) -> &[NodeId] {
        &self.nodes[ROOT].children
    }

    /// The model's node with this id (`TreeElement.getElementById`, by exact id).
    pub fn item_by_id(&self, id: &str) -> Option<NodeId> {
        self.nodes.iter().position(|n| n.id == id)
    }

    /// The symbol that encloses `pos`: the first child (in order) whose range contains the
    /// position, then its first matching child, and so on (`getItemEnclosingPosition`). With a
    /// `context` element, groups other than its own provider's are only used when no group
    /// of that provider matches.
    pub fn item_enclosing(&self, pos: Position, context: Option<NodeId>) -> Option<NodeId> {
        let preferred = context.and_then(|c| {
            let mut cand = self.nodes[c].parent;
            while let Some(p) = cand {
                if self.is_group(p) {
                    return Some(p);
                }
                cand = self.nodes[p].parent;
            }
            None
        });
        let mut result = None;
        for &group in &self.groups {
            result = self.enclosing_in(group, pos);
            if result.is_some() && (preferred.is_none() || preferred == Some(group)) {
                break;
            }
        }
        result
    }

    fn enclosing_in(&self, container: NodeId, pos: Position) -> Option<NodeId> {
        for &child in &self.nodes[container].children {
            let Item::Element { symbol, .. } = &self.nodes[child].item else {
                continue;
            };
            if !symbol.range.contains(pos) {
                continue;
            }
            return self.enclosing_in(child, pos).or(Some(child));
        }
        None
    }

    /// The chain of symbols from the top down to `id`, excluding groups.
    pub fn path_to(&self, id: NodeId) -> Vec<NodeId> {
        let mut path = vec![];
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == ROOT {
                break;
            }
            if !self.is_group(c) {
                path.push(c);
            }
            cur = self.nodes[c].parent;
        }
        path.reverse();
        path
    }

    /// Attaches markers to the symbols under `container` (a group, or the root when compacted).
    /// `markers` must be sorted by [`compare_ranges_using_starts`]; each marker a symbol takes is
    /// removed from the list, so later symbols and siblings do not count it again.
    pub fn update_markers_in(&mut self, container: NodeId, markers: &mut Vec<Marker>) {
        for child in self.nodes[container].children.clone() {
            self.update_marker_item(child, markers);
        }
    }

    fn update_marker_item(&mut self, item: NodeId, markers: &mut Vec<Marker>) {
        let Item::Element { symbol, .. } = &self.nodes[item].item else {
            return;
        };
        let range = symbol.range;
        let mut start = markers
            .partition_point(|m| compare_ranges_using_starts(&m.range, &range) == Ordering::Less);
        while start > 0 && are_intersecting(&markers[start - 1].range, &range) {
            start -= 1;
        }
        let mut end = start;
        while end < markers.len() && are_intersecting(&range, &markers[end].range) {
            end += 1;
        }
        let mut mine: Vec<Marker> = markers.drain(start..end).collect();
        let top = mine.iter().map(|m| m.severity).max();
        for child in self.nodes[item].children.clone() {
            self.update_marker_item(child, &mut mine);
        }
        if let Item::Element { marker, .. } = &mut self.nodes[item].item {
            *marker = top.map(|top_severity| MarkerCount {
                count: mine.len(),
                top_severity,
            });
        }
    }

    /// Attaches all markers (sorted in place) to every group.
    pub fn update_marker(&mut self, markers: &mut [Marker]) {
        markers.sort_by(|a, b| compare_ranges_using_starts(&a.range, &b.range));
        let groups = self.groups.clone();
        for group in groups {
            let mut copy = markers.to_vec();
            self.update_markers_in(group, &mut copy);
        }
    }

    /// The marker summary of an element.
    pub fn marker(&self, id: NodeId) -> Option<MarkerCount> {
        match self.nodes[id].item {
            Item::Element { marker, .. } => marker,
            _ => None,
        }
    }

    /// Every symbol, depth first, with its container name (the parent's name when the provider
    /// gave none) and sorted by start, then by end descending (`asListOfDocumentSymbols`). Used
    /// by Go to Symbol.
    pub fn list_of_symbols(&self) -> Vec<DocumentSymbol> {
        let mut roots: Vec<(NodeId, String)> = vec![];
        for &top in self.top_level() {
            if self.is_group(top) {
                for &c in &self.nodes[top].children {
                    roots.push((c, String::new()));
                }
            } else {
                roots.push((top, String::new()));
            }
        }
        let mut out = vec![];
        for (id, container) in roots {
            self.flatten_into(id, &container, &mut out);
        }
        out.sort_by(|a, b| {
            a.range
                .start()
                .cmp(&b.range.start())
                .then(b.range.end().cmp(&a.range.end()))
        });
        out
    }

    fn flatten_into(&self, id: NodeId, container: &str, out: &mut Vec<DocumentSymbol>) {
        let Item::Element { symbol, .. } = &self.nodes[id].item else {
            return;
        };
        let mut flat = symbol.clone();
        flat.children = vec![];
        if flat.container_name.as_deref().unwrap_or("").is_empty() {
            flat.container_name = (!container.is_empty()).then(|| container.to_string());
        }
        out.push(flat);
        for &c in &self.nodes[id].children {
            self.flatten_into(c, &symbol.name, out);
        }
    }

    /// The children of `parent` as the outline shows them: filtered by kind (an element that is
    /// hidden hides its children too; groups always show) and sorted.
    pub fn visible_children(
        &self,
        parent: NodeId,
        order: SortOrder,
        filter: &KindFilter,
    ) -> Vec<NodeId> {
        let mut kids: Vec<NodeId> = self.nodes[parent]
            .children
            .iter()
            .copied()
            .filter(|&c| match &self.nodes[c].item {
                Item::Element { symbol, .. } => filter.allows(symbol.kind),
                _ => true,
            })
            .collect();
        kids.sort_by(|&a, &b| self.compare(a, b, order));
        kids
    }

    /// `DocumentSymbolComparator`: groups by provider order; symbols by position, type or name.
    fn compare(&self, a: NodeId, b: NodeId, order: SortOrder) -> Ordering {
        match (&self.nodes[a].item, &self.nodes[b].item) {
            (Item::Group { order: x, .. }, Item::Group { order: y, .. }) => x.cmp(y),
            (Item::Element { symbol: sa, .. }, Item::Element { symbol: sb, .. }) => match order {
                SortOrder::Position => compare_ranges_using_starts(&sa.range, &sb.range)
                    .then_with(|| natural_cmp(&sa.name, &sb.name)),
                SortOrder::Type => sa
                    .kind
                    .cmp(&sb.kind)
                    .then_with(|| natural_cmp(&sa.name, &sb.name)),
                SortOrder::Name => natural_cmp(&sa.name, &sb.name)
                    .then_with(|| compare_ranges_using_starts(&sa.range, &sb.range)),
            },
            _ => Ordering::Equal,
        }
    }
}

/// `outline.sortOrder`-style ordering (`breadcrumbs.symbolSortOrder` uses the same values).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortOrder {
    #[default]
    Position,
    Name,
    Type,
}

impl SortOrder {
    /// Unknown values fall back to `position`, the default.
    pub fn parse(value: Option<&str>) -> Self {
        match value {
            Some("name") => Self::Name,
            Some("type") => Self::Type,
            _ => Self::Position,
        }
    }
}

/// The `show<Kind>` settings in `SymbolKind` order (`DocumentSymbolFilter.kindToConfigName`).
pub const KIND_SETTINGS: [&str; 26] = [
    "showFiles",
    "showModules",
    "showNamespaces",
    "showPackages",
    "showClasses",
    "showMethods",
    "showProperties",
    "showFields",
    "showConstructors",
    "showEnums",
    "showInterfaces",
    "showFunctions",
    "showVariables",
    "showConstants",
    "showStrings",
    "showNumbers",
    "showBooleans",
    "showArrays",
    "showObjects",
    "showKeys",
    "showNull",
    "showEnumMembers",
    "showStructs",
    "showEvents",
    "showOperators",
    "showTypeParameters",
];

/// Which symbol kinds are shown. `outline.show<Kind>` for the Outline view and
/// `breadcrumbs.show<Kind>` for the breadcrumbs dropdown.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct KindFilter {
    hidden: [bool; 26],
}

impl KindFilter {
    /// Reads `<prefix>.<showKind>` with `lookup`; a missing key shows the kind (the default).
    pub fn from_settings(prefix: &str, lookup: impl Fn(&str) -> Option<bool>) -> Self {
        let mut f = Self::default();
        for (kind, name) in KIND_SETTINGS.iter().enumerate() {
            if let Some(shown) = lookup(&format!("{prefix}.{name}")) {
                f.hidden[kind] = !shown;
            }
        }
        f
    }

    pub fn set(&mut self, kind: u32, shown: bool) {
        if let Some(h) = self.hidden.get_mut(kind as usize) {
            *h = !shown;
        }
    }

    /// Unknown kinds are always shown.
    pub fn allows(&self, kind: u32) -> bool {
        self.hidden.get(kind as usize).is_none_or(|h| !h)
    }
}

/// A natural order: runs of digits compare by value, other text case-insensitively, and the
/// exact text breaks ties. Stands in for `Intl.Collator(undefined, { numeric: true })`.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let (mut ai, mut bi) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (ai.peek().copied(), bi.peek().copied()) {
            (None, None) => break,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = take_digits(&mut ai);
                let nb = take_digits(&mut bi);
                let ta = na.trim_start_matches('0');
                let tb = nb.trim_start_matches('0');
                let o = ta.len().cmp(&tb.len()).then_with(|| ta.cmp(tb));
                if o != Ordering::Equal {
                    return o;
                }
            }
            (Some(x), Some(y)) => {
                let o = x.to_lowercase().cmp(y.to_lowercase());
                if o != Ordering::Equal {
                    return o;
                }
                ai.next();
                bi.next();
            }
        }
    }
    a.cmp(b)
}

fn take_digits(it: &mut std::iter::Peekable<std::str::Chars>) -> String {
    let mut s = String::new();
    while let Some(c) = it.peek().copied().filter(char::is_ascii_digit) {
        s.push(c);
        it.next();
    }
    s
}

/// What a request for a document's outline finds (`OutlineModelService.getOrCreate`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    /// The cached model of this version.
    Cached,
    /// The providers must be asked; the answer comes back through [`OutlineService::deliver`].
    /// The ticket is what to cancel with.
    Started(u64),
    /// The same version is already being asked; this ticket waits for that answer.
    Joined(u64),
}

#[derive(Debug, Default)]
struct Pending {
    version: u64,
    tickets: Vec<u64>,
}

/// The outline of each document, kept per version, with shared cancellation
/// (`OutlineModelService`). UI-free: the app asks the providers and calls [`deliver`](Self::deliver).
#[derive(Debug, Default)]
pub struct OutlineService {
    cache: HashMap<String, (u64, OutlineModel)>,
    pending: HashMap<String, Pending>,
    cancelled: Vec<String>,
    next_ticket: u64,
    ticket_uri: HashMap<u64, String>,
}

impl OutlineService {
    pub fn new() -> Self {
        Self::default()
    }

    /// Asks for the outline of `uri` at `version`. Cached when that version was delivered;
    /// joins a request already waiting for the same version; otherwise starts one. A waiting
    /// request for another version is superseded (its providers are reported by
    /// [`take_cancelled`](Self::take_cancelled)).
    pub fn request(&mut self, uri: &str, version: u64) -> Request {
        if let Some((v, _)) = self.cache.get(uri)
            && *v == version
        {
            return Request::Cached;
        }
        let ticket = self.next_ticket;
        self.next_ticket += 1;
        match self.pending.get_mut(uri) {
            Some(p) if p.version == version => {
                p.tickets.push(ticket);
                self.ticket_uri.insert(ticket, uri.to_string());
                Request::Joined(ticket)
            }
            Some(_) => {
                self.cancelled.push(uri.to_string());
                self.pending.insert(
                    uri.to_string(),
                    Pending {
                        version,
                        tickets: vec![ticket],
                    },
                );
                self.ticket_uri.insert(ticket, uri.to_string());
                Request::Started(ticket)
            }
            None => {
                self.pending.insert(
                    uri.to_string(),
                    Pending {
                        version,
                        tickets: vec![ticket],
                    },
                );
                self.ticket_uri.insert(ticket, uri.to_string());
                Request::Started(ticket)
            }
        }
    }

    /// One requester gives up. When the last one does, the providers' work for it is cancelled
    /// (see [`take_cancelled`](Self::take_cancelled)). Cancelling a cached or unknown ticket is a no-op.
    pub fn cancel(&mut self, ticket: u64) {
        let Some(uri) = self.ticket_uri.remove(&ticket) else {
            return;
        };
        let Some(p) = self.pending.get_mut(&uri) else {
            return;
        };
        p.tickets.retain(|&t| t != ticket);
        if p.tickets.is_empty() {
            self.pending.remove(&uri);
            self.cancelled.push(uri);
        }
    }

    /// Documents whose providers should be cancelled, since the last call.
    pub fn take_cancelled(&mut self) -> Vec<String> {
        std::mem::take(&mut self.cancelled)
    }

    /// The providers answered for `uri` at `version`. Ignored when that request is no longer
    /// pending (cancelled, or superseded by a newer version). Returns the new model.
    pub fn deliver(
        &mut self,
        uri: &str,
        version: u64,
        providers: Vec<ProviderSymbols>,
    ) -> Option<&OutlineModel> {
        let p = self.pending.get(uri)?;
        if p.version != version {
            return None;
        }
        let tickets = self.pending.remove(uri)?.tickets;
        for t in tickets {
            self.ticket_uri.remove(&t);
        }
        let model = OutlineModel::create(uri, providers);
        self.cache.insert(uri.to_string(), (version, model));
        self.cache.get(uri).map(|(_, m)| m)
    }

    /// The cached model of `uri`, whatever its version.
    pub fn cached(&self, uri: &str) -> Option<&OutlineModel> {
        self.cache.get(uri).map(|(_, m)| m)
    }

    /// Drops a document's outline (the document closed).
    pub fn forget(&mut self, uri: &str) {
        self.cache.remove(uri);
        if let Some(p) = self.pending.remove(uri) {
            for t in p.tickets {
                self.ticket_uri.remove(&t);
            }
        }
    }
}
