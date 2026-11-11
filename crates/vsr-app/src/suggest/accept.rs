//! upstream: src/vs/editor/contrib/suggest/browser/suggestController.ts @ 50e1350fc78d
//!
//! The suggest list's keys and what accepting does (ADR-0084): focus moves, accept as one undo
//! step, the lazy `resolveCompletionItem` of the focused item, and releasing the host's cached lists.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggestController.ts (`_insertSuggestion`,
//! `onDidFocus` resolving), suggestWidget.ts.

use gpui_kit::{Context, Window};
use vsr_editor::{Position as EdPos, Range};
use vsr_exthost::language_features::dto::CompletionItem;
use vsr_suggest::accept;
use vsr_suggest::filters::create_matches;
use vsr_suggest::item::{Position, Span, SuggestItem};
use vsr_suggest::list::{ListView, initial_focus};

use super::settings::AcceptOnEnter;
use super::widget::{Row, WidgetData};
use crate::Workbench;
use crate::extensions::worker::ToHost;
use crate::keymap::Command;

impl Workbench {
    /// A new answer: the lists of the providers that answered replace what was held for them; lists
    /// of providers that stayed silent stay (their items are reused by the retrigger).
    pub(super) fn suggest_hold(&mut self, lists: &[(u32, u64)], answered: &[u32]) {
        let (gone, kept): (Vec<_>, Vec<_>) =
            std::mem::take(&mut self.suggest.held).into_iter().partition(|(h, _)| answered.contains(h));
        self.suggest.held = kept;
        self.suggest.held.extend_from_slice(lists);
        self.suggest_release(gone);
    }

    pub(super) fn suggest_release_held(&mut self) {
        let held = std::mem::take(&mut self.suggest.held);
        self.suggest_release(held);
    }

    /// Tells the host to drop the cached lists.
    pub(crate) fn suggest_release(&self, lists: Vec<(u32, u64)>) {
        if !lists.is_empty() {
            self.ext_send(ToHost::ReleaseCompletions(lists));
        }
    }

    /// The items of the list, as the model filtered them.
    fn suggest_items(&mut self) -> Vec<SuggestItem> {
        self.suggest.session.model().map(|m| m.items().to_vec()).unwrap_or_default()
    }

    /// A list was (re)shown: focus the item the list asks for.
    pub(super) fn suggest_refocus(&mut self) {
        let items = self.suggest_items();
        self.suggest.list = ListView::default();
        self.suggest.list.focus(initial_focus(&items), items.len());
    }

    /// The focus may have changed: ask the host to resolve the focused item.
    pub(super) fn suggest_focus_changed(&mut self) {
        let items = self.suggest_items();
        self.suggest.list.clamp(items.len());
        let Some(item) = items.get(self.suggest.list.focused) else { return };
        let Some(source) = item.source.as_deref() else { return };
        let Some(key) = source.cache_id else { return };
        if self.suggest.resolved.contains_key(&key) || self.suggest.resolving.is_some_and(|(_, k)| k == key) || !self.ext.is_running() {
            return;
        }
        self.suggest.next_resolve += 1;
        let id = self.suggest.next_resolve;
        self.suggest.resolving = Some((id, key));
        self.ext_send(ToHost::ResolveCompletion { id, handle: item.provider, item: Box::new(source.clone()) });
    }

    pub(crate) fn suggest_on_resolved(&mut self, id: u64, item: CompletionItem, cx: &mut Context<Self>) {
        let Some((want, key)) = self.suggest.resolving else { return };
        if want != id {
            return;
        }
        self.suggest.resolving = None;
        self.suggest.resolved.insert(key, Box::new(item));
        self.suggest_focus_changed(); // the focus moved meanwhile
        cx.notify();
    }

    /// What the widget draws, `None` while no list shows.
    pub(super) fn suggest_widget_data(&mut self) -> Option<WidgetData> {
        if !self.suggest.visible {
            return None;
        }
        let list = self.suggest.list;
        let items = self.suggest_items();
        if items.is_empty() {
            return None;
        }
        let count = ListView::visible_rows(items.len());
        let rows: Vec<Row> = (list.top..(list.top + count).min(items.len()))
            .map(|index| {
                let item = &items[index];
                let source = item.source.as_deref();
                let plain_filter = item.completion.filter_text.as_deref().is_none_or(|f| f == item.completion.label);
                let matches = if plain_filter { create_matches(Some(&item.score)) } else { Vec::new() };
                let mut detail = String::new();
                if let Some(s) = source {
                    detail.push_str(s.label_detail.as_deref().unwrap_or(""));
                    if let Some(d) = s.label_description.as_deref() {
                        if !detail.is_empty() {
                            detail.push(' ');
                        }
                        detail.push_str(d);
                    }
                }
                Row {
                    index,
                    icon: vsr_suggest::item::kind::icon(item.completion.kind).1,
                    label: item.completion.label.clone(),
                    matches,
                    detail,
                    focused: index == list.focused,
                }
            })
            .collect();
        let focused = items.get(list.focused)?;
        let resolved_detail = focused
            .source
            .as_deref()
            .map(|s| self.suggest.resolved.get(&s.cache_id.unwrap_or((u64::MAX, 0))).map_or(s, |r| r.as_ref()))
            .and_then(|s| s.detail.clone());
        let cursor_column = self.suggest_cursor().map_or(1, |c| c.column.max(1) as usize);
        Some(WidgetData {
            rows,
            total: items.len(),
            top: list.top,
            anchor_column: (focused.edit_start.column.max(1) as usize).min(cursor_column),
            focused_detail: resolved_detail.filter(|_| self.suggest.settings.show_inline_details),
            show_icons: self.suggest.settings.show_icons,
        })
    }

    /// Whether accepting item `index` would change the text (`acceptSuggestionOnEnter: smart`).
    fn suggest_changes_text(&mut self, index: usize) -> bool {
        let items = self.suggest_items();
        let (Some(item), Some(cursor)) = (items.get(index), self.suggest_cursor()) else { return true };
        let indent: String = cursor.line_text.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let plan = accept::plan(item, Position { line: cursor.line, column: cursor.column }, self.suggest.settings.insert_mode, &indent);
        let units: Vec<u16> = cursor.line_text.encode_utf16().collect();
        let (s, e) = ((plan.start_column - 1).max(0) as usize, ((plan.end_column - 1).max(0) as usize).min(units.len()));
        let current = String::from_utf16_lossy(units.get(s..e.max(s)).unwrap_or(&[]));
        current != plan.text || !plan.additional.is_empty()
    }

    /// Up, Down, Page Up, Page Down and Enter / Tab while the list shows.
    pub(super) fn suggest_widget_command(&mut self, cmd: Command, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.suggest.visible {
            // The binding is scoped to the visible list; a stale key falls through to the editor.
            return false;
        }
        let len = self.suggest_items().len();
        let list = &mut self.suggest.list;
        match cmd {
            Command::SuggestNext => list.next(len),
            Command::SuggestPrevious => list.previous(len),
            Command::SuggestNextPage => list.next_page(len),
            Command::SuggestPreviousPage => list.previous_page(len),
            Command::AcceptSuggest => {
                let index = list.focused;
                self.suggest_accept_at(index, window, cx);
                return true;
            }
            Command::AcceptSuggestOnEnter => {
                let index = list.focused;
                let mode = self.suggest.settings.accept_on_enter;
                match mode {
                    AcceptOnEnter::Off => return false,
                    AcceptOnEnter::Smart if !self.suggest_changes_text(index) => return false,
                    _ => {}
                }
                self.suggest_accept_at(index, window, cx);
                return true;
            }
            _ => return false,
        }
        self.suggest_focus_changed();
        cx.notify();
        true
    }

    /// Accepts item `index` of the list: its text replaces what was typed, and the whole thing (with the
    /// provider's additional edits and the snippet's caret) is one undo step.
    pub(crate) fn suggest_accept_at(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let items = self.suggest_items();
        let Some(mut item) = items.get(index).cloned() else { return };
        let Some(cursor) = self.suggest_cursor() else { return };
        if !cursor.empty {
            return;
        }
        if let Some(key) = item.source.as_deref().and_then(|s| s.cache_id)
            && let Some(resolved) = self.suggest.resolved.get(&key)
        {
            item.source = Some(resolved.clone());
        }
        let indent: String = cursor.line_text.chars().take_while(|c| *c == ' ' || *c == '\t').collect();
        let plan = accept::plan(&item, Position { line: cursor.line, column: cursor.column }, self.suggest.settings.insert_mode, &indent);
        let range = |s: &Span| {
            Range::new(
                EdPos::new(s.start_line.max(1) as usize, s.start_column.max(1) as usize),
                EdPos::new(s.end_line.max(1) as usize, s.end_column.max(1) as usize),
            )
        };
        let main = Range::new(
            EdPos::new(plan.line as usize, plan.start_column.max(1) as usize),
            EdPos::new(plan.line as usize, plan.end_column.max(1) as usize),
        );
        let additional: Vec<(Range, String)> = plan.additional.iter().map(|(s, t)| (range(s), t.clone())).collect();
        let command = item.source.as_deref().and_then(|s| s.command.clone());
        if let Some(i) = self.editors.active
            && let Some(e) = self.editors.open.get_mut(i)
        {
            if e.is_read_only() {
                self.suggest_cancel(cx);
                return;
            }
            e.editor.apply_completion((main, plan.text), plan.selection, &additional);
            self.editor_view.edited(&self.editors.open[i], window);
        }
        self.suggest_cancel(cx);
        if let Some(c) = command {
            let known = cx.try_global::<crate::commands::Commands>().is_some_and(|k| k.execution(&c.id).is_some()) || self.ext_provides(&c.id);
            if known {
                self.execute_command_with(&c.id, c.arguments, window, cx);
            }
        }
        cx.notify();
    }
}
