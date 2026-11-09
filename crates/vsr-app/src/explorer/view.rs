//! The Explorer tree: rows, selection, keyboard navigation and the commands' targets (ADR-0041).
//! The tree state is in `Workbench` (`explorer` field); the rows come from the listings the
//! workbench keeps (`children`, `expanded`).
//!
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerView.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerViewer.ts @ 50e1350fc78d

use std::path::Path;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::actions::{self, CollapseOrParent, ExpandOrFirstChild, MoveSelectionDown, MoveSelectionUp, OpenOrToggle};
use crate::config::settings::OpenSource;
use crate::icons;
use crate::model::{DirEntry, list_dir};
use crate::ui::to_gpui;
use crate::Workbench;

impl Workbench {
    /// Expand or collapse a folder, listing it the first time it opens.
    pub(crate) fn toggle_dir(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
            self.children
                .entry(path.to_path_buf())
                .or_insert_with(|| list_dir(path, &self.root, &self.exclude).unwrap_or_default());
        }
    }

    /// Visible tree rows, depth-first, honouring `expanded`.
    pub(crate) fn visible_rows(&self) -> Vec<(DirEntry, usize)> {
        let mut out = Vec::new();
        self.flatten(&self.root, 0, &mut out);
        out
    }

    fn flatten(&self, dir: &Path, depth: usize, out: &mut Vec<(DirEntry, usize)>) {
        let Some(entries) = self.children.get(dir) else { return };
        for e in entries {
            out.push((e.clone(), depth));
            if e.is_dir && self.expanded.contains(&e.path) {
                self.flatten(&e.path, depth + 1, out);
            }
        }
    }

    /// Select a row and make it the command target.
    pub(crate) fn select(&mut self, path: &Path) {
        self.explorer.selected = Some(path.to_path_buf());
    }

    /// Down/Up: move the selection by `delta` rows (the first or last row when none is selected).
    pub(crate) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.visible_rows();
        if rows.is_empty() {
            return;
        }
        let current = self.explorer.selected.as_ref().and_then(|s| rows.iter().position(|(e, _)| &e.path == s));
        let next = match current {
            None if delta > 0 => 0,
            None => rows.len() - 1,
            Some(i) => (i as isize + delta).clamp(0, rows.len() as isize - 1) as usize,
        };
        self.select(&rows[next].0.path);
        self.explorer.scroll.scroll_to_item(next);
        cx.notify();
    }

    /// Left: collapse the selected folder, else select its parent folder (inside the workspace).
    pub(crate) fn collapse_or_parent(&mut self, cx: &mut Context<Self>) {
        let Some(sel) = self.explorer.selected.clone() else { return };
        if sel.is_dir() && self.expanded.contains(&sel) {
            self.toggle_dir(&sel);
        } else if let Some(parent) = sel.parent().filter(|p| p.starts_with(&self.root) && *p != self.root.as_path()) {
            self.select(&parent.to_path_buf());
            self.explorer.scroll.scroll_to_item(self.row_index(parent).unwrap_or(0));
        }
        cx.notify();
    }

    /// Right: expand the selected folder, else select its first child when it is open.
    pub(crate) fn expand_or_first_child(&mut self, cx: &mut Context<Self>) {
        let Some(sel) = self.explorer.selected.clone() else { return };
        let rows = self.visible_rows();
        let Some(i) = rows.iter().position(|(e, _)| e.path == sel) else { return };
        if !rows[i].0.is_dir {
            return;
        }
        if !self.expanded.contains(&sel) {
            self.toggle_dir(&sel);
        } else if let Some((child, _)) = rows.get(i + 1).filter(|(c, _)| c.path.parent() == Some(sel.as_path())) {
            self.select(&child.path.clone());
            self.explorer.scroll.scroll_to_item(i + 1);
        }
        cx.notify();
    }

    /// Space: toggle the selected folder, or open the selected file without moving focus to it.
    pub(crate) fn open_or_toggle(&mut self, cx: &mut Context<Self>) {
        let Some(sel) = self.explorer.selected.clone() else { return };
        if sel.is_dir() {
            self.toggle_dir(&sel);
        } else {
            self.open_from(&sel, OpenSource::Explorer { double_click: false });
        }
        cx.notify();
    }

    fn row_index(&self, path: &Path) -> Option<usize> {
        self.visible_rows().iter().position(|(e, _)| e.path == path)
    }

    /// Attach the Explorer command handlers to the workbench root (see `actions.rs`).
    pub(crate) fn bind_explorer_actions(el: Div, cx: &mut Context<Self>) -> Div {
        el.on_action(cx.listener(|this, _: &MoveSelectionUp, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &MoveSelectionDown, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &CollapseOrParent, _, cx| this.collapse_or_parent(cx)))
            .on_action(cx.listener(|this, _: &ExpandOrFirstChild, _, cx| this.expand_or_first_child(cx)))
            .on_action(cx.listener(|this, _: &OpenOrToggle, _, cx| this.open_or_toggle(cx)))
    }

    /// The Explorer view: a header and the tree rows (ADR-0039 live refresh, ADR-0041 selection).
    pub(crate) fn explorer(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let t = &self.theme;
        let indent = self.layout(cx).tree_indent;
        let rows = self.visible_rows();
        let active = self.editors.active_editor().map(|e| e.path.clone());
        let selected = self.explorer.selected.clone();

        let chevron = |c: char| {
            div()
                .flex_none()
                .w(px(16.))
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .child(icons::glyph(c))
        };

        let header = div()
            .flex_none()
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .child(chevron(icons::CHEVRON_DOWN))
            .child(crate::folder_name(&self.root).to_uppercase());

        let list = rows.into_iter().enumerate().map(|(i, (entry, depth))| {
            let is_active = active.as_deref() == Some(entry.path.as_path());
            let is_selected = selected.as_deref() == Some(entry.path.as_path());
            let open = self.expanded.contains(&entry.path);
            let path = entry.path.clone();
            let is_dir = entry.is_dir;
            div()
                .id(("tree", i))
                .flex_none()
                .h(px(22.))
                .w_full()
                .flex()
                .items_center()
                .pl(px(8. + depth as f32 * indent))
                .text_size(px(13.))
                .cursor_pointer()
                .when(is_active || is_selected, |d| d.bg(to_gpui(t.list_active_bg)))
                .when(!(is_active || is_selected), |d| d.hover(|s| s.bg(to_gpui(t.list_hover_bg))))
                .child(match (is_dir, open) {
                    (true, true) => chevron(icons::CHEVRON_DOWN),
                    (true, false) => chevron(icons::CHEVRON_RIGHT),
                    _ => div().flex_none().w(px(16.)),
                })
                .child(div().whitespace_nowrap().overflow_hidden().child(entry.name))
                .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                    this.select(&path);
                    if is_dir {
                        this.toggle_dir(&path);
                    } else {
                        // A single click previews, a double click pins (`workbench.editor.enablePreview`).
                        let source = OpenSource::Explorer { double_click: ev.click_count() >= 2 };
                        this.open_from(&path, source);
                    }
                    cx.notify();
                }))
        });

        let tree = div()
            .id("tree-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.explorer.scroll)
            .track_focus(&self.explorer.focus)
            .key_context(actions::KEY_CONTEXT)
            .flex()
            .flex_col()
            .children(list);

        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(tree)
    }
}
