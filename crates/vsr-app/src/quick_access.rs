//! The Quick Access providers on the Quick Pick, workbench side: the Command Palette (`>`),
//! Go to Line/Column (`:`) and the `?` help list; and the workbench commands they and the key
//! bindings run (toggle side bar, show Explorer, clear command history).
//!
//! upstream: src/vs/platform/quickinput/browser/quickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/commandsQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/quickaccess/browser/commandsQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/platform/quickinput/browser/helpQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/quickAccess/browser/gotoLineQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/codeEditor/browser/quickaccess/gotoLineQuickAccess.ts @ 50e1350fc78d
//!
//! One Quick Pick serves every provider (`quick_open.rs` creates it): the text typed decides the
//! provider through `QuickAccessRegistry` (longest prefix), so Backspace over `>` is Quick Open
//! again and typing `>` in Quick Open is the palette, with no second widget. The rows of `>` and
//! `?` come from the picker's source (they only need the global registry); Go to Line needs the
//! active editor, so the workbench fills its single row when the text changes.
//!
//! Not implemented: live reveal and decoration of the line while typing, the zero-based offset
//! toggle, relative line numbers handling, accepting in the background, the remove-from-history
//! button of recently used commands, tf-idf "similar commands", a confirmation for Clear Command
//! History (there are no dialogs yet), persisted command history.

#[cfg(test)]
#[path = "quick_access/e2e_tests.rs"]
mod e2e_tests;

use gpui_kit::*;
use vsr_commands::ContextKeys;
use vsr_quickinput::commands::{PREFIX as COMMANDS_PREFIX, command_picks};
use vsr_quickinput::goto_line::{GotoLineModel, parse_position};
use vsr_quickinput::pick::{PickItem, PickRow};
use vsr_quickinput::quick_access::help_picks;
use vsr_settings::Configuration;
use vsr_text::TextBuffer;

use crate::Workbench;
use crate::commands::{self, Commands, Execution, Scope};

/// `workbench.commandPalette.*`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaletteSettings {
    /// `history`: how many recently used commands to keep (0 turns the history off).
    pub history: usize,
    /// `preserveInput`: reopen with the last text.
    pub preserve_input: bool,
}

impl PaletteSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        Self {
            history: c.get_f64("workbench.commandPalette.history").map_or(50, |n| n.max(0.0) as usize),
            preserve_input: c.get_bool("workbench.commandPalette.preserveInput").unwrap_or(false),
        }
    }
}

/// The prefix of the provider that handles `text`.
pub fn provider_of(text: &str, ctx: &ContextKeys, cx: &App) -> String {
    cx.try_global::<Commands>().and_then(|c| c.quick_access.provider(text, ctx)).map(|p| p.prefix.clone()).unwrap_or_default()
}

/// The rows of the Command Palette for the typed `query` (which starts with `>`).
pub fn commands_rows(ctx: &ContextKeys, query: &str, cx: &App) -> Vec<PickRow> {
    let Some(commands) = cx.try_global::<Commands>() else { return Vec::new() };
    let filter = query.strip_prefix(COMMANDS_PREFIX).unwrap_or(query).trim();
    let palette = commands.registry.palette_commands(ctx);
    command_picks(&palette, filter, &commands.history, &|id| commands::keybinding_of_command(id, cx))
}

/// The rows of the `?` help list.
pub fn help_rows(ctx: &ContextKeys, cx: &App) -> Vec<PickRow> {
    let Some(commands) = cx.try_global::<Commands>() else { return Vec::new() };
    help_picks(&commands.quick_access, ctx, &|id| commands::keybinding_of_command(id, cx)).into_iter().map(PickRow::Item).collect()
}

/// The editor's text buffer as `parsePosition` reads it.
struct BufferModel<'a> {
    buffer: &'a TextBuffer,
    tab_size: usize,
}

impl GotoLineModel for BufferModel<'_> {
    fn line_count(&self) -> usize {
        self.buffer.line_count()
    }
    fn line_content(&self, line: usize) -> String {
        self.buffer.line_content(line).to_string()
    }
    fn value_length(&self) -> usize {
        self.buffer.len_utf16()
    }
    fn position_at(&self, offset: usize) -> (usize, usize) {
        let p = self.buffer.position_at(offset);
        (p.line, p.column)
    }
    fn tab_size(&self) -> usize {
        self.tab_size
    }
}

impl Workbench {
    /// The context keys the palette's preconditions read.
    pub(crate) fn context_keys(&self) -> ContextKeys {
        commands::context_keys(self.editors.active_editor().is_some() && self.editor_surface_active(), self.sidebar_visible)
    }

    /// The prefix of the provider for `text`, with the context keys of the open picker.
    pub(crate) fn provider_prefix(&self, text: &str, cx: &App) -> String {
        provider_of(text, &self.quick.data.borrow().ctx, cx)
    }

    pub(crate) fn placeholder_of(&self, prefix: &str, cx: &App) -> String {
        let ctx = self.quick.data.borrow().ctx.clone();
        cx.try_global::<Commands>()
            .and_then(|c| c.quick_access.provider(prefix, &ctx))
            .map(|p| p.placeholder.clone())
            .unwrap_or_default()
    }

    /// The text of the picker changed: follow a change of provider (placeholder) and give Go to
    /// Line its row.
    pub(crate) fn quick_access_changed(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pick) = self.quick.pick.clone() else { return };
        let prefix = self.provider_prefix(query, cx);
        if prefix != self.quick.provider {
            let placeholder = self.placeholder_of(&prefix, cx);
            self.quick.provider = prefix.clone();
            pick.update(cx, |p, cx| p.set_placeholder(&placeholder, window, cx));
        }
        if prefix == ":" {
            let rows = vec![PickRow::Item(PickItem::new(0, self.parse_go_to_line(query).label))];
            pick.update(cx, |p, cx| p.set_rows(rows, cx));
        }
    }

    /// `parsePosition` over the active text editor for the typed `query` (`:42:5`).
    fn parse_go_to_line(&self, query: &str) -> vsr_quickinput::goto_line::ParsedPosition {
        let text = query.trim().strip_prefix(':').unwrap_or(query);
        let model = self
            .editors
            .active_editor()
            .filter(|_| self.editor_surface_active())
            .map(|e| BufferModel { buffer: e.buffer(), tab_size: self.editors.options.tab_size });
        parse_position(model.as_ref().map(|m| m as &dyn GotoLineModel), text, false)
    }

    /// Enter in Go to Line: move the caret there, close and focus the editor.
    pub(crate) fn accept_go_to_line(&mut self, query: &str, window: &mut Window, cx: &mut Context<Self>) {
        let parsed = self.parse_go_to_line(query);
        let (Some(line), Some(column)) = (parsed.line, parsed.column) else { return };
        let Some(path) = self.editors.active_editor().map(|e| e.path.clone()) else { return };
        self.hide_quick_open(false, window, cx);
        let range = vsr_search::SearchRange::new(line, column, line, column);
        self.open_at(&path, range, crate::config::settings::OpenSource::QuickOpen, window, cx);
    }

    /// Enter on a help entry: switch to that provider (the text becomes its prefix).
    pub(crate) fn accept_help(&mut self, prefix: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(prefix), Some(pick)) = (prefix, self.quick.pick.clone()) else { return };
        pick.update(cx, |p, cx| p.set_value(&prefix, window, cx));
    }

    /// Enter on a command of the palette: close, remember it as recently used, run it.
    pub(crate) fn accept_command(&mut self, id: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = id else { return };
        self.hide_quick_open(true, window, cx);
        cx.update_global::<Commands, _>(|c, _| c.history.push(&id));
        self.execute_command(&id, window, cx);
    }

    /// `commands.executeCommand(id)`.
    pub(crate) fn execute_command(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.execute_command_with(id, Vec::new(), window, cx);
    }

    /// `commands.executeCommand(id, ...args)`: a workbench command, or an extension's.
    pub(crate) fn execute_command_with(&mut self, id: &str, args: Vec<serde_json::Value>, window: &mut Window, cx: &mut Context<Self>) {
        let execution = cx.try_global::<Commands>().and_then(|c| c.execution(id));
        let (action, scope) = match execution {
            Some(Execution::Dispatch { action, scope }) => (action, scope),
            Some(Execution::Extension) => return self.ext_run_command(id, args, cx),
            // An extension registered it at run time, without contributing it.
            None if self.ext_provides(id) => return self.ext_run_command(id, args, cx),
            None => {
                self.error = Some(format!("command '{id}' not found"));
                cx.notify();
                return;
            }
        };
        if scope == Scope::Editor {
            if self.editors.active.is_none() || !self.editor_surface_active() {
                return;
            }
            self.editor_view.focus(window, cx);
        }
        window.dispatch_action(action, cx);
    }

    // ---- workbench commands ------------------------------------------------------------------

    /// Cmd+B, `workbench.action.toggleSidebarVisibility`.
    pub(crate) fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.sidebar_visible = !self.sidebar_visible;
        cx.notify();
    }

    /// `workbench.view.explorer`: show the Explorer in the side bar.
    pub(crate) fn show_explorer(&mut self, cx: &mut Context<Self>) {
        self.view = crate::ActivityView::Explorer;
        self.sidebar_visible = true;
        cx.notify();
    }

    /// `workbench.action.clearCommandHistory`.
    pub(crate) fn clear_command_history(&mut self, cx: &mut Context<Self>) {
        cx.update_global::<Commands, _>(|c, _| c.history.clear());
    }
}
