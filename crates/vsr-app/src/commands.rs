//! The workbench's command registry (ADR-0030): every action rscode has, registered under its
//! VS Code command id with a title, a category and a precondition, so the Command Palette, menus,
//! key bindings and (later) the extension host's `commands.executeCommand` share one table.
//!
//! upstream: src/vs/platform/commands/common/commands.ts @ 50e1350fc78d
//! upstream: src/vs/platform/actions/common/actions.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/quickaccess/browser/commandsQuickAccess.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/actions/layoutActions.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/browser/actions/quickAccessActions.ts @ 50e1350fc78d
//!
//! A command here is a gpui action plus metadata. Running one dispatches the action from where
//! focus is (editor commands first focus the editor), so the handlers stay the `on_action`
//! listeners that already exist and a key press and a palette pick take the same path. The key
//! binding shown next to a command is looked up in gpui's keymap, never stored, so it cannot drift
//! from what actually fires.
//!
//! The registry lives in a gpui `Global` ([`Commands`]) with the Command Palette history and the
//! Quick Access providers. The extension host will register `Handler::Extension` entries through
//! `MainThreadCommands` (ADR-0020, ADR-0030).

use gpui_kit::*;
use vsr_commands::keybinding::{Chord, ui_label};
use vsr_commands::{CommandDescriptor, CommandsHistory, CommandsRegistry, ContextKeys};
use vsr_quickinput::quick_access::{ProviderDescriptor, QuickAccessRegistry};

use crate::editor_view::Run;
use crate::editor_view::find as fnd;
use crate::keymap::{BINDINGS, Command, Motion};
use crate::quick_open::GoToFile;
use crate::search_view as sv;
use crate::settings_view::OpenSettings;
use crate::Quit;

actions!(
    rscode,
    [ShowCommands, ToggleSidebar, ShowExplorer, GoToLine, ClearCommandHistory]
);

/// Where a command's action must be dispatched from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// From wherever focus is (the workbench root handles it).
    Workbench,
    /// From the editor: focus moves to the editor first.
    Editor,
}

/// What runs when a command is executed.
pub enum Handler {
    /// Dispatch a gpui action.
    Action { action: Box<dyn Action>, scope: Scope },
}

/// The command service state: registry, Command Palette history, Quick Access providers.
pub struct Commands {
    pub registry: CommandsRegistry<Handler>,
    pub history: CommandsHistory,
    pub quick_access: QuickAccessRegistry,
}

impl Global for Commands {}

/// What the workbench must do to run a command.
pub enum Execution {
    Dispatch { action: Box<dyn Action>, scope: Scope },
}

impl Commands {
    fn new() -> Self {
        let mut registry = CommandsRegistry::new();
        register_builtin(&mut registry);
        let mut quick_access = QuickAccessRegistry::new();
        register_quick_access(&mut quick_access);
        Self { registry, history: CommandsHistory::default(), quick_access }
    }

    /// How to run `id`, `None` for an unknown command.
    pub fn execution(&self, id: &str) -> Option<Execution> {
        match self.registry.handler(id)? {
            Handler::Action { action, scope } => Some(Execution::Dispatch { action: action.boxed_clone(), scope: *scope }),
        }
    }
}

/// Install the registry (idempotent). Tests and embedders that do not bind keys call this alone.
pub fn ensure_installed(cx: &mut App) {
    if !cx.has_global::<Commands>() {
        cx.set_global(Commands::new());
    }
}

/// Install the registry and bind the workbench-level keys (call once at startup).
pub fn init(cx: &mut App) {
    ensure_installed(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-shift-p", ShowCommands, None),
        KeyBinding::new("f1", ShowCommands, None),
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-shift-e", ShowExplorer, None),
        KeyBinding::new("ctrl-g", GoToLine, None),
    ]);
}

/// The key binding of `action` as the UI shows it: the first binding in the keymap (the
/// primary one), one entry per chord, one string per key box. Empty when it has none.
pub fn keybinding_of(action: &dyn Action, cx: &App) -> Vec<Vec<String>> {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let first = keymap.bindings_for_action(action).next();
    first
        .map(|b| {
            b.keystrokes()
                .iter()
                .map(|k| {
                    let k = k.inner();
                    ui_label(&Chord {
                        ctrl: k.modifiers.control,
                        shift: k.modifiers.shift,
                        alt: k.modifiers.alt,
                        cmd: k.modifiers.platform,
                        key: k.key.to_string(),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The key binding of a registered command.
pub fn keybinding_of_command(id: &str, cx: &App) -> Vec<Vec<String>> {
    let Some(commands) = cx.try_global::<Commands>() else { return Vec::new() };
    match commands.registry.handler(id) {
        Some(Handler::Action { action, .. }) => keybinding_of(&**action, cx),
        None => Vec::new(),
    }
}

// ---- the built-in commands ---------------------------------------------------------------------

/// Context key set when a text editor is the active editor (the Settings editor is not one).
pub const ACTIVE_EDITOR: &str = "activeEditor";

fn action<A: Action>(a: A, scope: Scope) -> Handler {
    Handler::Action { action: Box::new(a), scope }
}

fn register_builtin(r: &mut CommandsRegistry<Handler>) {
    use CommandDescriptor as D;
    use self::Scope::{Editor, Workbench};

    // Workbench.
    r.register(D::new("workbench.action.showCommands", "Show All Commands").f1(), action(ShowCommands, Workbench));
    r.register(D::new("workbench.action.quickOpen", "Go to File...").f1(), action(GoToFile, Workbench));
    r.register(D::new("workbench.action.gotoLine", "Go to Line/Column...").f1(), action(GoToLine, Workbench));
    r.register(
        D::new("workbench.action.toggleSidebarVisibility", "Toggle Primary Side Bar Visibility")
            .category("View")
            .description("Open/Show and Close/Hide Sidebar")
            .f1(),
        action(ToggleSidebar, Workbench),
    );
    r.register(D::new("workbench.view.explorer", "Show Explorer").category("View").f1(), action(ShowExplorer, Workbench));
    r.register(D::new("workbench.view.search", "Show Search").category("View").f1(), action(sv::FocusSearch, Workbench));
    r.register(D::new("workbench.action.openSettings", "Open Settings (UI)").category("Preferences").f1(), action(OpenSettings, Workbench));
    r.register(D::new("workbench.action.clearCommandHistory", "Clear Command History").f1(), action(ClearCommandHistory, Workbench));
    r.register(D::new("workbench.action.quit", "Quit"), action(Quit, Workbench));

    // Files and editors.
    r.register(
        D::new("workbench.action.files.save", "Save").category("File").f1().precondition(ACTIVE_EDITOR),
        action(Run(Command::Save), Editor),
    );
    r.register(
        D::new("workbench.action.closeActiveEditor", "Close Editor").category("View").f1().precondition(ACTIVE_EDITOR),
        action(Run(Command::CloseTab), Editor),
    );

    // Editor commands from the key map (the one table of default bindings).
    let mut seen: Vec<Command> = Vec::new();
    for &(_, cmd) in BINDINGS {
        if seen.contains(&cmd) || matches!(cmd, Command::Save | Command::CloseTab) {
            continue;
        }
        seen.push(cmd);
        let (id, title, f1) = editor_command(cmd);
        let mut d = D::new(id, title).precondition(ACTIVE_EDITOR);
        if cmd == Command::ToggleWordWrap {
            d = d.category("View");
        }
        d.f1 = f1;
        r.register(d, action(Run(cmd), Editor));
    }

    // Find widget (ADR-0027).
    for (id, title, f1, a) in [
        ("actions.find", "Find", true, Box::new(fnd::Find) as Box<dyn Action>),
        ("editor.action.startFindReplaceAction", "Replace", true, Box::new(fnd::FindReplace)),
        ("editor.action.nextMatchFindAction", "Find Next", true, Box::new(fnd::FindNext)),
        ("editor.action.previousMatchFindAction", "Find Previous", true, Box::new(fnd::FindPrevious)),
        ("actions.findWithSelection", "Find with Selection", true, Box::new(fnd::UseSelectionForFind)),
        ("closeFindWidget", "Close Find Widget", false, Box::new(fnd::CloseFind)),
        ("toggleFindCaseSensitive", "Toggle Find Case Sensitive", false, Box::new(fnd::ToggleMatchCase)),
        ("toggleFindWholeWord", "Toggle Find Whole Word", false, Box::new(fnd::ToggleWholeWord)),
        ("toggleFindRegex", "Toggle Find Regex", false, Box::new(fnd::ToggleRegex)),
        ("toggleFindInSelection", "Toggle Find in Selection", false, Box::new(fnd::ToggleFindInSelection)),
        ("toggleFindPreserveCase", "Toggle Find Preserve Case", false, Box::new(fnd::TogglePreserveCase)),
        ("editor.action.replaceOne", "Replace One", false, Box::new(fnd::ReplaceOne)),
        ("editor.action.replaceAll", "Replace All", false, Box::new(fnd::ReplaceAll)),
    ] {
        let mut d = D::new(id, title).precondition(ACTIVE_EDITOR);
        d.f1 = f1;
        r.register(d, Handler::Action { action: a, scope: Editor });
    }

    // Search view (focus-bound: they act only while the Search view has focus, so no palette entry).
    for (id, title, a) in [
        ("toggleSearchCaseSensitive", "Toggle Search Case Sensitive", Box::new(sv::ToggleCaseSensitive) as Box<dyn Action>),
        ("toggleSearchWholeWord", "Toggle Search Whole Word", Box::new(sv::ToggleWholeWord)),
        ("toggleSearchRegex", "Toggle Search Regex", Box::new(sv::ToggleRegex)),
        ("search.action.toggleQueryDetails", "Toggle Query Details", Box::new(sv::ToggleSearchDetails)),
        ("search.action.cancel", "Cancel Search", Box::new(sv::CancelSearch)),
    ] {
        r.register(D::new(id, title), Handler::Action { action: a, scope: Workbench });
    }
}

/// Id, title and palette visibility of an editor command (`Run`): the ids of `coreCommands.ts`
/// and the editor actions. Only the clipboard, undo/redo and select-all commands are in the
/// palette, like upstream (cursor movement and typing are not `f1`).
fn editor_command(cmd: Command) -> (&'static str, &'static str, bool) {
    use Command::*;
    use Motion::*;
    match cmd {
        Move(Left, false) => ("cursorLeft", "Move Cursor Left", false),
        Move(Left, true) => ("cursorLeftSelect", "Select Left", false),
        Move(Right, false) => ("cursorRight", "Move Cursor Right", false),
        Move(Right, true) => ("cursorRightSelect", "Select Right", false),
        Move(Up, false) => ("cursorUp", "Move Cursor Up", false),
        Move(Up, true) => ("cursorUpSelect", "Select Up", false),
        Move(Down, false) => ("cursorDown", "Move Cursor Down", false),
        Move(Down, true) => ("cursorDownSelect", "Select Down", false),
        Move(WordLeft, false) => ("cursorWordLeft", "Move Cursor Word Left", false),
        Move(WordLeft, true) => ("cursorWordLeftSelect", "Select Word Left", false),
        Move(WordRight, false) => ("cursorWordEndRight", "Move Cursor Word End Right", false),
        Move(WordRight, true) => ("cursorWordEndRightSelect", "Select Word End Right", false),
        Move(Home, false) => ("cursorHome", "Move Cursor to Line Start", false),
        Move(Home, true) => ("cursorHomeSelect", "Select to Line Start", false),
        Move(End, false) => ("cursorEnd", "Move Cursor to Line End", false),
        Move(End, true) => ("cursorEndSelect", "Select to Line End", false),
        Move(DocStart, false) => ("cursorTop", "Move Cursor to Document Start", false),
        Move(DocStart, true) => ("cursorTopSelect", "Select to Document Start", false),
        Move(DocEnd, false) => ("cursorBottom", "Move Cursor to Document End", false),
        Move(DocEnd, true) => ("cursorBottomSelect", "Select to Document End", false),
        Move(PageUp, false) => ("cursorPageUp", "Move Cursor Page Up", false),
        Move(PageUp, true) => ("cursorPageUpSelect", "Select Page Up", false),
        Move(PageDown, false) => ("cursorPageDown", "Move Cursor Page Down", false),
        Move(PageDown, true) => ("cursorPageDownSelect", "Select Page Down", false),
        SelectAll => ("editor.action.selectAll", "Select All", true),
        Backspace => ("deleteLeft", "Delete Left", false),
        Delete => ("deleteRight", "Delete Right", false),
        // Upstream types a "\n" with the `type` command; there is no id of its own.
        Enter => ("editor.action.insertLineBreak", "Insert Line Break", false),
        Tab => ("tab", "Insert Tab", false),
        Undo => ("undo", "Undo", true),
        Redo => ("redo", "Redo", true),
        Copy => ("editor.action.clipboardCopyAction", "Copy", true),
        Cut => ("editor.action.clipboardCutAction", "Cut", true),
        Paste => ("editor.action.clipboardPasteAction", "Paste", true),
        Save => ("workbench.action.files.save", "Save", true),
        CloseTab => ("workbench.action.closeActiveEditor", "Close Editor", true),
        ToggleWordWrap => ("editor.action.toggleWordWrap", "Toggle Word Wrap", true),
    }
}

/// The Quick Access providers (`quickAccess.contribution.ts` and friends).
fn register_quick_access(q: &mut QuickAccessRegistry) {
    q.register(
        ProviderDescriptor::new("", "Search files by name (append : to go to line)").help("Go to File", Some("workbench.action.quickOpen")),
    );
    q.register(
        ProviderDescriptor::new(vsr_quickinput::commands::PREFIX, "Type the name of a command to run.")
            .help("Show and Run Commands", Some("workbench.action.showCommands")),
    );
    q.register(
        ProviderDescriptor::new(":", "Type the line number and optional column to go to (e.g. :42:5 for line 42, column 5). Type :: to go to a character offset (e.g. ::1024 for character 1024 from the start of the file). Use negative values to navigate backwards.")
            .help("Go to Line/Column", Some("workbench.action.gotoLine")),
    );
    q.register(
        ProviderDescriptor::new(vsr_quickinput::quick_access::HELP_PREFIX, "Type '?' to get help on the actions you can take from here.")
            .help("Show all Quick Access Providers", None),
    );
}

/// Context keys in effect for the palette's preconditions.
pub fn context_keys(active_editor: bool, side_bar_visible: bool) -> ContextKeys {
    let mut c = ContextKeys::default();
    c.set_bool(ACTIVE_EDITOR, active_editor).set_bool("sideBarVisible", side_bar_visible);
    c
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: that glob brings gpui_kit's `test` attribute in and shadows `#[test]`.
    use super::{Handler, context_keys, editor_command, register_builtin};
    use crate::keymap::BINDINGS;
    use vsr_commands::{CommandsRegistry, ContextKeys};

    fn registry() -> CommandsRegistry<Handler> {
        let mut r = CommandsRegistry::new();
        register_builtin(&mut r);
        r
    }

    #[test]
    fn every_key_map_command_is_registered_once() {
        let r = registry();
        for &(keys, cmd) in BINDINGS {
            let (id, ..) = editor_command(cmd);
            assert!(r.contains(id), "{keys}: {id} is not registered");
        }
    }

    #[test]
    fn ids_are_unique_titles_non_empty() {
        let r = registry();
        let mut ids: Vec<&str> = r.commands().map(|(d, _)| d.id.as_str()).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate ids");
        assert!(r.commands().all(|(d, _)| !d.title.is_empty()));
    }

    #[test]
    fn the_vs_code_ids_the_task_names_are_there() {
        let r = registry();
        for id in [
            "workbench.action.showCommands",
            "workbench.action.quickOpen",
            "workbench.action.files.save",
            "workbench.action.closeActiveEditor",
            "workbench.action.toggleSidebarVisibility",
            "workbench.action.openSettings",
            "workbench.view.search",
            "workbench.view.explorer",
            "actions.find",
            "editor.action.startFindReplaceAction",
            "workbench.action.gotoLine",
            "workbench.action.quit",
            "editor.action.selectAll",
            "undo",
            "redo",
            "editor.action.toggleWordWrap",
        ] {
            assert!(r.contains(id), "{id}");
        }
    }

    #[test]
    fn palette_labels() {
        let r = registry();
        let label = |id: &str| r.descriptor(id).unwrap().label();
        assert_eq!(label("workbench.action.toggleSidebarVisibility"), "View: Toggle Primary Side Bar Visibility");
        assert_eq!(label("workbench.action.files.save"), "File: Save");
        assert_eq!(label("workbench.action.openSettings"), "Preferences: Open Settings (UI)");
        assert_eq!(label("workbench.action.quickOpen"), "Go to File...");
        assert_eq!(label("workbench.action.gotoLine"), "Go to Line/Column...");
        assert_eq!(label("actions.find"), "Find");
    }

    #[test]
    fn palette_hides_editor_commands_without_an_editor() {
        let r = registry();
        let ids = |ctx: &ContextKeys| r.palette_commands(ctx).iter().map(|d| d.id.clone()).collect::<Vec<_>>();
        let without = ids(&context_keys(false, true));
        assert!(without.iter().any(|i| i == "workbench.action.toggleSidebarVisibility"));
        assert!(!without.iter().any(|i| i == "actions.find" || i == "workbench.action.files.save"));
        let with = ids(&context_keys(true, true));
        assert!(with.iter().any(|i| i == "actions.find") && with.iter().any(|i| i == "workbench.action.files.save"));
        assert!(!with.iter().any(|i| i == "cursorLeft"), "cursor movement is not in the palette");
    }
}
