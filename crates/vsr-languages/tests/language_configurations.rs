//! The bundled language configurations (ADR-0046): every regular expression compiles, the pairs and
//! rules of a few languages are what their `language-configuration.json` says.

use serde_json::Value;
use vsr_languages::config::{AutoIndentStrategy, IndentAction, JsRegex, bundled_configurations, language_support};

fn patterns(v: &Value, out: &mut Vec<(String, String)>) {
    match v {
        Value::Object(map) => {
            if let (Some(Value::String(p)), flags) = (map.get("pattern"), map.get("flags")) {
                out.push((p.clone(), flags.and_then(Value::as_str).unwrap_or("").to_string()));
            }
            for (k, x) in map {
                let is_regex_key = matches!(
                    k.as_str(),
                    "beforeText" | "afterText" | "previousLineText" | "decreaseIndentPattern" | "increaseIndentPattern" | "indentNextLinePattern" | "unIndentedLinePattern" | "wordPattern"
                );
                match x {
                    Value::String(s) if is_regex_key => out.push((s.clone(), String::new())),
                    _ => patterns(x, out),
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| patterns(x, out)),
        _ => {}
    }
}

#[test]
fn every_bundled_regular_expression_compiles() {
    let configs = bundled_configurations();
    assert!(configs.len() > 40, "bundled configurations: {}", configs.len());
    let mut total = 0;
    for (path, json) in configs {
        let v: Value = serde_json::from_str(&json).unwrap();
        let mut found = Vec::new();
        patterns(&v, &mut found);
        for (p, flags) in found {
            total += 1;
            // `\p{Alphabetic}` and friends in `wordPattern` are JavaScript `u` flag syntax.
            assert!(JsRegex::new(&p, &flags).is_some(), "{path}: {p}");
        }
    }
    assert!(total > 50, "{total} patterns");
}

#[test]
fn every_bundled_configuration_parses() {
    for (path, json) in bundled_configurations() {
        assert!(vsr_languages::config::LanguageConfiguration::from_json(&json).is_ok(), "{path}");
    }
}

#[test]
fn rust_pairs_and_comments() {
    let s = language_support("rust");
    let opens: Vec<&str> = s.character_pair.auto_closing_pairs().iter().map(|p| p.open.as_str()).collect();
    assert_eq!(opens, ["{", "[", "(", "\""]);
    let comments = s.comments().unwrap();
    assert_eq!(comments.line_comment.as_deref(), Some("//"));
    assert_eq!(comments.block_comment, Some(("/*".to_string(), "*/".to_string())));
    // The `//` rule appends a comment marker when Enter is pressed inside a line comment.
    let action = s.on_enter(AutoIndentStrategy::Advanced, "", "    // note", "more").unwrap();
    assert_eq!(action.append_text.as_deref(), Some("// "));
    assert!(s.on_enter(AutoIndentStrategy::Brackets, "", "    // note", "more").is_none());
}

#[test]
fn python_on_enter_rule_indents_after_a_colon() {
    let s = language_support("python");
    let action = s.on_enter(AutoIndentStrategy::Advanced, "", "def f(x):", "").unwrap();
    assert_eq!(action.indent_action, IndentAction::Indent);
    assert_eq!(s.comments().unwrap().line_comment.as_deref(), Some("#"));
}

#[test]
fn plaintext_has_brackets_and_surrounding_pairs_like_upstream() {
    let s = language_support("plaintext");
    let opens: Vec<&str> = s.character_pair.auto_closing_pairs().iter().map(|p| p.open.as_str()).collect();
    assert_eq!(opens, ["(", "[", "{"]);
    assert_eq!(s.character_pair.surrounding_pairs().len(), 7);
    assert!(s.comments().is_none());
}

#[test]
fn unknown_languages_have_no_pairs() {
    let s = language_support("no-such-language");
    assert!(s.character_pair.auto_closing_pairs().is_empty());
    assert!(s.on_enter(AutoIndentStrategy::Full, "", "(", "").is_none());
}

#[test]
fn indentation_rules_of_rust() {
    let s = language_support("rust");
    let rules = s.indent_rules.as_ref().unwrap();
    assert!(rules.should_increase("fn main() {"));
    assert!(!rules.should_increase("let x = 1;"));
    assert!(rules.should_decrease("    }"));
}
