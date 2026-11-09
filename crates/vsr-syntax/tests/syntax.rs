//! The bundled grammars on real code, incremental re-tokenization, Dark Modern colors.
//!
//! upstream: src/vs/editor/common/model/tokens/tokenizationTextModelPart.ts @ 50e1350fc78d

use std::sync::Arc;
use std::time::Duration;

use vsr_syntax::{BackgroundTokenizer, BundledTheme, DocumentTokens, SyntaxService};
use vsr_textmate::{Grammar, Rgba, StateStack, TokenType};

fn tokenize_all(g: &Grammar, lines: &[&str]) -> Vec<Vec<(String, String)>> {
    let mut state = StateStack::initial();
    let mut out = Vec::new();
    for line in lines {
        let r = g.tokenize_line(line, &state);
        let row = r
            .tokens
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let end = r.tokens.get(i + 1).map_or(line.len(), |n| n.start as usize);
                (line[t.start as usize..end].to_string(), g.scope_names(t.scope).join(" "))
            })
            .collect();
        out.push(row);
        state = r.end_state;
    }
    out
}

/// Asserts that the token with exactly this text has a scope stack containing `scope`.
fn assert_scope(rows: &[Vec<(String, String)>], line: usize, text: &str, scope: &str) {
    let found = rows[line].iter().find(|(t, _)| t == text).unwrap_or_else(|| panic!("no token {text:?} in line {line}: {:?}", rows[line]));
    assert!(found.1.split(' ').any(|s| s == scope), "token {text:?} has scopes {:?}, wanted {scope}", found.1);
}

fn grammar(lang: &str) -> Arc<Grammar> {
    SyntaxService::new().grammar_for_language(lang).unwrap_or_else(|| panic!("no grammar for {lang}"))
}

#[test]
fn rust() {
    let g = grammar("rust");
    let rows = tokenize_all(&g, &["pub fn main() -> Result<(), String> {", "    let s = r#\"raw\"#; // c", "}"]);
    assert_scope(&rows, 0, "pub", "storage.modifier.visibility.rust");
    assert_scope(&rows, 0, "fn", "keyword.other.fn.rust");
    assert_scope(&rows, 0, "main", "entity.name.function.rust");
    assert_scope(&rows, 1, "let", "storage.type.rust");
    assert_scope(&rows, 1, " c", "comment.line.double-slash.rust");
}

#[test]
fn json_and_jsonc() {
    let rows = tokenize_all(&grammar("json"), &["{\"a\": [1, true, null, \"s\"]}"]);
    assert_scope(&rows, 0, "1", "constant.numeric.json");
    assert_scope(&rows, 0, "true", "constant.language.json");
    let rows = tokenize_all(&grammar("jsonc"), &["// c", "{\"a\": 1} /* d */"]);
    assert_scope(&rows, 0, " c", "comment.line.double-slash.js");
}

#[test]
fn typescript_with_jsdoc_injection_and_templates() {
    let g = grammar("typescript");
    let rows = tokenize_all(&g, &["/** doc @param x the x */", "export const a: number = `t${1 + 2}`;", "interface I<T> { m(x: T): void }"]);
    // The jsdoc grammar is injected into comments (injectTo source.ts).
    assert_scope(&rows, 0, "param", "storage.type.class.jsdoc");
    assert_scope(&rows, 1, "export", "keyword.control.export.ts");
    assert_scope(&rows, 1, "const", "storage.type.ts");
    assert_scope(&rows, 1, "${", "punctuation.definition.template-expression.begin.ts");
    assert_scope(&rows, 1, "1", "constant.numeric.decimal.ts");
    assert_scope(&rows, 2, "interface", "storage.type.interface.ts");
}

#[test]
fn javascript_and_jsx() {
    let rows = tokenize_all(&grammar("javascript"), &["const f = (a) => a + 1;", "class A extends B {}"]);
    assert_scope(&rows, 0, "const", "storage.type.js");
    assert_scope(&rows, 1, "class", "storage.type.class.js");
    let rows = tokenize_all(&grammar("javascriptreact"), &["const e = <div id=\"x\">hi</div>;"]);
    assert_scope(&rows, 0, "div", "entity.name.tag.js.jsx");
}

#[test]
fn markdown_with_fenced_code() {
    let g = grammar("markdown");
    let rows = tokenize_all(&g, &["# Title", "", "Some *emph* and `code`.", "", "```rust", "fn main() {}", "```", "- item"]);
    assert_scope(&rows, 0, "#", "punctuation.definition.heading.markdown");
    assert_scope(&rows, 2, "emph", "markup.italic.markdown");
    assert_scope(&rows, 2, "code", "markup.inline.raw.string.markdown");
    // The fenced block is handed to the rust grammar if one is registered for `rust`; the bundled
    // markdown grammar names it `source.rust`.
    assert!(rows[5].iter().any(|(t, s)| t == "fn" && s.contains("keyword.other.fn.rust")), "{:?}", rows[5]);
    assert_scope(&rows, 7, "-", "punctuation.definition.list.begin.markdown");
}

#[test]
fn other_languages_tokenize() {
    // (language, line, token text, scope the token must carry)
    let cases: &[(&str, &str, &str, &str)] = &[
        ("python", "def f(x): return 'a'", "def", "storage.type.function.python"),
        ("python", "import os  # c", "import", "keyword.control.import.python"),
        ("shellscript", "if [ -z \"$X\" ]; then echo hi; fi", "if", "keyword.control.if.shell"),
        ("css", "a:hover { color: #fff; }", "color", "support.type.property-name.css"),
        ("html", "<div class=\"a\">x</div>", "div", "entity.name.tag.html"),
        ("yaml", "key: value", "key", "entity.name.tag.yaml"),
        ("xml", "<a b=\"c\"/>", "a", "entity.name.tag.localname.xml"),
        ("c", "#include <stdio.h>", "#", "punctuation.definition.directive.c"),
        ("cpp", "template<typename T> class A {};", "template", "storage.type.template.cpp"),
        ("go", "func main() { var x int }", "func", "keyword.function.go"),
        ("java", "public class A { int x; }", "class", "storage.modifier.java"),
    ];
    for (lang, line, text, scope) in cases {
        let rows = tokenize_all(&grammar(lang), &[line]);
        let found = rows[0].iter().find(|(t, _)| t.trim() == *text).unwrap_or_else(|| panic!("{lang}: no token {text:?}: {:?}", rows[0]));
        assert!(found.1.contains(scope), "{lang}: {text:?} has {:?}, wanted {scope}", found.1);
    }
}

#[test]
fn html_embeds_css_and_javascript() {
    let g = grammar("html");
    let rows = tokenize_all(&g, &["<style>a { color: red }</style>", "<script>let x = 1;</script>"]);
    assert!(rows[0].iter().any(|(t, s)| t == "color" && s.contains("support.type.property-name.css")), "{:?}", rows[0]);
    assert!(rows[1].iter().any(|(t, s)| t == "let" && s.contains("storage.type.js")), "{:?}", rows[1]);
}

#[test]
fn every_bundled_grammar_loads_and_tokenizes() {
    let svc = SyntaxService::new();
    let langs = [
        "rust", "javascript", "javascriptreact", "typescript", "typescriptreact", "json", "jsonc", "jsonl", "snippets", "markdown", "python",
        "shellscript", "css", "html", "yaml", "xml", "xsl", "c", "cpp", "cuda-cpp", "go", "java",
    ];
    for lang in langs {
        let Some(g) = svc.grammar_for_language(lang) else { continue };
        let rows = tokenize_all(&g, &["abc 123 \"x\" // y", "{ [ ( ) ] }"]);
        assert_eq!(rows.len(), 2, "{lang}");
    }
}

#[test]
fn dark_modern_colors() {
    let svc = SyntaxService::new();
    svc.set_token_colors(&BundledTheme::dark_modern());
    let g = svc.grammar_for_language("rust").unwrap();
    let line = "fn main() { let x = \"s\"; // c";
    let r = g.tokenize_line(line, &StateStack::initial());
    let runs = svc.style_runs(&g, line.len(), &r.tokens);
    let style_of = |text: &str| runs.iter().find(|run| &line[run.start..run.end] == text).unwrap_or_else(|| panic!("{text}")).style;
    // Dark+ colors: keyword #569CD6, function #DCDCAA, variable #9CDCFE, string #CE9178, comment #6A9955.
    assert_eq!(style_of("fn").foreground, Some(Rgba([0x56, 0x9c, 0xd6, 255])));
    assert_eq!(style_of("main").foreground, Some(Rgba([0xdc, 0xdc, 0xaa, 255])));
    assert_eq!(style_of("x").foreground, Some(Rgba([0x9c, 0xdc, 0xfe, 255])));
    assert_eq!(style_of("\"s\"").foreground, Some(Rgba([0xce, 0x91, 0x78, 255])));
    assert_eq!(style_of("// c").foreground, Some(Rgba([0x6a, 0x99, 0x55, 255])));
}

#[test]
fn font_styles_come_from_the_theme() {
    let svc = SyntaxService::new();
    svc.set_token_colors(&BundledTheme::dark_modern());
    let g = svc.grammar_for_language("markdown").unwrap();
    let r = g.tokenize_line("**bold** *it* ~~x~~", &StateStack::initial());
    let line = "**bold** *it* ~~x~~";
    let runs = svc.style_runs(&g, line.len(), &r.tokens);
    let style_of = |text: &str| runs.iter().find(|run| line[run.start..run.end].contains(text)).unwrap_or_else(|| panic!("{text}")).style;
    assert!(style_of("bold").bold());
    assert!(style_of("it").italic());
}

fn lines_of(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("let a{i} = {i}; // line {i}")).collect()
}

fn doc(g: Arc<Grammar>, lines: &[String]) -> DocumentTokens {
    let mut d = DocumentTokens::new(g, 20_000);
    d.sync(&lines.to_vec());
    while !d.tokenize_some(usize::MAX, None) {}
    d
}

#[test]
fn an_edit_retokenizes_only_the_changed_line_when_the_state_does_not_change() {
    let g = grammar("rust");
    let mut lines = lines_of(1000);
    let mut d = doc(g, &lines);
    assert_eq!(d.tokenized_lines(), 1000);
    lines[500] = "let changed = 1;".into();
    assert_eq!(d.sync(&lines), Some(500));
    assert!(d.tokenize_some(usize::MAX, None));
    assert_eq!(d.tokenized_lines(), 1001);
}

#[test]
fn an_opened_comment_retokenizes_until_the_state_converges() {
    let g = grammar("rust");
    let mut lines = lines_of(1000);
    let mut d = doc(g.clone(), &lines);
    let base = d.tokenized_lines();
    // Opening a block comment at 100 changes the state of every line after it ...
    lines[100] = "/* open".into();
    d.sync(&lines);
    d.tokenize_some(usize::MAX, None);
    assert_eq!(d.tokenized_lines() - base, 900, "lines 100..1000 are all in the comment now");
    let t = d.tokens(500).unwrap();
    assert!(g.scope_names(t[0].scope).iter().any(|s| &**s == "comment.block.rust"));
    // ... closing it at 200 re-tokenizes the lines after it too: they were stored as "in a comment".
    let base = d.tokenized_lines();
    lines[200] = "close */ let z = 1;".into();
    d.sync(&lines);
    d.tokenize_some(usize::MAX, None);
    assert_eq!(d.tokenized_lines() - base, 800);
    let names = g.scope_names(d.tokens(500).unwrap()[0].scope);
    assert!(!names.iter().any(|s| s.starts_with("comment.block")), "{names:?}");
    // ... but undoing the opening at 100 converges at 200, where the state is the same again.
    let base = d.tokenized_lines();
    lines[100] = "let open = 1;".into();
    d.sync(&lines);
    d.tokenize_some(usize::MAX, None);
    let redone = d.tokenized_lines() - base;
    assert!((100..=102).contains(&redone), "re-tokenized {redone} lines");
}

#[test]
fn inserting_and_deleting_lines_keeps_the_rest() {
    let g = grammar("rust");
    let mut lines = lines_of(200);
    let mut d = doc(g, &lines);
    lines.insert(50, "let inserted = 1;".into());
    assert_eq!(d.sync(&lines), Some(50));
    d.tokenize_some(usize::MAX, None);
    assert_eq!(d.line_count(), 201);
    assert_eq!(d.tokenized_lines(), 201);
    lines.remove(10);
    lines.remove(10);
    d.sync(&lines);
    d.tokenize_some(usize::MAX, None);
    assert_eq!(d.line_count(), 199);
    assert!(d.is_up_to_date());
    // Every line has tokens.
    assert!((0..199).all(|i| d.tokens(i).is_some()));
}

#[test]
fn incremental_tokens_equal_a_full_run() {
    let g = grammar("typescript");
    let mut lines: Vec<String> = ["const a = 1;", "/* c", "d */ const b = `x", "y${1}`;", "function f() {", "  return /re/g;", "}"].iter().map(|s| s.to_string()).collect();
    let mut d = doc(g.clone(), &lines);
    for (at, text) in [(1usize, "// c"), (3, "y`;"), (2, "const q = 1;"), (0, "/*")] {
        lines[at] = text.to_string();
        d.sync(&lines);
        d.tokenize_some(usize::MAX, None);
        let fresh = doc(g.clone(), &lines);
        for i in 0..lines.len() {
            assert_eq!(d.tokens(i).as_deref(), fresh.tokens(i).as_deref(), "line {i} after setting line {at} to {text:?}");
        }
    }
}

#[test]
fn lines_longer_than_the_cap_are_not_tokenized() {
    let g = grammar("rust");
    let mut d = DocumentTokens::new(g, 20);
    let lines = vec!["let short = 1;".to_string(), "let a_very_long_line_over_the_cap = \"x\";".to_string(), "let again = 2;".to_string()];
    d.sync(&lines);
    d.tokenize_some(usize::MAX, None);
    assert!(d.tokens(0).unwrap().len() > 1);
    assert_eq!(d.tokens(1).unwrap().len(), 1, "a long line is one token");
    assert!(d.tokens(2).unwrap().len() > 1);
    d.set_max_line_length(100);
    d.tokenize_some(usize::MAX, None);
    assert!(d.tokens(1).unwrap().len() > 1);
}

#[test]
fn token_types_mark_strings_and_comments() {
    let g = grammar("rust");
    let mut d = DocumentTokens::new(g, 20_000);
    let lines = vec!["let s = \"abc\"; // note".to_string()];
    d.sync(&lines);
    d.tokenize_some(usize::MAX, None);
    assert_eq!(d.token_type_at(0, 0), TokenType::Other);
    assert_eq!(d.token_type_at(0, 10), TokenType::String);
    assert_eq!(d.token_type_at(0, 18), TokenType::Comment);
}

#[test]
fn background_tokenizer_catches_up_and_updates() {
    let svc = SyntaxService::new();
    let bg: BackgroundTokenizer = svc.tokenizer_for("rust", 20_000).unwrap();
    let mut lines = lines_of(20_000);
    bg.update(&lines, Duration::from_millis(1));
    assert!(bg.wait_idle(Duration::from_secs(30)), "the worker finishes");
    assert!(bg.with(|d| d.tokens(19_999).is_some()));
    lines[10] = "/* open".into();
    bg.update(&lines, Duration::from_millis(1));
    assert!(bg.wait_idle(Duration::from_secs(30)));
    let g = bg.with(|d| d.grammar().clone());
    let names = bg.with(|d| g.scope_names(d.tokens(19_999).unwrap()[0].scope));
    assert!(names.iter().any(|s| &**s == "comment.block.rust"));
    assert!(bg.progress() > 0);
}

#[test]
fn language_service_resolves_files_and_associations() {
    let svc = SyntaxService::new();
    assert_eq!(svc.language_for_file("/p/a.rs", None), "rust");
    assert_eq!(svc.language_display_name("rust"), "Rust");
    svc.set_configured_associations(&[("*.foo".into(), "json".into())]);
    assert_eq!(svc.language_for_file("/p/a.foo", None), "json");
    assert!(svc.grammar_for_language("plaintext").is_none());
    assert!(svc.grammar_for_language("json").is_some());
}

/// Tokenization speed on a large TypeScript file from the upstream clone (set `UPSTREAM_CLONE`;
/// `cargo test --release -p vsr-syntax --test syntax perf -- --ignored --nocapture`). The numbers
/// are in ADR-0038.
#[test]
#[ignore = "a measurement, needs the upstream clone"]
fn perf_editor_options_ts() {
    let path = std::env::var("UPSTREAM_CLONE").unwrap_or_default() + "/src/vs/editor/common/config/editorOptions.ts";
    let Ok(text) = std::fs::read_to_string(&path) else { return };
    let lines: Vec<&str> = text.lines().collect();
    let t = std::time::Instant::now();
    let g = grammar("typescript");
    println!("grammar build: {:?}", t.elapsed());
    let mut doc = DocumentTokens::new(g, 20_000);
    doc.sync(&lines);
    let t = std::time::Instant::now();
    while !doc.tokenize_some(usize::MAX, None) {}
    let cold = t.elapsed();
    doc.invalidate_all();
    let t = std::time::Instant::now();
    while !doc.tokenize_some(usize::MAX, None) {}
    let warm = t.elapsed();
    let mb = text.len() as f64 / 1e6;
    println!(
        "{} lines, {} bytes: cold {cold:?} ({:.2} MB/s), warm {warm:?} ({:.2} MB/s, {:.0} lines/s)",
        lines.len(),
        text.len(),
        mb / cold.as_secs_f64(),
        mb / warm.as_secs_f64(),
        lines.len() as f64 / warm.as_secs_f64()
    );
}
