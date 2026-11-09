//! Codicons: the icon font VS Code uses for all workbench icons.
//!
//! upstream: src/vs/base/common/codiconsLibrary.ts @ 50e1350fc78d
//!
//! The font is `@vscode/codicons` 0.0.46-49 (the version upstream's package.json pins),
//! bundled at `assets/codicon.ttf`, licensed CC BY 4.0 (`assets/codicon-LICENSE.txt`).
//! Codepoints below are copied from upstream's registry.

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use std::sync::Arc;

const FAMILY: &str = "codicon";

pub const FILES: char = '\u{EAF0}';
pub const SEARCH: char = '\u{EA6D}';
pub const SOURCE_CONTROL: char = '\u{EA68}';
pub const EXTENSIONS: char = '\u{EAE6}';
pub const ACCOUNT: char = '\u{EB99}';
pub const SETTINGS_GEAR: char = '\u{EB51}';
pub const CHEVRON_RIGHT: char = '\u{EAB6}';
pub const CHEVRON_DOWN: char = '\u{EAB4}';
pub const CLOSE: char = '\u{EA76}';
pub const GIT_BRANCH: char = '\u{EC6F}';
pub const ERROR: char = '\u{EA87}';
pub const WARNING: char = '\u{EA6C}';
pub const BELL: char = '\u{EAA2}';

/// Register the codicon font as its own family and as a fallback for the
/// regular families, so `"\u{EAF0}"` renders inside normal text too.
pub fn install(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        FAMILY.to_owned(),
        Arc::new(FontData::from_static(include_bytes!("../assets/codicon.ttf"))),
    );
    fonts
        .families
        .insert(FontFamily::Name(FAMILY.into()), vec![FAMILY.to_owned()]);
    for fam in [FontFamily::Proportional, FontFamily::Monospace] {
        fonts.families.entry(fam).or_default().push(FAMILY.to_owned());
    }
    ctx.set_fonts(fonts);
}

pub fn font(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(FAMILY.into()))
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
}
