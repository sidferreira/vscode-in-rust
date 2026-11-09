//! Key binding labels as the UI shows them (macOS).
//!
//! upstream: src/vs/base/common/keybindingLabels.ts @ 50e1350fc78d (`UILabelProvider`)
//! upstream: src/vs/platform/keybinding/common/usLayoutResolvedKeybinding.ts @ 50e1350fc78d
//!
//! A chord becomes one label per key: the modifiers as symbols in the order Control, Shift,
//! Option, Command (`⌃⇧⌥⌘`), then the key: arrows as `←↑→↓`, letters upper case, named keys as
//! upstream spells them (`Enter`, `Escape`, `PageUp`, `F1`...). A binding of several chords is
//! one such list per chord (`⌘K ⌘S`). Not ported: Windows/Linux labels (`Ctrl+Shift+P`), other
//! keyboard layouts.

/// One key press with its modifiers; `key` is gpui's spelling (`"p"`, `"left"`, `"f1"`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Chord {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub cmd: bool,
    pub key: String,
}

/// The boxes of a chord: `["⇧", "⌘", "P"]` for `shift-cmd-p`.
pub fn ui_label(chord: &Chord) -> Vec<String> {
    let mut out = Vec::new();
    for (on, symbol) in [(chord.ctrl, "\u{2303}"), (chord.shift, "⇧"), (chord.alt, "⌥"), (chord.cmd, "⌘")] {
        if on {
            out.push(symbol.to_string());
        }
    }
    out.push(key_label(&chord.key));
    out
}

/// `KeyCodeUtils.toString` with the macOS arrow glyphs.
pub fn key_label(key: &str) -> String {
    match key {
        "left" => "←".into(),
        "up" => "↑".into(),
        "right" => "→".into(),
        "down" => "↓".into(),
        "enter" => "Enter".into(),
        "escape" => "Escape".into(),
        "backspace" => "Backspace".into(),
        "delete" => "Delete".into(),
        "tab" => "Tab".into(),
        "space" => "Space".into(),
        "pageup" => "PageUp".into(),
        "pagedown" => "PageDown".into(),
        "home" => "Home".into(),
        "end" => "End".into(),
        k if k.len() > 1 && k.starts_with('f') && k[1..].chars().all(|c| c.is_ascii_digit()) => k.to_uppercase(),
        k => k.to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chord(ctrl: bool, shift: bool, alt: bool, cmd: bool, key: &str) -> Chord {
        Chord { ctrl, shift, alt, cmd, key: key.into() }
    }

    #[test]
    fn modifiers_are_symbols_in_the_upstream_order() {
        assert_eq!(ui_label(&chord(false, true, false, true, "p")), ["⇧", "⌘", "P"]);
        assert_eq!(ui_label(&chord(true, false, false, false, "g")), ["⌃", "G"]);
        assert_eq!(ui_label(&chord(false, true, true, true, "f")), ["⇧", "⌥", "⌘", "F"]);
        assert_eq!(ui_label(&chord(true, true, true, true, "j")), ["⌃", "⇧", "⌥", "⌘", "J"]);
    }

    #[test]
    fn keys() {
        assert_eq!(ui_label(&chord(false, false, false, true, "left")), ["⌘", "←"]);
        assert_eq!(ui_label(&chord(false, false, false, false, "f1")), ["F1"]);
        assert_eq!(ui_label(&chord(false, false, false, false, "f12")), ["F12"]);
        assert_eq!(ui_label(&chord(false, false, false, false, "pagedown")), ["PageDown"]);
        assert_eq!(ui_label(&chord(false, false, false, true, ",")), ["⌘", ","]);
        assert_eq!(ui_label(&chord(false, false, false, true, "1")), ["⌘", "1"]);
        assert_eq!(ui_label(&chord(false, false, false, false, "escape")), ["Escape"]);
        assert_eq!(ui_label(&chord(false, false, false, true, "enter")), ["⌘", "Enter"]);
    }
}
