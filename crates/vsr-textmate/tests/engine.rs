//! Engine tests on small grammars: each pins one vscode-textmate behaviour.

use std::collections::HashMap;
use std::sync::Arc;

use vsr_textmate::{Grammar, GrammarSource, RawGrammar, StateStack};

struct Src {
    grammars: HashMap<String, Arc<RawGrammar>>,
    inject_to: HashMap<String, Vec<String>>,
}

impl GrammarSource for Src {
    fn grammar(&self, scope: &str) -> Option<Arc<RawGrammar>> {
        self.grammars.get(scope).cloned()
    }
    fn injections_for(&self, scope: &str) -> Vec<String> {
        self.inject_to.get(scope).cloned().unwrap_or_default()
    }
}

fn src(grammars: &[&str]) -> Src {
    Src {
        grammars: grammars
            .iter()
            .map(|g| {
                let raw = RawGrammar::from_json(g).unwrap();
                (raw.scope_name.clone(), Arc::new(raw))
            })
            .collect(),
        inject_to: HashMap::new(),
    }
}

/// Tokenizes `lines` and renders each token as `text:scope1 scope2` (without the root scope).
fn run(source: &Src, scope: &str, lines: &[&str]) -> Vec<Vec<String>> {
    let g = Grammar::new(source, scope, &[]).unwrap();
    let mut state = StateStack::initial();
    let mut out = Vec::new();
    for line in lines {
        let r = g.tokenize_line(line, &state);
        let mut row = Vec::new();
        for (i, t) in r.tokens.iter().enumerate() {
            let end = r.tokens.get(i + 1).map_or(line.len(), |n| n.start as usize);
            let names = g.scope_names(t.scope);
            let rest: Vec<&str> = names.iter().skip(1).map(|s| &**s).collect();
            row.push(format!("{}:{}", &line[t.start as usize..end], rest.join(" ")));
        }
        out.push(row);
        state = r.end_state;
    }
    out
}

#[test]
fn match_and_begin_end_across_lines() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"match": "\\b(fn)\\b", "name": "keyword.fn"},
        {"begin": "/\\*", "end": "\\*/", "name": "comment.block", "contentName": "comment.content"}
      ]}"##]);
    let out = run(&s, "source.t", &["fn /* a", "b */ fn"]);
    assert_eq!(out[0], ["fn:keyword.fn", " :", "/*:comment.block", " a:comment.block comment.content"]);
    assert_eq!(out[1], ["b :comment.block comment.content", "*/:comment.block", " :", "fn:keyword.fn"]);
}

#[test]
fn captures_with_names_and_nested_patterns() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"match": "(let) (\\w+) = (.*)", "captures": {
          "1": {"name": "storage"},
          "2": {"name": "variable"},
          "3": {"patterns": [{"match": "\\d+", "name": "number"}]}
        }}
      ]}"##]);
    let out = run(&s, "source.t", &["let x = a 12"]);
    assert_eq!(out[0], ["let:storage", " :", "x:variable", " = :", "a :", "12:number"]);
}

#[test]
fn back_references_in_end_patterns() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"begin": "<<(\\w+)", "end": "^\\1$", "name": "string.heredoc"}
      ]}"##]);
    let out = run(&s, "source.t", &["x <<EOT", "EOTX", "EOT", "y"]);
    assert_eq!(out[0], ["x :", "<<EOT:string.heredoc"]);
    assert_eq!(out[1], ["EOTX:string.heredoc"]);
    assert_eq!(out[2], ["EOT:string.heredoc"]);
    assert_eq!(out[3], ["y:"]);
}

#[test]
fn capture_references_in_names() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"match": "(\\w+)\\.(\\w+)", "name": "ref.$1.${2:/upcase}"}
      ]}"##]);
    let out = run(&s, "source.t", &["ab.cd"]);
    assert_eq!(out[0], ["ab.cd:ref.ab.CD"]);
}

#[test]
fn while_rules_continue_until_the_condition_fails() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"begin": "^> ", "while": "^> ", "name": "quote", "contentName": "quote.text",
         "beginCaptures": {"0": {"name": "marker"}}, "whileCaptures": {"0": {"name": "marker"}}}
      ]}"##]);
    let out = run(&s, "source.t", &["> a", "> b", "c"]);
    assert_eq!(out[0], ["> :quote marker", "a:quote quote.text"]);
    assert_eq!(out[1], ["> :quote quote.text marker", "b:quote quote.text"]);
    assert_eq!(out[2], ["c:"]);
}

#[test]
fn repository_includes_self_and_external_grammars() {
    let outer = r##"{
      "scopeName": "source.outer",
      "patterns": [
        {"begin": "\\(", "end": "\\)", "name": "paren", "patterns": [{"include": "$self"}, {"include": "#word"}]},
        {"include": "source.inner#num"},
        {"include": "source.inner"}
      ],
      "repository": {"word": {"match": "[a-z]+", "name": "word"}}}"##;
    let inner = r##"{
      "scopeName": "source.inner",
      "patterns": [{"match": "!", "name": "bang"}],
      "repository": {"num": {"match": "\\d+", "name": "num"}}}"##;
    let s = src(&[outer, inner]);
    let out = run(&s, "source.outer", &["(a(b)) 12!"]);
    assert_eq!(
        out[0],
        ["(:paren", "a:paren word", "(:paren paren", "b:paren paren word", "):paren paren", "):paren", " :", "12:num", "!:bang"]
    );
}

#[test]
fn injections_with_priority() {
    let base = r##"{
      "scopeName": "source.t",
      "patterns": [{"match": "\\w+", "name": "word"}],
      "injections": {"L:source.t": {"patterns": [{"match": "TODO", "name": "todo"}]}}}"##;
    let s = src(&[base]);
    let out = run(&s, "source.t", &["a TODO b"]);
    assert_eq!(out[0], ["a:word", " :", "TODO:todo", " :", "b:word"]);
}

#[test]
fn injections_from_another_grammar_via_inject_to() {
    let base = r##"{"scopeName": "source.t", "patterns": [{"match": "\\w+", "name": "word"}]}"##;
    let inj = r##"{"scopeName": "inj.t", "injectionSelector": "L:source.t word",
      "patterns": [{"match": "x", "name": "x"}]}"##;
    let mut s = src(&[base, inj]);
    s.inject_to.insert("source.t".into(), vec!["inj.t".into()]);
    // `x` alone is a word; the injection sees scopes [source.t, word]? No: it is only tried where
    // the stack has a `word`, and a match rule is popped at once, so nothing changes here.
    let out = run(&s, "source.t", &["xy x"]);
    assert_eq!(out[0], ["xy:word", " :", "x:word"]);
}

#[test]
fn a_rule_that_matches_nothing_does_not_loop() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [{"begin": "(?=a)", "end": "(?=b)", "name": "weird"}, {"match": "a", "name": "a"}]}"##]);
    let out = run(&s, "source.t", &["aaa"]);
    // It terminates and covers the whole line.
    let joined: String = out[0].iter().map(|t| t.split(':').next().unwrap().to_string()).collect();
    assert_eq!(joined, "aaa");
}

#[test]
fn apply_end_pattern_last() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"begin": "\\[", "end": "\\]", "name": "br", "applyEndPatternLast": 1,
         "patterns": [{"match": "[^\\]]+|\\]", "name": "any"}]}
      ]}"##]);
    // With the end pattern last, `any` (which also matches `]`) wins at the same position.
    let out = run(&s, "source.t", &["[a]"]);
    assert_eq!(out[0], ["[:br", "a:br any", "]:br any"]);
}

#[test]
fn anchors_g_and_a() {
    let s = src(&[r##"{
      "scopeName": "source.t",
      "patterns": [
        {"begin": "<", "end": ">", "name": "tag", "patterns": [{"match": "\\G\\w+", "name": "tagname"}, {"match": "\\w+", "name": "attr"}]},
        {"match": "\\Afirst", "name": "first"}
      ]}"##]);
    let out = run(&s, "source.t", &["first <a b>", "first"]);
    assert_eq!(out[0], ["first:first", " :", "<:tag", "a:tag tagname", " :tag", "b:tag attr", ">:tag"]);
    // `\A` only matches on the first line of the document.
    assert_eq!(out[1], ["first:"]);
}

#[test]
fn unicode_offsets_are_bytes_of_the_line() {
    let s = src(&[r##"{"scopeName": "source.t", "patterns": [{"match": "é+", "name": "e"}]}"##]);
    let out = run(&s, "source.t", &["aéé b"]);
    assert_eq!(out[0], ["a:", "éé:e", " b:"]);
}

#[test]
fn state_equality_detects_convergence() {
    let s = src(&[r##"{"scopeName": "source.t", "patterns": [{"begin": "\\{", "end": "\\}", "name": "blk"}]}"##]);
    let g = Grammar::new(&s, "source.t", &[]).unwrap();
    let a = g.tokenize_line("x {", &StateStack::initial()).end_state;
    let b = g.tokenize_line("{", &StateStack::initial()).end_state;
    let c = g.tokenize_line("{ }", &StateStack::initial()).end_state;
    assert_eq!(a, b);
    assert_ne!(a, c);
}

#[test]
fn token_types_follow_the_scope_names() {
    use vsr_textmate::TokenType;
    let s = src(&[r##"{"scopeName": "source.t", "patterns": [
      {"match": "#.*", "name": "comment.line"}, {"match": "\"[^\"]*\"", "name": "string.quoted"}, {"match": "\\w+", "name": "word"}]}"##]);
    let g = Grammar::new(&s, "source.t", &[]).unwrap();
    let r = g.tokenize_line("a \"b\" #c", &StateStack::initial());
    let types: Vec<TokenType> = r.tokens.iter().map(|t| g.token_type(t.scope)).collect();
    assert_eq!(types, [TokenType::Other, TokenType::Other, TokenType::String, TokenType::Other, TokenType::Comment]);
}
