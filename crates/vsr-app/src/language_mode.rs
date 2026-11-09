//! "Change Language Mode" (`workbench.action.editor.changeLanguageMode`, Cmd+K M) and the repaint
//! poll that shows tokens the background tokenizer finishes (ADR-0038).
//!
//! upstream: src/vs/workbench/browser/parts/editor/editorStatus.ts @ 50e1350fc78d (`ChangeLanguageAction`)
//!
//! The pick lists every registered language by name with its id (`(rust)`) as the description, the
//! current one marked `Configured Language`, and "Auto Detect" first when the document is not
//! empty. Not ported: "Configure File Association for '.ext'...", "Configure '<language>' language
//! based settings..." and the marketplace entry (they need the Settings editor's JSON mode and the
//! extension gallery).

use std::rc::Rc;
use std::time::Duration;

use gpui_kit::*;
use vsr_quickinput::pick::{PickItem, PickRow, filter_items};
use vsr_syntax::SyntaxService;

use crate::Workbench;
use crate::quick_input::{PickResult, QuickPick, QuickPickEvent};

actions!(rscode, [ChangeLanguageMode]);

/// How often a busy tokenizer asks for a repaint.
const POLL: Duration = Duration::from_millis(60);
/// `PickItem::value` of the "Auto Detect" entry.
const AUTO_DETECT: &str = "\u{0}auto";

/// Register the key binding (call once at startup).
pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new("cmd-k cmd-m", ChangeLanguageMode, None)]);
}

#[derive(Default)]
pub struct LanguageUi {
    pick: Option<Entity<QuickPick>>,
    _sub: Option<Subscription>,
    return_focus: Option<FocusHandle>,
    poll: Option<Task<()>>,
}

impl LanguageUi {
    /// The picker, to be drawn above everything else.
    pub fn view(&self) -> Option<Entity<QuickPick>> {
        self.pick.clone()
    }

    #[cfg(test)]
    pub fn picker(&self) -> Option<&Entity<QuickPick>> {
        self.pick.as_ref()
    }
}

/// The items of the pick: "Auto Detect" (when `non_empty`), then every language. Item `id`s index
/// the returned list; `value` is the language id (or `AUTO_DETECT`).
pub fn language_items(syntax: &SyntaxService, current_id: Option<&str>, non_empty: bool) -> Vec<PickItem> {
    let current_name = current_id.and_then(|id| syntax.language_name(id));
    let mut items = Vec::new();
    if non_empty {
        items.push(PickItem { value: Some(AUTO_DETECT.to_string()), ..PickItem::new(0, "Auto Detect") });
    }
    for pair in syntax.sorted_language_names() {
        let description = if current_name.as_deref() == Some(pair.language_name.as_str()) {
            format!("({}) - Configured Language", pair.language_id)
        } else {
            format!("({})", pair.language_id)
        };
        let mut item = PickItem::new(items.len(), pair.language_name).with_description(description);
        item.value = Some(pair.language_id);
        items.push(item);
    }
    items
}

/// The rows for a typed text: the separator only when nothing is typed (the list is not filtered).
pub fn language_rows(items: &[PickItem], query: &str) -> Vec<PickRow> {
    if !query.trim().is_empty() {
        return filter_items(items, query);
    }
    let mut rows = Vec::with_capacity(items.len() + 1);
    let mut rest = items;
    if let Some(auto) = items.first().filter(|i| i.value.as_deref() == Some(AUTO_DETECT)) {
        rows.push(PickRow::Item(auto.clone()));
        rest = &items[1..];
    }
    rows.push(PickRow::Separator("languages (identifier)".to_string()));
    rows.extend(rest.iter().cloned().map(PickRow::Item));
    rows
}

impl Workbench {
    /// Cmd+K M: pick a language for the active editor.
    pub(crate) fn show_language_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.lang.pick.is_some() {
            return;
        }
        let items = match self.editors.active_editor() {
            Some(e) => {
                let non_empty = e.buffer().len_utf16() > 0;
                language_items(&self.editors.syntax, Some(e.language_id()), non_empty)
            }
            None => vec![PickItem::new(0, "No text editor active at this time")],
        };
        let items = Rc::new(items);
        let source_items = Rc::clone(&items);
        let source = move |query: &str, _cx: &mut App| PickResult::Ready(language_rows(&source_items, query));
        self.lang.return_focus = window.focused(cx);
        let pick = cx.new(|cx| QuickPick::new("Select Language Mode", "", None, true, source, window, cx));
        self.lang._sub = Some(cx.subscribe_in(&pick, window, |this, _, ev: &QuickPickEvent, window, cx| match ev {
            QuickPickEvent::Accept { item, .. } => this.accept_language(item.value.clone(), window, cx),
            QuickPickEvent::Dismiss => this.hide_language_picker(true, window, cx),
            QuickPickEvent::Change { .. } => {}
        }));
        self.lang.pick = Some(pick);
        cx.notify();
    }

    fn hide_language_picker(&mut self, restore_focus: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.lang.pick = None;
        self.lang._sub = None;
        let focus = self.lang.return_focus.take();
        if restore_focus && let Some(f) = focus.or_else(|| self.focus.clone()) {
            f.focus(window, cx);
        }
        cx.notify();
    }

    /// A language (or Auto Detect) was picked: set it on the active editor and go back to it.
    fn accept_language(&mut self, value: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.hide_language_picker(true, window, cx);
        let (Some(value), Some(index)) = (value, self.editors.active) else { return };
        if value == AUTO_DETECT {
            self.editors.auto_detect_language(index);
        } else {
            self.editors.set_language_of(index, &value);
        }
        cx.notify();
    }

    /// `files.associations` and `editor.maxTokenizationLineLength`, read from the configuration.
    pub(crate) fn apply_language_settings(&mut self, cx: &mut Context<Self>) {
        let configuration = crate::config::AppConfig::get(cx).configuration();
        let associations: Vec<(String, String)> = match configuration.get_value("files.associations") {
            Some(serde_json::Value::Object(map)) => {
                // A value that is not a string is ignored (upstream issue 147284).
                map.iter().filter_map(|(k, v)| v.as_str().map(|id| (k.clone(), id.to_string()))).collect()
            }
            _ => Vec::new(),
        };
        let max = configuration
            .get_f64("editor.maxTokenizationLineLength")
            .map_or(vsr_syntax::DEFAULT_MAX_TOKENIZATION_LINE_LENGTH, |n| n.max(0.0) as usize);
        self.editors.apply_associations(&associations);
        if max != self.editors.max_tokenization_line_length {
            self.editors.set_max_tokenization_line_length(max);
        }
    }

    /// While the active editor's tokenizer has lines to do, repaints every `POLL` so the colors
    /// appear as they are computed (a gpui window only repaints on notify).
    pub(crate) fn poll_syntax(&mut self, cx: &mut Context<Self>) {
        if self.lang.poll.is_some() || !self.editors.active_editor().is_some_and(|e| e.syntax_busy()) {
            return;
        }
        self.lang.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let more = this
                    .update(cx, |wb, cx| {
                        cx.notify();
                        let busy = wb.editors.active_editor().is_some_and(|e| e.syntax_busy());
                        if !busy {
                            wb.lang.poll = None;
                        }
                        busy
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[core::prelude::v1::test]
    fn items_mark_the_current_language_and_offer_auto_detect() {
        let s = SyntaxService::new();
        let items = language_items(&s, Some("rust"), true);
        assert_eq!(items[0].label, "Auto Detect");
        let rust = items.iter().find(|i| i.value.as_deref() == Some("rust")).unwrap();
        assert_eq!(rust.label, "Rust");
        assert_eq!(rust.description.as_deref(), Some("(rust) - Configured Language"));
        let ts = items.iter().find(|i| i.value.as_deref() == Some("typescript")).unwrap();
        assert_eq!(ts.description.as_deref(), Some("(typescript)"));
        // Sorted by name, case-insensitively.
        let names: Vec<_> = items.iter().skip(1).map(|i| i.label.to_lowercase()).collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
        // An empty document has no Auto Detect.
        assert_ne!(language_items(&s, None, false)[0].label, "Auto Detect");
    }

    #[core::prelude::v1::test]
    fn rows_have_a_separator_until_the_user_types() {
        let s = SyntaxService::new();
        let items = language_items(&s, Some("rust"), true);
        let rows = language_rows(&items, "");
        assert!(matches!(&rows[0], PickRow::Item(i) if i.label == "Auto Detect"));
        assert_eq!(rows[1], PickRow::Separator("languages (identifier)".into()));
        let filtered = language_rows(&items, "rus");
        assert!(filtered.iter().all(|r| r.item().is_some()));
        assert_eq!(filtered[0].item().unwrap().label, "Rust");
    }
}
