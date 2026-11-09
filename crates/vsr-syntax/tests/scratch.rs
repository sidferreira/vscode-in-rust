use vsr_syntax::{BundledGrammars, DocumentTokens, SyntaxService};
use vsr_textmate::StateStack;

fn dump(lang: &str, lines: &[&str]) {
    let g = BundledGrammars::new();
    let scope = g.scope_for_language(lang).unwrap().to_string();
    let t = std::time::Instant::now();
    let gr = g.load(&scope).unwrap();
    eprintln!("load {scope}: {:?}, {} rules", t.elapsed(), gr.rule_count());
    let mut st = StateStack::initial();
    for l in lines {
        let r = gr.tokenize_line(l, &st);
        eprintln!("LINE {l:?}");
        for (i, tok) in r.tokens.iter().enumerate() {
            let end = r.tokens.get(i + 1).map_or(l.len(), |n| n.start as usize);
            let names = gr.scope_names(tok.scope);
            eprintln!("  {:?} -> {}", &l[tok.start as usize..end], names.join(" "));
        }
        st = r.end_state;
    }
}

#[test]
fn ts_snippet() {
    dump("typescript", &["/** doc @param x the x */", "export const a: number = `t${1 + 2}`;", "const re = /ab+c/g;"]);
}

#[test]
fn perf_editor_options() {
    let Ok(text) = std::fs::read_to_string("/Users/sidney.ferreira/dev/personal/vscode-in-rust/vscode-original/src/vs/editor/common/config/editorOptions.ts") else { return };
    let lines: Vec<&str> = text.lines().collect();
    let svc = SyntaxService::new();
    let t = std::time::Instant::now();
    let g = svc.grammar_for_language("typescript").unwrap();
    eprintln!("grammar load {:?}", t.elapsed());
    let mut doc = DocumentTokens::new(g, 20000);
    doc.sync(&lines);
    let t = std::time::Instant::now();
    while !doc.tokenize_some(usize::MAX, None) {}
    let el = t.elapsed();
    eprintln!("{} lines, {} bytes: {:?} ({:.1} MB/s, {:.0} lines/s)", lines.len(), text.len(), el, text.len() as f64 / el.as_secs_f64() / 1e6, lines.len() as f64 / el.as_secs_f64());
    // second run: warm
    doc.invalidate_all();
    let t = std::time::Instant::now();
    while !doc.tokenize_some(usize::MAX, None) {}
    let el = t.elapsed();
    eprintln!("warm: {:?} ({:.1} MB/s)", el, text.len() as f64 / el.as_secs_f64() / 1e6);
}
