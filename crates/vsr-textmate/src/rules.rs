//! Compiled rules: the arena built from raw grammars, with `include` resolution.
//!
//! upstream: vscode-textmate src/rule.ts (`RuleFactory`, `Rule` classes) (microsoft/vscode-textmate,
//! main, 2026-11-09)
//!
//! vscode-textmate compiles lazily and memoizes by stamping an `id` on the raw rule objects. This
//! builds the whole reachable graph eagerly (no regex is compiled until a scanner needs it) and
//! memoizes by the raw rule's `Arc` pointer, which has the same identity semantics.

use std::collections::HashMap;
use std::sync::Arc;

use crate::raw::{RawGrammar, RawRule};
use crate::regex::RegExpSource;

pub type RuleId = i32;
/// The id of an `end` pattern in a scanner.
pub const END_RULE_ID: RuleId = -1;
/// The id of a `while` pattern in a scanner.
pub const WHILE_RULE_ID: RuleId = -2;

/// A scope name, possibly with `$1` / `${1:/downcase}` capture references.
#[derive(Debug, Clone)]
pub struct NameTemplate {
    pub text: String,
    pub capturing: bool,
}

impl NameTemplate {
    fn new(text: &str) -> Self {
        Self { text: text.to_string(), capturing: has_capture_reference(text) }
    }

    /// upstream `Rule.getName` (with `RegExpSource.replaceCaptures`).
    pub fn resolve<'a>(&'a self, line: &str, captures: Option<&[(usize, usize)]>) -> std::borrow::Cow<'a, str> {
        match captures {
            Some(caps) if self.capturing => std::borrow::Cow::Owned(replace_captures(&self.text, line, caps)),
            _ => std::borrow::Cow::Borrowed(&self.text),
        }
    }
}

/// `/\$(\d+)|\${(\d+):\/(downcase|upcase)}/`
fn has_capture_reference(s: &str) -> bool {
    let b = s.as_bytes();
    (0..b.len()).any(|i| b[i] == b'$' && b.get(i + 1).is_some_and(|c| c.is_ascii_digit() || *c == b'{'))
}

fn replace_captures(template: &str, line: &str, caps: &[(usize, usize)]) -> String {
    let b = template.as_bytes();
    let mut out = String::new();
    let mut i = 0;
    let mut plain = 0;
    while i < b.len() {
        if b[i] == b'$' {
            // $N
            if let Some(n) = b.get(i + 1).filter(|c| c.is_ascii_digit()) {
                let _ = n;
                let mut j = i + 1;
                while j < b.len() && b[j].is_ascii_digit() {
                    j += 1;
                }
                out.push_str(&template[plain..i]);
                let idx: usize = template[i + 1..j].parse().unwrap_or(usize::MAX);
                out.push_str(&capture_text(&template[i..j], line, caps.get(idx)));
                plain = j;
                i = j;
                continue;
            }
            // ${N:/downcase|upcase}
            if b.get(i + 1) == Some(&b'{')
                && let Some(close) = template[i..].find('}')
            {
                let inner = &template[i + 2..i + close];
                if let Some((num, cmd)) = inner.split_once(":/")
                    && let Ok(idx) = num.parse::<usize>()
                    && matches!(cmd, "downcase" | "upcase")
                {
                    out.push_str(&template[plain..i]);
                    let text = capture_text(&template[i..i + close + 1], line, caps.get(idx));
                    // An unmatched reference stays as written (no transform).
                    if caps.get(idx).is_some() {
                        out.push_str(&if cmd == "downcase" { text.to_lowercase() } else { text.to_uppercase() });
                    } else {
                        out.push_str(&text);
                    }
                    plain = i + close + 1;
                    i = plain;
                    continue;
                }
            }
        }
        i += 1;
    }
    out.push_str(&template[plain..]);
    out
}

fn capture_text(original: &str, line: &str, cap: Option<&(usize, usize)>) -> String {
    match cap {
        Some(&(s, e)) => line[s..e].trim_start_matches('.').to_string(),
        None => original.to_string(),
    }
}

#[derive(Debug)]
pub struct CaptureRule {
    pub name: Option<NameTemplate>,
    pub content_name: Option<NameTemplate>,
    pub retokenize: Option<RuleId>,
}

pub type Captures = Vec<Option<CaptureRule>>;

#[derive(Debug)]
pub enum RuleKind {
    Match { re: RegExpSource, captures: Captures },
    Include { patterns: Vec<RuleId>, has_missing: bool },
    BeginEnd {
        begin: RegExpSource,
        end: RegExpSource,
        apply_end_pattern_last: bool,
        begin_captures: Captures,
        end_captures: Captures,
        patterns: Vec<RuleId>,
        has_missing: bool,
    },
    BeginWhile {
        begin: RegExpSource,
        while_: RegExpSource,
        begin_captures: Captures,
        while_captures: Captures,
        patterns: Vec<RuleId>,
        has_missing: bool,
    },
}

#[derive(Debug)]
pub struct Rule {
    pub id: RuleId,
    pub name: Option<NameTemplate>,
    pub content_name: Option<NameTemplate>,
    pub kind: RuleKind,
}

/// Where a raw rule is compiled: the grammar it sits in (for `$self` and the top level
/// repository) and the repositories of the enclosing rules.
#[derive(Clone)]
struct Env {
    grammar: Arc<RawGrammar>,
    layers: Option<Arc<Layer>>,
}

struct Layer {
    rule: Arc<RawRule>,
    parent: Option<Arc<Layer>>,
}

impl Env {
    fn root(grammar: &Arc<RawGrammar>) -> Env {
        Env { grammar: grammar.clone(), layers: None }
    }

    fn with_repository_of(&self, rule: &Arc<RawRule>) -> Env {
        if rule.repository.as_ref().is_some_and(|r| !r.is_empty()) {
            Env { grammar: self.grammar.clone(), layers: Some(Arc::new(Layer { rule: rule.clone(), parent: self.layers.clone() })) }
        } else {
            self.clone()
        }
    }

    fn lookup(&self, name: &str) -> Option<Arc<RawRule>> {
        let mut layer = self.layers.as_ref();
        while let Some(l) = layer {
            if let Some(r) = l.rule.repository.as_ref().and_then(|m| m.get(name)) {
                return Some(r.clone());
            }
            layer = l.parent.as_ref();
        }
        self.grammar.repository.get(name).cloned()
    }
}

/// Where grammars come from, for `include: "source.other"`.
pub trait GrammarSource: Send + Sync {
    fn grammar(&self, scope_name: &str) -> Option<Arc<RawGrammar>>;
    /// The scope names of the grammars that inject into `scope_name` (`injectTo`).
    fn injections_for(&self, scope_name: &str) -> Vec<String>;
}

pub(crate) struct Builder<'a> {
    source: &'a dyn GrammarSource,
    pub(crate) rules: Vec<Option<Rule>>,
    memo: HashMap<*const RawRule, RuleId>,
    grammars: HashMap<String, Option<Arc<RawGrammar>>>,
    base_root: RuleId,
}

impl<'a> Builder<'a> {
    pub(crate) fn new(source: &'a dyn GrammarSource) -> Self {
        Self { source, rules: Vec::new(), memo: HashMap::new(), grammars: HashMap::new(), base_root: 0 }
    }

    /// Compiles `grammar` as the base grammar; returns the root rule id.
    pub(crate) fn compile_root(&mut self, grammar: &Arc<RawGrammar>) -> RuleId {
        self.grammars.insert(grammar.scope_name.clone(), Some(grammar.clone()));
        // The id is allocated first so `$base` can refer to it while compiling.
        let id = self.compile_rule(&grammar.self_rule.clone(), &Env::root(grammar));
        self.base_root = id;
        id
    }

    /// Compiles the injections of the base grammar: its own `injections` and the grammars that
    /// declare `injectTo` for it. Returns `(selector, priority, rule id)`.
    pub(crate) fn compile_injections(&mut self, grammar: &Arc<RawGrammar>) -> Vec<(crate::scope::Selector, i32, RuleId)> {
        let mut out = Vec::new();
        let env = Env::root(grammar);
        for (selector, rule) in &grammar.injections {
            let id = self.compile_rule(rule, &env);
            for m in crate::scope::create_matchers(selector) {
                out.push((m.selector, m.priority, id));
            }
        }
        for scope in self.source.injections_for(&grammar.scope_name) {
            let Some(g) = self.external(&scope) else { continue };
            let Some(selector) = g.injection_selector.clone() else { continue };
            let id = self.compile_rule(&g.self_rule.clone(), &Env::root(&g));
            for m in crate::scope::create_matchers(&selector) {
                out.push((m.selector, m.priority, id));
            }
        }
        out.sort_by_key(|(_, p, _)| *p);
        out
    }

    fn external(&mut self, scope: &str) -> Option<Arc<RawGrammar>> {
        if let Some(g) = self.grammars.get(scope) {
            return g.clone();
        }
        let g = self.source.grammar(scope);
        self.grammars.insert(scope.to_string(), g.clone());
        g
    }

    fn compile_rule(&mut self, raw: &Arc<RawRule>, env: &Env) -> RuleId {
        let key = Arc::as_ptr(raw);
        if let Some(&id) = self.memo.get(&key) {
            return id;
        }
        let id = self.rules.len() as RuleId;
        self.rules.push(None);
        self.memo.insert(key, id);
        let env = env.with_repository_of(raw);

        let name = raw.name.as_deref().map(NameTemplate::new);
        let content_name = raw.content_name.as_deref().map(NameTemplate::new);
        let kind = if let Some(m) = &raw.match_ {
            RuleKind::Match { re: RegExpSource::new(m), captures: self.captures(raw.captures.as_ref(), &env) }
        } else if raw.begin.is_none() {
            let patterns = match (&raw.patterns, &raw.include) {
                (Some(p), _) => p.clone(),
                (None, Some(inc)) => vec![Arc::new(RawRule { include: Some(inc.clone()), ..Default::default() })],
                (None, None) => Vec::new(),
            };
            let (patterns, has_missing) = self.patterns(&patterns, &env);
            RuleKind::Include { patterns, has_missing }
        } else if let Some(w) = &raw.while_ {
            let (patterns, has_missing) = self.patterns(raw.patterns.as_deref().unwrap_or(&[]), &env);
            let begin_caps = raw.begin_captures.as_ref().or(raw.captures.as_ref());
            let while_caps = raw.while_captures.as_ref().or(raw.captures.as_ref());
            RuleKind::BeginWhile {
                begin: RegExpSource::new(raw.begin.as_deref().unwrap_or_default()),
                while_: RegExpSource::new(w),
                begin_captures: self.captures(begin_caps, &env),
                while_captures: self.captures(while_caps, &env),
                patterns,
                has_missing,
            }
        } else {
            let (patterns, has_missing) = self.patterns(raw.patterns.as_deref().unwrap_or(&[]), &env);
            let begin_caps = raw.begin_captures.as_ref().or(raw.captures.as_ref());
            let end_caps = raw.end_captures.as_ref().or(raw.captures.as_ref());
            RuleKind::BeginEnd {
                begin: RegExpSource::new(raw.begin.as_deref().unwrap_or_default()),
                end: RegExpSource::new(raw.end.as_deref().unwrap_or("\u{ffff}")),
                apply_end_pattern_last: raw.apply_end_pattern_last,
                begin_captures: self.captures(begin_caps, &env),
                end_captures: self.captures(end_caps, &env),
                patterns,
                has_missing,
            }
        };
        self.rules[id as usize] = Some(Rule { id, name, content_name, kind });
        id
    }

    fn captures(&mut self, caps: Option<&HashMap<String, Arc<RawRule>>>, env: &Env) -> Captures {
        let Some(caps) = caps else { return Vec::new() };
        let numeric: Vec<(usize, &Arc<RawRule>)> = caps.iter().filter_map(|(k, v)| k.parse::<usize>().ok().map(|n| (n, v))).collect();
        let Some(max) = numeric.iter().map(|(n, _)| *n).max() else { return Vec::new() };
        let mut out: Captures = (0..=max).map(|_| None).collect();
        for (n, raw) in numeric {
            let retokenize = raw.patterns.is_some().then(|| self.compile_rule(raw, env));
            out[n] = Some(CaptureRule {
                name: raw.name.as_deref().map(NameTemplate::new),
                content_name: raw.content_name.as_deref().map(NameTemplate::new),
                retokenize,
            });
        }
        out
    }

    /// upstream `compilePatterns`: includes are resolved, missing ones dropped.
    fn patterns(&mut self, patterns: &[Arc<RawRule>], env: &Env) -> (Vec<RuleId>, bool) {
        let mut out = Vec::new();
        let mut has_missing = false;
        for p in patterns {
            if p.disabled {
                continue;
            }
            let id: Option<RuleId> = if let Some(inc) = &p.include {
                self.resolve_include(inc, env)
            } else {
                Some(self.compile_rule(p, env))
            };
            match id {
                None => has_missing = true,
                Some(id) => {
                    // A rule with no usable patterns of its own is skipped. An unfinished rule
                    // (recursion) is not.
                    let skip = match self.rules[id as usize].as_ref().map(|r| &r.kind) {
                        Some(RuleKind::Include { patterns, has_missing })
                        | Some(RuleKind::BeginEnd { patterns, has_missing, .. })
                        | Some(RuleKind::BeginWhile { patterns, has_missing, .. }) => *has_missing && patterns.is_empty(),
                        _ => false,
                    };
                    if !skip {
                        out.push(id);
                    }
                }
            }
        }
        (out, has_missing)
    }

    fn resolve_include(&mut self, include: &str, env: &Env) -> Option<RuleId> {
        if let Some(name) = include.strip_prefix('#') {
            let rule = env.lookup(name)?;
            return Some(self.compile_rule(&rule, env));
        }
        if include == "$base" {
            return Some(self.base_root);
        }
        if include == "$self" {
            let root = env.grammar.self_rule.clone();
            return Some(self.compile_rule(&root, &Env::root(&env.grammar)));
        }
        let (scope, name) = match include.split_once('#') {
            Some((s, n)) => (s, Some(n)),
            None => (include, None),
        };
        let g = self.external(scope)?;
        let ext_env = Env::root(&g);
        match name {
            Some(n) => {
                let rule = g.repository.get(n)?.clone();
                Some(self.compile_rule(&rule, &ext_env))
            }
            None => {
                let root = g.self_rule.clone();
                Some(self.compile_rule(&root, &ext_env))
            }
        }
    }
}
