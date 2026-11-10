//! The Outline view (ADR-0092): the symbols of the active document, in a section under the
//! Explorer tree. [`rows`] and [`status_of`] are UI free and hold the behaviour (the tests
//! assert on them); the section draws them, and a click on a row selects its symbol.
//!
//! Upstream source: src/vs/workbench/contrib/outline/browser/outlinePane.ts (the section, the
//! messages, `collapsed` by default)
//! Upstream source: src/vs/workbench/contrib/codeEditor/browser/outline/documentSymbolsTree.ts (rows, icons)
//! Upstream source: src/vs/editor/common/languages.ts (`SymbolKinds.toIcon`, the codicon per kind)

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gpui_kit::*;
use vsr_exthost::language_features::dto::Range as SymbolRange;
use vsr_exthost::language_features::outline::{Item, KindFilter, NodeId, OutlineModel, ROOT, SortOrder};
use vsr_search::SearchRange;
use vsr_settings::Configuration;

use crate::Workbench;
use crate::config::AppConfig;
use crate::config::settings::OpenSource;
use crate::icons;
use crate::ui::to_gpui;

/// Height of the section's list when it is open.
const BODY_HEIGHT: f32 = 240.;

/// The messages of the section (upstream's `outlinePane.ts`).
pub const NO_PROVIDER: &str = "The active editor cannot provide outline information.";

/// The `outline.*` settings the view reads.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineSettings {
    /// `outline.collapseItems`: `alwaysCollapse` starts every item collapsed.
    pub collapse_all: bool,
    /// `outline.icons`: a codicon per symbol kind.
    pub icons: bool,
    /// The `outline.show<Kind>` filter.
    pub filter: KindFilter,
}

impl OutlineSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            collapse_all: c.get_str("outline.collapseItems") == Some("alwaysCollapse"),
            icons: c.get_bool("outline.icons").unwrap_or(true),
            filter: KindFilter::from_settings("outline", |key| c.get_bool(key)),
        }
    }
}

/// One row of the view: a symbol, or a provider's group when several providers answered.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineRow {
    /// The stable id of the node (`TreeElement.findId`); expansion is kept per id.
    pub id: String,
    pub depth: usize,
    pub name: String,
    pub detail: String,
    /// The symbol kind; `None` for a provider group.
    pub kind: Option<u32>,
    /// The range a click selects; `None` for a group.
    pub range: Option<SymbolRange>,
    pub expandable: bool,
    pub expanded: bool,
}

/// The rows of `model`, depth first. An element hidden by the kind filter hides its subtree.
/// `toggled` holds the ids whose expansion differs from the default (`collapseItems`).
pub fn rows(model: &OutlineModel, settings: &OutlineSettings, toggled: &HashSet<String>) -> Vec<OutlineRow> {
    let mut out = Vec::new();
    walk(model, &model.visible_children(ROOT, SortOrder::Position, &settings.filter), 0, settings, toggled, &mut out);
    out
}

fn walk(
    model: &OutlineModel,
    nodes: &[NodeId],
    depth: usize,
    settings: &OutlineSettings,
    toggled: &HashSet<String>,
    out: &mut Vec<OutlineRow>,
) {
    for &id in nodes {
        let node = model.node(id);
        let (name, detail, kind, range) = match &node.item {
            Item::Root => continue,
            Item::Group { label, .. } => (label.clone(), String::new(), None, None),
            Item::Element { symbol, .. } => {
                (symbol.name.clone(), symbol.detail.clone(), Some(symbol.kind), Some(symbol.selection_range))
            }
        };
        let children = model.visible_children(id, SortOrder::Position, &settings.filter);
        let expandable = !children.is_empty();
        let expanded = !settings.collapse_all != toggled.contains(&node.id);
        out.push(OutlineRow { id: node.id.clone(), depth, name, detail, kind, range, expandable, expanded });
        if expandable && expanded {
            walk(model, &children, depth + 1, settings, toggled, out);
        }
    }
}

/// What the section shows in place of the rows (or the rows, when there is something to show).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OutlineStatus {
    /// No editor shows a file (or the provider cannot give symbols for it).
    NoEditor,
    /// The symbols are being asked for; the file's name.
    Loading(String),
    /// No provider answered for the document.
    NoProvider,
    /// The providers answered with nothing; the file's name.
    NoSymbols(String),
    Rows,
}

/// The status of the view. `doc` is the file's name when an editor shows a file; `answered` is
/// how many providers answered for it (`None`: nothing has answered yet).
pub fn status_of(
    doc: Option<&str>,
    model: Option<&OutlineModel>,
    answered: Option<usize>,
    host_running: bool,
    has_rows: bool,
) -> OutlineStatus {
    let Some(name) = doc else {
        return OutlineStatus::NoEditor;
    };
    if answered == Some(0) {
        return OutlineStatus::NoProvider;
    }
    match model {
        None if host_running => OutlineStatus::Loading(name.to_string()),
        None => OutlineStatus::NoProvider,
        Some(_) if !has_rows => OutlineStatus::NoSymbols(name.to_string()),
        Some(_) => OutlineStatus::Rows,
    }
}

/// The codicon of a symbol kind (`SymbolKinds.toIcon`); an unknown kind shows the property icon.
pub fn symbol_glyph(kind: u32) -> char {
    match kind {
        0 => '\u{EB60}',                // file
        1..=3 | 18 => '\u{EA8B}',       // module, namespace, package, object
        4 => '\u{EB5B}',                // class
        5 | 8 | 11 => '\u{EA8C}',       // method, constructor, function
        6 => '\u{EB65}',                // property
        7 => '\u{EB5F}',                // field
        9 => '\u{EA95}',                // enum
        10 => '\u{EB61}',               // interface
        12 => '\u{EA88}',               // variable
        13 => '\u{EB5D}',               // constant
        14 => '\u{EB8D}',               // string
        15 => '\u{EA90}',               // number
        16 | 20 => '\u{EA8F}',          // boolean, null
        17 => '\u{EA8A}',               // array
        19 => '\u{EA93}',               // key
        21 => '\u{EB5E}',               // enum member
        22 => '\u{EA91}',               // struct
        23 => '\u{EA86}',               // event
        24 => '\u{EB64}',               // operator
        25 => '\u{EA92}',               // type parameter
        _ => '\u{EB65}',
    }
}

/// The view's state, kept in the outline (ADR-0092).
#[derive(Default)]
pub struct ViewState {
    /// The section is open. Upstream's Outline view starts collapsed.
    pub open: bool,
    /// Per document: the ids whose expansion was toggled since the default.
    pub toggled: std::collections::HashMap<String, HashSet<String>>,
}

/// A symbol's range as the editor selects it. Symbols and editor positions are both 1-based
/// (`Selection` starts at 1:1), so the range passes through unchanged.
pub(crate) fn symbol_search_range(range: SymbolRange) -> SearchRange {
    SearchRange {
        start_line: range.start_line as usize,
        start_column: range.start_column as usize,
        end_line: range.end_line as usize,
        end_column: range.end_column as usize,
    }
}

impl Workbench {
    /// The file the view describes: the active editor's file, when an editor shows one.
    pub(crate) fn outline_document(&self) -> Option<PathBuf> {
        self.editor_surface_active()
            .then(|| self.editors.active_editor())
            .flatten()
            .filter(|e| e.path.is_absolute() && !e.is_untitled())
            .map(|e| e.path.clone())
    }

    /// The rows the section shows for the active document.
    pub(crate) fn outline_rows(&self, cx: &App) -> Vec<OutlineRow> {
        let Some(path) = self.outline_document() else {
            return Vec::new();
        };
        let Some(model) = self.outline.model(&path) else {
            return Vec::new();
        };
        let settings = OutlineSettings::from_configuration(AppConfig::get(cx).configuration());
        let toggled = self.outline.view.toggled.get(&super::key(&path)).cloned().unwrap_or_default();
        rows(model, &settings, &toggled)
    }

    /// What the section shows for the active document (the rows when there are any).
    pub(crate) fn outline_status(&self, cx: &App) -> OutlineStatus {
        let doc = self.outline_document();
        let name = doc.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
        let model = doc.as_deref().and_then(|p| self.outline.model(p));
        let answered = doc.as_deref().and_then(|p| self.outline.answered(p));
        let has_rows = !self.outline_rows(cx).is_empty();
        status_of(name.as_deref(), model, answered, self.ext.is_running(), has_rows)
    }

    /// Opens or closes the section's list.
    pub(crate) fn toggle_outline_section(&mut self, cx: &mut Context<Self>) {
        self.outline.view.open = !self.outline.view.open;
        cx.notify();
    }

    /// Expands or collapses the item `id` of the active document.
    pub(crate) fn toggle_outline_item(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(path) = self.outline_document() else {
            return;
        };
        let set = self.outline.view.toggled.entry(super::key(&path)).or_default();
        if !set.remove(id) {
            set.insert(id.to_string());
        }
        cx.notify();
    }

    /// Selects a symbol's range in the editor of `path`, opening the file from `source` (the
    /// outline's click is code navigation; Go to Symbol is a Quick Open).
    pub(crate) fn select_symbol(
        &mut self,
        path: &Path,
        range: SymbolRange,
        source: OpenSource,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        self.open_at(path, symbol_search_range(range), source, window, cx);
        cx.notify();
    }

    /// The Outline section under the Explorer tree (`outline` view, ADR-0092).
    pub(crate) fn outline_section(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let open = self.outline.view.open;
        let header = div()
            .id("outline-header")
            .debug_selector(|| "outline-header".to_string())
            .flex_none()
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .cursor_pointer()
            .child(
                div()
                    .flex_none()
                    .w(px(16.))
                    .font_family(icons::FAMILY)
                    .text_size(px(16.))
                    .child(icons::glyph(if open { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT })),
            )
            .child("OUTLINE")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_outline_section(cx)));
        let section = div().flex_none().flex().flex_col().child(header);
        if !open {
            return section.into_any_element();
        }

        let message = |text: String| {
            div()
                .px(px(20.))
                .py(px(4.))
                .text_size(px(12.))
                .text_color(to_gpui(t.tab_inactive_fg))
                .child(text)
        };
        let body = match self.outline_status(cx) {
            OutlineStatus::NoEditor | OutlineStatus::NoProvider => message(NO_PROVIDER.to_string()).into_any_element(),
            OutlineStatus::Loading(name) => {
                message(format!("Loading document symbols for '{name}'...")).into_any_element()
            }
            OutlineStatus::NoSymbols(name) => {
                message(format!("No symbols found in document '{name}'")).into_any_element()
            }
            OutlineStatus::Rows => {
                let doc = self.outline_document().unwrap_or_default();
                let items: Vec<AnyElement> = self
                    .outline_rows(cx)
                    .into_iter()
                    .enumerate()
                    .map(|(i, row)| self.outline_row(i, row, &doc, cx))
                    .collect();
                div()
                    .id("outline-scroll")
                    .flex_none()
                    .h(px(BODY_HEIGHT))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(items)
                    .into_any_element()
            }
        };
        section.child(body).into_any_element()
    }

    /// One row: the disclosure chevron (toggles the item), the kind's icon, the name and detail.
    /// A click on a symbol selects its range; a click on a group toggles it.
    fn outline_row(&self, i: usize, row: OutlineRow, doc: &Path, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let indent = self.layout(cx).tree_indent;
        let icons_on = OutlineSettings::from_configuration(AppConfig::get(cx).configuration()).icons;
        let disclosure = if row.expandable {
            let (id, debug) = (row.id.clone(), format!("outline-toggle:{}", row.id));
            div()
                .id(("outline-toggle", i))
                .debug_selector(move || debug.clone())
                .flex_none()
                .w(px(16.))
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .child(icons::glyph(if row.expanded { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.toggle_outline_item(&id, cx);
                }))
                .into_any_element()
        } else {
            div().flex_none().w(px(16.)).into_any_element()
        };
        let icon = icons_on.then(|| {
            div()
                .flex_none()
                .w(px(16.))
                .mr(px(4.))
                .font_family(icons::FAMILY)
                .text_size(px(14.))
                .child(icons::glyph(symbol_glyph(row.kind.unwrap_or(0))))
        });
        let detail = (!row.detail.is_empty()).then(|| {
            div()
                .ml(px(6.))
                .overflow_hidden()
                .whitespace_nowrap()
                .text_color(to_gpui(t.tab_inactive_fg))
                .child(row.detail.clone())
        });
        let debug = format!("outline-row:{}", row.id);
        let (id, range, doc) = (row.id.clone(), row.range, doc.to_path_buf());
        div()
            .id(("outline", i))
            .flex_none()
            .h(px(22.))
            .w_full()
            .flex()
            .items_center()
            .pl(px(8. + row.depth as f32 * indent))
            .pr(px(8.))
            .text_size(px(13.))
            .cursor_pointer()
            .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
            .debug_selector(move || debug.clone())
            .child(disclosure)
            .children(icon)
            .child(div().overflow_hidden().whitespace_nowrap().child(row.name.clone()))
            .children(detail)
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| match range {
                Some(range) => this.select_symbol(&doc, range, OpenSource::CodeNavigation, window, cx),
                None => this.toggle_outline_item(&id, cx),
            }))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // Explicit imports: `super::*` would bring gpui_kit's glob exports (its own `test`) into scope.
    use std::collections::HashSet;

    use super::{OutlineRow, OutlineSettings, OutlineStatus, rows, status_of, symbol_glyph};
    use vsr_exthost::language_features::dto::{DocumentSymbol, Range};
    use vsr_exthost::language_features::outline::{KindFilter, OutlineModel, ProviderSymbols};

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

    /// `class C { fn m() {} }` on lines 1-3, and `const X` on line 5.
    fn model() -> OutlineModel {
        let m = sym("m", 5, Range::new(2, 3, 2, 14), vec![]);
        let c = sym("C", 4, Range::new(1, 1, 3, 2), vec![m]);
        let x = sym("X", 13, Range::new(5, 1, 5, 12), vec![]);
        OutlineModel::create("file:///a.ts", vec![ProviderSymbols::new(Some("ts"), vec![c, x])])
    }

    fn settings(collapse_all: bool) -> OutlineSettings {
        OutlineSettings { collapse_all, icons: true, filter: KindFilter::default() }
    }

    fn names(rows: &[OutlineRow]) -> Vec<(&str, usize)> {
        rows.iter().map(|r| (r.name.as_str(), r.depth)).collect()
    }

    #[test]
    fn items_start_expanded_unless_collapse_all() {
        let m = model();
        let none = HashSet::new();
        assert_eq!(names(&rows(&m, &settings(false), &none)), [("C", 0), ("m", 1), ("X", 0)]);
        assert_eq!(names(&rows(&m, &settings(true), &none)), [("C", 0), ("X", 0)]);
    }

    #[test]
    fn a_toggled_id_flips_its_default() {
        let m = model();
        let id = rows(&m, &settings(false), &HashSet::new())[0].id.clone();
        let toggled: HashSet<String> = [id].into_iter().collect();
        assert_eq!(names(&rows(&m, &settings(false), &toggled)), [("C", 0), ("X", 0)], "collapsed by the click");
        assert_eq!(names(&rows(&m, &settings(true), &toggled)), [("C", 0), ("m", 1), ("X", 0)], "expanded by the click");
    }

    #[test]
    fn a_kind_the_filter_hides_takes_its_children_with_it() {
        let m = model();
        let mut s = settings(false);
        s.filter.set(4, false); // class
        assert_eq!(names(&rows(&m, &s, &HashSet::new())), [("X", 0)]);
    }

    #[test]
    fn the_rows_carry_the_selection_range_of_each_symbol() {
        let m = model();
        let r = rows(&m, &settings(false), &HashSet::new());
        assert_eq!(r[0].range, Some(Range::new(1, 1, 3, 2)));
        assert!(r[0].expandable);
        assert!(!r[2].expandable);
    }

    #[test]
    fn several_providers_show_their_groups_first() {
        let a = sym("a", 11, Range::new(1, 1, 1, 5), vec![]);
        let b = sym("b", 11, Range::new(2, 1, 2, 5), vec![]);
        let m = OutlineModel::create(
            "file:///x",
            vec![ProviderSymbols::new(Some("one"), vec![a]), ProviderSymbols::new(Some("two"), vec![b])],
        );
        let r = rows(&m, &settings(false), &HashSet::new());
        assert_eq!(names(&r), [("one", 0), ("a", 1), ("two", 0), ("b", 1)]);
        assert_eq!(r[0].range, None, "a group selects nothing");
    }

    #[test]
    fn the_status_names_the_empty_states_in_upstreams_words() {
        let m = model();
        assert_eq!(status_of(None, None, None, false, false), OutlineStatus::NoEditor);
        assert_eq!(status_of(Some("a.ts"), None, None, false, false), OutlineStatus::NoProvider);
        assert_eq!(status_of(Some("a.ts"), None, None, true, false), OutlineStatus::Loading("a.ts".into()));
        assert_eq!(status_of(Some("a.ts"), Some(&m), Some(0), true, false), OutlineStatus::NoProvider);
        assert_eq!(status_of(Some("a.ts"), Some(&m), Some(1), true, false), OutlineStatus::NoSymbols("a.ts".into()));
        assert_eq!(status_of(Some("a.ts"), Some(&m), Some(1), true, true), OutlineStatus::Rows);
    }

    #[test]
    fn each_kind_has_the_codicon_of_upstreams_table() {
        assert_eq!(symbol_glyph(4), '\u{EB5B}', "class");
        assert_eq!(symbol_glyph(11), '\u{EA8C}', "function");
        assert_eq!(symbol_glyph(25), '\u{EA92}', "type parameter");
        assert_eq!(symbol_glyph(99), '\u{EB65}', "unknown falls back to property");
    }
}
