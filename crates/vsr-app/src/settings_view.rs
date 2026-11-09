//! The Settings editor: search header, User/Workspace tabs, table of contents and a
//! virtualised, grouped settings list. Mirrors VS Code's `settingsEditor2` layout.
//!
//! The pure row/TOC/markdown/validation logic lives in `settings/` (unit tested); this file
//! is the gpui view. Persistence goes through `vsr_settings::UserSettings` to the path given
//! at construction (the real user path in the app, a temp file in tests).
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsEditor2.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTree.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsWidgets.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/media/settingsEditor2.css @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/preferences/browser/media/settingsWidgets.css @ 50e1350fc78d

#[path = "settings/markdown.rs"]
pub mod markdown;
#[path = "settings/model.rs"]
pub mod model;
#[path = "settings/validate.rs"]
pub mod validate;

#[cfg(test)]
#[path = "settings/e2e_tests.rs"]
mod e2e_tests;

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;

use gpui_kit::component::Sizable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::theme::{Theme as ComponentTheme, ThemeMode};
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use serde_json::Value;
use vsr_settings::{SettingKind, SettingSchema, SettingsRegistry, UserSettings};

use crate::config::{AppConfig, wired};
use crate::icons;
use crate::theme::{Rgba8, Theme};
use crate::ui::to_gpui;
use markdown::{Span, Style};
use model::{Row, TocEntry};

actions!(rscode, [OpenSettings]);

/// Register the Settings key bindings and align the component library's theme (used by its
/// text input) with Dark Modern. Call once at startup, after `gpui_kit::init`.
pub fn install(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-,", OpenSettings, None)]);
    ComponentTheme::change(ThemeMode::Dark, None, cx);
    let t = Theme::dark_modern();
    let theme = ComponentTheme::global_mut(cx);
    theme.foreground = to_gpui(t.input_fg);
    theme.muted_foreground = to_gpui(t.input_placeholder_fg);
    theme.caret = to_gpui(t.foreground);
    theme.selection = to_gpui(Rgba8::rgb(0x26, 0x4F, 0x78));
}

/// What the view asks the workbench to do.
pub enum SettingsEvent {
    /// Open the user's settings.json in an editor tab.
    OpenSettingsJson(PathBuf),
}

impl EventEmitter<SettingsEvent> for SettingsView {}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    User,
    Workspace,
}

/// A lazily created text/number input and the setting it edits.
struct FieldInput {
    key: String,
    state: Entity<InputState>,
}

pub struct SettingsView {
    theme: Theme,
    registry: Rc<SettingsRegistry>,
    user: UserSettings,
    /// The settings.json behind the configuration (`None` when it is in memory).
    path: Option<PathBuf>,
    scope: Scope,
    search: Entity<InputState>,
    query: String,
    /// Setting keys matching the query, in rank order (unfiltered by the TOC category).
    search_keys: Vec<String>,
    /// The TOC node clicked while searching: the list shows only its settings (upstream `categoryFilter`).
    category: Option<String>,
    /// The TOC while searching: matching nodes with their counts.
    search_toc: Vec<TocEntry>,
    rows: Vec<Row>,
    header_ix: HashMap<String, usize>,
    list: ListState,
    toc_expanded: HashSet<String>,
    selected_toc: Option<String>,
    toc_width: f32,
    /// (mouse x, toc width) when the divider drag started.
    resize: Option<(f32, f32)>,
    fields: HashMap<String, FieldInput>,
    errors: HashMap<String, String>,
    dropdown: Option<String>,
    menu: Option<String>,
    focus: FocusHandle,
    save_error: Option<String>,
    _subs: Vec<Subscription>,
}

const TOC_DEFAULT_WIDTH: f32 = 200.0;

impl SettingsView {
    /// The view edits the app's `AppConfig` (writes apply immediately and are saved to its
    /// settings.json). When no configuration is installed yet, `path` is loaded as the settings.json.
    pub fn new(path: impl Into<Option<PathBuf>>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        if !cx.has_global::<AppConfig>() {
            AppConfig::install(cx, path.into());
        }
        let app = AppConfig::get(cx);
        let registry: Rc<SettingsRegistry> = app.registry();
        let user = app.user().clone();
        let save_error = app.error().map(str::to_string);
        let path = app.path().map(PathBuf::from);
        let rows = model::build_rows(registry.toc());
        let header_ix = model::header_indices(&rows);
        let list = ListState::new(rows.len(), ListAlignment::Top, px(600.));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search settings"));
        let mut subs = Vec::new();
        subs.push(cx.subscribe_in(&search, window, |this, state, ev: &InputEvent, _w, cx| {
            if matches!(ev, InputEvent::Change) {
                let q = state.read(cx).value().to_string();
                this.set_query(q, cx);
            }
        }));
        // External edits of settings.json (and writes from elsewhere) refresh the view.
        subs.push(cx.observe_global::<AppConfig>(|this, cx| {
            let app = AppConfig::get(cx);
            this.user = app.user().clone();
            this.save_error = app.error().map(str::to_string);
            cx.notify();
        }));
        let weak = cx.entity().downgrade();
        list.set_scroll_handler(move |ev, _w, app| {
            let first = ev.visible_range.start;
            weak.update(app, |this, cx| this.on_scroll(first, cx)).ok();
        });
        Self {
            theme: Theme::dark_modern(),
            registry,
            user,
            path,
            scope: Scope::User,
            search,
            query: String::new(),
            search_keys: Vec::new(),
            category: None,
            search_toc: Vec::new(),
            rows,
            header_ix,
            list,
            toc_expanded: HashSet::new(),
            selected_toc: Some("commonlyUsed".into()),
            toc_width: TOC_DEFAULT_WIDTH,
            resize: None,
            fields: HashMap::new(),
            errors: HashMap::new(),
            dropdown: None,
            menu: None,
            focus: cx.focus_handle(),
            save_error,
            _subs: subs,
        }
    }

    // ---- state ----------------------------------------------------------------

    /// Focus the search input (VS Code does this when the editor opens).
    pub fn focus_search(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |s, cx| s.focus(window, cx));
    }

    #[cfg(test)]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[cfg(test)]
    pub fn user_settings(&self) -> &UserSettings {
        &self.user
    }

    /// Visible TOC lines as (label, count). Counts are `Some` only while searching.
    #[cfg(test)]
    pub fn visible_toc(&self) -> Vec<(String, Option<usize>)> {
        self.toc_entries().into_iter().map(|e| (e.label, e.count)).collect()
    }

    /// Number of settings the current search found (the header's "N Settings Found").
    #[cfg(test)]
    pub fn result_count(&self) -> usize {
        self.search_keys.len()
    }

    /// Keys of the setting rows currently listed, in order.
    #[cfg(test)]
    pub fn visible_keys(&self) -> Vec<String> {
        self.rows
            .iter()
            .filter_map(|r| match r {
                Row::Setting { key, .. } => Some(key.clone()),
                Row::Header { .. } => None,
            })
            .collect()
    }

    fn searching(&self) -> bool {
        !self.query.trim().is_empty()
    }

    fn set_query(&mut self, query: String, cx: &mut Context<Self>) {
        self.query = query;
        self.dropdown = None;
        self.menu = None;
        // A new query clears the TOC category (upstream: `categoryFilter = undefined`).
        self.category = None;
        self.search_keys = if self.searching() {
            // `@id:<key>` shows exactly that setting (upstream's id filter).
            match self.query.trim().strip_prefix("@id:") {
                Some(id) => self.registry.get(id.trim()).map(|s| s.key.clone()).into_iter().collect(),
                None => self.registry.search(&self.query).into_iter().map(|s| s.key.clone()).collect(),
            }
        } else {
            Vec::new()
        };
        self.search_toc = if self.searching() {
            let matches: HashSet<&str> = self.search_keys.iter().map(String::as_str).collect();
            model::flatten_search_toc(self.registry.toc(), &matches)
        } else {
            Vec::new()
        };
        self.refresh_rows();
        if !self.searching() {
            // Leaving search collapses the TOC and selects Commonly Used (upstream `collapseAll`).
            self.toc_expanded.clear();
            self.selected_toc = Some("commonlyUsed".into());
        }
        cx.notify();
    }

    /// Rebuild the list from the search results (narrowed to the TOC category, if any) or the full TOC.
    fn refresh_rows(&mut self) {
        self.rows = if self.searching() {
            let keys: Vec<&str> = match &self.category {
                Some(id) => {
                    let group = model::subtree_keys(self.registry.toc(), id);
                    self.search_keys.iter().filter(|k| group.contains(k.as_str())).map(String::as_str).collect()
                }
                None => self.search_keys.iter().map(String::as_str).collect(),
            };
            model::search_rows(keys)
        } else {
            model::build_rows(self.registry.toc())
        };
        self.header_ix = model::header_indices(&self.rows);
        self.list.reset(self.rows.len());
    }

    /// The TOC lines to show: the matching nodes while searching, else the expandable full TOC.
    fn toc_entries(&self) -> Vec<TocEntry> {
        if self.searching() {
            self.search_toc.clone()
        } else {
            model::flatten_toc(self.registry.toc(), &self.toc_expanded)
        }
    }

    /// Replace the search text (for `#setting.id#` links and tests).
    pub fn set_search_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.search.update(cx, |s, cx| s.set_value(text.to_string(), window, cx));
        self.set_query(text.to_string(), cx);
    }

    fn clear_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_search_text("", window, cx);
    }

    fn on_scroll(&mut self, first_visible: usize, cx: &mut Context<Self>) {
        if self.searching() {
            return;
        }
        if let Some(group) = model::group_at(&self.rows, first_visible).map(str::to_string)
            && self.selected_toc.as_deref() != Some(&group)
        {
            for a in model::ancestors(self.registry.toc(), &group) {
                self.toc_expanded.insert(a);
            }
            self.selected_toc = Some(group);
            cx.notify();
        }
    }

    fn select_toc(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.searching() {
            // Upstream `onDidChangeFocus`: the list is filtered to the node's settings and scrolled
            // to the top (there are no group headers in search results to scroll to).
            if self.category.as_deref() != Some(id) {
                self.category = Some(id.to_string());
                self.refresh_rows();
            }
            self.list.scroll_to(ListOffset { item_ix: 0, offset_in_item: px(0.) });
            cx.notify();
            return;
        }
        self.selected_toc = Some(id.to_string());
        if let Some(&ix) = self.header_ix.get(id) {
            self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        }
        cx.notify();
    }

    fn current_value(&self, s: &SettingSchema) -> Value {
        self.user.get(&s.key).cloned().unwrap_or_else(|| s.default.clone())
    }

    /// Write a value (or reset when it equals the default, like upstream) and save.
    fn write(&mut self, key: &str, value: Value, cx: &mut Context<Self>) {
        let is_default = self.registry.get(key).is_some_and(|s| s.default == value);
        self.apply(key, (!is_default).then_some(value), cx);
    }

    fn reset(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.apply(key, None, cx);
        self.errors.retain(|id, _| !id.ends_with(&format!(":{key}")));
        let default = self.registry.get(key).map(|s| value_text(&s.default)).unwrap_or_default();
        for f in self.fields.values().filter(|f| f.key == key) {
            f.state.update(cx, |st, cx| st.set_value(default.clone(), window, cx));
        }
        self.menu = None;
        cx.notify();
    }

    /// Writes through the app configuration: saved to disk and applied to the running app.
    fn apply(&mut self, key: &str, value: Option<Value>, cx: &mut Context<Self>) {
        self.save_error = AppConfig::set_value(cx, key, value).err();
        self.user = AppConfig::get(cx).user().clone();
        cx.notify();
    }

    fn open_settings_json(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.path.clone() else { return };
        if !path.exists() {
            let _ = self.user.save(&path);
        }
        cx.emit(SettingsEvent::OpenSettingsJson(path));
    }

    // ---- rich text ------------------------------------------------------------

    /// Description markdown as one wrapped, clickable text element.
    fn rich_text(
        &self,
        id: String,
        md: &str,
        base: Hsla,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = &self.theme;
        let spans = markdown::parse_inline(md);
        let mut text = String::new();
        let mut runs: Vec<TextRun> = Vec::new();
        let mut links: Vec<(Range<usize>, Span)> = Vec::new();
        let ui_font = |weight: FontWeight| Font {
            family: ".SystemUIFont".into(),
            features: Default::default(),
            fallbacks: None,
            weight,
            style: FontStyle::Normal,
        };
        for sp in &spans {
            let start = text.len();
            text.push_str(&sp.text);
            let len = sp.text.len();
            let (font, color, bg, underline) = match &sp.style {
                Style::Plain => (ui_font(FontWeight::NORMAL), base, None, None),
                Style::Bold => (ui_font(FontWeight::BOLD), base, None, None),
                Style::Code => (
                    Font { family: "Menlo".into(), ..ui_font(FontWeight::NORMAL) },
                    to_gpui(t.text_preformat_fg),
                    Some(to_gpui(t.text_preformat_bg)),
                    None,
                ),
                Style::Link(_) | Style::SettingRef(_) => {
                    links.push((start..start + len, sp.clone()));
                    (ui_font(FontWeight::NORMAL), to_gpui(t.text_link_fg), None, None::<UnderlineStyle>)
                }
            };
            if len > 0 {
                runs.push(TextRun {
                    len,
                    font,
                    color,
                    background_color: bg,
                    underline,
                    strikethrough: None,
                });
            }
        }
        if text.is_empty() {
            return div().into_any_element();
        }
        let styled = StyledText::new(text).with_runs(runs);
        let view = cx.entity();
        let ranges: Vec<Range<usize>> = links.iter().map(|(r, _)| r.clone()).collect();
        let targets: Vec<Span> = links.into_iter().map(|(_, s)| s).collect();
        InteractiveText::new(id, styled)
            .on_click(ranges, move |i, window, app| match &targets[i].style {
                Style::Link(target) => {
                    if target.starts_with("http://") || target.starts_with("https://") {
                        app.open_url(target);
                    }
                }
                Style::SettingRef(key) => {
                    let key = key.clone();
                    view.update(app, |this, cx| this.set_search_text(&key, window, cx));
                }
                _ => {}
            })
            .into_any_element()
    }

    // ---- inputs ---------------------------------------------------------------

    fn field(
        &mut self,
        row_id: &str,
        schema: &SettingSchema,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<InputState> {
        if let Some(f) = self.fields.get(row_id) {
            return f.state.clone();
        }
        let initial = value_text(&self.current_value(schema));
        let state = cx.new(|cx| {
            let mut s = InputState::new(window, cx);
            s.set_value(initial, window, cx);
            s
        });
        let key = schema.key.clone();
        let kind = schema.kind.clone();
        let rid = row_id.to_string();
        self._subs.push(cx.subscribe_in(&state, window, move |this, st, ev: &InputEvent, _w, cx| {
            if !matches!(ev, InputEvent::Change) {
                return;
            }
            let text = st.read(cx).value().to_string();
            match &kind {
                SettingKind::Number { min, max, integer } => match validate::validate_number(&text, *min, *max, *integer) {
                    Ok(n) => {
                        this.errors.remove(&rid);
                        let v = if *integer || (n.fract() == 0.0 && n.abs() < 1e15) { Value::from(n as i64) } else { Value::from(n) };
                        this.write(&key, v, cx);
                    }
                    Err(msg) => {
                        this.errors.insert(rid.clone(), msg);
                        cx.notify();
                    }
                },
                _ => this.write(&key, Value::String(text), cx),
            }
        }));
        self.fields.insert(row_id.to_string(), FieldInput { key: schema.key.clone(), state: state.clone() });
        state
    }

    fn input_box(&self, row_id: &str, state: &Entity<InputState>, width: f32, bg: Rgba8, invalid: bool) -> Div {
        let t = &self.theme;
        div()
            .w(px(width))
            .h(px(26.))
            .flex_none()
            .overflow_hidden()
            .rounded(px(2.))
            .border_1()
            .border_color(to_gpui(if invalid { t.validation_error_border } else { t.input_border }))
            .bg(to_gpui(bg))
            .text_size(px(13.))
            .debug_selector(|| format!("input:{row_id}"))
            .child(Input::new(state).appearance(false).bordered(false).small())
    }

    // ---- rendering: rows -------------------------------------------------------

    fn render_row(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = self.rows.get(ix).cloned() else {
            return div().into_any_element();
        };
        match row {
            Row::Header { id, label, level } => self.render_header(&id, &label, level),
            Row::Setting { id, key } => match self.registry.clone().get(&key) {
                Some(schema) => self.render_setting(&id, schema, window, cx),
                None => div().into_any_element(),
            },
        }
    }

    fn render_header(&self, id: &str, label: &str, level: u8) -> AnyElement {
        let t = &self.theme;
        let size = match level {
            1 => 26.,
            2 => 22.,
            _ => 18.,
        };
        div()
            .id(format!("group:{id}"))
            .w_full()
            .px(px(24.))
            .child(
                div()
                    .w_full()
                    .p(px(10.))
                    .pl(px(15.))
                    .text_size(px(size))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(to_gpui(t.settings_header_fg))
                    .child(label.to_string()),
            )
            .into_any_element()
    }

    fn render_setting(
        &mut self,
        row_id: &str,
        s: &SettingSchema,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let t = self.theme.clone();
        let modified = self.user.is_modified(&s.key);
        let value = self.current_value(s);
        let is_bool = matches!(s.kind, SettingKind::Bool);
        let fg = to_gpui(t.foreground);
        let dim = fg.opacity(0.9);
        let group: SharedString = format!("row:{row_id}").into();
        let key = s.key.clone();

        // Title: "Category: " + "Name".
        let mut title = div().flex().flex_wrap().pb(px(2.)).font_weight(FontWeight::SEMIBOLD);
        if !s.category_label.is_empty() {
            title = title.child(div().text_color(dim).child(format!("{}: ", s.category_label)));
        }
        title = title.child(div().text_color(to_gpui(t.settings_header_fg)).mr(px(7.)).child(s.name_label.clone()));

        let description = self.rich_text(format!("desc:{row_id}"), &s.description, dim, cx);
        // Settings the app does not read yet say so (config/wired.rs is the list).
        let unsupported = (!wired::is_wired(&s.key)).then(|| {
            div()
                .id(format!("unsupported:{row_id}"))
                .mt(px(3.))
                .text_size(px(12.))
                .italic()
                .text_color(to_gpui(t.description_fg))
                .debug_selector(|| format!("unsupported:{row_id}"))
                .child("Not yet supported in rscode")
        });

        let mut body = div().flex().flex_col().child(title);
        if let Some(dep) = &s.deprecated {
            let msg = self.rich_text(format!("dep:{row_id}"), dep, to_gpui(t.setting_error_fg), cx);
            body = body.child(div().mt(px(3.)).child(msg));
        }

        match &s.kind {
            SettingKind::Bool => {
                let on = value.as_bool().unwrap_or(false);
                let toggle_key = key.clone();
                let toggle = cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    let cur = this.user.get(&toggle_key).and_then(Value::as_bool).or_else(|| {
                        this.registry.get(&toggle_key).and_then(|s| s.default.as_bool())
                    });
                    this.write(&toggle_key, Value::Bool(!cur.unwrap_or(false)), cx);
                });
                let checkbox = div()
                    .id(format!("checkbox:{row_id}"))
                    .flex_none()
                    .w(px(18.))
                    .h(px(18.))
                    .mr(px(9.))
                    .border_1()
                    .border_color(to_gpui(t.settings_checkbox_border))
                    .rounded(px(3.))
                    .bg(to_gpui(t.settings_checkbox_bg))
                    .text_color(to_gpui(t.settings_checkbox_fg))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .font_family(icons::FAMILY)
                    .text_size(px(16.))
                    .when(on, |d| d.child(icons::glyph(icons::CHECK)))
                    .debug_selector(|| format!("checkbox:{row_id}"))
                    .on_click(toggle);
                body = body.child(div().flex().items_start().mt(px(-1.)).child(checkbox).child(div().flex_1().child(description).children(unsupported)));
            }
            kind => {
                body = body.child(div().mt(px(-1.)).child(description).children(unsupported));
                match kind {
                    SettingKind::Number { .. } => {
                        let state = self.field(row_id, s, window, cx);
                        let err = self.errors.get(row_id).cloned();
                        let mut col = div().mt(px(9.)).flex().flex_col().child(self.input_box(
                            row_id,
                            &state,
                            200.,
                            t.settings_number_input_bg,
                            err.is_some(),
                        ));
                        if let Some(msg) = err {
                            col = col.child(self.validation_message(&msg));
                        }
                        body = body.child(col);
                    }
                    SettingKind::String => {
                        let state = self.field(row_id, s, window, cx);
                        body = body.child(div().mt(px(9.)).child(self.input_box(row_id, &state, 420., t.settings_text_input_bg, false)));
                    }
                    SettingKind::Enum { values, labels, descriptions } => {
                        let sel = values.iter().position(|v| *v == value);
                        let shown = sel
                            .and_then(|i| labels.get(i).cloned())
                            .unwrap_or_else(|| value_text(&value));
                        let detail = sel.and_then(|i| descriptions.get(i)).filter(|d| !d.is_empty()).cloned();
                        let mut col = div().mt(px(9.)).flex().flex_col().child(self.dropdown_control(
                            row_id,
                            &key,
                            &shown,
                            values,
                            labels,
                            sel,
                            cx,
                        ));
                        if let Some(d) = detail {
                            col = col.child(div().mt(px(8.)).child(self.rich_text(format!("enumdesc:{row_id}"), &d, dim, cx)));
                        }
                        body = body.child(col);
                    }
                    SettingKind::StringArray | SettingKind::Object | SettingKind::Complex => {
                        let link_color = to_gpui(t.text_link_fg);
                        body = body.child(
                            div().mt(px(9.)).child(
                                div()
                                    .id(format!("edit-json:{row_id}"))
                                    .cursor_pointer()
                                    .text_color(link_color)
                                    .hover(|s| s.underline().text_color(to_gpui(t.text_link_active_fg)))
                                    .child("Edit in settings.json")
                                    .on_click(cx.listener(|this, _: &ClickEvent, _w, cx| this.open_settings_json(cx))),
                            ),
                        );
                    }
                    SettingKind::Bool => {}
                }
            }
        }

        // Modified indicator: left bar, 2px wide (settingsEditor2.css .is-configured).
        let indicator = modified.then(|| {
            div()
                .absolute()
                .left(px(5.))
                .top(px(15.))
                .bottom(px(if is_bool { 23. } else { 18. }))
                .w(px(2.))
                .bg(to_gpui(t.settings_modified_indicator))
        });

        let menu_open = self.menu.as_deref() == Some(row_id);
        let gear = self.more_actions(row_id, &key, modified, menu_open, &group, cx);

        div()
            .id(format!("setting:{row_id}"))
            .group(group.clone())
            .w_full()
            .px(px(24.))
            .child(
                div()
                    .relative()
                    .w_full()
                    .pt(px(12.))
                    .pb(px(if is_bool { 26. } else { 18. }))
                    .px(px(14.))
                    .line_height(relative(1.4))
                    .text_size(px(13.))
                    .text_color(fg)
                    .hover(|s| s.bg(to_gpui(t.settings_row_hover_bg)))
                    // A row with an open menu/dropdown counts as focused.
                    .when(menu_open || self.dropdown.as_deref() == Some(row_id), |d| {
                        d.bg(to_gpui(t.settings_focused_row_bg))
                    })
                    .children(indicator)
                    .child(gear)
                    .child(body),
            )
            .into_any_element()
    }

    fn validation_message(&self, msg: &str) -> Div {
        let t = &self.theme;
        div()
            .w(px(200.))
            .mt(px(-1.))
            .p(px(5.))
            .border_1()
            .border_color(to_gpui(t.validation_error_border))
            .bg(to_gpui(t.validation_error_bg))
            .text_color(to_gpui(t.foreground))
            .child(msg.to_string())
    }

    /// The enum dropdown: a 320px select box and, when open, a list under it.
    #[allow(clippy::too_many_arguments)]
    fn dropdown_control(
        &self,
        row_id: &str,
        key: &str,
        shown: &str,
        values: &[Value],
        labels: &[String],
        selected: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = &self.theme;
        let open = self.dropdown.as_deref() == Some(row_id);
        let rid = row_id.to_string();
        let select = div()
            .id(format!("select:{row_id}"))
            .debug_selector(|| format!("select:{row_id}"))
            .w(px(320.))
            .h(px(26.))
            .px(px(6.))
            .flex()
            .items_center()
            .justify_between()
            .rounded(px(2.))
            .border_1()
            .border_color(to_gpui(if open { t.focus_border } else { t.settings_dropdown_border }))
            .bg(to_gpui(t.settings_dropdown_bg))
            .text_color(to_gpui(t.settings_dropdown_fg))
            .cursor_pointer()
            .child(div().overflow_hidden().whitespace_nowrap().child(shown.to_string()))
            .child(div().font_family(icons::FAMILY).text_size(px(16.)).child(icons::glyph(icons::CHEVRON_DOWN)))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.menu = None;
                this.dropdown = if this.dropdown.as_deref() == Some(rid.as_str()) { None } else { Some(rid.clone()) };
                cx.notify();
            }));
        let mut c = div().relative().child(select);
        if open {
            let mut list = div()
                .id(format!("options:{row_id}"))
                .occlude()
                .absolute()
                .top(px(27.))
                .left_0()
                .w(px(320.))
                .py(px(2.))
                .border_1()
                .border_color(to_gpui(t.settings_dropdown_border))
                .bg(to_gpui(t.settings_dropdown_list_bg))
                .text_color(to_gpui(t.settings_dropdown_fg))
                .on_mouse_down_out(cx.listener(|this, _, _w, cx| {
                    this.dropdown = None;
                    cx.notify();
                }));
            for (i, v) in values.iter().enumerate() {
                let label = labels.get(i).cloned().unwrap_or_else(|| value_text(v));
                let v = v.clone();
                let k = key.to_string();
                list = list.child(
                    div()
                        .id(format!("option:{row_id}:{i}"))
                        .debug_selector(|| format!("option:{row_id}:{i}"))
                        .h(px(22.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .when(selected == Some(i), |d| d.bg(to_gpui(t.list_active_bg)))
                        .hover(|s| s.bg(to_gpui(t.menu_selection_bg)).text_color(gpui_kit::white()))
                        .child(label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            this.write(&k, v.clone(), cx);
                            this.dropdown = None;
                            cx.notify();
                        })),
                );
            }
            c = c.child(deferred(list).with_priority(2));
        }
        c
    }

    /// The hover "More Actions" gear and its menu.
    fn more_actions(
        &self,
        row_id: &str,
        key: &str,
        modified: bool,
        open: bool,
        group: &SharedString,
        cx: &mut Context<Self>,
    ) -> Div {
        let t = &self.theme;
        let rid = row_id.to_string();
        let button = div()
            .id(format!("more-actions:{row_id}"))
            .debug_selector(|| format!("more-actions:{row_id}"))
            .w(px(22.))
            .h(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(to_gpui(t.foreground))
            .opacity(if open { 1. } else { 0. })
            .group_hover(group.clone(), |s| s.opacity(1.))
            .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
            .child(icons::glyph(icons::GEAR))
            .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                this.dropdown = None;
                this.menu = if this.menu.as_deref() == Some(rid.as_str()) { None } else { Some(rid.clone()) };
                cx.notify();
            }));
        let mut holder = div().absolute().left(px(-22.)).top(px(8.)).child(button);
        if open {
            let item = |id: String, label: &'static str, enabled: bool| {
                let sel = id.clone();
                div()
                    .id(id)
                    .debug_selector(move || sel)
                    .h(px(24.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .whitespace_nowrap()
                    .text_size(px(13.))
                    .when(enabled, |d| d.cursor_pointer().hover(|s| s.bg(to_gpui(t.menu_selection_bg)).text_color(gpui_kit::white())))
                    .when(!enabled, |d| d.opacity(0.5))
                    .child(label)
            };
            let reset_key = key.to_string();
            let copy_key = key.to_string();
            let reset = item(format!("reset:{row_id}"), "Reset Setting", modified).when(modified, |d| {
                d.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.reset(&reset_key, window, cx)))
            });
            let copy = item(format!("copy-id:{row_id}"), "Copy Setting ID", true).on_click(cx.listener(
                move |this, _: &ClickEvent, _w, cx| {
                    cx.write_to_clipboard(ClipboardItem::new_string(copy_key.clone()));
                    this.menu = None;
                    cx.notify();
                },
            ));
            holder = holder.child(deferred(
                div()
                    .id(format!("menu:{row_id}"))
                    .occlude()
                    .absolute()
                    .top(px(24.))
                    .left_0()
                    .min_w(px(160.))
                    .py(px(4.))
                    .rounded(px(5.))
                    .border_1()
                    .border_color(to_gpui(t.menu_border))
                    .bg(to_gpui(t.menu_bg))
                    .text_color(to_gpui(t.foreground))
                    .on_mouse_down_out(cx.listener(|this, _, _w, cx| {
                        this.menu = None;
                        cx.notify();
                    }))
                    .child(reset)
                    .child(copy),
            ).with_priority(3));
        }
        holder
    }

    // ---- rendering: chrome -----------------------------------------------------

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let count = self.searching().then(|| match self.search_keys.len() {
            0 => "No Settings Found".to_string(),
            1 => "1 Setting Found".to_string(),
            n => format!("{n} Settings Found"),
        });
        let search_box = div()
            .relative()
            .w_full()
            .h(px(30.))
            .flex()
            .items_center()
            .rounded(px(2.))
            .border_1()
            .border_color(to_gpui(t.input_border))
            .bg(to_gpui(t.settings_text_input_bg))
            .text_size(px(13.))
            .child(div().flex_1().min_w_0().child(Input::new(&self.search).appearance(false).bordered(false)))
            .when_some(count, |d, c| {
                d.child(
                    div()
                        .id("settings-count")
                        .flex_none()
                        .mr(px(6.))
                        .text_color(to_gpui(t.description_fg))
                        .child(c),
                )
            });

        let tab = |scope: Scope, label: &'static str| {
            let active = self.scope == scope;
            div()
                .id(format!("scope-tab:{label}"))
                .px(px(8.))
                .pt(px(7.))
                .pb(px(6.5))
                .text_size(px(13.))
                .cursor_pointer()
                .border_b_1()
                .border_color(if active { to_gpui(t.settings_header_fg) } else { gpui_kit::transparent_black() })
                .text_color(if active { to_gpui(t.settings_header_fg) } else { to_gpui(t.foreground).opacity(0.9) })
                .hover(|s| s.text_color(to_gpui(t.settings_header_fg)))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    this.scope = scope;
                    cx.notify();
                }))
        };
        div()
            .flex_none()
            .w_full()
            .mt(px(11.))
            .pt(px(3.))
            .px(px(24.))
            .child(search_box)
            .child(
                div()
                    .mt(px(10.))
                    .flex()
                    .border_b_1()
                    .border_color(to_gpui(t.settings_header_border))
                    .child(tab(Scope::User, "User"))
                    .child(tab(Scope::Workspace, "Workspace")),
            )
    }

    fn toc(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let searching = self.searching();
        let entries = self.toc_entries();
        let rows = entries.into_iter().enumerate().map(|(i, e)| {
            // While searching, the selection is the clicked category; the TOC stays fully expanded.
            let selected = if searching {
                self.category.as_deref() == Some(e.id.as_str())
            } else {
                self.selected_toc.as_deref() == Some(e.id.as_str())
            };
            let id_select = e.id.clone();
            let id_toggle = e.id.clone();
            let debug_id = format!("toc:{}", e.id);
            let chevron = div()
                .id(("toc-twistie", i))
                .flex_none()
                .w(px(16.))
                .h(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .when(e.expandable, |d| {
                    let d = d.cursor_pointer().child(icons::glyph(if e.expanded { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT }));
                    if searching {
                        d
                    } else {
                        d.on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                            cx.stop_propagation();
                            if !this.toc_expanded.remove(&id_toggle) {
                                this.toc_expanded.insert(id_toggle.clone());
                            }
                            cx.notify();
                        }))
                    }
                });
            div()
                .id(format!("toc:{}", e.id))
                .debug_selector(move || debug_id.clone())
                .flex_none()
                .h(px(22.))
                .w_full()
                .pl(px(8. * e.depth as f32))
                .flex()
                .items_center()
                .cursor_pointer()
                .text_size(px(13.))
                .line_height(px(22.))
                .text_color(to_gpui(t.foreground).opacity(if selected { 1. } else { 0.9 }))
                .when(selected, |d| d.bg(to_gpui(t.list_active_bg)).font_weight(FontWeight::BOLD))
                .when(!selected, |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
                .child(chevron)
                .child(div().overflow_hidden().whitespace_nowrap().child(e.label.clone()))
                // upstream `.settings-toc-count`: " (N)", opacity 0.8 of the row, margin-left 3px, not bold.
                .when_some(e.count, |d, n| {
                    let alpha = if selected { 0.8 } else { 0.72 };
                    d.child(
                        div()
                            .flex_none()
                            .ml(px(3.))
                            .font_weight(FontWeight::NORMAL)
                            .text_color(to_gpui(t.foreground).opacity(alpha))
                            .child(format!("({n})")),
                    )
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _w, cx| {
                    if let Some(true) = this.registry.toc().iter().find(|n| n.id == id_select).map(|n| !n.children.is_empty()) {
                        this.toc_expanded.insert(id_select.clone());
                    } else if !model::ancestors(this.registry.toc(), &id_select).is_empty() {
                        // nested node: keep parents open
                    }
                    this.select_toc(&id_select, cx);
                }))
        });
        div()
            .id("settings-toc")
            .flex_none()
            .w(px(self.toc_width))
            .h_full()
            .pl(px(24.))
            .overflow_y_scroll()
            .children(rows.collect::<Vec<_>>())
    }

    fn body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        if self.scope == Scope::Workspace {
            return div()
                .id("settings-workspace-note")
                .flex_1()
                .mt(px(20.))
                .px(px(24.))
                .text_color(to_gpui(t.description_fg))
                .child("Workspace settings are not supported yet.")
                .into_any_element();
        }
        if self.searching() && self.search_keys.is_empty() {
            return div()
                .id("settings-no-results")
                .flex_1()
                .mt(px(20.))
                .px(px(24.))
                .child(format!("No Settings Found for \"{}\".", self.query.trim()))
                .into_any_element();
        }
        let sash = div()
            .id("settings-sash")
            .flex_none()
            .relative()
            .w(px(1.))
            .h_full()
            .bg(to_gpui(t.settings_header_border))
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left(px(-2.))
                    .w(px(5.))
                    .h_full()
                    .cursor_col_resize()
                    .hover(|s| s.bg(to_gpui(t.focus_border)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, ev: &MouseDownEvent, _w, cx| {
                            this.resize = Some((f32::from(ev.position.x), this.toc_width));
                            cx.notify();
                        }),
                    ),
            );
        let tree = list(self.list.clone(), cx.processor(|this, ix: usize, window, cx| this.render_row(ix, window, cx)))
            .size_full();
        div()
            .flex_1()
            .min_h_0()
            .w_full()
            .mt(px(14.))
            .flex()
            .child(self.toc(cx))
            .child(sash)
            .child(div().id("settings-tree").flex_1().min_w_0().h_full().child(tree))
            .into_any_element()
    }
}

impl Focusable for SettingsView {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for SettingsView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let t = self.theme.clone();
        div()
            .id("settings-editor")
            .key_context("SettingsEditor")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(to_gpui(t.editor_bg))
            .text_color(to_gpui(t.foreground))
            .font_family(".SystemUIFont")
            .text_size(px(13.))
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.key == "escape" {
                    if this.dropdown.take().is_some() || this.menu.take().is_some() {
                        cx.notify();
                    } else if this.searching() {
                        this.clear_search(window, cx);
                    }
                }
            }))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _w, cx| {
                if let Some((x0, w0)) = this.resize {
                    if ev.pressed_button == Some(MouseButton::Left) {
                        this.toc_width = (w0 + f32::from(ev.position.x) - x0).clamp(150., 450.);
                    } else {
                        this.resize = None;
                    }
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _w, cx| {
                    if this.resize.take().is_some() {
                        cx.notify();
                    }
                }),
            )
            .child(
                // max-width 1400px, centred (settingsEditor2.css .settings-editor).
                div()
                    .w_full()
                    .max_w(px(1400.))
                    .h_full()
                    .flex()
                    .flex_col()
                    .child(self.header(cx))
                    .when_some(self.save_error.clone(), |d, e| {
                        d.child(div().px(px(24.)).mt(px(6.)).text_color(to_gpui(t.setting_error_fg)).child(e))
                    })
                    .child(self.body(cx)),
            )
    }
}

/// JSON value as input text: strings unquoted, integral numbers without ".0".
fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn value_text_formats() {
        assert_eq!(value_text(&Value::from(14)), "14");
        assert_eq!(value_text(&Value::from("abc")), "abc");
        assert_eq!(value_text(&Value::Null), "");
        assert_eq!(value_text(&Value::Bool(true)), "true");
    }
}
