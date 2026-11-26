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
    /// The tab stops, by index (1.., then 0 last), each with every place it occurs (UTF-16 offsets
    /// into `text`, in source order). A snippet without `$0` gets one at its end (upstream
    /// `createEditsAndSnippetsFromSelections`, `enforceFinalTabstop`).
    pub stops: Vec<Tabstop>,
}

/// One tab stop: the placeholder and its mirrors (`$1` written again).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tabstop {
    pub index: u32,
    /// `(start, end)` UTF-16 offsets; the first is the placeholder, the rest are mirrors.
    pub ranges: Vec<(usize, usize)>,
    /// Which of `ranges` is the placeholder: the one written `${n:text}` / `${n|a,b|}`, else the first
    /// (`$1 ${1:x}` has its placeholder second).
    pub primary: usize,
}

/// A snippet's text with nothing in it that needs expanding (a plain completion): the whole text,
/// the caret at its end.
pub fn plain(text: &str) -> Expanded {
    let end = text.encode_utf16().count();
    Expanded {
        text: text.to_string(),
        selection: (end, end),
        stops: Vec::new(),
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
    /// The text of each placeholder seen so far: a later `$n` or `${n}` mirrors it.
    texts: std::collections::HashMap<u32, String>,
    /// Positions in `stops` written with a default (`${n:..}`, `${n|..|}`): the placeholders.
    defaults: Vec<usize>,
}

/// Expands `snippet`.
pub fn expand(snippet: &str) -> Expanded {
    let chars: Vec<char> = snippet.chars().collect();
    // A mirror may come before its placeholder (`$1 ${1:x}`): the first pass learns the defaults,
    // the second expands the mirrors with them.
    let first = run(&chars, std::collections::HashMap::new());
    let mut p = run(&chars, first.texts);
    // Upstream adds a final tab stop at the end when the snippet has none.
    if !p.stops.iter().any(|s| s.0 == 0) {
        let end = p.units;
        p.stops.push((0, end, end));
    }
    let mut indices: Vec<u32> = p.stops.iter().map(|s| s.0).collect();
    indices.sort_unstable();
    indices.dedup();
    indices.retain(|i| *i != 0);
    indices.push(0);
    let stops: Vec<Tabstop> = indices
        .into_iter()
        .map(|index| {
            let at: Vec<usize> = (0..p.stops.len()).filter(|&i| p.stops[i].0 == index).collect();
            Tabstop {
                index,
                ranges: at.iter().map(|&i| (p.stops[i].1, p.stops[i].2)).collect(),
                primary: at.iter().position(|i| p.defaults.contains(i)).unwrap_or(0),
            }
        })
        .collect();
    // The first tab stop by index (not zero), then `$0`, then the end.
    let selection = stops
        .iter()
        .find(|t| t.index != 0)
        .or_else(|| stops.first())
        .map_or((p.units, p.units), |t| t.ranges[t.primary]);
    Expanded {
        text: p.out,
        selection,
        stops,
    }
}

fn run(chars: &[char], texts: std::collections::HashMap<u32, String>) -> Parser<'_> {
    let mut p = Parser {
        src: chars,
        i: 0,
        out: String::new(),
        units: 0,
        stops: Vec::new(),
        texts,
        defaults: Vec::new(),
    };
    p.seq(false);
    p
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
                let start = self.units;
                if let Some(text) = self.texts.get(&n).cloned() {
                    self.push_str(&text);
                }
                self.stops.push((n, start, self.units));
            }
            Some('{') => {
                self.i += 1;
                match self.peek() {
                    Some(c) if c.is_ascii_digit() => {
                        let n = self.number();
                        let start = self.units;
                        let start_byte = self.out.len();
                        let mirror = self.texts.get(&n).cloned();
                        let mut has_default = false;
                        match self.peek() {
                            Some(':') => {
                                self.i += 1;
                                has_default = true;
                                self.seq(true);
                            }
                            Some('}') if mirror.is_some() => {
                                self.push_str(&mirror.unwrap_or_default());
                            }
                            Some('|') => {
                                self.i += 1;
                                has_default = true;
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
                        self.texts.entry(n).or_insert_with(|| self.out[start_byte..].to_string());
                        if has_default {
                            self.defaults.push(self.stops.len());
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
        assert_eq!(ex("\\$1 \\} \\\\ \\n"), ("$1 } \\ \\n".into(), (9, 9)));
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
    fn a_mirror_repeats_the_placeholder_text() {
        assert_eq!(ex("${1:x} = $1; ${1}"), ("x = x; x".into(), (0, 1)));
    }

    #[test]
    fn a_stray_dollar_stays() {
        assert_eq!(ex("cost: $ 5"), ("cost: $ 5".into(), (9, 9)));
    }

    fn stops(s: &str) -> Vec<(u32, Vec<(usize, usize)>)> {
        expand(s).stops.into_iter().map(|t| (t.index, t.ranges)).collect()
    }

    #[test]
    fn stops_group_by_index_with_zero_last_and_keep_mirrors() {
        // `${1:x} = $2; $1` gives "x = ; x": stop 1 twice (placeholder, then mirror), stop 2, implicit $0 at the end.
        assert_eq!(
            stops("${1:x} = $2; $1"),
            vec![(1, vec![(0, 1), (6, 7)]), (2, vec![(4, 4)]), (0, vec![(7, 7)])]
        );
    }

    #[test]
    fn a_snippet_without_a_final_stop_gets_one_at_its_end() {
        assert_eq!(expand("a${1:b}c").stops.last().map(|t| t.index), Some(0));
        assert_eq!(stops("a${1:b}c$0").len(), 2, "an explicit $0 is not doubled");
    }

    #[test]
    fn a_mirror_before_its_placeholder_gets_the_default_text() {
        let e = expand("$1 ${1:x}");
        assert_eq!(e.text, "x x");
        assert_eq!(e.stops[0].ranges, vec![(0, 1), (2, 3)], "ranges are in source order: the mirror first");
        assert_eq!(e.stops[0].primary, 1, "the placeholder is the one with the default");
        assert_eq!(e.selection, (2, 3), "and it is what gets selected");
        let nested = expand("${1:a${2:b}} $1");
        assert_eq!(nested.stops[0].primary, 0);
    }

    #[test]
    fn a_plain_text_has_only_the_final_stop_at_its_end() {
        assert_eq!(expand("hi"), Expanded { text: "hi".into(), selection: (2, 2), stops: vec![Tabstop { index: 0, ranges: vec![(2, 2)], primary: 0 }] });
    }
}
