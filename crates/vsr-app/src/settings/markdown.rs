//! Minimal inline markdown for setting descriptions.
//!
//! Supports `` `code` ``, `**bold**`, `[label](target)` links and `#setting.id#`
//! references (shown as links, like upstream's `fixSettingLinks`). Everything else is
//! plain text. Newlines are kept.
//!
//! upstream: src/vs/workbench/contrib/preferences/browser/settingsTree.ts @ 50e1350fc78d

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Style {
    Plain,
    Code,
    Bold,
    /// A link; the target is a URL or a `command:` URI.
    Link(String),
    /// A `#setting.id#` reference.
    SettingRef(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

fn span(text: impl Into<String>, style: Style) -> Span {
    Span { text: text.into(), style }
}

fn is_key_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')
}

/// Splits `md` into styled spans. Adjacent plain text is merged.
pub fn parse_inline(md: &str) -> Vec<Span> {
    let chars: Vec<char> = md.chars().collect();
    let mut out: Vec<Span> = Vec::new();
    let mut plain = String::new();
    let flush = |plain: &mut String, out: &mut Vec<Span>| {
        if !plain.is_empty() {
            out.push(span(std::mem::take(plain), Style::Plain));
        }
    };
    let find = |from: usize, pat: &[char]| -> Option<usize> {
        (from..chars.len().saturating_sub(pat.len() - 1)).find(|&i| chars[i..i + pat.len()] == *pat)
    };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        match c {
            '\\' if chars.get(i + 1).is_some_and(|n| n.is_ascii_punctuation()) => {
                plain.push(chars[i + 1]);
                i += 2;
            }
            '`' => match find(i + 1, &['`']) {
                Some(end) if end > i + 1 => {
                    flush(&mut plain, &mut out);
                    out.push(span(chars[i + 1..end].iter().collect::<String>(), Style::Code));
                    i = end + 1;
                }
                _ => {
                    plain.push(c);
                    i += 1;
                }
            },
            '*' if chars.get(i + 1) == Some(&'*') => match find(i + 2, &['*', '*']) {
                Some(end) if end > i + 2 => {
                    flush(&mut plain, &mut out);
                    out.push(span(chars[i + 2..end].iter().collect::<String>(), Style::Bold));
                    i = end + 2;
                }
                _ => {
                    plain.push(c);
                    i += 1;
                }
            },
            '[' => {
                // [label](target)
                let link = find(i + 1, &[']'])
                    .filter(|&close| chars.get(close + 1) == Some(&'('))
                    .and_then(|close| find(close + 2, &[')']).map(|end| (close, end)));
                match link {
                    Some((close, end)) => {
                        flush(&mut plain, &mut out);
                        let label: String = chars[i + 1..close].iter().filter(|c| **c != '`').collect();
                        let target: String = chars[close + 2..end].iter().collect();
                        out.push(span(label, Style::Link(target)));
                        i = end + 1;
                    }
                    None => {
                        plain.push(c);
                        i += 1;
                    }
                }
            }
            '#' => {
                let end = (i + 1..chars.len()).find(|&j| !is_key_char(chars[j]));
                match end {
                    Some(e) if chars[e] == '#' && e > i + 1 && chars[i + 1..e].contains(&'.') => {
                        flush(&mut plain, &mut out);
                        let key: String = chars[i + 1..e].iter().collect();
                        out.push(span(key.clone(), Style::SettingRef(key)));
                        i = e + 1;
                    }
                    _ => {
                        plain.push(c);
                        i += 1;
                    }
                }
            }
            _ => {
                plain.push(c);
                i += 1;
            }
        }
    }
    flush(&mut plain, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(t: &str) -> Span {
        span(t, Style::Plain)
    }

    #[test]
    fn plain_text_is_one_span() {
        assert_eq!(parse_inline("Controls the font size."), vec![p("Controls the font size.")]);
        assert_eq!(parse_inline(""), vec![]);
    }

    #[test]
    fn code_spans() {
        assert_eq!(
            parse_inline("Set to `true` or `false`."),
            vec![p("Set to "), span("true", Style::Code), p(" or "), span("false", Style::Code), p(".")]
        );
    }

    #[test]
    fn unmatched_backtick_stays_literal() {
        assert_eq!(parse_inline("a ` b"), vec![p("a ` b")]);
    }

    #[test]
    fn bold() {
        assert_eq!(parse_inline("a **b** c"), vec![p("a "), span("b", Style::Bold), p(" c")]);
    }

    #[test]
    fn links_keep_label_and_target() {
        assert_eq!(
            parse_inline("See [the docs](https://example.com/x) now"),
            vec![p("See "), span("the docs", Style::Link("https://example.com/x".into())), p(" now")]
        );
        assert_eq!(
            parse_inline("[`files.exclude`](command:workbench.action.openSettings?x)"),
            vec![span("files.exclude", Style::Link("command:workbench.action.openSettings?x".into()))]
        );
    }

    #[test]
    fn setting_references() {
        assert_eq!(
            parse_inline("Also see #editor.fontSize# and #notakey#."),
            vec![p("Also see "), span("editor.fontSize", Style::SettingRef("editor.fontSize".into())), p(" and #notakey#.")]
        );
    }

    #[test]
    fn escapes_and_newlines() {
        assert_eq!(parse_inline(r"a \* b\nc"), vec![p("a * b\\nc")]);
        assert_eq!(parse_inline("a\n\nb"), vec![p("a\n\nb")]);
    }
}
