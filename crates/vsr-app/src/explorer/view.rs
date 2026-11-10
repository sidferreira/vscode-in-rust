//! The Explorer tree: rows, selection, keyboard navigation and the commands' targets (ADR-0041).
//! The tree state is in `Workbench` (`explorer` field); the rows come from the listings the
//! workbench keeps (`children`, `expanded`).
//!
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerView.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerViewer.ts @ 50e1350fc78d

use std::path::{Path, PathBuf};

use gpui_kit::component::input::{Enter as InputEnter, Escape as InputEscape, Input};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;

use super::inline::{self, InlineKind};
use super::names::Severity;

/// The group of the Explorer header: its title actions show while it is hovered.
const HEADER_GROUP: &str = "explorer-header";
use super::settings::{AutoReveal, ExplorerSettings, copied_path_text};
use crate::config::AppConfig;
use super::actions::{
    CancelDelete, CollapseAll, CollapseOrParent, CopyPath, CopyRelativePath, DeleteFile, ExpandOrFirstChild, MoveFileToTrash,
    MoveSelectionDown, MoveSelectionUp, NewFile, NewFolder, OpenOrToggle, RefreshAll, RenameFile,
};
use crate::config::settings::OpenSource;
use crate::icons;
use crate::model::DirEntry;
use crate::ui::to_gpui;
use crate::Workbench;

/// A row of the tree: an entry, or the inline name box at a depth.
enum Row {
    Entry(DirEntry, usize),
    Input(usize),
}

impl Workbench {
    /// The inline name box as a tree row, with its validation message under it.
    fn inline_row(&self, depth: usize, cx: &App) -> impl IntoElement {
        let t = &self.theme;
        let indent = self.layout(cx).tree_indent;
        let Some(edit) = self.explorer.inline.as_ref() else { return div().into_any_element() };
        let message = edit.message.as_ref().map(|m| {
            let (bg, border) = match m.severity {
                Severity::Error => (t.validation_error_bg, t.validation_error_border),
                Severity::Warning => (t.validation_error_bg, t.validation_error_border),
            };
            div()
                .mx(px(8.))
                .px(px(6.))
                .py(px(3.))
                .border_1()
                .border_color(to_gpui(border))
                .bg(to_gpui(bg))
                .text_size(px(12.))
                .whitespace_normal()
                .child(m.text.clone())
        });
        div()
            .flex_none()
            .w_full()
            .flex()
            .flex_col()
            .pl(px(8. + depth as f32 * indent))
            .pr(px(8.))
            .text_size(px(13.))
            .child(Input::new(&edit.input).h(px(22.)))
            .children(message)
            .into_any_element()
    }

    /// Expand or collapse a folder, listing it the first time it opens.
    pub(crate) fn toggle_dir(&mut self, path: &Path) {
        if !self.expanded.remove(path) {
            self.expanded.insert(path.to_path_buf());
            if !self.children.contains_key(path) {
                let entries = self.list(path);
                self.children.insert(path.to_path_buf(), entries);
            }
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
            let row = if e.is_dir && self.explorer.settings.compact_folders { self.compacted(e) } else { e.clone() };
            let descend = row.is_dir && self.expanded.contains(&row.path);
            let path = row.path.clone();
            out.push((row, depth));
            if descend {
                self.flatten(&path, depth + 1, out);
            }
        }
    }

    /// `explorer.compactFolders`: a folder whose only child is a folder shares its row with that
    /// child (`a/b/c`). The row stands for the deepest folder, which is the one it opens.
    fn compacted(&self, top: &DirEntry) -> DirEntry {
        let mut row = top.clone();
        while let Some(child) = self.single_child_dir(&row.path) {
            row.name = format!("{}/{}", row.name, child.name);
            row.path = child.path;
            row.modified = child.modified;
        }
        row
    }

    /// The only entry of `dir` when that entry is a folder. Listed on demand when `dir` is not loaded.
    fn single_child_dir(&self, dir: &Path) -> Option<DirEntry> {
        let listed;
        let entries: &[DirEntry] = match self.children.get(dir) {
            Some(v) => v.as_slice(),
            None => {
                listed = self.list(dir);
                listed.as_slice()
            }
        };
        match entries {
            [only] if only.is_dir => Some(only.clone()),
            _ => None,
        }
    }

    /// `explorer.autoReveal`: the file the editor shows is expanded to, selected, and (with `true`)
    /// scrolled into view.
    pub(crate) fn reveal_in_tree(&mut self, path: &Path) {
        let mode = self.explorer.settings.auto_reveal;
        if mode == AutoReveal::Off || path == self.root || !path.starts_with(&self.root) || self.reveal_excluded(path) {
            return;
        }
        let Some(parent) = path.parent().map(Path::to_path_buf) else { return };
        self.expand_to(&parent);
        self.explorer.selected = Some(path.to_path_buf());
        if let Some(ix) = self.row_index(path).filter(|_| mode == AutoReveal::On) {
            self.explorer.scroll.scroll_to_item(ix);
        }
    }

    /// `explorer.autoRevealExclude`: whether `path` or a folder between it and the root matches.
    /// Like upstream's `shouldAutoRevealItem`, a `when` clause checks the siblings on disk.
    fn reveal_excluded(&self, path: &Path) -> bool {
        let rules = &self.explorer.reveal_exclude;
        if rules.is_empty() {
            return false;
        }
        let mut current = path;
        while current != self.root.as_path() && current.starts_with(&self.root) {
            let Ok(rel) = current.strip_prefix(&self.root) else { break };
            let parent = current.parent();
            let has_sibling = |name: &str| parent.is_some_and(|p| p.join(name).symlink_metadata().is_ok());
            if rules.is_excluded(&rel.to_string_lossy(), &has_sibling) {
                return true;
            }
            match parent {
                Some(p) => current = p,
                None => break,
            }
        }
        false
    }

    /// `explorer.*` changed in the configuration: take the new values, and list again when the
    /// sort order changed.
    pub(crate) fn apply_explorer_settings(&mut self, cx: &mut Context<Self>) {
        let configuration = AppConfig::get(cx).configuration();
        let new = ExplorerSettings::from_configuration(configuration);
        self.explorer.reveal_exclude = super::settings::reveal_exclude(configuration);
        let old = self.explorer.settings;
        let sort_changed = (new.sort_order, new.lexicographic, new.sort_reverse) != (old.sort_order, old.lexicographic, old.sort_reverse);
        self.explorer.settings = new;
        if sort_changed {
            self.refresh_all(cx);
        }
    }

    /// Expand `dir` and the folders above it (inside the workspace), so its rows are visible.
    pub(crate) fn expand_to(&mut self, dir: &Path) {
        let mut chain: Vec<PathBuf> = dir
            .ancestors()
            .take_while(|p| p.starts_with(&self.root) && *p != self.root.as_path())
            .map(Path::to_path_buf)
            .collect();
        chain.reverse();
        for folder in chain {
            if !self.children.contains_key(&folder) {
                let entries = self.list(&folder);
                self.children.insert(folder.clone(), entries);
            }
            self.expanded.insert(folder);
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
        } else {
            // The nearest folder above that has a row (a compacted folder hides its parents' rows).
            let rows = self.visible_rows();
            let above = sel.ancestors().skip(1).take_while(|p| p.starts_with(&self.root) && *p != self.root.as_path());
            if let Some(row) = above.filter_map(|p| rows.iter().position(|(e, _)| e.path == p)).next() {
                self.select(&rows[row].0.path.clone());
                self.explorer.scroll.scroll_to_item(row);
            }
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

    /// Refresh Explorer: re-list every folder the tree shows.
    pub(crate) fn refresh_all(&mut self, cx: &mut Context<Self>) {
        let dirs: Vec<PathBuf> = self.children.keys().cloned().collect();
        for dir in dirs {
            self.refresh_dir(&dir);
        }
        cx.notify();
    }

    /// Copy Path / Copy Relative Path: the selected entry, else the active editor's file. The
    /// separator comes from `explorer.copyPathSeparator` / `explorer.copyRelativePathSeparator`.
    pub(crate) fn copy_path(&mut self, relative: bool, cx: &mut Context<Self>) {
        let path = self.explorer.selected.clone().or_else(|| self.editors.active_editor().map(|e| e.path.clone()));
        let Some(path) = path else { return };
        let separator = if relative {
            self.explorer.settings.copy_relative_path_separator
        } else {
            self.explorer.settings.copy_path_separator
        };
        cx.write_to_clipboard(ClipboardItem::new_string(copied_path_text(&path, &self.root, relative, separator)));
    }

    /// Collapse Folders in Explorer: every folder closes.
    pub(crate) fn collapse_all(&mut self, cx: &mut Context<Self>) {
        self.expanded.clear();
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
            .on_action(cx.listener(|this, _: &NewFile, window, cx| this.start_inline(InlineKind::NewFile, window, cx)))
            .on_action(cx.listener(|this, _: &NewFolder, window, cx| this.start_inline(InlineKind::NewFolder, window, cx)))
            .on_action(cx.listener(|this, _: &RenameFile, window, cx| {
                // Enter confirms a pending delete; otherwise it renames the selection.
                if this.explorer.confirm.is_some() {
                    this.confirm_delete(cx);
                } else {
                    this.start_inline(InlineKind::Rename, window, cx);
                }
            }))
            .on_action(cx.listener(|this, _: &MoveFileToTrash, _, cx| this.request_delete(false, cx)))
            .on_action(cx.listener(|this, _: &DeleteFile, _, cx| this.request_delete(true, cx)))
            .on_action(cx.listener(|this, _: &CancelDelete, _, cx| this.cancel_delete(cx)))
            .on_action(cx.listener(|this, _: &RefreshAll, _, cx| this.refresh_all(cx)))
            .on_action(cx.listener(|this, _: &CollapseAll, _, cx| this.collapse_all(cx)))
            .on_action(cx.listener(|this, _: &CopyPath, _, cx| this.copy_path(false, cx)))
            .on_action(cx.listener(|this, _: &CopyRelativePath, _, cx| this.copy_path(true, cx)))
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

        // View title actions (ADR-0041): shown while the header is hovered, as upstream's
        // `navigation` group of the Explorer's view title.
        let title_button = |id: &'static str, glyph: char, tip: &'static str| {
            div()
                .id(id)
                .flex_none()
                .w(px(22.))
                .h(px(22.))
                .rounded(px(5.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .font_family(icons::FAMILY)
                .text_size(px(16.))
                .text_color(to_gpui(t.side_bar_fg))
                .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
                .child(icons::glyph(glyph))
                .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
        };
        let title_actions = div()
            .flex_none()
            .flex()
            .items_center()
            .invisible()
            .group_hover(HEADER_GROUP, |s| s.visible())
            .child(
                title_button("explorer-new-file", icons::NEW_FILE, "New File...").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| this.start_inline(InlineKind::NewFile, window, cx),
                )),
            )
            .child(
                title_button("explorer-new-folder", icons::NEW_FOLDER, "New Folder...").on_click(cx.listener(
                    |this, _: &ClickEvent, window, cx| this.start_inline(InlineKind::NewFolder, window, cx),
                )),
            )
            .child(
                title_button("explorer-refresh", icons::REFRESH, "Refresh Explorer")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_all(cx))),
            )
            .child(
                title_button("explorer-collapse", icons::COLLAPSE_ALL, "Collapse Folders in Explorer")
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.collapse_all(cx))),
            );

        let header = div()
            .group(HEADER_GROUP)
            .flex_none()
            .h(px(22.))
            .px(px(8.))
            .flex()
            .items_center()
            .text_size(px(11.))
            .font_weight(FontWeight::BOLD)
            .child(chevron(icons::CHEVRON_DOWN))
            .child(div().flex_1().min_w_0().overflow_hidden().whitespace_nowrap().child(crate::folder_name(&self.root).to_uppercase()))
            .child(title_actions);

        let inline_open = self.explorer.inline.is_some();
        let new_parent = self.explorer.inline.as_ref().filter(|e| e.is_new()).map(|e| e.parent.clone());
        let rename_target = self.explorer.inline.as_ref().and_then(|e| e.target.clone());
        let mut items: Vec<Row> = Vec::new();
        if new_parent.as_deref() == Some(self.root.as_path()) {
            items.push(Row::Input(0));
        }
        for (entry, depth) in rows {
            if rename_target.as_deref() == Some(entry.path.as_path()) {
                items.push(Row::Input(depth));
                continue;
            }
            let opens_input = new_parent.as_deref() == Some(entry.path.as_path());
            items.push(Row::Entry(entry, depth));
            if opens_input {
                items.push(Row::Input(depth + 1));
            }
        }

        let list = items.into_iter().enumerate().map(|(i, row)| {
            let (entry, depth) = match row {
                Row::Input(depth) => return self.inline_row(depth, cx).into_any_element(),
                Row::Entry(entry, depth) => (entry, depth),
            };
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
                .into_any_element()
        });

        let mut tree = div()
            .id("tree-scroll")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.explorer.scroll)
            .track_focus(&self.explorer.focus)
            .flex()
            .flex_col()
            // Enter and Escape in the box commit or cancel it (captured before the box sees them).
            .capture_action(cx.listener(|this, _: &InputEnter, window, cx| {
                if this.explorer.inline.is_some() {
                    cx.stop_propagation();
                    this.enter_inline(window, cx);
                }
            }))
            .capture_action(cx.listener(|this, _: &InputEscape, window, cx| {
                if this.explorer.inline.is_some() {
                    cx.stop_propagation();
                    this.cancel_inline(window, cx);
                }
            }))
            .children(list);
        if let Some(ctx) = inline::tree_key_context(inline_open) {
            tree = tree.key_context(ctx);
        }

        let bar = self.delete_bar(cx);
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .children(bar)
            .child(tree)
    }
}
