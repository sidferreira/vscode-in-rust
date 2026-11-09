//! Glue between the UI-agnostic `theme` and gpui.

use crate::theme::Rgba8;
use gpui_kit::{Hsla, rgba};

/// Convert a theme color to a gpui color.
pub fn to_gpui(c: Rgba8) -> Hsla {
    rgba(c.to_u32()).into()
}
