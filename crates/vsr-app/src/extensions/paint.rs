//! Extension decorations and diagnostics as what the editor paints (ADR-0067): per line, the
//! boxes (`backgroundColor`, `border`), the text after a range (`after.contentText`), and the
//! squiggles of diagnostics. Pure: a [`Snapshot`] and a colour resolver in, plain values out.
//!
//! Upstream source: src/vs/editor/browser/services/abstractCodeEditorService.ts (`DecorationTypeOptionsProvider`)
//! Upstream source: src/vs/editor/common/standaloneStrings.ts
//! Upstream source: src/vs/workbench/api/common/extHostTypes.ts (`ThemeColor`)
//!
//! Not painted yet: `color` (text colour of the range), `before`, `outline*`, `textDecoration`,
//! `gutterIconPath`, `overviewRulerColor`, hover messages.

use super::snapshot::Snapshot;
use crate::theme::Rgba8;
use serde_json::Value;
use std::collections::HashMap;
use vsr_exthost::diagnostics::{ERROR, INFO, WARNING};

/// Where a box ends when the range runs to the end of its line.
pub const TO_LINE_END: usize = usize::MAX;

#[derive(Debug, Clone, PartialEq)]
pub struct DecoBox {
    /// First column (1-based).
    pub from: usize,
    /// End column, exclusive; [`TO_LINE_END`] for the rest of the line.
    pub to: usize,
    /// `isWholeLine`: the box spans the whole row, not just the text.
    pub whole_line: bool,
    pub background: Option<Rgba8>,
    /// Width in px and colour.
    pub border: Option<(f32, Rgba8)>,
}

/// Text drawn after a range (`after.contentText`).
#[derive(Debug, Clone, PartialEq)]
pub struct Inline {
    /// The column the text follows.
    pub column: usize,
    pub text: String,
    pub color: Option<Rgba8>,
    pub background: Option<Rgba8>,
    pub margin_left: f32,
    pub margin_right: f32,
    pub italic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Squiggle {
    pub from: usize,
    pub to: usize,
    pub severity: Severity,
    pub color: Rgba8,
    pub message: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct LinePaint {
    pub boxes: Vec<DecoBox>,
    pub after: Vec<Inline>,
    pub squiggles: Vec<Squiggle>,
}

/// Everything painted on one document, by 1-based line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PathPaint {
    pub lines: HashMap<usize, LinePaint>,
}

impl PathPaint {
    pub fn line(&self, line: usize) -> Option<&LinePaint> {
        self.lines.get(&line)
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
}

/// What the builder needs from the app.
pub struct PaintContext<'a> {
    /// A dark theme: `dark` render options apply, otherwise `light`.
    pub dark: bool,
    pub font_size: f32,
    /// A theme colour by key (`editorError.foreground`).
    pub resolve: &'a dyn Fn(&str) -> Option<Rgba8>,
    /// The number of lines and the max column (length + 1) of a line, from the document.
    pub line_count: usize,
    pub max_column: &'a dyn Fn(usize) -> usize,
}

pub fn build(snapshot: &Snapshot, resource: &str, cx: &PaintContext) -> PathPaint {
    let mut out = PathPaint::default();
    for td in snapshot.decorations.get(resource).into_iter().flatten() {
        let options = snapshot.decoration_types.get(&td.type_key).unwrap_or(&Value::Null);
        add_decoration(&mut out, options, td, cx);
    }
    for marker in snapshot.diagnostics.get(resource).into_iter().flatten() {
        add_squiggle(&mut out, marker, cx);
    }
    out
}

/// `options` with the `light` or `dark` variant merged over it, then `instance` over that.
fn effective(options: &Value, instance: &Value, dark: bool) -> Value {
    let mut base = options.clone();
    let variant = if dark { "dark" } else { "light" };
    if let Some(v) = options.get(variant) {
        base = merge(base, v.clone());
    }
    if !instance.is_null() {
        base = merge(base, instance.clone());
        if let Some(v) = instance.get(variant) {
            base = merge(base, v.clone());
        }
    }
    base
}

fn merge(a: Value, b: Value) -> Value {
    match (a, b) {
        (Value::Object(mut a), Value::Object(b)) => {
            for (k, v) in b {
                let m = match a.remove(&k) {
                    Some(old) => merge(old, v),
                    None => v,
                };
                a.insert(k, m);
            }
            Value::Object(a)
        }
        (_, b) => b,
    }
}

fn add_decoration(out: &mut PathPaint, options: &Value, td: &super::snapshot::TypedDecoration, cx: &PaintContext) {
    let d = &td.decoration;
    let o = effective(options, &d.render_options, cx.dark);
    let whole = o.get("isWholeLine").and_then(Value::as_bool).unwrap_or(false);
    let background = o.get("backgroundColor").and_then(|v| parse_color(v, cx.resolve));
    let border = parse_border(&o, cx.resolve);
    let after = o.get("after").filter(|a| a.get("contentText").and_then(Value::as_str).is_some_and(|t| !t.is_empty()));
    let (sl, el) = (d.start_line as usize, d.end_line as usize);
    if sl == 0 || el < sl {
        return;
    }
    if background.is_some() || border.is_some() {
        for line in sl..=el {
            if line > cx.line_count {
                break;
            }
            let from = if line == sl { d.start_column as usize } else { 1 };
            let to = if line == el { d.end_column as usize } else { TO_LINE_END };
            out.lines.entry(line).or_default().boxes.push(DecoBox {
                from,
                to,
                whole_line: whole,
                background,
                border,
            });
        }
    }
    if let Some(a) = after {
        let line = el.min(cx.line_count.max(1));
        let column = (d.end_column as usize).min((cx.max_column)(line));
        let (left, right) = a
            .get("margin")
            .and_then(Value::as_str)
            .map_or((0.0, 0.0), |m| parse_margin(m, cx.font_size));
        out.lines.entry(line).or_default().after.push(Inline {
            column,
            text: a["contentText"].as_str().unwrap_or("").to_string(),
            color: a.get("color").and_then(|v| parse_color(v, cx.resolve)),
            background: a.get("backgroundColor").and_then(|v| parse_color(v, cx.resolve)),
            margin_left: left,
            margin_right: right,
            italic: a.get("fontStyle").and_then(Value::as_str) == Some("italic"),
        });
    }
}

fn add_squiggle(out: &mut PathPaint, marker: &Value, cx: &PaintContext) {
    let n = |k: &str| marker.get(k).and_then(Value::as_u64).unwrap_or(0) as usize;
    let (severity, key, fallback) = match marker.get("severity").and_then(Value::as_u64) {
        Some(ERROR) => (Severity::Error, "editorError.foreground", Rgba8(0xf4, 0x87, 0x71, 0xff)),
        Some(WARNING) => (Severity::Warning, "editorWarning.foreground", Rgba8(0xcc, 0xa7, 0x00, 0xff)),
        Some(INFO) => (Severity::Info, "editorInfo.foreground", Rgba8(0x37, 0x94, 0xff, 0xff)),
        _ => return, // hints are not drawn
    };
    let color = (cx.resolve)(key).unwrap_or(fallback);
    let (sl, el) = (n("startLineNumber"), n("endLineNumber"));
    if sl == 0 || el < sl {
        return;
    }
    let message = marker.get("message").and_then(Value::as_str).unwrap_or("").to_string();
    for line in sl..=el {
        if line > cx.line_count {
            break;
        }
        let from = if line == sl { n("startColumn").max(1) } else { 1 };
        let mut to = if line == el { n("endColumn") } else { (cx.max_column)(line) };
        if to <= from {
            to = from + 1; // an empty range still shows one character's worth
        }
        out.lines.entry(line).or_default().squiggles.push(Squiggle {
            from,
            to,
            severity,
            color,
            message: message.clone(),
        });
    }
}

/// `border` (`1px solid red`, any order) with `borderColor` and `borderWidth` overriding.
fn parse_border(o: &Value, resolve: &dyn Fn(&str) -> Option<Rgba8>) -> Option<(f32, Rgba8)> {
    let mut width = None;
    let mut color = None;
    if let Some(shorthand) = o.get("border").and_then(Value::as_str) {
        for tok in css_tokens(shorthand) {
            if let Some(px) = parse_length(&tok, 0.0) {
                width = Some(px);
            } else if !matches!(tok.as_str(), "solid" | "dashed" | "dotted" | "double" | "none") {
                color = parse_css_color(&tok);
            }
        }
    }
    if let Some(c) = o.get("borderColor").and_then(|v| parse_color(v, resolve)) {
        color = Some(c);
    }
    if let Some(w) = o.get("borderWidth").and_then(Value::as_str).and_then(|w| parse_length(w, 0.0)) {
        width = Some(w);
    }
    color.map(|c| (width.unwrap_or(1.0).max(1.0), c))
}

/// `(left, right)` of a CSS `margin` shorthand, in px (`em` is `font_size`).
pub fn parse_margin(margin: &str, font_size: f32) -> (f32, f32) {
    let v: Vec<f32> = css_tokens(margin).iter().map(|t| parse_length(t, font_size).unwrap_or(0.0)).collect();
    match v.as_slice() {
        [a] => (*a, *a),
        [_, h] => (*h, *h),
        [_, r, _] => (*r, *r),
        [_, r, _, l, ..] => (*l, *r),
        [] => (0.0, 0.0),
    }
}

/// `2em`, `10px`, `0`, `1.5ch` (a character is taken as 0.6em) as px.
pub fn parse_length(s: &str, font_size: f32) -> Option<f32> {
    let s = s.trim();
    let split = s.find(|c: char| c.is_ascii_alphabetic() || c == '%').unwrap_or(s.len());
    let (num, unit) = s.split_at(split);
    let n: f32 = num.parse().ok()?;
    match unit {
        "" | "px" => Some(n),
        "em" | "rem" => Some(n * font_size),
        "ch" => Some(n * font_size * 0.6),
        "pt" => Some(n * 4.0 / 3.0),
        _ => None,
    }
}

/// Split on spaces outside parentheses.
fn css_tokens(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '(' => {
                depth += 1;
                cur.push(c);
            }
            ')' => {
                depth -= 1;
                cur.push(c);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                }
            }
            c => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur);
    }
    out
}

/// A colour option: a CSS string, or a `ThemeColor` (`{ id }`) resolved through the theme.
pub fn parse_color(v: &Value, resolve: &dyn Fn(&str) -> Option<Rgba8>) -> Option<Rgba8> {
    match v {
        Value::String(s) => parse_css_color(s),
        Value::Object(o) => o.get("id").and_then(Value::as_str).and_then(resolve),
        _ => None,
    }
}

/// `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, `rgb()`, `rgba()` and a few names.
pub fn parse_css_color(s: &str) -> Option<Rgba8> {
    let s = s.trim().to_ascii_lowercase();
    if let Some(h) = s.strip_prefix('#') {
        let d: Vec<u8> = h.chars().map(|c| c.to_digit(16).map(|d| d as u8)).collect::<Option<_>>()?;
        return match d.len() {
            3 => Some(Rgba8(d[0] * 17, d[1] * 17, d[2] * 17, 255)),
            4 => Some(Rgba8(d[0] * 17, d[1] * 17, d[2] * 17, d[3] * 17)),
            6 => Some(Rgba8(d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], 255)),
            8 => Some(Rgba8(d[0] * 16 + d[1], d[2] * 16 + d[3], d[4] * 16 + d[5], d[6] * 16 + d[7])),
            _ => None,
        };
    }
    if let Some(args) = s.strip_prefix("rgba(").or_else(|| s.strip_prefix("rgb(")).and_then(|r| r.strip_suffix(')')) {
        let parts: Vec<&str> = args.split([',', ' ', '/']).filter(|p| !p.is_empty()).collect();
        if parts.len() < 3 {
            return None;
        }
        let ch = |p: &str| -> Option<u8> {
            Some(match p.strip_suffix('%') {
                Some(pc) => (pc.parse::<f32>().ok()? * 2.55).round().clamp(0.0, 255.0) as u8,
                None => p.parse::<f32>().ok()?.round().clamp(0.0, 255.0) as u8,
            })
        };
        let alpha = match parts.get(3) {
            Some(a) => match a.strip_suffix('%') {
                Some(pc) => (pc.parse::<f32>().ok()? * 2.55).round().clamp(0.0, 255.0) as u8,
                None => (a.parse::<f32>().ok()? * 255.0).round().clamp(0.0, 255.0) as u8,
            },
            None => 255,
        };
        return Some(Rgba8(ch(parts[0])?, ch(parts[1])?, ch(parts[2])?, alpha));
    }
    Some(match s.as_str() {
        "transparent" => Rgba8(0, 0, 0, 0),
        "black" => Rgba8(0, 0, 0, 255),
        "white" => Rgba8(255, 255, 255, 255),
        "red" => Rgba8(255, 0, 0, 255),
        "green" => Rgba8(0, 128, 0, 255),
        "blue" => Rgba8(0, 0, 255, 255),
        "yellow" => Rgba8(255, 255, 0, 255),
        "orange" => Rgba8(255, 165, 0, 255),
        "gray" | "grey" => Rgba8(128, 128, 128, 255),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extensions::snapshot::TypedDecoration;
    use serde_json::json;
    use vsr_exthost::editors::Decoration;

    fn none(_: &str) -> Option<Rgba8> {
        None
    }

    #[test]
    fn colors() {
        assert_eq!(parse_css_color("#f00"), Some(Rgba8(255, 0, 0, 255)));
        assert_eq!(parse_css_color("#ff000080"), Some(Rgba8(255, 0, 0, 128)));
        assert_eq!(parse_css_color("rgba(255, 0, 0, 0.1)"), Some(Rgba8(255, 0, 0, 26)));
        assert_eq!(parse_css_color("rgb(1,2,3)"), Some(Rgba8(1, 2, 3, 255)));
        assert_eq!(parse_css_color("RED"), Some(Rgba8(255, 0, 0, 255)));
        assert_eq!(parse_css_color("nonsense"), None);
        let resolve = |k: &str| (k == "editorError.foreground").then_some(Rgba8(1, 1, 1, 255));
        assert_eq!(parse_color(&json!({ "id": "editorError.foreground" }), &resolve), Some(Rgba8(1, 1, 1, 255)));
        assert_eq!(parse_color(&json!({ "id": "unknown" }), &resolve), None);
    }

    #[test]
    fn lengths_and_margins() {
        assert_eq!(parse_length("2em", 10.0), Some(20.0));
        assert_eq!(parse_length("12px", 10.0), Some(12.0));
        assert_eq!(parse_length("0", 10.0), Some(0.0));
        assert_eq!(parse_margin("0 0 0 2em", 10.0), (20.0, 0.0));
        assert_eq!(parse_margin("4px 8px", 10.0), (8.0, 8.0));
        assert_eq!(parse_margin("3px", 10.0), (3.0, 3.0));
    }

    #[test]
    fn border_shorthand_and_overrides() {
        assert_eq!(parse_border(&json!({ "border": "2px solid red" }), &none), Some((2.0, Rgba8(255, 0, 0, 255))));
        assert_eq!(
            parse_border(&json!({ "border": "solid rgba(0, 0, 255, 1) 1px", "borderColor": "#00ff00" }), &none),
            Some((1.0, Rgba8(0, 255, 0, 255)))
        );
        assert_eq!(parse_border(&json!({ "backgroundColor": "red" }), &none), None);
    }

    fn snapshot_with(options: Value, d: Decoration) -> Snapshot {
        let mut s = Snapshot::default();
        s.decoration_types.insert("t".into(), options);
        s.decorations.insert("file:///a".into(), vec![TypedDecoration { type_key: "t".into(), decoration: d }]);
        s
    }

    fn ctx<'a>(resolve: &'a dyn Fn(&str) -> Option<Rgba8>, max: &'a dyn Fn(usize) -> usize) -> PaintContext<'a> {
        PaintContext { dark: true, font_size: 10.0, resolve, line_count: 5, max_column: max }
    }

    fn dec(l1: u64, c1: u64, l2: u64, c2: u64, render: Value) -> Decoration {
        Decoration {
            start_line: l1,
            start_column: c1,
            end_line: l2,
            end_column: c2,
            hover_message: Value::Null,
            render_options: render,
        }
    }

    #[test]
    fn errorlens_style_decoration_is_a_whole_line_box_and_text_after_the_line() {
        let s = snapshot_with(
            json!({ "isWholeLine": true, "backgroundColor": "rgba(255, 0, 0, 0.1)", "after": { "margin": "0 0 0 2em", "color": "#ff0000" } }),
            dec(2, 1, 2, 9, json!({ "after": { "contentText": " boom" } })),
        );
        let max = |_: usize| 9;
        let p = build(&s, "file:///a", &ctx(&none, &max));
        let l = p.line(2).unwrap();
        assert_eq!(
            l.boxes,
            [DecoBox { from: 1, to: 9, whole_line: true, background: Some(Rgba8(255, 0, 0, 26)), border: None }]
        );
        assert_eq!(
            l.after,
            [Inline {
                column: 9,
                text: " boom".into(),
                color: Some(Rgba8(255, 0, 0, 255)),
                background: None,
                margin_left: 20.0,
                margin_right: 0.0,
                italic: false
            }]
        );
        assert!(p.line(1).is_none());
    }

    #[test]
    fn instance_options_and_the_theme_variant_override_the_type() {
        let s = snapshot_with(
            json!({ "backgroundColor": "red", "dark": { "backgroundColor": "blue" } }),
            dec(1, 1, 1, 3, json!({ "dark": { "border": "1px solid white" } })),
        );
        let max = |_: usize| 4;
        let dark = build(&s, "file:///a", &ctx(&none, &max));
        let b = &dark.line(1).unwrap().boxes[0];
        assert_eq!(b.background, Some(Rgba8(0, 0, 255, 255)));
        assert_eq!(b.border, Some((1.0, Rgba8(255, 255, 255, 255))));
        let mut light = ctx(&none, &max);
        light.dark = false;
        assert_eq!(build(&s, "file:///a", &light).line(1).unwrap().boxes[0].background, Some(Rgba8(255, 0, 0, 255)));
    }

    #[test]
    fn a_range_over_several_lines_is_split_per_line() {
        let s = snapshot_with(json!({ "backgroundColor": "red" }), dec(2, 4, 4, 2, Value::Null));
        let max = |_: usize| 10;
        let p = build(&s, "file:///a", &ctx(&none, &max));
        assert_eq!((p.line(2).unwrap().boxes[0].from, p.line(2).unwrap().boxes[0].to), (4, TO_LINE_END));
        assert_eq!((p.line(3).unwrap().boxes[0].from, p.line(3).unwrap().boxes[0].to), (1, TO_LINE_END));
        assert_eq!((p.line(4).unwrap().boxes[0].from, p.line(4).unwrap().boxes[0].to), (1, 2));
    }

    #[test]
    fn diagnostics_become_squiggles_with_theme_colors() {
        let mut s = Snapshot::default();
        let m = |sev: u64, l: u64, c1: u64, c2: u64| {
            json!({ "severity": sev, "message": "m", "startLineNumber": l, "startColumn": c1, "endLineNumber": l, "endColumn": c2 })
        };
        s.diagnostics.insert("file:///a".into(), vec![m(8, 1, 2, 5), m(4, 2, 3, 3), m(1, 3, 1, 2)]);
        let resolve = |k: &str| (k == "editorError.foreground").then_some(Rgba8(9, 9, 9, 255));
        let max = |_: usize| 10;
        let p = build(&s, "file:///a", &ctx(&resolve, &max));
        let e = &p.line(1).unwrap().squiggles[0];
        assert_eq!((e.from, e.to, e.severity, e.color), (2, 5, Severity::Error, Rgba8(9, 9, 9, 255)));
        let w = &p.line(2).unwrap().squiggles[0];
        assert_eq!((w.from, w.to, w.severity), (3, 4, Severity::Warning), "an empty range covers one character");
        assert!(p.line(3).is_none(), "hints are not drawn");
        assert!(build(&s, "file:///other", &ctx(&resolve, &max)).is_empty());
    }
}
