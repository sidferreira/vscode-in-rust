//! The macOS find pasteboard (`NSPasteboardNameFind`), which other apps share with the Find widget:
//! `editor.find.globalFindClipboard` (upstream `clipboardService.readFindText` / `writeFindText`).
//!
//! Behind a trait so that the widget's logic is tested with an in-memory pasteboard. The tests never
//! touch the user's find pasteboard.

use objc2_app_kit::{NSPasteboard, NSPasteboardNameFind, NSPasteboardTypeString};
use objc2_foundation::NSString;
#[cfg(test)]
use std::cell::RefCell;
#[cfg(test)]
use std::rc::Rc;

pub(crate) trait FindPasteboard {
    /// The text on the find pasteboard, `None` when it holds none.
    fn read(&self) -> Option<String>;
    fn write(&self, text: &str);
}

/// The system find pasteboard.
pub(crate) struct SystemFindPasteboard;

impl FindPasteboard for SystemFindPasteboard {
    fn read(&self) -> Option<String> {
        let pasteboard = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameFind });
        let text = pasteboard.stringForType(unsafe { NSPasteboardTypeString })?;
        Some(text.to_string())
    }

    fn write(&self, text: &str) {
        let pasteboard = NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameFind });
        pasteboard.clearContents();
        pasteboard.setString_forType(&NSString::from_str(text), unsafe { NSPasteboardTypeString });
    }
}

/// An in-memory find pasteboard for the tests (the handle is shared with the test that reads it).
#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct MemoryFindPasteboard(pub Rc<RefCell<Option<String>>>);

#[cfg(test)]
impl FindPasteboard for MemoryFindPasteboard {
    fn read(&self) -> Option<String> {
        self.0.borrow().clone()
    }

    fn write(&self, text: &str) {
        *self.0.borrow_mut() = Some(text.to_string());
    }
}
