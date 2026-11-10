//! Codicons: the icon font VS Code uses for all workbench icons.
//!
//! upstream: src/vs/base/common/codiconsLibrary.ts @ 50e1350fc78d
//!
//! The font is `@vscode/codicons` 0.0.46-49 (the version upstream's package.json pins),
//! bundled at `assets/codicon.ttf`, licensed CC BY 4.0 (`assets/codicon-LICENSE.txt`).
//! The bundled copy is modified so gpui will load it ('m' mapped to the blank glyph):
//! see `assets/codicon-NOTICE.md` and ADR-0021.
//! Codepoints below are copied from upstream's registry.

use gpui_kit::{App, SharedString};
use std::borrow::Cow;

/// Font family name inside `codicon.ttf` (use with `.font_family(icons::FAMILY)`).
pub const FAMILY: &str = "codicon";

pub const FILES: char = '\u{EAF0}';
pub const SEARCH: char = '\u{EA6D}';
pub const SOURCE_CONTROL: char = '\u{EA68}';
pub const EXTENSIONS: char = '\u{EAE6}';
pub const ACCOUNT: char = '\u{EB99}';
pub const SETTINGS_GEAR: char = '\u{EB51}';
pub const CHEVRON_RIGHT: char = '\u{EAB6}';
pub const CHEVRON_DOWN: char = '\u{EAB4}';
pub const CLOSE: char = '\u{EA76}';
pub const LOCK: char = '\u{EA75}';
pub const GIT_BRANCH: char = '\u{EC6F}';
pub const ERROR: char = '\u{EA87}';
pub const WARNING: char = '\u{EA6C}';
pub const BELL: char = '\u{EAA2}';
pub const CHECK: char = '\u{EAB2}';
pub const GEAR: char = '\u{EAF8}';
pub const CIRCLE_FILLED: char = '\u{EA71}';
pub const CASE_SENSITIVE: char = '\u{EAB1}';
pub const WHOLE_WORD: char = '\u{EB7E}';
pub const REGEX: char = '\u{EB38}';
pub const ELLIPSIS: char = '\u{EA7C}';
pub const COLLAPSE_ALL: char = '\u{EAC5}';
pub const CLEAR_ALL: char = '\u{EABF}';
pub const REFRESH: char = '\u{EB37}';
pub const NEW_FILE: char = '\u{EA7F}';
pub const NEW_FOLDER: char = '\u{EA80}';
pub const EXCLUDE: char = '\u{EAE5}';
pub const FILE: char = '\u{EA7B}';
pub const FOLDER: char = '\u{EA83}';
pub const STOP_CIRCLE: char = '\u{EBA5}';
pub const PRESERVE_CASE: char = '\u{EB2E}';
pub const REPLACE_ALL: char = '\u{EB3C}';
pub const REPLACE: char = '\u{EB3D}';
pub const GO_TO_FILE: char = '\u{EA94}';

/// Register the codicon TTF with gpui's text system.
pub fn install(cx: &App) {
    cx.text_system()
        .add_fonts(vec![Cow::Borrowed(include_bytes!("../assets/codicon.ttf"))])
        .expect("bundled codicon.ttf is a valid font");
}

/// A glyph as a string, ready for `div().font_family(FAMILY).child(..)`.
pub fn glyph(c: char) -> SharedString {
    c.to_string().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codepoints_are_in_private_use_area() {
        for c in [FILES, SEARCH, SOURCE_CONTROL, EXTENSIONS, ACCOUNT, SETTINGS_GEAR, CHEVRON_RIGHT, CHEVRON_DOWN, CLOSE, GIT_BRANCH] {
            assert!(('\u{E000}'..='\u{F8FF}').contains(&c), "{c:?}");
        }
    }

    #[test]
    fn bundled_font_is_a_truetype_file() {
        let bytes = include_bytes!("../assets/codicon.ttf");
        assert_eq!(&bytes[..4], &[0x00, 0x01, 0x00, 0x00], "TrueType magic");
    }

    /// Regression: gpui's macOS loader silently skips fonts without an 'm' glyph
    /// (gpui-pre-apple text_system.rs `load_family`), so every icon fell back to
    /// the system font and rendered as a missing-glyph box. Uses the REAL CoreText
    /// text system: gpui's TestAppContext has a no-op text system that accepts anything.
    #[test]
    fn codicon_loads_in_real_macos_text_system() {
        use gpui::PlatformTextSystem;
        let ts = gpui_apple::AppleTextSystem::new();
        ts.add_fonts(vec![Cow::Borrowed(include_bytes!("../assets/codicon.ttf"))])
            .expect("add_fonts");
        let id = ts
            .font_id(&gpui::font(FAMILY))
            .expect("gpui refused to load the codicon family (missing 'm' glyph?)");
        for c in [FILES, SEARCH, SOURCE_CONTROL, EXTENSIONS, ACCOUNT, SETTINGS_GEAR, CHEVRON_RIGHT, CHEVRON_DOWN, CLOSE, GIT_BRANCH, CIRCLE_FILLED] {
            assert!(ts.glyph_for_char(id, c).is_some(), "no glyph for {c:?} in the resolved font");
        }
    }
}
