//! The raw grammar: a `.tmLanguage.json` document as vscode-textmate's `IRawGrammar` /
//! `IRawRule` types see it. Parsed by hand from `serde_json::Value` so that odd values
//! (`applyEndPatternLast: 1`, numbers where strings are expected) do not fail the whole grammar.
//!
//! upstream: src/vs/workbench/services/textMate/common/TMGrammars.ts @ 50e1350fc78d
//! upstream: vscode-textmate src/rawGrammar.ts (microsoft/vscode-textmate, main, 2026-11-09)

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::Value;

/// `IRawRule`. Children are `Arc`s because rule identity (the pointer) is the memo key when the
/// rules are compiled, like the `id` field vscode-textmate stamps on the raw objects.
#[derive(Debug, Default)]
pub struct RawRule {
    pub name: Option<String>,
    pub content_name: Option<String>,
    pub include: Option<String>,
    pub match_: Option<String>,
    pub begin: Option<String>,
    pub end: Option<String>,
    pub while_: Option<String>,
    pub captures: Option<HashMap<String, Arc<RawRule>>>,
    pub begin_captures: Option<HashMap<String, Arc<RawRule>>>,
    pub end_captures: Option<HashMap<String, Arc<RawRule>>>,
    pub while_captures: Option<HashMap<String, Arc<RawRule>>>,
    pub patterns: Option<Vec<Arc<RawRule>>>,
    pub repository: Option<HashMap<String, Arc<RawRule>>>,
    pub apply_end_pattern_last: bool,
    pub disabled: bool,
}

/// `IRawGrammar`.
#[derive(Debug)]
pub struct RawGrammar {
    pub scope_name: String,
    pub file_types: Vec<String>,
    /// The grammar as a rule (`$self`): its `patterns`, named after the scope.
    pub self_rule: Arc<RawRule>,
    pub repository: HashMap<String, Arc<RawRule>>,
    /// Selector to rule.
    pub injections: Vec<(String, Arc<RawRule>)>,
    pub injection_selector: Option<String>,
}

fn string(v: &Value, key: &str) -> Option<String> {
    match v.get(key)? {
        Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

fn truthy(v: Option<&Value>) -> bool {
    match v {
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0),
        Some(Value::String(s)) => !s.is_empty(),
        _ => false,
    }
}

fn rule_map(v: Option<&Value>) -> Option<HashMap<String, Arc<RawRule>>> {
    let obj = v?.as_object()?;
    Some(obj.iter().map(|(k, v)| (k.clone(), Arc::new(RawRule::from_value(v)))).collect())
}

impl RawRule {
    pub fn from_value(v: &Value) -> RawRule {
        RawRule {
            name: string(v, "name"),
            content_name: string(v, "contentName"),
            include: string(v, "include"),
            match_: string(v, "match"),
            begin: string(v, "begin"),
            end: string(v, "end"),
            while_: string(v, "while"),
            captures: rule_map(v.get("captures")),
            begin_captures: rule_map(v.get("beginCaptures")),
            end_captures: rule_map(v.get("endCaptures")),
            while_captures: rule_map(v.get("whileCaptures")),
            patterns: v
                .get("patterns")
                .and_then(Value::as_array)
                .map(|a| a.iter().map(|p| Arc::new(RawRule::from_value(p))).collect()),
            repository: rule_map(v.get("repository")),
            apply_end_pattern_last: truthy(v.get("applyEndPatternLast")),
            disabled: truthy(v.get("disabled")),
        }
    }
}

impl RawGrammar {
    pub fn from_json(text: &str) -> Result<RawGrammar, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        Ok(Self::from_value(&v))
    }

    pub fn from_value(v: &Value) -> RawGrammar {
        let scope_name = string(v, "scopeName").unwrap_or_default();
        let patterns = v
            .get("patterns")
            .and_then(Value::as_array)
            .map(|a| a.iter().map(|p| Arc::new(RawRule::from_value(p))).collect());
        let self_rule = Arc::new(RawRule { name: Some(scope_name.clone()), patterns, ..Default::default() });
        let injections = v
            .get("injections")
            .and_then(Value::as_object)
            .map(|o| o.iter().map(|(k, v)| (k.clone(), Arc::new(RawRule::from_value(v)))).collect())
            .unwrap_or_default();
        RawGrammar {
            scope_name,
            file_types: v
                .get("fileTypes")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default(),
            self_rule,
            repository: rule_map(v.get("repository")).unwrap_or_default(),
            injections,
            injection_selector: string(v, "injectionSelector"),
        }
    }
}
