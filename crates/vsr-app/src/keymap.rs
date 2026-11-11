//! Editor commands and the default macOS key bindings, as plain data.
//!
//! upstream: src/vs/editor/browser/coreCommands.ts @ 50e1350fc78d
//!
//! The table below is the single source of truth: `editor_view::init` turns each row into a gpui
//! `KeyBinding`, and `resolve` looks a keystroke up in the same table (used by unit tests, so
//! the bindings are verifiable without a window). Only the subset the `vsr-editor` core
//! supports is bound. Not yet: word delete (Alt+Backspace), line delete (Cmd+Backspace),
//! outdent (Shift+Tab), line copy/cut when nothing is selected, multi-cursor.

use vsr_editor::comments::CommentType;
use vsr_editor::{Editor, FoldAction};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Motion {
    Left,
    Right,
    Up,
    Down,
    /// Start of the previous word (`cursorWordLeft`).
    WordLeft,
    /// End of the next word (`cursorWordRight`, macOS Option+Right).
    WordRight,
    /// First non-whitespace character, then column 1 (upstream `cursorHome`).
    Home,
    End,
    DocStart,
    DocEnd,
    PageUp,
    PageDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Move the caret; `true` extends the selection (Shift held).
    Move(Motion, bool),
    SelectAll,
    Backspace,
    Delete,
    Enter,
    Tab,
    Undo,
    Redo,
    Copy,
    Cut,
    Paste,
    Save,
    CloseTab,
    /// Alt+Z: wrap long lines at the viewport, or stop wrapping (`editor.action.toggleWordWrap`).
    ToggleWordWrap,
    /// The folding commands (`editor.fold`, `editor.foldAll`, ...; ADR-0042).
    Fold(FoldAction),
    /// Cmd+/ (`editor.action.commentLine`), Cmd+K Cmd+C (`addCommentLine`) and Cmd+K Cmd+U
    /// (`removeCommentLine`): ADR-0046.
    LineComment(CommentType),
    /// Shift+Alt+A (`editor.action.blockComment`).
    BlockComment,
    /// Ctrl+Space (`editor.action.triggerSuggest`): ask for completions.
    TriggerSuggest,
    /// Escape while the suggest widget shows (`hideSuggestWidget`).
    HideSuggest,
    /// F12 (`editor.action.revealDefinition`), Cmd+F12 (`goToImplementation`) and the other go-to
    /// commands (ADR-0108).
    Navigate(crate::navigation::Nav),
}

use Command::*;
use Motion::*;

/// Keystroke (gpui syntax, macOS: `cmd` is the primary modifier) to command.
pub const BINDINGS: &[(&str, Command)] = &[
    ("left", Move(Left, false)),
    ("shift-left", Move(Left, true)),
    ("right", Move(Right, false)),
    ("shift-right", Move(Right, true)),
    ("up", Move(Up, false)),
    ("shift-up", Move(Up, true)),
    ("down", Move(Down, false)),
    ("shift-down", Move(Down, true)),
    ("alt-left", Move(WordLeft, false)),
    ("shift-alt-left", Move(WordLeft, true)),
    ("alt-right", Move(WordRight, false)),
    ("shift-alt-right", Move(WordRight, true)),
    ("cmd-left", Move(Home, false)),
    ("shift-cmd-left", Move(Home, true)),
    ("home", Move(Home, false)),
    ("shift-home", Move(Home, true)),
    ("cmd-right", Move(End, false)),
    ("shift-cmd-right", Move(End, true)),
    ("end", Move(End, false)),
    ("shift-end", Move(End, true)),
    ("cmd-up", Move(DocStart, false)),
    ("shift-cmd-up", Move(DocStart, true)),
    ("cmd-down", Move(DocEnd, false)),
    ("shift-cmd-down", Move(DocEnd, true)),
    ("pageup", Move(PageUp, false)),
    ("shift-pageup", Move(PageUp, true)),
    ("pagedown", Move(PageDown, false)),
    ("shift-pagedown", Move(PageDown, true)),
    ("backspace", Backspace),
    ("shift-backspace", Backspace),
    ("delete", Delete),
    ("enter", Enter),
    ("shift-enter", Enter),
    ("tab", Tab),
    ("cmd-a", SelectAll),
    ("cmd-c", Copy),
    ("cmd-x", Cut),
    ("cmd-v", Paste),
    ("cmd-z", Undo),
    ("shift-cmd-z", Redo),
    ("cmd-s", Save),
    ("cmd-w", CloseTab),
    ("alt-z", ToggleWordWrap),
    // Comments (comment.ts): macOS keys.
    ("cmd-/", LineComment(CommentType::Toggle)),
    ("cmd-k cmd-c", LineComment(CommentType::ForceAdd)),
    ("cmd-k cmd-u", LineComment(CommentType::ForceRemove)),
    ("shift-alt-a", BlockComment),
    ("ctrl-space", TriggerSuggest),
    // Go to Definition (F12), Go to Implementations (Cmd+F12): upstream's macOS keys (ADR-0108).
    ("f12", Navigate(crate::navigation::Nav::Definition)),
    ("cmd-f12", Navigate(crate::navigation::Nav::Implementation)),
    // Folding (folding.ts): macOS keys.
    ("alt-cmd-[", Fold(FoldAction::Fold)),
    ("alt-cmd-]", Fold(FoldAction::Unfold)),
    ("cmd-k cmd-l", Fold(FoldAction::ToggleFold)),
    ("cmd-k cmd-[", Fold(FoldAction::FoldRecursively)),
    ("cmd-k cmd-]", Fold(FoldAction::UnfoldRecursively)),
    ("cmd-k cmd-shift-l", Fold(FoldAction::ToggleFoldRecursively)),
    ("cmd-k cmd-0", Fold(FoldAction::FoldAll)),
    ("cmd-k cmd-j", Fold(FoldAction::UnfoldAll)),
    ("cmd-k cmd-1", Fold(FoldAction::FoldLevel(1))),
    ("cmd-k cmd-2", Fold(FoldAction::FoldLevel(2))),
    ("cmd-k cmd-3", Fold(FoldAction::FoldLevel(3))),
    ("cmd-k cmd-4", Fold(FoldAction::FoldLevel(4))),
    ("cmd-k cmd-5", Fold(FoldAction::FoldLevel(5))),
    ("cmd-k cmd-6", Fold(FoldAction::FoldLevel(6))),
    ("cmd-k cmd-7", Fold(FoldAction::FoldLevel(7))),
    ("cmd-k cmd-8", Fold(FoldAction::FoldAllMarkerRegions)),
    ("cmd-k cmd-9", Fold(FoldAction::UnfoldAllMarkerRegions)),
    ("cmd-k cmd--", Fold(FoldAction::FoldAllExcept)),
    ("cmd-k cmd-=", Fold(FoldAction::UnfoldAllExcept)),
];

/// Commands without a default key binding (they are in the Command Palette).
pub const UNBOUND: &[Command] = &[
    Fold(FoldAction::GotoParentFold),
    Fold(FoldAction::GotoPreviousFold),
    Fold(FoldAction::GotoNextFold),
];

#[cfg(test)]
/// Modifier order used to compare keystrokes regardless of how they are spelled.
fn normalize(keystroke: &str) -> String {
    let mut parts: Vec<&str> = keystroke.split('-').collect();
    let key = parts.pop().unwrap_or("");
    parts.sort_by_key(|m| ["ctrl", "alt", "shift", "cmd"].iter().position(|x| x == m));
    parts.push(key);
    parts.join("-")
}

#[cfg(test)]
/// The command bound to `keystroke` (e.g. `"alt-shift-left"`), if any.
pub fn resolve(keystroke: &str) -> Option<Command> {
    let want = normalize(keystroke);
    BINDINGS.iter().find(|(k, _)| normalize(k) == want).map(|&(_, c)| c)
}

/// Apply the commands that only touch the editing core. Returns false for commands that need
/// the view (clipboard, save, close). `page` is the number of visible lines (for PageUp/Down).
pub fn apply(editor: &mut Editor, cmd: Command, page: usize) -> bool {
    match cmd {
        Move(m, extend) => match m {
            Left => editor.move_left(extend),
            Right => editor.move_right(extend),
            Up => editor.move_up(extend),
            Down => editor.move_down(extend),
            WordLeft => editor.move_word_left(extend),
            WordRight => editor.move_word_end_right(extend),
            Home => editor.move_home(extend),
            End => editor.move_end(extend),
            DocStart => editor.move_doc_start(extend),
            DocEnd => editor.move_doc_end(extend),
            PageUp => editor.page_up(page, extend),
            PageDown => editor.page_down(page, extend),
        },
        SelectAll => editor.select_all(),
        Backspace => editor.backspace(),
        Delete => editor.delete(),
        Enter => editor.enter(),
        Tab => editor.tab(),
        Undo => {
            editor.undo();
        }
        Redo => {
            editor.redo();
        }
        Fold(action) => {
            editor.fold_action(action);
        }
        Copy | Cut | Paste | Save | CloseTab | ToggleWordWrap | LineComment(_) | BlockComment | TriggerSuggest | HideSuggest | Navigate(_) => {
            return false;
        }
    }
    true
}

/// Whether `cmd` changes the document (as opposed to moving the caret or leaving the editor).
pub fn is_edit(cmd: Command) -> bool {
    matches!(cmd, Backspace | Delete | Enter | Tab | Undo | Redo | Cut | Paste | LineComment(_) | BlockComment)
}

#[cfg(test)]
mod tests {
    use super::*;
    use vsr_editor::{EditorOptions, Position};
    use vsr_text::{DefaultEol, TextBuffer};

    #[test]
    fn macos_defaults_resolve() {
        assert_eq!(resolve("left"), Some(Move(Left, false)));
        assert_eq!(resolve("shift-left"), Some(Move(Left, true)));
        assert_eq!(resolve("alt-left"), Some(Move(WordLeft, false)));
        assert_eq!(resolve("cmd-left"), Some(Move(Home, false)));
        assert_eq!(resolve("cmd-down"), Some(Move(DocEnd, false)));
        assert_eq!(resolve("cmd-z"), Some(Undo));
        assert_eq!(resolve("cmd-shift-z"), Some(Redo));
        assert_eq!(resolve("delete"), Some(Delete));
        assert_eq!(resolve("cmd-s"), Some(Save));
        assert_eq!(resolve("cmd-w"), Some(CloseTab));
        assert_eq!(resolve("alt-z"), Some(ToggleWordWrap));
        assert_eq!(resolve("ctrl-q"), None);
    }

    #[test]
    fn modifier_spelling_order_does_not_matter() {
        assert_eq!(resolve("alt-shift-right"), Some(Move(WordRight, true)));
        assert_eq!(resolve("shift-alt-right"), Some(Move(WordRight, true)));
        assert_eq!(resolve("cmd-shift-up"), resolve("shift-cmd-up"));
    }

    #[test]
    fn every_binding_is_unique() {
        for (i, (k, _)) in BINDINGS.iter().enumerate() {
            assert!(BINDINGS[..i].iter().all(|(o, _)| normalize(o) != normalize(k)), "{k}");
        }
    }

    #[test]
    fn apply_drives_the_editing_core() {
        let buf = TextBuffer::new("ab\ncd", DefaultEol::Lf, true);
        let mut ed = Editor::new(buf, EditorOptions { tab_size: 4, insert_spaces: true });
        assert!(apply(&mut ed, Move(DocEnd, false), 10));
        assert_eq!(ed.selection().active, Position::new(2, 3));
        assert!(apply(&mut ed, Move(Left, true), 10));
        assert_eq!(ed.selection().anchor, Position::new(2, 3));
        assert!(apply(&mut ed, Backspace, 10));
        assert_eq!(ed.buffer().value(), "ab\nc");
        assert!(apply(&mut ed, Undo, 10));
        assert_eq!(ed.buffer().value(), "ab\ncd");
        assert!(!apply(&mut ed, Save, 10));
        assert!(is_edit(Paste) && !is_edit(Save));
    }
}
