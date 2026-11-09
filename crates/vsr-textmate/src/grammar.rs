//! The tokenizer: rule stack state, `tokenize_line`, captures, `while` rules, injections.
//!
//! upstream: vscode-textmate src/grammar/grammar.ts and src/grammar/tokenizeString.ts
//! (microsoft/vscode-textmate, main, 2026-11-09)
//!
//! Positions are UTF-8 byte offsets into the line (with the `\n` vscode-textmate appends). A
//! token is `(start, scope id)`; it ends where the next one starts, the last at the end of the
//! line. The theme is not part of tokenizing: styles are resolved from scope ids afterwards, so a
//! theme change does not re-tokenize.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::raw::RawGrammar;
use crate::regex::{RegExpSource, ScanCache, ScanMatch, Scanner};
use crate::rules::{Builder, Captures, END_RULE_ID, GrammarSource, Rule, RuleId, RuleKind, WHILE_RULE_ID};
use crate::scope::{ScopeId, ScopeInterner, Selector, TokenType, parse_selector};

/// One token: from `start` (byte offset) to the next token's start.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Token {
    pub start: u32,
    pub scope: ScopeId,
}

#[derive(Debug)]
struct Frame {
    parent: Option<Arc<Frame>>,
    rule_id: RuleId,
    enter_pos: i64,
    anchor_pos: i64,
    begin_rule_captured_eol: bool,
    end_rule: Option<Arc<str>>,
    name_scopes: ScopeId,
    content_scopes: ScopeId,
    depth: u32,
}

/// The rule stack at a line boundary (upstream `StateStack`). Cheap to clone.
#[derive(Debug, Clone, Default)]
pub struct StateStack(Option<Arc<Frame>>);

impl PartialEq for StateStack {
    fn eq(&self, other: &Self) -> bool {
        frames_equal(&self.0, &other.0)
    }
}
impl Eq for StateStack {}

fn frames_equal(a: &Option<Arc<Frame>>, b: &Option<Arc<Frame>>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => {
            if Arc::ptr_eq(x, y) {
                return true;
            }
            x.depth == y.depth
                && x.rule_id == y.rule_id
                && x.end_rule == y.end_rule
                && x.name_scopes == y.name_scopes
                && x.content_scopes == y.content_scopes
                && frames_equal(&x.parent, &y.parent)
        }
        _ => false,
    }
}

impl StateStack {
    /// The state before the first line.
    pub fn initial() -> Self {
        Self(None)
    }

    pub fn is_initial(&self) -> bool {
        self.0.is_none()
    }

    fn frame(&self) -> &Frame {
        self.0.as_ref().expect("a non-initial state")
    }

    fn rule_id(&self) -> RuleId {
        self.frame().rule_id
    }

    fn content_scopes(&self) -> ScopeId {
        self.frame().content_scopes
    }

    fn name_scopes(&self) -> ScopeId {
        self.frame().name_scopes
    }

    #[allow(clippy::too_many_arguments)]
    fn push(&self, rule_id: RuleId, enter_pos: i64, anchor_pos: i64, eol: bool, end_rule: Option<Arc<str>>, name: ScopeId, content: ScopeId) -> StateStack {
        StateStack(Some(Arc::new(Frame {
            parent: self.0.clone(),
            rule_id,
            enter_pos,
            anchor_pos,
            begin_rule_captured_eol: eol,
            end_rule,
            name_scopes: name,
            content_scopes: content,
            depth: self.0.as_ref().map_or(1, |f| f.depth + 1),
        })))
    }

    fn pop(&self) -> StateStack {
        StateStack(self.frame().parent.clone())
    }

    fn safe_pop(&self) -> StateStack {
        match &self.frame().parent {
            Some(p) => StateStack(Some(p.clone())),
            None => self.clone(),
        }
    }

    fn with_content_scopes(&self, content: ScopeId) -> StateStack {
        let f = self.frame();
        if f.content_scopes == content {
            return self.clone();
        }
        StateStack(Some(Arc::new(Frame { parent: f.parent.clone(), content_scopes: content, end_rule: f.end_rule.clone(), ..*f })))
    }

    fn with_end_rule(&self, end_rule: String) -> StateStack {
        let f = self.frame();
        StateStack(Some(Arc::new(Frame { parent: f.parent.clone(), end_rule: Some(end_rule.into()), ..*f })))
    }

    /// upstream `hasSameRuleAs`.
    fn has_same_rule_as(&self, other: &StateStack) -> bool {
        let mut el = self.0.as_ref();
        let other_enter = other.frame().enter_pos;
        while let Some(f) = el {
            if f.enter_pos != other_enter {
                break;
            }
            if f.rule_id == other.rule_id() {
                return true;
            }
            el = f.parent.as_ref();
        }
        false
    }

    /// Positions are only meaningful within a line: reset them for the next one.
    fn reset(&self) -> StateStack {
        fn go(f: &Option<Arc<Frame>>) -> Option<Arc<Frame>> {
            let arc = f.as_ref()?;
            let f: &Frame = arc;
            if f.enter_pos == -1 && f.anchor_pos == -1 && f.parent.is_none() {
                return Some(arc.clone());
            }
            Some(Arc::new(Frame {
                parent: go(&f.parent),
                enter_pos: -1,
                anchor_pos: -1,
                end_rule: f.end_rule.clone(),
                ..*f
            }))
        }
        StateStack(go(&self.0))
    }

    /// The scope stack of the innermost rule's content (what a token at this point carries).
    pub fn content_scope(&self) -> ScopeId {
        self.0.as_ref().map_or(0, |f| f.content_scopes)
    }
}

struct Injection {
    selector: Selector,
    priority: i32,
    rule_id: RuleId,
}

/// Which of the four anchor variants a scanner was built for, and its end/while source.
#[derive(Clone, PartialEq, Eq, Hash)]
struct ScannerKey {
    rule: RuleId,
    /// `Some(end or while source)` when it has back references.
    dynamic: Option<Arc<str>>,
    while_scanner: bool,
    allow_a: bool,
    allow_g: bool,
}

pub struct Grammar {
    scope_name: String,
    rules: Vec<Rule>,
    root_id: RuleId,
    injections: Vec<Injection>,
    scopes: Mutex<ScopeInterner>,
    scanners: Mutex<HashMap<ScannerKey, Arc<Scanner>>>,
}

/// The result of tokenizing a line.
#[derive(Debug, Clone)]
pub struct LineTokens {
    pub tokens: Vec<Token>,
    pub end_state: StateStack,
}

impl Grammar {
    /// Builds the grammar `scope_name` from `source`. `token_types` are the grammar's
    /// `tokenTypes` contribution (selector to `comment` / `string` / `regex` / `other`).
    pub fn new(source: &dyn GrammarSource, scope_name: &str, token_types: &[(String, String)]) -> Result<Grammar, String> {
        let raw: Arc<RawGrammar> = source.grammar(scope_name).ok_or_else(|| format!("no grammar for {scope_name}"))?;
        let mut builder = Builder::new(source);
        let root_id = builder.compile_root(&raw);
        let injections = builder
            .compile_injections(&raw)
            .into_iter()
            .map(|(selector, priority, rule_id)| Injection { selector, priority, rule_id })
            .collect();
        let rules: Vec<Rule> = builder.rules.into_iter().map(|r| r.expect("every allocated rule is built")).collect();
        let types = token_types
            .iter()
            .filter_map(|(sel, ty)| TokenType::parse(ty).map(|t| (parse_selector(sel), t)))
            .collect();
        Ok(Grammar {
            scope_name: raw.scope_name.clone(),
            rules,
            root_id,
            injections,
            scopes: Mutex::new(ScopeInterner::new(types)),
            scanners: Mutex::new(HashMap::new()),
        })
    }

    pub fn scope_name(&self) -> &str {
        &self.scope_name
    }

    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }

    /// Run `f` with the scope interner (to resolve ids to names, parents and token types).
    pub fn with_scopes<R>(&self, f: impl FnOnce(&mut ScopeInterner) -> R) -> R {
        f(&mut self.scopes.lock().expect("scope interner lock"))
    }

    pub fn scope_names(&self, id: ScopeId) -> Arc<Vec<Arc<str>>> {
        self.with_scopes(|s| s.names(id))
    }

    pub fn token_type(&self, id: ScopeId) -> TokenType {
        self.with_scopes(|s| s.token_type(id))
    }

    fn rule(&self, id: RuleId) -> &Rule {
        &self.rules[id as usize]
    }

    fn push_scope(&self, parent: ScopeId, name: Option<&str>) -> ScopeId {
        match name {
            Some(n) => self.scopes.lock().expect("scope interner lock").push(parent, n),
            None => parent,
        }
    }

    /// The patterns of a rule as `(source, rule id)` for the anchor permissions.
    fn collect_patterns(&self, id: RuleId, allow_a: bool, allow_g: bool, out: &mut Vec<(String, RuleId)>, depth: u32) {
        if depth > 64 {
            return;
        }
        let rule = self.rule(id);
        match &rule.kind {
            RuleKind::Match { re, .. } => out.push((re.resolve_anchors(allow_a, allow_g).to_string(), id)),
            RuleKind::Include { patterns, .. } => {
                for p in patterns {
                    self.collect_patterns(*p, allow_a, allow_g, out, depth + 1);
                }
            }
            RuleKind::BeginEnd { begin, .. } | RuleKind::BeginWhile { begin, .. } => {
                out.push((begin.resolve_anchors(allow_a, allow_g).to_string(), id))
            }
        }
    }

    fn scanner(&self, rule_id: RuleId, end_source: Option<&str>, while_scanner: bool, allow_a: bool, allow_g: bool) -> Arc<Scanner> {
        let rule = self.rule(rule_id);
        // The end (or while) pattern with back references is resolved per stack frame.
        let dynamic = match (&rule.kind, while_scanner) {
            (RuleKind::BeginEnd { end, .. }, false) if end.has_back_references => end_source.map(Arc::from),
            (RuleKind::BeginWhile { while_, .. }, true) if while_.has_back_references => end_source.map(Arc::from),
            _ => None,
        };
        let key = ScannerKey { rule: rule_id, dynamic: dynamic.clone(), while_scanner, allow_a, allow_g };
        if let Some(s) = self.scanners.lock().expect("scanner cache lock").get(&key) {
            return s.clone();
        }
        let mut sources: Vec<(String, RuleId)> = Vec::new();
        match (&rule.kind, while_scanner) {
            (RuleKind::BeginWhile { while_, .. }, true) => {
                let src = match &dynamic {
                    Some(d) => while_.with_source(d).resolve_anchors(allow_a, allow_g).to_string(),
                    None => while_.resolve_anchors(allow_a, allow_g).to_string(),
                };
                sources.push((src, WHILE_RULE_ID));
            }
            (RuleKind::BeginEnd { end, apply_end_pattern_last, patterns, .. }, _) => {
                let end_src = match &dynamic {
                    Some(d) => end.with_source(d).resolve_anchors(allow_a, allow_g).to_string(),
                    None => end.resolve_anchors(allow_a, allow_g).to_string(),
                };
                let mut inner = Vec::new();
                for p in patterns {
                    self.collect_patterns(*p, allow_a, allow_g, &mut inner, 0);
                }
                if *apply_end_pattern_last {
                    sources.extend(inner);
                    sources.push((end_src, END_RULE_ID));
                } else {
                    sources.push((end_src, END_RULE_ID));
                    sources.extend(inner);
                }
            }
            (RuleKind::BeginWhile { patterns, .. }, false) | (RuleKind::Include { patterns, .. }, _) => {
                for p in patterns {
                    self.collect_patterns(*p, allow_a, allow_g, &mut sources, 0);
                }
            }
            (RuleKind::Match { re, .. }, _) => sources.push((re.resolve_anchors(allow_a, allow_g).to_string(), rule_id)),
        }
        let scanner = Arc::new(Scanner::new(&sources));
        let mut cache = self.scanners.lock().expect("scanner cache lock");
        if cache.len() > 8192 {
            cache.clear();
        }
        cache.insert(key, scanner.clone());
        scanner
    }

    /// Tokenizes one line (without its terminator). `prev` is the state after the previous line,
    /// `StateStack::initial()` for the first.
    pub fn tokenize_line(&self, line: &str, prev: &StateStack) -> LineTokens {
        let is_first = prev.is_initial();
        let mut stack = if is_first {
            let root = self.push_scope(0, self.rule(self.root_id).name.as_ref().map(|n| n.text.as_str()));
            StateStack::initial().push(self.root_id, -1, -1, false, None, root, root)
        } else {
            prev.clone()
        };
        let mut text = String::with_capacity(line.len() + 1);
        text.push_str(line);
        text.push('\n');
        let mut tokens = Tokens { out: Vec::new(), last_end: 0 };
        stack = self.tokenize_string(&text, is_first, 0, stack, &mut tokens, true);
        // The token that is only the newline goes away; an empty line keeps one empty token.
        let len = text.len();
        if tokens.out.last().is_some_and(|t| t.start as usize == len - 1) {
            tokens.out.pop();
        }
        if tokens.out.is_empty() {
            tokens.out.push(Token { start: 0, scope: stack.content_scopes() });
        }
        LineTokens { tokens: tokens.out, end_state: stack.reset() }
    }

    fn tokenize_string(&self, text: &str, mut is_first: bool, mut pos: usize, mut stack: StateStack, tokens: &mut Tokens, check_while: bool) -> StateStack {
        let line_len = text.len();
        let mut anchor_pos: i64 = -1;
        let mut cache = ScanCache::default();
        if check_while {
            let r = self.check_while_conditions(text, is_first, pos, stack, tokens, &mut cache);
            stack = r.0;
            pos = r.1;
            is_first = r.2;
            anchor_pos = r.3;
        }
        loop {
            let Some((caps, matched)) = self.match_rule_or_injections(text, is_first, pos, &stack, anchor_pos, &mut cache) else {
                tokens.produce(stack.content_scopes(), line_len);
                break;
            };
            let has_advanced = caps[0].1 > pos;
            if matched == END_RULE_ID {
                // The end of the current rule: pop it.
                let popped_rule = self.rule(stack.rule_id());
                tokens.produce(stack.content_scopes(), caps[0].0);
                stack = stack.with_content_scopes(stack.name_scopes());
                if let RuleKind::BeginEnd { end_captures, .. } = &popped_rule.kind {
                    self.handle_captures(text, is_first, &stack, tokens, end_captures, &caps);
                }
                tokens.produce(stack.content_scopes(), caps[0].1);
                let popped = stack.clone();
                stack = stack.pop();
                anchor_pos = popped.frame().anchor_pos;
                if !has_advanced && popped.frame().enter_pos == pos as i64 {
                    // The grammar pushed and popped a rule without advancing: stop, or it loops.
                    stack = popped;
                    tokens.produce(stack.content_scopes(), line_len);
                    break;
                }
            } else {
                let rule = self.rule(matched);
                tokens.produce(stack.content_scopes(), caps[0].0);
                let before_push = stack.clone();
                let name = rule.name.as_ref().map(|n| n.resolve(text, Some(&caps)));
                let name_scopes = self.push_scope(stack.content_scopes(), name.as_deref());
                stack = stack.push(matched, pos as i64, anchor_pos, caps[0].1 == line_len, None, name_scopes, name_scopes);
                match &rule.kind {
                    RuleKind::BeginEnd { end, begin_captures, .. } => {
                        self.handle_captures(text, is_first, &stack, tokens, begin_captures, &caps);
                        tokens.produce(stack.content_scopes(), caps[0].1);
                        anchor_pos = caps[0].1 as i64;
                        let content = rule.content_name.as_ref().map(|n| n.resolve(text, Some(&caps)));
                        let content_scopes = self.push_scope(name_scopes, content.as_deref());
                        stack = stack.with_content_scopes(content_scopes);
                        if end.has_back_references {
                            stack = stack.with_end_rule(end.resolve_back_references(text, &caps));
                        }
                        if !has_advanced && before_push.has_same_rule_as(&stack) {
                            stack = stack.pop();
                            tokens.produce(stack.content_scopes(), line_len);
                            break;
                        }
                    }
                    RuleKind::BeginWhile { while_, begin_captures, .. } => {
                        self.handle_captures(text, is_first, &stack, tokens, begin_captures, &caps);
                        tokens.produce(stack.content_scopes(), caps[0].1);
                        anchor_pos = caps[0].1 as i64;
                        let content = rule.content_name.as_ref().map(|n| n.resolve(text, Some(&caps)));
                        let content_scopes = self.push_scope(name_scopes, content.as_deref());
                        stack = stack.with_content_scopes(content_scopes);
                        if while_.has_back_references {
                            stack = stack.with_end_rule(while_.resolve_back_references(text, &caps));
                        }
                        if !has_advanced && before_push.has_same_rule_as(&stack) {
                            stack = stack.pop();
                            tokens.produce(stack.content_scopes(), line_len);
                            break;
                        }
                    }
                    RuleKind::Match { captures, .. } => {
                        self.handle_captures(text, is_first, &stack, tokens, captures, &caps);
                        tokens.produce(stack.content_scopes(), caps[0].1);
                        // A match rule pops immediately.
                        stack = stack.pop();
                        if !has_advanced {
                            stack = stack.safe_pop();
                            tokens.produce(stack.content_scopes(), line_len);
                            break;
                        }
                    }
                    RuleKind::Include { .. } => {}
                }
            }
            if caps[0].1 > pos {
                pos = caps[0].1;
                is_first = false;
            }
        }
        stack
    }

    /// upstream `_checkWhileConditions`: the `while` rules on the stack, outermost first.
    fn check_while_conditions(&self, text: &str, mut is_first: bool, mut pos: usize, mut stack: StateStack, tokens: &mut Tokens, cache: &mut ScanCache) -> (StateStack, usize, bool, i64) {
        let mut anchor_pos: i64 = if stack.0.as_ref().is_some_and(|f| f.begin_rule_captured_eol) { 0 } else { -1 };
        let mut while_rules: Vec<StateStack> = Vec::new();
        let mut node = stack.clone();
        while node.0.is_some() {
            if matches!(self.rule(node.rule_id()).kind, RuleKind::BeginWhile { .. }) {
                while_rules.push(node.clone());
            }
            node = node.pop();
        }
        while let Some(wr) = while_rules.pop() {
            let rule_id = wr.rule_id();
            let end_source = wr.frame().end_rule.as_deref();
            let scanner = self.scanner(rule_id, end_source, true, is_first, pos as i64 == anchor_pos);
            match scanner.find_next_match(text, pos, cache) {
                Some(ScanMatch { index, captures }) => {
                    if scanner.rule_ids[index] != WHILE_RULE_ID {
                        stack = wr.pop();
                        break;
                    }
                    if !captures.is_empty() {
                        tokens.produce(wr.content_scopes(), captures[0].0);
                        if let RuleKind::BeginWhile { while_captures, .. } = &self.rule(rule_id).kind {
                            self.handle_captures(text, is_first, &wr, tokens, while_captures, &captures);
                        }
                        tokens.produce(wr.content_scopes(), captures[0].1);
                        anchor_pos = captures[0].1 as i64;
                        if captures[0].1 > pos {
                            pos = captures[0].1;
                            is_first = false;
                        }
                    }
                }
                None => {
                    stack = wr.pop();
                    break;
                }
            }
        }
        (stack, pos, is_first, anchor_pos)
    }

    fn match_rule(&self, text: &str, is_first: bool, pos: usize, stack: &StateStack, anchor_pos: i64, cache: &mut ScanCache) -> Option<(Vec<(usize, usize)>, RuleId)> {
        let rule_id = stack.rule_id();
        let end_source = stack.frame().end_rule.as_deref();
        let scanner = self.scanner(rule_id, end_source, false, is_first, pos as i64 == anchor_pos);
        let m = scanner.find_next_match(text, pos, cache)?;
        Some((m.captures, scanner.rule_ids[m.index]))
    }

    fn match_injections(&self, text: &str, is_first: bool, pos: usize, stack: &StateStack, anchor_pos: i64, cache: &mut ScanCache) -> Option<(Vec<(usize, usize)>, RuleId, bool)> {
        let names = self.scope_names(stack.content_scopes());
        let refs: Vec<&str> = names.iter().map(|s| &**s).collect();
        let mut best_rating = usize::MAX;
        let mut best: Option<(Vec<(usize, usize)>, RuleId)> = None;
        let mut best_priority = 0;
        for inj in &self.injections {
            if !inj.selector.matches(&refs) {
                continue;
            }
            let scanner = self.scanner(inj.rule_id, None, false, is_first, pos as i64 == anchor_pos);
            let Some(m) = scanner.find_next_match(text, pos, cache) else { continue };
            let rating = m.captures[0].0;
            if rating >= best_rating {
                continue;
            }
            best_rating = rating;
            best_priority = inj.priority;
            best = Some((m.captures, scanner.rule_ids[m.index]));
            if best_rating == pos {
                break;
            }
        }
        best.map(|(c, r)| (c, r, best_priority == -1))
    }

    fn match_rule_or_injections(&self, text: &str, is_first: bool, pos: usize, stack: &StateStack, anchor_pos: i64, cache: &mut ScanCache) -> Option<(Vec<(usize, usize)>, RuleId)> {
        let result = self.match_rule(text, is_first, pos, stack, anchor_pos, cache);
        if self.injections.is_empty() {
            return result;
        }
        let Some((icaps, irule, priority_match)) = self.match_injections(text, is_first, pos, stack, anchor_pos, cache) else {
            return result;
        };
        let Some((caps, rule)) = result else { return Some((icaps, irule)) };
        let (score, iscore) = (caps[0].0, icaps[0].0);
        if iscore < score || (priority_match && iscore == score) {
            Some((icaps, irule))
        } else {
            Some((caps, rule))
        }
    }

    /// upstream `handleCaptures`.
    fn handle_captures(&self, text: &str, is_first: bool, stack: &StateStack, tokens: &mut Tokens, captures: &Captures, indices: &[(usize, usize)]) {
        if captures.is_empty() {
            return;
        }
        let len = captures.len().min(indices.len());
        let mut local: Vec<(ScopeId, usize)> = Vec::new();
        let max_end = indices[0].1;
        for i in 0..len {
            let Some(cap) = &captures[i] else { continue };
            let (cs, ce) = indices[i];
            if ce == cs {
                continue;
            }
            if cs > max_end {
                break;
            }
            while let Some(&(scopes, end)) = local.last() {
                if end > cs {
                    break;
                }
                tokens.produce(scopes, end);
                local.pop();
            }
            match local.last() {
                Some(&(scopes, _)) => tokens.produce(scopes, cs),
                None => tokens.produce(stack.content_scopes(), cs),
            }
            if let Some(rule_id) = cap.retokenize {
                let name = cap.name.as_ref().map(|n| n.resolve(text, Some(indices)));
                let name_scopes = self.push_scope(stack.content_scopes(), name.as_deref());
                let content = cap.content_name.as_ref().map(|n| n.resolve(text, Some(indices)));
                let content_scopes = self.push_scope(name_scopes, content.as_deref());
                let clone = stack.push(rule_id, cs as i64, -1, false, None, name_scopes, content_scopes);
                // The captured part only: the rest of the line is not visible to its rules.
                self.tokenize_string(&text[..ce], is_first && cs == 0, cs, clone, tokens, false);
                continue;
            }
            let name = cap.name.as_ref().map(|n| n.resolve(text, Some(indices)));
            if let Some(name) = name {
                let base = local.last().map_or(stack.content_scopes(), |l| l.0);
                let scopes = self.push_scope(base, Some(&name));
                local.push((scopes, ce));
            }
        }
        while let Some((scopes, end)) = local.pop() {
            tokens.produce(scopes, end);
        }
    }
}

struct Tokens {
    out: Vec<Token>,
    last_end: usize,
}

impl Tokens {
    fn produce(&mut self, scope: ScopeId, end: usize) {
        if self.last_end >= end {
            return;
        }
        self.out.push(Token { start: self.last_end as u32, scope });
        self.last_end = end;
    }
}

#[allow(dead_code)]
fn _assert_send_sync() {
    fn is<T: Send + Sync>() {}
    is::<Grammar>();
    is::<StateStack>();
    let _ = RegExpSource::new("");
}
