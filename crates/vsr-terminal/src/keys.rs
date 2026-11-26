//! Keyboard input as the bytes a terminal program expects (xterm encoding).
//!
//! Upstream source: src/vs/workbench/contrib/terminal/browser/terminalInstance.ts (key handling is
//! xterm.js's `evaluateKeyboardEvent`); the sequences are xterm's, as documented in "XTerm Control Sequences".

/// A key that is not just text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Tab,
    Backspace,
    Escape,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
    Delete,
    Insert,
    /// F1 to F12.
    F(u8),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    pub alt: bool,
    pub ctrl: bool,
}

impl Modifiers {
    /// The xterm modifier parameter (1 + shift 1 + alt 2 + ctrl 4); 1 means none.
    fn param(self) -> u8 {
        1 + self.shift as u8 + 2 * self.alt as u8 + 4 * self.ctrl as u8
    }
    fn any(self) -> bool {
        self.shift || self.alt || self.ctrl
    }
}

/// The bytes for `key`, or `None` when the key sends nothing (an unknown function key).
pub fn encode_key(key: &Key, mods: Modifiers, app_cursor: bool) -> Option<Vec<u8>> {
    let csi_letter = |letter: char| -> Vec<u8> {
        if mods.any() {
            format!("\x1b[1;{}{letter}", mods.param()).into_bytes()
        } else if app_cursor {
            format!("\x1bO{letter}").into_bytes()
        } else {
            format!("\x1b[{letter}").into_bytes()
        }
    };
    let csi_tilde = |n: u8| -> Vec<u8> {
        if mods.any() { format!("\x1b[{n};{}~", mods.param()).into_bytes() } else { format!("\x1b[{n}~").into_bytes() }
    };
    Some(match key {
        Key::Char(c) => return encode_char(*c, mods),
        Key::Enter => with_alt(b"\r".to_vec(), mods),
        Key::Tab if mods.shift => b"\x1b[Z".to_vec(),
        Key::Tab => with_alt(b"\t".to_vec(), mods),
        Key::Backspace => with_alt(if mods.ctrl { vec![0x08] } else { vec![0x7f] }, mods),
        Key::Escape => with_alt(vec![0x1b], mods),
        Key::Up => csi_letter('A'),
        Key::Down => csi_letter('B'),
        Key::Right => csi_letter('C'),
        Key::Left => csi_letter('D'),
        Key::Home => csi_letter('H'),
        Key::End => csi_letter('F'),
        Key::Insert => csi_tilde(2),
        Key::Delete => csi_tilde(3),
        Key::PageUp => csi_tilde(5),
        Key::PageDown => csi_tilde(6),
        Key::F(n @ 1..=4) => {
            let letter = ['P', 'Q', 'R', 'S'][*n as usize - 1];
            if mods.any() { format!("\x1b[1;{}{letter}", mods.param()).into_bytes() } else { format!("\x1bO{letter}").into_bytes() }
        }
        Key::F(n @ 5..=12) => csi_tilde([15, 17, 18, 19, 20, 21, 23, 24][*n as usize - 5]),
        Key::F(_) => return None,
    })
}

fn with_alt(bytes: Vec<u8>, mods: Modifiers) -> Vec<u8> {
    if mods.alt {
        let mut v = vec![0x1b];
        v.extend(bytes);
        v
    } else {
        bytes
    }
}

fn encode_char(c: char, mods: Modifiers) -> Option<Vec<u8>> {
    let mut buf = [0u8; 4];
    let bytes = if mods.ctrl {
        let b = match c.to_ascii_lowercase() {
            c @ 'a'..='z' => c as u8 - b'a' + 1,
            ' ' | '@' | '2' => 0,
            '[' | '3' => 0x1b,
            '\\' | '4' => 0x1c,
            ']' | '5' => 0x1d,
            '^' | '6' => 0x1e,
            '_' | '7' | '-' => 0x1f,
            '8' | '?' => 0x7f,
            _ => return None,
        };
        vec![b]
    } else {
        c.encode_utf8(&mut buf).as_bytes().to_vec()
    };
    Some(with_alt(bytes, mods))
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONE: Modifiers = Modifiers { shift: false, alt: false, ctrl: false };
    const CTRL: Modifiers = Modifiers { shift: false, alt: false, ctrl: true };
    const ALT: Modifiers = Modifiers { shift: false, alt: true, ctrl: false };

    fn enc(k: Key, m: Modifiers, app: bool) -> String {
        String::from_utf8(encode_key(&k, m, app).unwrap()).unwrap()
    }

    #[test]
    fn text_and_controls() {
        assert_eq!(enc(Key::Char('a'), NONE, false), "a");
        assert_eq!(enc(Key::Char('é'), NONE, false), "é");
        assert_eq!(enc(Key::Char('c'), CTRL, false), "\x03");
        assert_eq!(enc(Key::Char('d'), CTRL, false), "\x04");
        assert_eq!(enc(Key::Char('['), CTRL, false), "\x1b");
        assert_eq!(enc(Key::Char('f'), ALT, false), "\x1bf");
        assert_eq!(encode_key(&Key::Char('1'), CTRL, false), None);
        assert_eq!(enc(Key::Enter, NONE, false), "\r");
        assert_eq!(enc(Key::Tab, NONE, false), "\t");
        assert_eq!(enc(Key::Tab, Modifiers { shift: true, ..NONE }, false), "\x1b[Z");
        assert_eq!(enc(Key::Backspace, NONE, false), "\x7f");
        assert_eq!(enc(Key::Escape, NONE, false), "\x1b");
    }

    #[test]
    fn arrows_follow_the_application_cursor_mode() {
        assert_eq!(enc(Key::Up, NONE, false), "\x1b[A");
        assert_eq!(enc(Key::Up, NONE, true), "\x1bOA");
        assert_eq!(enc(Key::Left, CTRL, false), "\x1b[1;5D");
        assert_eq!(enc(Key::Right, ALT, true), "\x1b[1;3C");
        assert_eq!(enc(Key::Home, NONE, false), "\x1b[H");
        assert_eq!(enc(Key::End, NONE, true), "\x1bOF");
    }

    #[test]
    fn editing_and_function_keys() {
        assert_eq!(enc(Key::Delete, NONE, false), "\x1b[3~");
        assert_eq!(enc(Key::PageUp, NONE, false), "\x1b[5~");
        assert_eq!(enc(Key::PageDown, CTRL, false), "\x1b[6;5~");
        assert_eq!(enc(Key::F(1), NONE, false), "\x1bOP");
        assert_eq!(enc(Key::F(5), NONE, false), "\x1b[15~");
        assert_eq!(enc(Key::F(12), NONE, false), "\x1b[24~");
        assert_eq!(encode_key(&Key::F(13), NONE, false), None);
    }
}
