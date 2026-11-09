//! Context key expressions (`when` clauses and command preconditions).
//!
//! upstream: src/vs/platform/contextkey/common/contextkey.ts @ 50e1350fc78d
//!
//! A deliberately small subset of `ContextKeyExpr`: `key`, `!key`, `key == value`,
//! `key != value`, `&&`, `||` and parentheses. Not supported: `=~`, `in`, `<`, `>`, `^=`.
//! A key that is not set is falsy and equals nothing.

use std::collections::HashMap;
use std::fmt;

/// A context key value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Str(String),
}

impl Value {
    fn truthy(&self) -> bool {
        match self {
            Value::Bool(b) => *b,
            Value::Str(s) => !s.is_empty(),
        }
    }

    fn as_text(&self) -> String {
        match self {
            Value::Bool(b) => b.to_string(),
            Value::Str(s) => s.clone(),
        }
    }
}

/// The context keys in effect (`IContextKeyService`, flattened).
#[derive(Clone, Debug, Default)]
pub struct ContextKeys(HashMap<String, Value>);

impl ContextKeys {
    pub fn set_bool(&mut self, key: &str, value: bool) -> &mut Self {
        self.0.insert(key.to_string(), Value::Bool(value));
        self
    }

    pub fn set_str(&mut self, key: &str, value: &str) -> &mut Self {
        self.0.insert(key.to_string(), Value::Str(value.to_string()));
        self
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0.get(key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum When {
    /// `key`: truthy.
    Has(String),
    /// `!key`, or `!(...)`.
    Not(Box<When>),
    /// `key == value`.
    Equals(String, String),
    /// `key != value`.
    NotEquals(String, String),
    And(Vec<When>),
    Or(Vec<When>),
}

impl When {
    /// Parse an expression. An empty string is an error (use `None` for "no condition").
    pub fn parse(src: &str) -> Result<When, String> {
        let tokens = tokenize(src)?;
        let mut p = Parser { tokens, pos: 0 };
        let e = p.or()?;
        if p.pos != p.tokens.len() {
            return Err(format!("unexpected `{}`", p.tokens[p.pos].text()));
        }
        Ok(e)
    }

    pub fn evaluate(&self, ctx: &ContextKeys) -> bool {
        match self {
            When::Has(k) => ctx.get(k).is_some_and(Value::truthy),
            When::Not(e) => !e.evaluate(ctx),
            When::Equals(k, v) => ctx.get(k).is_some_and(|x| x.as_text() == *v),
            // Upstream: `key != value` is also true when the key is not set.
            When::NotEquals(k, v) => ctx.get(k).is_none_or(|x| x.as_text() != *v),
            When::And(es) => es.iter().all(|e| e.evaluate(ctx)),
            When::Or(es) => es.iter().any(|e| e.evaluate(ctx)),
        }
    }

    /// Whether the expression reads `key`.
    pub fn mentions(&self, key: &str) -> bool {
        match self {
            When::Has(k) | When::Equals(k, _) | When::NotEquals(k, _) => k == key,
            When::Not(e) => e.mentions(key),
            When::And(es) | When::Or(es) => es.iter().any(|e| e.mentions(key)),
        }
    }
}

impl fmt::Display for When {
    /// `ContextKeyExpression.serialize()`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            When::Has(k) => write!(f, "{k}"),
            When::Not(e) => match **e {
                When::Has(ref k) => write!(f, "!{k}"),
                ref other => write!(f, "!({other})"),
            },
            When::Equals(k, v) => write!(f, "{k} == '{v}'"),
            When::NotEquals(k, v) => write!(f, "{k} != '{v}'"),
            When::And(es) => join(f, es, " && ", |e| matches!(e, When::Or(_))),
            When::Or(es) => join(f, es, " || ", |_| false),
        }
    }
}

fn join(f: &mut fmt::Formatter<'_>, es: &[When], sep: &str, paren: impl Fn(&When) -> bool) -> fmt::Result {
    for (i, e) in es.iter().enumerate() {
        if i > 0 {
            f.write_str(sep)?;
        }
        if paren(e) {
            write!(f, "({e})")?;
        } else {
            write!(f, "{e}")?;
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Not,
    And,
    Or,
    Eq,
    Ne,
    Open,
    Close,
}

impl Tok {
    fn text(&self) -> String {
        match self {
            Tok::Word(w) => w.clone(),
            Tok::Not => "!".into(),
            Tok::And => "&&".into(),
            Tok::Or => "||".into(),
            Tok::Eq => "==".into(),
            Tok::Ne => "!=".into(),
            Tok::Open => "(".into(),
            Tok::Close => ")".into(),
        }
    }
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || "_.-:/".contains(c)
}

fn tokenize(src: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let next = chars.get(i + 1).copied();
        match c {
            ' ' | '\t' => i += 1,
            '(' => {
                out.push(Tok::Open);
                i += 1;
            }
            ')' => {
                out.push(Tok::Close);
                i += 1;
            }
            '&' if next == Some('&') => {
                out.push(Tok::And);
                i += 2;
            }
            '|' if next == Some('|') => {
                out.push(Tok::Or);
                i += 2;
            }
            '=' if next == Some('=') => {
                out.push(Tok::Eq);
                i += 2;
            }
            '!' if next == Some('=') => {
                out.push(Tok::Ne);
                i += 2;
            }
            '!' => {
                out.push(Tok::Not);
                i += 1;
            }
            '\'' | '"' => {
                let start = i + 1;
                let mut j = start;
                while j < chars.len() && chars[j] != c {
                    j += 1;
                }
                if j == chars.len() {
                    return Err("unterminated string".into());
                }
                out.push(Tok::Word(chars[start..j].iter().collect()));
                i = j + 1;
            }
            c if is_word_char(c) => {
                let start = i;
                while i < chars.len() && is_word_char(chars[i]) {
                    i += 1;
                }
                out.push(Tok::Word(chars[start..i].iter().collect()));
            }
            other => return Err(format!("unexpected character `{other}`")),
        }
    }
    Ok(out)
}

struct Parser {
    tokens: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.tokens.get(self.pos)
    }

    fn or(&mut self) -> Result<When, String> {
        let mut parts = vec![self.and()?];
        while self.peek() == Some(&Tok::Or) {
            self.pos += 1;
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 { parts.remove(0) } else { When::Or(parts) })
    }

    fn and(&mut self) -> Result<When, String> {
        let mut parts = vec![self.unary()?];
        while self.peek() == Some(&Tok::And) {
            self.pos += 1;
            parts.push(self.unary()?);
        }
        Ok(if parts.len() == 1 { parts.remove(0) } else { When::And(parts) })
    }

    fn unary(&mut self) -> Result<When, String> {
        match self.peek().cloned() {
            Some(Tok::Not) => {
                self.pos += 1;
                Ok(When::Not(Box::new(self.unary()?)))
            }
            Some(Tok::Open) => {
                self.pos += 1;
                let e = self.or()?;
                if self.peek() != Some(&Tok::Close) {
                    return Err("missing `)`".into());
                }
                self.pos += 1;
                Ok(e)
            }
            Some(Tok::Word(key)) => {
                self.pos += 1;
                let eq = match self.peek() {
                    Some(Tok::Eq) => true,
                    Some(Tok::Ne) => false,
                    _ => return Ok(When::Has(key)),
                };
                self.pos += 1;
                match self.peek().cloned() {
                    Some(Tok::Word(v)) => {
                        self.pos += 1;
                        Ok(if eq { When::Equals(key, v) } else { When::NotEquals(key, v) })
                    }
                    _ => Err("expected a value after the comparison".into()),
                }
            }
            Some(t) => Err(format!("unexpected `{}`", t.text())),
            None => Err("unexpected end of expression".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ContextKeys {
        let mut c = ContextKeys::default();
        c.set_bool("editorFocus", true).set_bool("sideBarVisible", false).set_str("activeView", "explorer");
        c
    }

    fn eval(src: &str) -> bool {
        When::parse(src).unwrap().evaluate(&ctx())
    }

    #[test]
    fn keys_and_negation() {
        assert!(eval("editorFocus"));
        assert!(!eval("sideBarVisible"));
        assert!(eval("!sideBarVisible"));
        assert!(!eval("unknownKey"));
        assert!(eval("!unknownKey"));
    }

    #[test]
    fn and_or_and_parentheses() {
        assert!(eval("editorFocus && !sideBarVisible"));
        assert!(!eval("editorFocus && sideBarVisible"));
        assert!(eval("sideBarVisible || editorFocus"));
        assert!(eval("(sideBarVisible || editorFocus) && !sideBarVisible"));
        assert!(!eval("sideBarVisible || (editorFocus && sideBarVisible)"));
    }

    #[test]
    fn comparisons() {
        assert!(eval("activeView == explorer"));
        assert!(eval("activeView == 'explorer'"));
        assert!(!eval("activeView == search"));
        assert!(eval("activeView != search"));
        assert!(eval("missing != search"), "an unset key differs from any value");
        assert!(!eval("missing == search"));
        assert!(eval("editorFocus == true"));
    }

    #[test]
    fn serialize_round_trips() {
        for src in ["editorFocus", "!sideBarVisible", "a && b", "a || b", "(a || b) && c", "k == 'v'", "k != 'v'"] {
            let e = When::parse(src).unwrap();
            assert_eq!(e.to_string(), src);
            assert_eq!(When::parse(&e.to_string()).unwrap(), e);
        }
    }

    #[test]
    fn errors() {
        assert!(When::parse("").is_err());
        assert!(When::parse("a &&").is_err());
        assert!(When::parse("(a").is_err());
        assert!(When::parse("a b").is_err());
        assert!(When::parse("a ==").is_err());
        assert!(When::parse("a # b").is_err());
    }

    #[test]
    fn mentions_keys() {
        let e = When::parse("a && !(b || c == d)").unwrap();
        assert!(e.mentions("a") && e.mentions("b") && e.mentions("c"));
        assert!(!e.mentions("d"));
    }
}
