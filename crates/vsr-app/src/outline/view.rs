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

use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use vsr_exthost::language_features::dto::{Position, Range as SymbolRange};
use vsr_exthost::language_features::outline::{
    Item, KindFilter, MarkerCount, NodeId, OutlineModel, ROOT, SortOrder, severity,
};
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
    /// `outline.problems.enabled`: diagnostics are shown on the symbols at all.
    pub problems: bool,
    /// `outline.problems.colors`: a symbol with a problem has its name in the severity's colour.
    pub problem_colors: bool,
    /// `outline.problems.badges`: a symbol with a problem shows the severity's icon.
    pub problem_badges: bool,
}

impl OutlineSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let on = |key: &str| c.get_bool(key).unwrap_or(true);
        Self {
            collapse_all: c.get_str("outline.collapseItems") == Some("alwaysCollapse"),
            icons: on("outline.icons"),
            filter: KindFilter::from_settings("outline", |key| c.get_bool(key)),
            problems: on("outline.problems.enabled"),
            problem_colors: on("outline.problems.colors"),
            problem_badges: on("outline.problems.badges"),
        }
    }
}

/// The severity a row's name takes its colour from, when colours are on (`outline.problems.colors`).
pub fn problem_color_severity(settings: &OutlineSettings, marker: Option<MarkerCount>) -> Option<u8> {
    marker.filter(|_| settings.problems && settings.problem_colors).map(|m| m.top_severity)
}

/// The severity of the badge a row shows, when badges are on (`outline.problems.badges`).
pub fn problem_badge_severity(settings: &OutlineSettings, marker: Option<MarkerCount>) -> Option<u8> {
    marker.filter(|_| settings.problems && settings.problem_badges).map(|m| m.top_severity)
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
    /// The symbol at the caret (follow cursor).
    pub selected: bool,
    /// The diagnostics under the symbol (`outline.problems.enabled`; none for a group).
    pub marker: Option<MarkerCount>,
}

/// What the rows depend on besides the settings: the view's own state.
#[derive(Clone, Copy, Debug)]
pub struct RowView<'a> {
    /// The ids whose expansion differs from the default (`collapseItems`).
    pub toggled: &'a HashSet<String>,
    /// The sort (`position`, `name` or `type`, the view's Sort By).
    pub order: SortOrder,
    /// The id of the symbol at the caret, when follow cursor is on; its ancestors are shown open.
    pub selected: Option<&'a str>,
    /// The text typed under filter on type: the rows whose name contains it, with their ancestors,
    /// all open. Empty: no filter.
    pub text: &'a str,
}

/// The rows of `model`, depth first. An element hidden by the kind filter hides its subtree.
pub fn rows(model: &OutlineModel, settings: &OutlineSettings, view: &RowView) -> Vec<OutlineRow> {
    let open = ancestors_of(model, view.selected);
    let mut out = Vec::new();
    walk(model, &model.visible_children(ROOT, view.order, &settings.filter), 0, settings, view, &open, &mut out);
    if !view.text.is_empty() {
        let needle = view.text.to_lowercase();
        let keep = filter_keep(&out, &needle);
        out = out.into_iter().zip(keep).filter_map(|(row, k)| k.then_some(row)).collect();
    }
    out
}

/// The ids of the ancestors of `id`: they are shown open, so the symbol is visible.
fn ancestors_of(model: &OutlineModel, id: Option<&str>) -> HashSet<String> {
    let Some(node) = id.and_then(|id| model.item_by_id(id)) else {
        return HashSet::new();
    };
    model.path_to(node).into_iter().filter(|&n| n != node).map(|n| model.node(n).id.clone()).collect()
}

fn walk(
    model: &OutlineModel,
    nodes: &[NodeId],
    depth: usize,
    settings: &OutlineSettings,
    view: &RowView,
    open: &HashSet<String>,
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
        let children = model.visible_children(id, view.order, &settings.filter);
        let expandable = !children.is_empty();
        let expanded = !view.text.is_empty() || open.contains(&node.id) || (!settings.collapse_all != view.toggled.contains(&node.id));
        let selected = view.selected == Some(node.id.as_str());
        let marker = settings.problems.then(|| model.marker(id)).flatten();
        out.push(OutlineRow {
            id: node.id.clone(),
            depth,
            name,
            detail,
            kind,
            range,
            expandable,
            expanded,
            selected,
            marker,
        });
        if expandable && expanded {
            walk(model, &children, depth + 1, settings, view, open, out);
        }
    }
}

/// Which rows survive the filter: a row whose name contains `needle`, and every ancestor of one.
fn filter_keep(rows: &[OutlineRow], needle: &str) -> Vec<bool> {
    let mut keep = vec![false; rows.len()];
    for i in 0..rows.len() {
        if !rows[i].name.to_lowercase().contains(needle) {
            continue;
        }
        keep[i] = true;
        let mut depth = rows[i].depth;
        for j in (0..i).rev() {
            if depth == 0 {
                break;
            }
            if rows[j].depth < depth {
                keep[j] = true;
                depth = rows[j].depth;
            }
        }
    }
    keep
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

/// The Sort By of the view (upstream's `OutlineSortOrder`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortBy {
    #[default]
    Position,
    Name,
    Category,
}

impl SortBy {
    /// The name kept in the workspace state.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Position => "position",
            Self::Name => "name",
            Self::Category => "category",
        }
    }

    /// Unknown names fall back to position, the default.
    pub fn parse(name: &str) -> Self {
        match name {
            "name" => Self::Name,
            "category" => Self::Category,
            _ => Self::Position,
        }
    }

    /// The next order, as the Sort By action cycles them.
    pub fn next(self) -> Self {
        match self {
            Self::Position => Self::Name,
            Self::Name => Self::Category,
            Self::Category => Self::Position,
        }
    }

    /// The symbol order it stands for (`Category` sorts by kind, upstream's `type`).
    pub fn order(self) -> SortOrder {
        match self {
            Self::Position => SortOrder::Position,
            Self::Name => SortOrder::Name,
            Self::Category => SortOrder::Type,
        }
    }
}

/// The view's state (ADR-0092, ADR-0093). The three toggles and the sort are kept per workspace.
#[derive(Default)]
pub struct ViewState {
    /// The section is open. Upstream's Outline view starts collapsed.
    pub open: bool,
    /// Per document: the ids whose expansion was toggled since the default.
    pub toggled: std::collections::HashMap<String, HashSet<String>>,
    /// Follow cursor: the symbol at the caret is selected and shown.
    pub follow_cursor: bool,
    pub sort_by: SortBy,
    /// Filter on type: typing narrows the rows; the text is not kept between sessions.
    pub filter_on_type: bool,
    pub filter_text: String,
    /// Where the keys go while the section is focused (filter on type reads them).
    pub focus: Option<gpui_kit::FocusHandle>,
}

impl ViewState {
    /// The part kept in the workspace state.
    pub fn persisted(&self) -> crate::session::state::OutlineViewState {
        crate::session::state::OutlineViewState {
            follow_cursor: self.follow_cursor,
            sort_by: self.sort_by.as_str().into(),
            filter_on_type: self.filter_on_type,
        }
    }

    /// Restores what the workspace state kept.
    pub fn restore(&mut self, state: &crate::session::state::OutlineViewState) {
        self.follow_cursor = state.follow_cursor;
        self.sort_by = SortBy::parse(&state.sort_by);
        self.filter_on_type = state.filter_on_type;
    }
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

/// The codicons of the header actions (`sort-precedence`, `eye`, `filter`).
const SORT_GLYPH: char = '\u{EB55}';
const FOLLOW_GLYPH: char = '\u{EA70}';
const FILTER_GLYPH: char = '\u{EAF1}';

impl Workbench {
    /// The file the view describes: the active editor's file, when an editor shows one.
    pub(crate) fn outline_document(&self) -> Option<PathBuf> {
        self.editor_surface_active()
            .then(|| self.editors.active_editor())
            .flatten()
            .filter(|e| e.path.is_absolute() && !e.is_untitled())
            .map(|e| e.path.clone())
    }

    /// The symbol at the caret of the active editor, when follow cursor is on (ADR-0093).
    fn outline_follow_target(&self, model: &OutlineModel) -> Option<String> {
        if !self.outline.view.follow_cursor {
            return None;
        }
        let sel = self.editors.active_editor()?.editor.selection();
        let at = Position::new(sel.active.line as u32, sel.active.column as u32);
        let node = model.item_enclosing(at, None)?;
        Some(model.node(node).id.clone())
    }

    /// The rows the section shows for the active document.
    pub(crate) fn outline_rows(&self, cx: &App) -> Vec<OutlineRow> {
        let Some(path) = self.outline_document() else {
            return Vec::new();
        };
        let Some(model) = self.outline.view_model(&path) else {
            return Vec::new();
        };
        let settings = OutlineSettings::from_configuration(AppConfig::get(cx).configuration());
        let toggled = self.outline.view.toggled.get(&super::key(&path)).cloned().unwrap_or_default();
        let selected = self.outline_follow_target(model);
        let text = if self.outline.view.filter_on_type { self.outline.view.filter_text.as_str() } else { "" };
        let view = RowView {
            toggled: &toggled,
            order: self.outline.view.sort_by.order(),
            selected: selected.as_deref(),
            text,
        };
        rows(model, &settings, &view)
    }

    /// What the section shows for the active document (the rows when there are any).
    pub(crate) fn outline_status(&self, cx: &App) -> OutlineStatus {
        let doc = self.outline_document();
        let name = doc.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().into_owned());
        let model = doc.as_deref().and_then(|p| self.outline.view_model(p));
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

    /// Filter on type: typing edits the filter text while the section is focused. Returns true
    /// when the key was taken.
    pub(crate) fn outline_filter_key(&mut self, ev: &KeyDownEvent) -> bool {
        if !self.outline.view.filter_on_type {
            return false;
        }
        let text = &mut self.outline.view.filter_text;
        match ev.keystroke.key.as_str() {
            "backspace" => {
                text.pop();
                true
            }
            "escape" => {
                text.clear();
                true
            }
            _ => {
                let m = &ev.keystroke.modifiers;
                match &ev.keystroke.key_char {
                    Some(c) if !c.is_empty() && !m.control && !m.platform && !m.alt => {
                        text.push_str(c);
                        true
                    }
                    _ => false,
                }
            }
        }
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
        let view = &self.outline.view;
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
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child("OUTLINE"))
            .when(open, |h| {
                h.child(self.outline_action(
                    "outline-sort",
                    SORT_GLYPH,
                    false,
                    format!("Sort By: {}", view.sort_by.as_str()),
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.outline.view.sort_by = this.outline.view.sort_by.next();
                        cx.notify();
                    }),
                ))
                .child(self.outline_action(
                    "outline-follow",
                    FOLLOW_GLYPH,
                    view.follow_cursor,
                    "Follow Cursor".to_string(),
                    cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.outline.view.follow_cursor = !this.outline.view.follow_cursor;
                        cx.notify();
                    }),
                ))
                .child(self.outline_action(
                    "outline-filter",
                    FILTER_GLYPH,
                    view.filter_on_type,
                    "Filter on Type".to_string(),
                    cx.listener(|this, _: &ClickEvent, window, cx| {
                        let on = !this.outline.view.filter_on_type;
                        this.outline.view.filter_on_type = on;
                        this.outline.view.filter_text.clear();
                        if let (true, Some(focus)) = (on, this.outline.view.focus.clone()) {
                            window.focus(&focus, cx);
                        }
                        cx.notify();
                    }),
                ))
            })
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
        let filter_line = view.filter_on_type.then(|| {
            let shown = if view.filter_text.is_empty() { "Type to filter".to_string() } else { view.filter_text.clone() };
            div()
                .flex_none()
                .h(px(22.))
                .px(px(8.))
                .flex()
                .items_center()
                .text_size(px(12.))
                .text_color(to_gpui(t.tab_inactive_fg))
                .child(shown)
        });
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
                let mut list = div()
                    .id("outline-scroll")
                    .flex_none()
                    .h(px(BODY_HEIGHT))
                    .overflow_y_scroll()
                    .flex()
                    .flex_col()
                    .children(items);
                if let Some(focus) = view.focus.as_ref() {
                    list = list.track_focus(focus);
                }
                list.on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| {
                    if this.outline_filter_key(ev) {
                        cx.stop_propagation();
                        cx.notify();
                    }
                }))
                .into_any_element()
            }
        };
        section.children(filter_line).child(body).into_any_element()
    }

    /// A header action: an icon button, shown active while its toggle is on.
    fn outline_action(
        &self,
        id: &'static str,
        glyph: char,
        active: bool,
        tip: String,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let t = &self.theme;
        div()
            .id(id)
            .debug_selector(move || id.to_string())
            .flex_none()
            .w(px(20.))
            .h(px(20.))
            .rounded(px(4.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .font_family(icons::FAMILY)
            .text_size(px(14.))
            .text_color(to_gpui(t.side_bar_fg))
            .when(active, |d| d.bg(to_gpui(t.list_active_bg)))
            .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
            .child(icons::glyph(glyph))
            .tooltip(move |w, cx| Tooltip::new(tip.clone()).build(w, cx))
            .on_click(move |ev, window, cx| {
                cx.stop_propagation();
                on_click(ev, window, cx);
            })
    }

    /// One row: the disclosure chevron (toggles the item), the kind's icon, the name and detail.
    /// A click on a symbol selects its range; a click on a group toggles it.
    fn outline_row(&self, i: usize, row: OutlineRow, doc: &Path, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let indent = self.layout(cx).tree_indent;
        let settings = OutlineSettings::from_configuration(AppConfig::get(cx).configuration());
        let icons_on = settings.icons;
        let name_color = problem_color_severity(&settings, row.marker).map(|s| {
            if s == severity::ERROR { to_gpui(t.error_fg) } else { to_gpui(t.warning_fg) }
        });
        let badge = problem_badge_severity(&settings, row.marker).map(|s| {
            let (glyph, color) = if s == severity::ERROR {
                (icons::ERROR, to_gpui(t.error_fg))
            } else {
                (icons::WARNING, to_gpui(t.warning_fg))
            };
            div()
                .id(("outline-badge", i))
                .debug_selector({
                    let key = format!("outline-badge:{}", row.id);
                    move || key.clone()
                })
                .flex_none()
                .ml_auto()
                .pl(px(6.))
                .font_family(icons::FAMILY)
                .text_size(px(12.))
                .text_color(color)
                .child(icons::glyph(glyph))
        });
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
            let key = format!("outline-icon:{}", row.id);
            div()
                .debug_selector(move || key.clone())
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
            .when(row.selected, |d| d.bg(to_gpui(t.list_active_bg)))
            .when(!row.selected, |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
            .debug_selector(move || debug.clone())
            .child(disclosure)
            .children(icon)
            .child({
                let name = div().overflow_hidden().whitespace_nowrap().child(row.name.clone());
                match name_color {
                    Some(color) => name.text_color(color),
                    None => name,
                }
            })
            .children(detail)
            .children(badge)
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

    use super::{
        OutlineRow, OutlineSettings, OutlineStatus, RowView, problem_badge_severity, problem_color_severity, rows,
        status_of, symbol_glyph,
    };
    use vsr_exthost::language_features::dto::{DocumentSymbol, Range};
    use vsr_exthost::language_features::outline::{
        KindFilter, Marker, OutlineModel, ProviderSymbols, SortOrder, severity,
    };

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
        OutlineSettings {
            collapse_all,
            icons: true,
            filter: KindFilter::default(),
            problems: true,
            problem_colors: true,
            problem_badges: true,
        }
    }

    /// The view with no toggles, the position sort, no caret and no filter.
    fn plain(toggled: &HashSet<String>) -> RowView<'_> {
        RowView { toggled, order: SortOrder::Position, selected: None, text: "" }
    }

    fn texts(rows: &[OutlineRow]) -> Vec<&str> {
        rows.iter().map(|r| r.name.as_str()).collect()
    }

    fn names(rows: &[OutlineRow]) -> Vec<(&str, usize)> {
        rows.iter().map(|r| (r.name.as_str(), r.depth)).collect()
    }

    #[test]
    fn items_start_expanded_unless_collapse_all() {
        let m = model();
        let none = HashSet::new();
        assert_eq!(names(&rows(&m, &settings(false), &plain(&none))), [("C", 0), ("m", 1), ("X", 0)]);
        assert_eq!(names(&rows(&m, &settings(true), &plain(&none))), [("C", 0), ("X", 0)]);
    }

    #[test]
    fn a_toggled_id_flips_its_default() {
        let m = model();
        let id = rows(&m, &settings(false), &plain(&HashSet::new()))[0].id.clone();
        let toggled: HashSet<String> = [id].into_iter().collect();
        assert_eq!(names(&rows(&m, &settings(false), &plain(&toggled))), [("C", 0), ("X", 0)], "collapsed by the click");
        assert_eq!(names(&rows(&m, &settings(true), &plain(&toggled))), [("C", 0), ("m", 1), ("X", 0)], "expanded by the click");
    }

    #[test]
    fn a_kind_the_filter_hides_takes_its_children_with_it() {
        let m = model();
        let mut s = settings(false);
        s.filter.set(4, false); // class
        assert_eq!(names(&rows(&m, &s, &plain(&HashSet::new()))), [("X", 0)]);
    }

    #[test]
    fn the_rows_carry_the_selection_range_of_each_symbol() {
        let m = model();
        let r = rows(&m, &settings(false), &plain(&HashSet::new()));
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
        let r = rows(&m, &settings(false), &plain(&HashSet::new()));
        assert_eq!(names(&r), [("one", 0), ("a", 1), ("two", 0), ("b", 1)]);
        assert_eq!(r[0].range, None, "a group selects nothing");
    }

    /// A view with the given parts (the other tests use `plain`).
    fn view_with<'a>(toggled: &'a HashSet<String>, order: SortOrder, selected: Option<&'a str>, text: &'a str) -> RowView<'a> {
        RowView { toggled, order, selected, text }
    }

    #[test]
    fn follow_cursor_shows_the_caret_symbol_and_opens_its_ancestors() {
        let m = model();
        let none = HashSet::new();
        let inner = rows(&m, &settings(false), &plain(&none))[1].id.clone();
        let r = rows(&m, &settings(true), &view_with(&none, SortOrder::Position, Some(&inner), ""));
        assert_eq!(names(&r), [("C", 0), ("m", 1), ("X", 0)], "the ancestor of the caret symbol opens");
        assert!(r[1].selected, "the caret symbol is selected");
        assert!(!r[0].selected && !r[2].selected);
        assert!(r[0].expanded, "the ancestor is shown open even when collapseItems closes it");
    }

    #[test]
    fn sorting_by_name_and_by_category() {
        let m = model();
        let none = HashSet::new();
        let by_name = rows(&m, &settings(false), &view_with(&none, SortOrder::Name, None, ""));
        assert_eq!(texts(&by_name), ["C", "m", "X"], "names in order, case-insensitively");
        let by_kind = rows(&m, &settings(false), &view_with(&none, SortOrder::Type, None, ""));
        assert_eq!(texts(&by_kind), ["C", "m", "X"], "class (4), method (5), constant (13)");
    }

    #[test]
    fn filter_on_type_keeps_the_matches_and_their_ancestors_open() {
        let m = model();
        let none = HashSet::new();
        let r = rows(&m, &settings(true), &view_with(&none, SortOrder::Position, None, "M"));
        assert_eq!(texts(&r), ["C", "m"], "case-insensitive; the ancestor stays to show where the match is");
        assert!(r[0].expanded);
        let none_match = rows(&m, &settings(false), &view_with(&none, SortOrder::Position, None, "zzz"));
        assert!(none_match.is_empty());
        let all = rows(&m, &settings(true), &view_with(&none, SortOrder::Position, None, ""));
        assert_eq!(all.len(), 2, "an empty filter is no filter (collapseItems applies)");
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
    fn a_marker_marks_its_symbol_and_the_parent_it_is_inside() {
        let mut m = model();
        // Inside `m` (line 2) and therefore inside `C`: `C` keeps no count of its own, but its severity.
        let mut markers = vec![Marker { range: Range::new(2, 4, 2, 9), severity: severity::ERROR }];
        m.update_marker(&mut markers);
        let r = rows(&m, &settings(false), &plain(&HashSet::new()));
        let (class, method) = (r[0].marker.expect("the class has its severity"), r[1].marker.expect("the method has the problem"));
        assert_eq!(class.top_severity, severity::ERROR);
        assert_eq!(class.count, 0, "the method took the marker");
        assert_eq!((method.count, method.top_severity), (1, severity::ERROR));
        assert!(r[2].marker.is_none(), "a symbol without problems has none");
    }

    #[test]
    fn problems_off_hide_every_marker_and_each_setting_its_own_effect() {
        let mut m = model();
        let mut markers = vec![Marker { range: Range::new(2, 4, 2, 9), severity: severity::WARNING }];
        m.update_marker(&mut markers);
        let mut off = settings(false);
        off.problems = false;
        assert!(rows(&m, &off, &plain(&HashSet::new())).iter().all(|r| r.marker.is_none()));

        let on = settings(false);
        let marker = rows(&m, &on, &plain(&HashSet::new()))[1].marker;
        assert_eq!(problem_color_severity(&on, marker), Some(severity::WARNING));
        assert_eq!(problem_badge_severity(&on, marker), Some(severity::WARNING));
        let mut no_colors = on.clone();
        no_colors.problem_colors = false;
        assert_eq!(problem_color_severity(&no_colors, marker), None, "outline.problems.colors off");
        assert_eq!(problem_badge_severity(&no_colors, marker), Some(severity::WARNING), "badges still on");
        let mut no_badges = on.clone();
        no_badges.problem_badges = false;
        assert_eq!(problem_badge_severity(&no_badges, marker), None, "outline.problems.badges off");
    }

    #[test]
    fn each_kind_has_the_codicon_of_upstreams_table() {
        assert_eq!(symbol_glyph(4), '\u{EB5B}', "class");
        assert_eq!(symbol_glyph(11), '\u{EA8C}', "function");
        assert_eq!(symbol_glyph(25), '\u{EA92}', "type parameter");
        assert_eq!(symbol_glyph(99), '\u{EB65}', "unknown falls back to property");
    }
}
