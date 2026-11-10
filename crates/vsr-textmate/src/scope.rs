//! Scope stacks (interned), scope selector matchers, and standard token types.
//!
//! upstream: src/vs/workbench/services/textMate/common/TMScopeRegistry.ts @ 50e1350fc78d
//! upstream: vscode-textmate src/matcher.ts (microsoft/vscode-textmate, main, 2026-11-09)
//! upstream: vscode-textmate src/grammar/grammar.ts (`nameMatcher`, `AttributedScopeStack`)
//!
//! A scope stack is an id into an interner: `ScopeId(0)` is the empty stack and every other id is
//! "parent id + one scope name". Tokens carry one `ScopeId` each, which keeps lines small and lets
//! styles be resolved (and cached) per id instead of per token.

use std::collections::HashMap;
use std::sync::Arc;

pub type ScopeId = u32;

/// upstream `StandardTokenType` (the ones that matter to editing: comment, string, regex).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TokenType {
    #[default]
    Other,
    Comment,
    String,
    RegEx,
}

impl TokenType {
    pub fn parse(s: &str) -> Option<TokenType> {
        match s {
            "other" => Some(TokenType::Other),
            "comment" => Some(TokenType::Comment),
            "string" => Some(TokenType::String),
            "regex" => Some(TokenType::RegEx),
            _ => None,
        }
    }
}

/// vscode's default: the first of `comment`, `string`, `regex`, `regexp` found as a whole word in
/// a scope name (`\b(comment|string|regex|regexp)\b`).
fn standard_type_of(scope: &str) -> Option<TokenType> {
    let bytes = scope.as_bytes();
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let mut best: Option<(usize, TokenType)> = None;
    for (word, ty) in [("comment", TokenType::Comment), ("string", TokenType::String), ("regexp", TokenType::RegEx), ("regex", TokenType::RegEx)] {
        let mut from = 0;
        while let Some(i) = scope[from..].find(word) {
            let start = from + i;
            let end = start + word.len();
            let before_ok = start == 0 || !is_word(bytes[start - 1]);
            let after_ok = end == bytes.len() || !is_word(bytes[end]);
            if before_ok && after_ok {
                if best.is_none_or(|(p, _)| start < p) {
                    best = Some((start, ty));
                }
                break;
            }
            from = start + 1;
        }
    }
    best.map(|(_, t)| t)
}

struct Node {
    parent: ScopeId,
    name: Arc<str>,
    /// Scope names from the root to this node.
    names: Arc<Vec<Arc<str>>>,
    token_type: TokenType,
}

/// Interns scope stacks. One per grammar instance, since the token type selectors of the root
/// grammar are part of how a node is built.
pub struct ScopeInterner {
    nodes: Vec<Node>,
    /// Per node, its children by name.
    children: Vec<HashMap<Arc<str>, ScopeId>>,
    token_types: Vec<(Selector, TokenType)>,
}

impl ScopeInterner {
    pub fn new(token_types: Vec<(Selector, TokenType)>) -> Self {
        let empty = Node { parent: 0, name: "".into(), names: Arc::new(Vec::new()), token_type: TokenType::Other };
        Self { nodes: vec![empty], children: vec![HashMap::new()], token_types }
    }

    fn push_one(&mut self, parent: ScopeId, name: &str) -> ScopeId {
        if let Some(&id) = self.children[parent as usize].get(name) {
            return id;
        }
        let name: Arc<str> = Arc::from(name);
        let p = &self.nodes[parent as usize];
        let mut names = (*p.names).clone();
        names.push(name.clone());
        let mut token_type = p.token_type;
        if let Some(t) = standard_type_of(&name) {
            token_type = t;
        }
        if !self.token_types.is_empty() {
            let refs: Vec<&str> = names.iter().map(|s| &**s).collect();
            for (sel, ty) in &self.token_types {
                if sel.matches(&refs) {
                    token_type = *ty;
                }
            }
        }
        let id = self.nodes.len() as ScopeId;
        self.nodes.push(Node { parent, name: name.clone(), names: Arc::new(names), token_type });
        self.children.push(HashMap::new());
        self.children[parent as usize].insert(name, id);
        id
    }

    /// `pushAttributed`: a name with spaces pushes each of its scopes.
    pub fn push(&mut self, parent: ScopeId, name: &str) -> ScopeId {
        if !name.contains(' ') {
            return self.push_one(parent, name);
        }
        let mut cur = parent;
        for part in name.split(' ') {
            cur = self.push_one(cur, part);
        }
        cur
    }

    pub fn parent(&self, id: ScopeId) -> ScopeId {
        self.nodes[id as usize].parent
    }

    pub fn leaf_name(&self, id: ScopeId) -> &str {
        &self.nodes[id as usize].name
    }

    pub fn names(&self, id: ScopeId) -> Arc<Vec<Arc<str>>> {
        self.nodes[id as usize].names.clone()
    }

    pub fn token_type(&self, id: ScopeId) -> TokenType {
        self.nodes[id as usize].token_type
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.len() <= 1
    }
}

// --- Selectors (matcher.ts) ---

/// A parsed scope selector expression (`text.html - comment`, `a | b`, `(a, b) c`).
#[derive(Debug, Clone)]
pub enum Selector {
    /// All identifiers match in order, each a scope or a scope prefix (`source.js`, `string`).
    Names(Vec<String>),
    Not(Box<Selector>),
    And(Vec<Selector>),
    Or(Vec<Selector>),
    /// Matches nothing (a missing operand).
    Never,
}

impl Selector {
    pub fn matches(&self, scopes: &[&str]) -> bool {
        self.matches_by(scopes, &name_matcher)
    }

    /// Like `matches`, with the identifier matching supplied (upstream's `matchesName`).
    pub fn matches_by(&self, scopes: &[&str], names: &dyn Fn(&[String], &[&str]) -> bool) -> bool {
        match self {
            Selector::Names(ids) => names(ids, scopes),
            Selector::Not(inner) => !inner.matches_by(scopes, names),
            Selector::And(all) => all.iter().all(|m| m.matches_by(scopes, names)),
            Selector::Or(any) => any.iter().any(|m| m.matches_by(scopes, names)),
            Selector::Never => false,
        }
    }
}

/// upstream `nameMatcher`.
fn name_matcher(identifiers: &[String], scopes: &[&str]) -> bool {
    if scopes.len() < identifiers.len() {
        return false;
    }
    let mut last = 0;
    identifiers.iter().all(|identifier| {
        for (i, scope) in scopes.iter().enumerate().skip(last) {
            if scopes_are_matching(scope, identifier) {
                last = i + 1;
                return true;
            }
        }
        false
    })
}

fn scopes_are_matching(this_scope: &str, scope: &str) -> bool {
    if this_scope.is_empty() {
        return false;
    }
    if this_scope == scope {
        return true;
    }
    this_scope.len() > scope.len() && this_scope.starts_with(scope) && this_scope.as_bytes()[scope.len()] == b'.'
}

/// A selector with its injection priority (`L:` = -1, `R:` = 1, else 0).
#[derive(Debug, Clone)]
pub struct PrioritizedSelector {
    pub selector: Selector,
    pub priority: i32,
}

/// The tokens of `matcher.ts`'s tokenizer: `([LR]:|[\w\.:][\w\.:\-]*|[\,\|\-\(\)])`.
fn tokenize_selector(s: &str) -> Vec<String> {
    let b: Vec<char> = s.chars().collect();
    let is_word = |c: char| c.is_alphanumeric() || c == '_' || c == '.' || c == ':';
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if (c == 'L' || c == 'R') && b.get(i + 1) == Some(&':') {
            out.push(format!("{c}:"));
            i += 2;
        } else if is_word(c) {
            let start = i;
            i += 1;
            while i < b.len() && (is_word(b[i]) || b[i] == '-') {
                i += 1;
            }
            out.push(b[start..i].iter().collect());
        } else if matches!(c, ',' | '|' | '-' | '(' | ')') {
            out.push(c.to_string());
            i += 1;
        } else {
            i += 1;
        }
    }
    out
}

fn is_identifier(t: &str) -> bool {
    t.chars().next().is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == ':') && !matches!(t, "L:" | "R:")
}

/// upstream `createMatchers`.
pub fn create_matchers(selector: &str) -> Vec<PrioritizedSelector> {
    struct P {
        toks: Vec<String>,
        pos: usize,
    }
    impl P {
        fn cur(&self) -> Option<&str> {
            self.toks.get(self.pos).map(String::as_str)
        }
        fn next(&mut self) {
            self.pos += 1;
        }
        fn operand(&mut self) -> Option<Selector> {
            match self.cur()? {
                "-" => {
                    self.next();
                    let inner = self.operand();
                    Some(Selector::Not(Box::new(inner.unwrap_or(Selector::Never))))
                }
                "(" => {
                    self.next();
                    let inner = self.inner_expression();
                    if self.cur() == Some(")") {
                        self.next();
                    }
                    Some(inner)
                }
                t if is_identifier(t) => {
                    let mut ids = Vec::new();
                    while let Some(t) = self.cur().filter(|t| is_identifier(t)) {
                        ids.push(t.to_string());
                        self.next();
                    }
                    Some(Selector::Names(ids))
                }
                _ => None,
            }
        }
        fn conjunction(&mut self) -> Selector {
            let mut all = Vec::new();
            while let Some(m) = self.operand() {
                all.push(m);
            }
            Selector::And(all)
        }
        fn inner_expression(&mut self) -> Selector {
            let mut any = Vec::new();
            loop {
                // note: upstream loops on `while (matcher = parseConjunction())`, where an empty conjunction is a
                // matcher too (it matches everything); the loop ends at a missing separator.
                let m = self.conjunction();
                any.push(m);
                if matches!(self.cur(), Some("|") | Some(",")) {
                    while matches!(self.cur(), Some("|") | Some(",")) {
                        self.next();
                    }
                } else {
                    break;
                }
            }
            Selector::Or(any)
        }
    }
    let mut p = P { toks: tokenize_selector(selector), pos: 0 };
    let mut results = Vec::new();
    while p.cur().is_some() {
        let mut priority = 0;
        if let Some(t) = p.cur()
            && t.len() == 2
            && t.ends_with(':')
        {
            priority = if t.starts_with('R') { 1 } else { -1 };
            p.next();
        }
        let selector = p.conjunction();
        results.push(PrioritizedSelector { selector, priority });
        if p.cur() != Some(",") {
            break;
        }
        p.next();
    }
    results
}

/// A selector as one matcher (all comma separated alternatives), for token type selectors.
pub fn parse_selector(selector: &str) -> Selector {
    Selector::Or(create_matchers(selector).into_iter().map(|m| m.selector).collect())
}
