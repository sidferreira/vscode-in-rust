//! The replace string: escapes (`\n`, `\t`, `\\`), capture groups (`$1`, `$&`, `$$`), case
//! modifiers (`\u`, `\U`, `\l`, `\L`) and "Preserve Case".
//!
//! upstream: src/vs/editor/contrib/find/browser/replacePattern.ts @ 50e1350fc78d
//! upstream: src/vs/base/common/search.ts @ 50e1350fc78d
//!
//! JavaScript's `exec` result is `string[]` with `undefined` for groups that did not take part;
//! here the match is a slice of strings where such a group is `""` (upstream reads it as `''`
//! anyway). Case operations work on `char`s where upstream uses UTF-16 units, which only differs for
//! characters outside the BMP.

/// A piece of a parsed replace string: literal text, or a capture group with case operations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacePiece {
    pub static_value: Option<String>,
    pub match_index: usize,
    pub case_ops: Option<Vec<char>>,
}

impl ReplacePiece {
    pub fn static_value(value: &str) -> Self {
        Self { static_value: Some(value.to_string()), match_index: 0, case_ops: None }
    }

    pub fn match_index(index: usize) -> Self {
        Self { static_value: None, match_index: index, case_ops: None }
    }

    pub fn case_ops(index: usize, ops: &[char]) -> Self {
        Self {
            static_value: None,
            match_index: index,
            case_ops: if ops.is_empty() { None } else { Some(ops.to_vec()) },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    /// The pattern is entirely static.
    StaticValue(String),
    DynamicPieces(Vec<ReplacePiece>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplacePattern {
    state: State,
}

impl ReplacePattern {
    pub fn from_static_value(value: &str) -> Self {
        Self::new(vec![ReplacePiece::static_value(value)])
    }

    pub fn new(pieces: Vec<ReplacePiece>) -> Self {
        let state = if pieces.is_empty() {
            State::StaticValue(String::new())
        } else if pieces.len() == 1 && pieces[0].static_value.is_some() {
            State::StaticValue(pieces[0].static_value.clone().unwrap_or_default())
        } else {
            State::DynamicPieces(pieces)
        };
        Self { state }
    }

    pub fn has_replacement_patterns(&self) -> bool {
        matches!(self.state, State::DynamicPieces(_))
    }

    /// The replacement for one match. `matches` is the full match followed by the groups
    /// (`None` when the search was not run with captures).
    pub fn build_replace_string(&self, matches: Option<&[String]>, preserve_case: bool) -> String {
        let pieces = match &self.state {
            State::StaticValue(v) => {
                return if preserve_case {
                    build_replace_string_with_case_preserved(matches, v)
                } else {
                    v.clone()
                };
            }
            State::DynamicPieces(p) => p,
        };
        let mut result = String::new();
        for piece in pieces {
            if let Some(s) = &piece.static_value {
                result.push_str(s);
                continue;
            }
            let mut m = substitute(piece.match_index, matches);
            if let Some(ops) = piece.case_ops.as_ref().filter(|o| !o.is_empty()) {
                let mut out = String::new();
                let mut op_idx = 0;
                let chars: Vec<char> = m.chars().collect();
                for (idx, &ch) in chars.iter().enumerate() {
                    if op_idx >= ops.len() {
                        out.extend(&chars[idx..]);
                        break;
                    }
                    match ops[op_idx] {
                        'U' => out.extend(ch.to_uppercase()),
                        'u' => {
                            out.extend(ch.to_uppercase());
                            op_idx += 1;
                        }
                        'L' => out.extend(ch.to_lowercase()),
                        'l' => {
                            out.extend(ch.to_lowercase());
                            op_idx += 1;
                        }
                        _ => out.push(ch),
                    }
                }
                m = out;
            }
            result.push_str(&m);
        }
        result
    }
}

/// `$n` with JavaScript's `String.replace` semantics: `$nn` is a group only if it exists,
/// otherwise `$n` followed by the digit.
fn substitute(mut match_index: usize, matches: Option<&[String]>) -> String {
    let Some(matches) = matches else { return String::new() };
    if match_index == 0 {
        return matches.first().cloned().unwrap_or_default();
    }
    let mut remainder = String::new();
    while match_index > 0 {
        if match_index < matches.len() {
            return format!("{}{}", matches[match_index], remainder);
        }
        remainder = format!("{}{}", match_index % 10, remainder);
        match_index /= 10;
    }
    format!("${remainder}")
}

struct Builder<'a> {
    source: &'a [char],
    last_char_index: usize,
    result: Vec<ReplacePiece>,
    current_static: String,
}

impl<'a> Builder<'a> {
    fn new(source: &'a [char]) -> Self {
        Self { source, last_char_index: 0, result: Vec::new(), current_static: String::new() }
    }

    fn emit_unchanged(&mut self, to: usize) {
        let s: String = self.source[self.last_char_index..to.max(self.last_char_index)].iter().collect();
        self.current_static.push_str(&s);
        self.last_char_index = to;
    }

    fn emit_static(&mut self, value: &str, to: usize) {
        self.current_static.push_str(value);
        self.last_char_index = to;
    }

    fn flush_static(&mut self) {
        if !self.current_static.is_empty() {
            self.result.push(ReplacePiece::static_value(&self.current_static));
            self.current_static.clear();
        }
    }

    fn emit_match_index(&mut self, index: usize, to: usize, case_ops: &[char]) {
        self.flush_static();
        self.result.push(ReplacePiece::case_ops(index, case_ops));
        self.last_char_index = to;
    }

    fn finalize(mut self) -> ReplacePattern {
        self.emit_unchanged(self.source.len());
        self.flush_static();
        ReplacePattern::new(self.result)
    }
}

/// Parses a replace string:
///
/// * `\n` LF, `\t` TAB, `\\` a backslash;
/// * `\u` / `\U` upper-case one / all following characters of the next group, `\l` / `\L` lower-case;
/// * `$$` a dollar, `$&` and `$0` the match, `$n` / `$nn` the nth group;
/// * everything else stays as typed.
pub fn parse_replace_string(replace_string: &str) -> ReplacePattern {
    if replace_string.is_empty() {
        return ReplacePattern::new(Vec::new());
    }
    let chars: Vec<char> = replace_string.chars().collect();
    let len = chars.len();
    let mut case_ops: Vec<char> = Vec::new();
    let mut result = Builder::new(&chars);

    let mut i = 0;
    while i < len {
        let ch = chars[i];
        if ch == '\\' {
            i += 1;
            if i >= len {
                break; // string ends with a backslash
            }
            match chars[i] {
                '\\' => {
                    result.emit_unchanged(i - 1);
                    result.emit_static("\\", i + 1);
                }
                'n' => {
                    result.emit_unchanged(i - 1);
                    result.emit_static("\n", i + 1);
                }
                't' => {
                    result.emit_unchanged(i - 1);
                    result.emit_static("\t", i + 1);
                }
                c @ ('u' | 'U' | 'l' | 'L') => {
                    result.emit_unchanged(i - 1);
                    result.emit_static("", i + 1);
                    case_ops.push(c);
                }
                _ => {}
            }
            i += 1;
            continue;
        }

        if ch == '$' {
            i += 1;
            if i >= len {
                break; // string ends with a dollar
            }
            let next = chars[i];
            if next == '$' {
                result.emit_unchanged(i - 1);
                result.emit_static("$", i + 1);
                i += 1;
                continue;
            }
            if next == '0' || next == '&' {
                result.emit_unchanged(i - 1);
                result.emit_match_index(0, i + 1, &case_ops);
                case_ops.clear();
                i += 1;
                continue;
            }
            if ('1'..='9').contains(&next) {
                let mut match_index = next as usize - '0' as usize;
                if i + 1 < len && chars[i + 1].is_ascii_digit() {
                    // $nn
                    i += 1;
                    match_index = match_index * 10 + (chars[i] as usize - '0' as usize);
                    result.emit_unchanged(i - 2);
                    result.emit_match_index(match_index, i + 1, &case_ops);
                    case_ops.clear();
                    i += 1;
                    continue;
                }
                result.emit_unchanged(i - 1);
                result.emit_match_index(match_index, i + 1, &case_ops);
                case_ops.clear();
                i += 1;
                continue;
            }
        }
        i += 1;
    }
    result.finalize()
}

// ---- Preserve Case (base/common/search.ts) -------------------------------------------------

fn contains_uppercase_character(c: char) -> bool {
    c.to_lowercase().ne(std::iter::once(c))
}

fn validate_specific_special_character(matches: &[String], pattern: &str, special: char) -> bool {
    let m0 = &matches[0];
    let contains = m0.contains(special) && pattern.contains(special);
    contains && m0.split(special).count() == pattern.split(special).count()
}

fn build_replace_string_for_special_character(matches: &[String], pattern: &str, special: char) -> String {
    let split_pattern: Vec<&str> = pattern.split(special).collect();
    let split_match: Vec<&str> = matches[0].split(special).collect();
    let mut out = String::new();
    for (index, value) in split_pattern.iter().enumerate() {
        let m = vec![split_match.get(index).copied().unwrap_or_default().to_string()];
        out.push_str(&build_replace_string_with_case_preserved(Some(&m), value));
        out.push(special);
    }
    out.pop();
    out
}

fn upper_first(s: &str) -> String {
    let mut it = s.chars();
    it.next().map_or_else(String::new, |c| c.to_uppercase().chain(it).collect())
}

fn lower_first(s: &str) -> String {
    let mut it = s.chars();
    it.next().map_or_else(String::new, |c| c.to_lowercase().chain(it).collect())
}

/// `buildReplaceStringWithCasePreserved`: give `pattern` the case shape of the match
/// (all upper, all lower, capitalised, and `-` / `_` separated words one by one).
pub fn build_replace_string_with_case_preserved(matches: Option<&[String]>, pattern: &str) -> String {
    let Some(matches) = matches.filter(|m| m.first().is_some_and(|s| !s.is_empty())) else {
        return pattern.to_string();
    };
    let hyphens = validate_specific_special_character(matches, pattern, '-');
    let underscores = validate_specific_special_character(matches, pattern, '_');
    if hyphens && !underscores {
        return build_replace_string_for_special_character(matches, pattern, '-');
    } else if !hyphens && underscores {
        return build_replace_string_for_special_character(matches, pattern, '_');
    }
    let m0 = &matches[0];
    let first = m0.chars().next().unwrap_or(' ');
    if m0.to_uppercase() == *m0 {
        pattern.to_uppercase()
    } else if m0.to_lowercase() == *m0 {
        pattern.to_lowercase()
    } else if contains_uppercase_character(first) && !pattern.is_empty() {
        upper_first(pattern)
    } else if first.to_uppercase().ne(std::iter::once(first)) && !pattern.is_empty() {
        lower_first(pattern)
    } else {
        pattern.to_string()
    }
}
