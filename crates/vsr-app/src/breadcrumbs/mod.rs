//! Breadcrumbs: the path of the active file, in a 22px bar under the tab strip (ADR-0047).
//! Clicking a crumb opens a dropdown with the entries of its folder (folders open in place, files
//! open in the editor). Keyboard: `breadcrumbs.focus` moves focus to the last crumb, Left and Right
//! move between crumbs, Enter opens the dropdown of the focused crumb, Escape gives focus back.
//!
//! upstream: src/vs/workbench/browser/parts/editor/breadcrumbs.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/parts/editor/breadcrumbsControl.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/parts/editor/media/breadcrumbscontrol.css @ 50e1350fc78d
//!
//! The file path is the model's (`model.rs`). The symbol path needs document symbols and is
//! blocked (`upstream/settings.toml`). Differences from VS Code: the dropdown is the Quick Pick
//! (centred at the top, not anchored under the crumb), it has no filter-by-typing on the folder
//! structure beyond the Quick Pick's own, and there is no editor type switcher.

#[cfg(test)]
#[path = "e2e_tests.rs"]
mod e2e_tests;
pub mod model;

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui_kit::*;
use serde_json::Value;
use vsr_quickinput::pick::{PickItem, PickRow, filter_items};

use crate::config::AppConfig;
use crate::icons;
use crate::quick_input::{PickResult, QuickPick, QuickPickEvent};
use crate::quick_open;
use crate::theme::metrics as m;
use crate::ui::to_gpui;
use crate::config::settings::OpenSource;
use crate::Workbench;
use model::{BreadcrumbsSettings, Child, Segment};

actions!(
    rscode,
    [
        BreadcrumbsToggle,
        BreadcrumbsFocus,
        BreadcrumbsFocusAndSelect,
        BreadcrumbsLeft,
        BreadcrumbsRight,
        BreadcrumbsOpen,
        BreadcrumbsExit,
    ]
);

/// The key context of the bar: the arrow and Enter keys below apply only while it has focus.
const CONTEXT: &str = "Breadcrumbs";

/// Register the keys (call once at startup, after `commands::init`).
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-shift-;", BreadcrumbsFocus, None),
        KeyBinding::new("cmd-shift-.", BreadcrumbsFocusAndSelect, None),
        KeyBinding::new("left", BreadcrumbsLeft, Some(CONTEXT)),
        KeyBinding::new("right", BreadcrumbsRight, Some(CONTEXT)),
        KeyBinding::new("enter", BreadcrumbsOpen, Some(CONTEXT)),
        KeyBinding::new("escape", BreadcrumbsExit, Some(CONTEXT)),
    ]);
}

/// The bar's state: its focus handle, the crumb with keyboard focus, the crumb whose dropdown is
/// open, and the folder that dropdown lists (shared with its source, which runs on every change).
#[derive(Default)]
pub(crate) struct Crumbs {
    focus: Option<FocusHandle>,
    active: Option<usize>,
    picker_for: Option<usize>,
    dir: Rc<RefCell<PathBuf>>,
}

impl Crumbs {
    /// The crumb whose dropdown is open (it is drawn with the active selection colour).
    pub(crate) fn picker_open(&self) -> bool {
        self.picker_for.is_some()
    }

    /// Called when the Quick Pick closes: a dropdown does not outlive its picker.
    pub(crate) fn end_picker(&mut self) {
        self.picker_for = None;
    }
}

/// The dropdown rows: the entries of a folder, the id of each is its index in the listing.
fn child_items(children: &[Child]) -> Vec<PickItem> {
    children
        .iter()
        .enumerate()
        .map(|(id, c)| {
            let mut item = PickItem::new(id, c.name.clone());
            item.value = Some(c.path.to_string_lossy().into_owned());
            item
        })
        .collect()
}

impl Workbench {
    /// The crumbs of the active text editor, with the mode the settings say. Empty when there is
    /// no text editor in front, or when `breadcrumbs.filePath` is `off`.
    fn crumb_segments(&self, settings: &BreadcrumbsSettings) -> Vec<Segment> {
        if !self.editor_surface_active() {
            return Vec::new();
        }
        match self.editors.active_editor() {
            // An untitled text has no file path to show (ADR-0058).
            Some(e) if !e.is_untitled() => model::file_segments(&self.root, &e.path, settings.file_path),
            _ => Vec::new(),
        }
    }

    /// The bar, or nothing when `breadcrumbs.enabled` is off or there is nothing to show.
    pub(crate) fn breadcrumb_bar(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let settings = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration());
        if !settings.enabled {
            return None;
        }
        let segments = self.crumb_segments(&settings);
        if segments.is_empty() {
            return None;
        }
        let focus = self.crumbs.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        let t = self.theme.clone();
        let mut bar = div()
            .id("breadcrumbs")
            .debug_selector(|| "breadcrumbs".to_string())
            .key_context(CONTEXT)
            .track_focus(&focus)
            .on_action(cx.listener(|this, _: &BreadcrumbsLeft, _, cx| this.move_crumb(false, cx)))
            .on_action(cx.listener(|this, _: &BreadcrumbsRight, _, cx| this.move_crumb(true, cx)))
            .on_action(cx.listener(|this, _: &BreadcrumbsOpen, window, cx| this.open_focused_crumb(window, cx)))
            .on_action(cx.listener(|this, _: &BreadcrumbsExit, _, cx| this.leave_crumbs(cx)))
            .flex_none()
            .w_full()
            .h(px(m::BREADCRUMBS_HEIGHT))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(2.))
            .overflow_hidden()
            .bg(to_gpui(t.breadcrumb_bg))
            .text_size(px(12.));
        for (i, seg) in segments.iter().enumerate() {
            if i > 0 {
                bar = bar.child(
                    div()
                        .flex_none()
                        .font_family(icons::FAMILY)
                        .text_size(px(14.))
                        .text_color(to_gpui(t.breadcrumb_fg))
                        .child(icons::glyph(icons::CHEVRON_RIGHT)),
                );
            }
            let color = if self.crumbs.picker_for == Some(i) {
                t.breadcrumb_active_fg
            } else if self.crumbs.active == Some(i) {
                t.breadcrumb_focus_fg
            } else {
                t.breadcrumb_fg
            };
            let mut item = div()
                .id(("breadcrumb", i))
                .debug_selector(move || format!("breadcrumb:{i}"))
                .flex_none()
                .flex()
                .items_center()
                .gap(px(4.))
                .px(px(3.))
                .rounded(px(3.))
                .cursor_pointer()
                .text_color(to_gpui(color))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.show_crumb_picker(i, window, cx)));
            if settings.icons {
                let glyph = if seg.is_dir { icons::FOLDER } else { icons::FILE };
                item = item.child(div().font_family(icons::FAMILY).text_size(px(14.)).child(icons::glyph(glyph)));
            }
            bar = bar.child(item.child(seg.name.clone()));
        }
        Some(bar.into_any_element())
    }

    /// Crumb `index` was clicked, or Enter on the focused crumb: list the entries of its parent
    /// folder (the siblings of the crumb), with the crumb's own entry selected.
    pub(crate) fn show_crumb_picker(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let settings = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration());
        let segments = self.crumb_segments(&settings);
        let Some(seg) = segments.get(index) else { return };
        let parent = seg.path.parent().map(Path::to_path_buf).unwrap_or_else(|| self.root.clone());
        if self.quick.pick.is_some() {
            self.hide_quick_open(false, window, cx);
        }
        self.crumbs.active = Some(index);
        let listing = model::list_children(&self.root, &parent, &quick_open::exclude_globs(AppConfig::get(cx).configuration()));
        let selected = listing.iter().position(|c| c.path == seg.path);
        *self.crumbs.dir.borrow_mut() = parent;
        let dir = Rc::clone(&self.crumbs.dir);
        let root = self.root.clone();
        let excludes = quick_open::exclude_globs(AppConfig::get(cx).configuration());
        let source = move |query: &str, _cx: &mut App| -> PickResult {
            let folder = dir.borrow().clone();
            let items = child_items(&model::list_children(&root, &folder, &excludes));
            let rows = if query.trim().is_empty() {
                items.into_iter().map(PickRow::Item).collect()
            } else {
                filter_items(&items, query)
            };
            PickResult::Ready(rows)
        };
        let close_on_focus_lost = quick_open::QuickOpenSettings::from_configuration(AppConfig::get(cx).configuration()).close_on_focus_lost;
        self.quick.return_focus = window.focused(cx);
        self.quick.provider = String::from("breadcrumbs");
        self.crumbs.picker_for = Some(index);
        let pick = cx.new(|cx| QuickPick::new("", "", None, close_on_focus_lost, source, window, cx));
        if let Some(i) = selected {
            pick.update(cx, |p, _| p.set_active_item(i));
        }
        self.quick._sub = Some(cx.subscribe_in(&pick, window, |this, _, ev: &QuickPickEvent, window, cx| match ev {
            QuickPickEvent::Accept { item, .. } => this.accept_crumb_item(item, window, cx),
            QuickPickEvent::Dismiss => this.hide_quick_open(true, window, cx),
            QuickPickEvent::Change { .. } | QuickPickEvent::ActiveChange { .. } => {}
        }));
        self.quick.pick = Some(pick);
        cx.notify();
    }

    /// An entry of the dropdown was accepted: a folder lists its own entries, a file opens.
    fn accept_crumb_item(&mut self, item: &PickItem, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = item.value.as_deref().map(PathBuf::from) else { return };
        if path.is_dir() {
            *self.crumbs.dir.borrow_mut() = path;
            if let Some(pick) = self.quick.pick.clone() {
                pick.update(cx, |p, cx| {
                    p.set_value("", window, cx);
                    p.refresh(cx);
                });
            }
            return;
        }
        self.hide_quick_open(false, window, cx);
        if !path.is_file() {
            self.error = Some(format!("{}: file not found", path.display()));
            return;
        }
        self.open_from(&path, OpenSource::QuickOpen);
        if let Some(e) = self.editors.active.and_then(|i| self.editors.open.get(i)).filter(|e| e.path == path) {
            self.editor_view.reveal(e);
        }
        cx.notify();
    }

    /// `breadcrumbs.focus` (`focusAndSelect` also opens the dropdown of the last crumb).
    pub(crate) fn focus_crumbs(&mut self, select: bool, window: &mut Window, cx: &mut Context<Self>) {
        let settings = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration());
        if !settings.enabled {
            return;
        }
        let count = self.crumb_segments(&settings).len();
        if count == 0 {
            return;
        }
        self.crumbs.active = Some(count - 1);
        let focus = self.crumbs.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        focus.focus(window, cx);
        if select {
            self.show_crumb_picker(count - 1, window, cx);
        }
        cx.notify();
    }

    /// Left and Right move the keyboard focus between crumbs (it stops at the ends).
    fn move_crumb(&mut self, forward: bool, cx: &mut Context<Self>) {
        let settings = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration());
        let count = self.crumb_segments(&settings).len();
        if count == 0 {
            return;
        }
        let current = self.crumbs.active.unwrap_or(count - 1).min(count - 1);
        self.crumbs.active = Some(if forward { (current + 1).min(count - 1) } else { current.saturating_sub(1) });
        cx.notify();
    }

    /// Enter on the focused crumb.
    fn open_focused_crumb(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(i) = self.crumbs.active {
            self.show_crumb_picker(i, window, cx);
        }
    }

    /// Escape: give the keyboard focus back to the editor.
    fn leave_crumbs(&mut self, cx: &mut Context<Self>) {
        self.crumbs.active = None;
        if let Some(e) = self.editors.active.and_then(|i| self.editors.open.get(i)) {
            self.editor_view.reveal(e);
        }
        cx.notify();
    }

    /// `breadcrumbs.toggle`: flips `breadcrumbs.enabled` in the user settings.
    pub(crate) fn toggle_breadcrumbs(&mut self, cx: &mut Context<Self>) {
        let on = BreadcrumbsSettings::from_configuration(AppConfig::get(cx).configuration()).enabled;
        if let Err(e) = AppConfig::set_value(cx, "breadcrumbs.enabled", Some(Value::Bool(!on))) {
            self.error = Some(e);
        }
        cx.notify();
    }
}
