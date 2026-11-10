//! Explorer commands and key bindings (ADR-0041). The commands are registered in the registry
//! (`commands.rs`) under their VS Code ids; the key bindings only apply while the tree has focus
//! (key context `FilesExplorer`). The handlers live on the workbench root, so a Command Palette
//! pick works from any focus and a key press bubbles up to them from the tree.
//!
//! upstream: src/vs/workbench/contrib/files/browser/fileActions.contribution.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/files/browser/views/explorerView.ts @ 50e1350fc78d

use gpui_kit::*;

actions!(
    rscode_explorer,
    [
        NewFile,
        NewFolder,
        RenameFile,
        MoveFileToTrash,
        DeleteFile,
        RefreshExplorer,
        CollapseFolders,
        MoveSelectionUp,
        MoveSelectionDown,
        CollapseOrParent,
        ExpandOrFirstChild,
        OpenOrToggle,
        CancelDelete,
        CollapseAll,
        RefreshAll,
        CopyPath,
        CopyRelativePath,
    ]
);

/// The key context of the Explorer tree. Its bindings are the explorer keys (ADR-0041).
pub const KEY_CONTEXT: &str = "FilesExplorer";

/// Explorer key bindings. Call once at startup, after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("up", MoveSelectionUp, Some(KEY_CONTEXT)),
        KeyBinding::new("down", MoveSelectionDown, Some(KEY_CONTEXT)),
        KeyBinding::new("left", CollapseOrParent, Some(KEY_CONTEXT)),
        KeyBinding::new("right", ExpandOrFirstChild, Some(KEY_CONTEXT)),
        KeyBinding::new("space", OpenOrToggle, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", RenameFile, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-backspace", MoveFileToTrash, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", MoveFileToTrash, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-alt-backspace", DeleteFile, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", CancelDelete, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-cmd-c", CopyPath, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-alt-cmd-c", CopyRelativePath, Some(KEY_CONTEXT)),
    ]);
}
