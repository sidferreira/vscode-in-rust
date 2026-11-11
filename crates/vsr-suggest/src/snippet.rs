//! Expands a completion's snippet text to plain text and the caret it leaves (ADR-0084).
//!
//! Upstream source: src/vs/editor/contrib/snippet/browser/snippetParser.ts (the grammar),
//! src/vs/editor/contrib/snippet/browser/snippetSession.ts (what a session does with it).
//!
//! rscode has no snippet session: there is no Tab to the next tab stop. The text is inserted with
//! placeholders expanded to their default text (a choice to its first option, a variable to its
//! default or, when it has none, to nothing for the variables the editor resolves and to its name
//! for the others, as upstream), and the caret goes to the first tab stop (its placeholder text
//! selected), else to `$0`, else to the end. Transforms are dropped. Offsets are UTF-16 units.

/// The expanded text and the selection it leaves (`start..end`, equal for a caret).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expanded {
    pub text: String,
    pub selection: (usize, usize),
}

/// A snippet's text with nothing in it that needs expanding (a plain completion): the whole text,
/// the caret at its end.
pub fn plain(text: &str) -> Expanded {
    let end = text.encode_utf16().count();
    Expanded {
        text: text.to_string(),
        selection: (end, end),
    }
}

/// Variables the editor resolves to a value; without a model of the file they expand to nothing.
const KNOWN_VARIABLES: &[&str] = &[
    "TM_SELECTED_TEXT",
    "TM_CURRENT_LINE",
    "TM_CURRENT_WORD",
    "TM_LINE_INDEX",
    "TM_LINE_NUMBER",
    "TM_FILENAME",
    "TM_FILENAME_BASE",
    "TM_DIRECTORY",
    "TM_FILEPATH",
    "RELATIVE_FILEPATH",
    "CLIPBOARD",
    "WORKSPACE_NAME",
    "WORKSPACE_FOLDER",
    "CURSOR_INDEX",
    "CURSOR_NUMBER",
    "CURRENT_YEAR",
    "CURRENT_YEAR_SHORT",
    "CURRENT_MONTH",
    "CURRENT_DATE",
    "CURRENT_HOUR",
    "CURRENT_MINUTE",
    "CURRENT_SECOND",
    "CURRENT_DAY_NAME",
    "CURRENT_DAY_NAME_SHORT",
    "CURRENT_MONTH_NAME",
    "CURRENT_MONTH_NAME_SHORT",
    "CURRENT_SECONDS_UNIX",
    "CURRENT_TIMEZONE_OFFSET",
    "RANDOM",
    "RANDOM_HEX",
    "UUID",
    "BLOCK_COMMENT_START",
    "BLOCK_COMMENT_END",
    "LINE_COMMENT",
];

struct Parser<'a> {
    src: &'a [char],
    i: usize,
    out: String,
    units: usize,
    /// `(index, start, end)` of each tab stop, in order of appearance.
    stops: Vec<(u32, usize, usize)>,
}

/// Expands `snippet`.
pub fn expand(snippet: &str) -> Expanded {
    let chars: Vec<char> = snippet.chars().collect();
    let mut p = Parser {
        src: &chars,
        i: 0,
        out: String::new(),
        units: 0,
        stops: Vec::new(),
    };
    p.seq(false);
    // The first tab stop by index (not zero), then `$0`, then the end.
    let first = p
        .stops
        .iter()
        .filter(|s| s.0 != 0)
        .min_by_key(|s| s.0)
        .or_else(|| p.stops.iter().find(|s| s.0 == 0));
    let selection = first.map_or((p.units, p.units), |s| (s.1, s.2));
    Expanded {
        text: p.out,
        selection,
    }
}

impl Parser<'_> {
    fn peek(&self) -> Option<char> {
        self.src.get(self.i).copied()
    }

    fn push(&mut self, c: char) {
        self.out.push(c);
        self.units += c.len_utf16();
    }

    fn push_str(&mut self, s: &str) {
        for c in s.chars() {
            self.push(c);
        }
    }

    /// Text up to the end, or up to an unescaped `}` inside a placeholder (not consumed).
    fn seq(&mut self, in_brace: bool) {
        while let Some(c) = self.peek() {
            match c {
                '}' if in_brace => break,
                '\\' => {
                    self.i += 1;
                    match self.peek() {
                        Some(n @ ('$' | '}' | '\\')) => {
                            self.push(n);
                            self.i += 1;
                        }
                        _ => self.push('\\'),
                    }
                }
                '$' => self.dollar(),
                c => {
                    self.push(c);
                    self.i += 1;
                }
            }
        }
    }

    fn number(&mut self) -> u32 {
        let mut n: u32 = 0;
        while let Some(d) = self.peek().and_then(|c| c.to_digit(10)) {
            n = n.saturating_mul(10).saturating_add(d);
            self.i += 1;
        }
        n
    }

    fn name(&mut self) -> String {
        let mut s = String::new();
        while let Some(c) = self.peek().filter(|c| c.is_ascii_alphanumeric() || *c == '_') {
            s.push(c);
            self.i += 1;
        }
        s
    }

    /// Skips a `/regex/format/options` transform up to its closing `}` (not consumed).
    fn skip_transform(&mut self) {
        while let Some(c) = self.peek() {
            match c {
                '}' => break,
                '\\' => self.i += 2,
                _ => self.i += 1,
            }
        }
    }

    fn close(&mut self) {
        if self.peek() == Some('}') {
            self.i += 1;
        }
    }

    fn variable(&mut self, name: &str, default: Option<(usize, usize)>) {
        // A default text was already written by `seq`; a variable without one is its name for an
        // unknown variable and nothing for one the editor resolves.
        if default.is_none_or(|(a, b)| a == b) && !KNOWN_VARIABLES.contains(&name) {
            self.push_str(name);
        }
    }

    fn dollar(&mut self) {
        self.i += 1; // the `$`
        match self.peek() {
            Some(c) if c.is_ascii_digit() => {
                let n = self.number();
                self.stops.push((n, self.units, self.units));
            }
            Some('{') => {
                self.i += 1;
                match self.peek() {
                    Some(c) if c.is_ascii_digit() => {
                        let n = self.number();
                        let start = self.units;
                        match self.peek() {
                            Some(':') => {
                                self.i += 1;
                                self.seq(true);
                            }
                            Some('|') => {
                                self.i += 1;
                                let mut first = true;
                                let mut done = false;
                                while let Some(c) = self.peek() {
                                    self.i += 1;
                                    match c {
                                        ',' => first = false,
                                        '|' => {
                                            done = true;
                                            break;
                                        }
                                        '\\' => {
                                            if let Some(n) = self.peek() {
                                                self.i += 1;
                                                if first {
                                                    self.push(n);
                                                }
                                            }
                                        }
                                        c if first => self.push(c),
                                        _ => {}
                                    }
                                }
                                let _ = done;
                            }
                            Some('/') => self.skip_transform(),
                            _ => {}
                        }
                        self.stops.push((n, start, self.units));
                        self.close();
                    }
                    Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                        let name = self.name();
                        let start = self.units;
                        match self.peek() {
                            Some(':') => {
                                self.i += 1;
                                self.seq(true);
                            }
                            Some('/') => self.skip_transform(),
                            _ => {}
                        }
                        self.variable(&name, Some((start, self.units)));
                        self.close();
                    }
                    _ => self.push_str("${"),
                }
            }
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                let name = self.name();
                self.variable(&name, None);
            }
            _ => self.push('$'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(s: &str) -> (String, (usize, usize)) {
        let e = expand(s);
        (e.text, e.selection)
    }

    #[test]
    fn plain_text_keeps_the_caret_at_the_end() {
        assert_eq!(ex("hello"), ("hello".into(), (5, 5)));
        assert_eq!(plain("a\u{1F600}").selection, (3, 3));
    }

    #[test]
    fn placeholders_expand_to_their_text_and_the_first_stop_is_selected() {
        // `let ${1:name} = foo$0`: the caret goes to the first stop, `name` selected.
        assert_eq!(ex("let ${1:name} = foo$0"), ("let name = foo".into(), (4, 8)));
        assert_eq!(ex("a$1b$0"), ("ab".into(), (1, 1)));
        assert_eq!(ex("x$0y${2:z}"), ("xyz".into(), (2, 3)), "$0 comes last: stop 2 first");
    }

    #[test]
    fn the_final_stop_alone_places_the_caret() {
        assert_eq!(ex("foo($0)"), ("foo()".into(), (4, 4)));
    }

    #[test]
    fn nested_placeholders_choices_and_escapes() {
        assert_eq!(ex("${1:a ${2:b}}"), ("a b".into(), (0, 3)));
        assert_eq!(ex("${1|one,two,three|}"), ("one".into(), (0, 3)));
        assert_eq!(ex("\\$1 \\} \\\\ \\n"), ("$1 } \\ \\n".into(), (10, 10)));
        assert_eq!(ex("${1:a\\}b}"), ("a}b".into(), (0, 3)));
    }

    #[test]
    fn variables_use_their_default_or_expand_to_nothing_or_their_name() {
        assert_eq!(ex("$TM_FILENAME|"), ("|".into(), (1, 1)));
        assert_eq!(ex("${TM_FILENAME:none}|"), ("none|".into(), (5, 5)));
        assert_eq!(ex("$unknown|"), ("unknown|".into(), (8, 8)));
        assert_eq!(ex("${foo/(.*)/$1/}|"), ("foo|".into(), (4, 4)));
    }

    #[test]
    fn a_stray_dollar_stays() {
        assert_eq!(ex("cost: $ 5"), ("cost: $ 5".into(), (9, 9)));
    }
}
