//! upstream: src/vs/editor/test/browser/controller/cursor.test.ts @ 50e1350fc78d
//!
//! Ports of the Enter, auto indent and electric character cases of `Editor Controller` in
//! cursor.test.ts. `indentRulesLanguage` is upstream's test language: only `indentationRules`.
//! Not ported: the cases that use several cursors, Tab on whitespace-only lines and the
//! formatting-on-type cases (they need `getGoodIndentForLine` on Tab, which `Editor::tab` does not
//! use yet).

use std::sync::Arc;

use vsr_editor::{Editor, EditorOptions, Position, Selection, TypingOptions};
use vsr_languages::config::{AutoClosingPairConfig, IndentationRule, JsRegex, LanguageConfiguration, LanguageSupport, language_support};
use vsr_text::{DefaultEol, TextBuffer};

fn re(p: &str) -> JsRegex {
    JsRegex::new(p, "").unwrap_or_else(|| panic!("pattern does not compile: {p}"))
}

fn indent_rules_language() -> LanguageConfiguration {
    LanguageConfiguration {
        indentation_rules: Some(IndentationRule {
            decrease_indent_pattern: re(r"^\s*((?!\S.*\/[*]).*[*]\/\s*)?[})\]]|^\s*(case\b.*|default):\s*(\/\/.*|\/[*].*[*]\/\s*)?$"),
            increase_indent_pattern: re(r#"^((?!\/\/).)*(\{[^}"'`]*|\([^)"']*|\[[^\]"']*|^\s*(\{\}|\(\)|\[\]|(case\b.*|default):))\s*(\/\/.*|\/[*].*[*]\/\s*)?$"#),
            indent_next_line_pattern: Some(re(r"^\s*(for|while|if|else)\b(?!.*[;{}]\s*(\/\/.*|\/[*].*[*]\/\s*)?$)")),
            un_indented_line_pattern: Some(re(
                r#"^(?!.*([;{}]|\S:)\s*(\/\/.*|\/[*].*[*]\/\s*)?$)(?!.*(\{[^}"']*|\([^)"']*|\[[^\]"']*|^\s*(\{\}|\(\)|\[\]|(case\b.*|default):))\s*(\/\/.*|\/[*].*[*]\/\s*)?$)(?!^\s*((?!\S.*\/[*]).*[*]\/\s*)?[})\]]|^\s*(case\b.*|default):\s*(\/\/.*|\/[*].*[*]\/\s*)?$)(?!^\s*(for|while|if|else)\b(?!.*[;{}]\s*(\/\/.*|\/[*].*[*]\/\s*)?$))"#,
            )),
        }),
        ..Default::default()
    }
}

fn editor(lines: &[&str], config: Option<LanguageConfiguration>, insert_spaces: bool) -> Editor {
    let mut e = Editor::new(TextBuffer::new(&lines.join("\n"), DefaultEol::Lf, true), EditorOptions { tab_size: 4, insert_spaces });
    match config {
        Some(c) => e.set_language_support(Some(Arc::new(LanguageSupport::new(c)))),
        None => e.set_language_support(Some(language_support("plaintext"))),
    }
    e.set_typing_options(TypingOptions::default());
    e
}

fn move_to(e: &mut Editor, line: usize, column: usize) {
    e.set_cursor(Position::new(line, column), false);
}

fn select(e: &mut Editor, from: (usize, usize), to: (usize, usize)) {
    e.set_cursor(Position::new(from.0, from.1), false);
    e.set_cursor(Position::new(to.0, to.1), true);
}

fn at(e: &Editor) -> (usize, usize) {
    let s = e.selection();
    assert_eq!(s.anchor, s.active, "expected a caret");
    (s.active.line, s.active.column)
}

fn line(e: &Editor, n: usize) -> String {
    e.buffer().line_content(n).to_string()
}

fn indent_rules_editor(lines: &[&str], insert_spaces: bool) -> Editor {
    editor(lines, Some(indent_rules_language()), insert_spaces)
}

// upstream: Editor Controller > Enter honors increaseIndentPattern
#[test]
fn enter_honors_increase_indent_pattern() {
    let mut e = indent_rules_editor(&["if (true) {", "\tif (true) {"], false);
    move_to(&mut e, 1, 12);
    e.type_text("\n");
    assert_eq!(at(&e), (2, 2));
    move_to(&mut e, 3, 13);
    e.type_text("\n");
    assert_eq!(at(&e), (4, 3));
}

// upstream: Editor Controller > Type honors decreaseIndentPattern
#[test]
fn type_honors_decrease_indent_pattern() {
    let mut e = indent_rules_editor(&["if (true) {", "\t"], true);
    move_to(&mut e, 2, 2);
    e.type_text("}");
    assert_eq!(at(&e), (2, 2));
    assert_eq!(line(&e, 2), "}");
}

// upstream: Editor Controller > Enter honors unIndentedLinePattern
#[test]
fn enter_honors_un_indented_line_pattern() {
    let mut e = indent_rules_editor(&["if (true) {", "\t\t\treturn true"], false);
    move_to(&mut e, 2, 15);
    e.type_text("\n");
    assert_eq!(at(&e), (3, 2));
}

// upstream: Editor Controller > Enter honors indentNextLinePattern
#[test]
fn enter_honors_indent_next_line_pattern() {
    let mut e = indent_rules_editor(&["if (true)", "\treturn true;", "if (true)", "\t\t\t\treturn true"], false);
    move_to(&mut e, 2, 14);
    e.type_text("\n");
    assert_eq!(at(&e), (3, 1));
    move_to(&mut e, 5, 16);
    e.type_text("\n");
    assert_eq!(at(&e), (6, 2));
}

// upstream: Editor Controller > Enter honors indentNextLinePattern 2
#[test]
fn enter_honors_indent_next_line_pattern_2() {
    let mut e = indent_rules_editor(&["if (true)", "\tif (true)"], false);
    move_to(&mut e, 2, 11);
    e.type_text("\n");
    assert_eq!(at(&e), (3, 3));
    e.type_text("console.log();");
    e.type_text("\n");
    assert_eq!(at(&e), (4, 1));
}

// upstream: Editor Controller > Enter honors intential indent
#[test]
fn enter_honors_intential_indent() {
    let mut e = indent_rules_editor(&["if (true) {", "\tif (true) {", "return true;", "}}"], true);
    move_to(&mut e, 3, 13);
    e.type_text("\n");
    assert_eq!(at(&e), (4, 1));
    assert_eq!(line(&e, 3), "return true;");
}

// upstream: Editor Controller > Enter supports selection 1
#[test]
fn enter_supports_selection_1() {
    let mut e = indent_rules_editor(&["if (true) {", "\tif (true) {", "\t\treturn true;", "\t}a}"], false);
    select(&mut e, (4, 3), (4, 4));
    e.type_text("\n");
    assert_eq!(at(&e), (5, 1));
    assert_eq!(line(&e, 4), "\t}");
}

// upstream: Editor Controller > Enter supports selection 2
#[test]
fn enter_supports_selection_2() {
    let mut e = indent_rules_editor(&["if (true) {", "\tif (true) {"], false);
    select(&mut e, (2, 12), (2, 13));
    e.type_text("\n");
    assert_eq!(at(&e), (3, 3));
    e.type_text("\n");
    assert_eq!(at(&e), (4, 3));
}

// upstream: Editor Controller > Enter honors tabSize and insertSpaces 1
#[test]
fn enter_honors_tab_size_and_insert_spaces_1() {
    let mut e = indent_rules_editor(&["if (true) {", "\tif (true) {"], true);
    move_to(&mut e, 1, 12);
    e.type_text("\n");
    assert_eq!(at(&e), (2, 5));
    move_to(&mut e, 3, 13);
    e.type_text("\n");
    assert_eq!(at(&e), (4, 9));
}

// upstream: Editor Controller > Enter honors tabSize and insertSpaces 2
#[test]
fn enter_honors_tab_size_and_insert_spaces_2() {
    let mut e = indent_rules_editor(&["if (true) {", "    if (true) {"], true);
    move_to(&mut e, 1, 12);
    e.type_text("\n");
    assert_eq!(at(&e), (2, 5));
    move_to(&mut e, 3, 16);
    e.type_text("\n");
    assert_eq!(line(&e, 3), "    if (true) {");
    assert_eq!(at(&e), (4, 9));
}

// upstream: Editor Controller > Enter honors tabSize and insertSpaces 3
#[test]
fn enter_honors_tab_size_and_insert_spaces_3() {
    let mut e = indent_rules_editor(&["if (true) {", "    if (true) {"], false);
    move_to(&mut e, 1, 12);
    e.type_text("\n");
    assert_eq!(at(&e), (2, 2));
    move_to(&mut e, 3, 16);
    e.type_text("\n");
    assert_eq!(line(&e, 3), "    if (true) {");
    assert_eq!(at(&e), (4, 3));
}

// upstream: Editor Controller > onEnter works if there are no indentation rules
#[test]
fn on_enter_works_if_there_are_no_indentation_rules() {
    let mut e = editor(&["<?", "\tif (true) {", "\t\techo $hi;", "\t\techo $bye;", "\t}", "?>"], None, false);
    move_to(&mut e, 5, 3);
    e.type_text("\n");
    assert_eq!(line(&e, 6), "\t");
    assert_eq!(at(&e), (6, 2));
    assert_eq!(line(&e, 5), "\t}");
}

// upstream: Editor Controller > onEnter works if there are no indentation rules 2
#[test]
fn on_enter_works_if_there_are_no_indentation_rules_2() {
    let mut e = editor(&["\tif (5)", "\t\treturn 5;", "\t"], None, false);
    move_to(&mut e, 3, 2);
    e.type_text("\n");
    assert_eq!(at(&e), (4, 2));
    assert_eq!(line(&e, 4), "\t");
}

// ---- electric characters ----------------------------------------------------------------------------

fn electric_char_language() -> LanguageConfiguration {
    // `__electricCharacterSupport.docComment` adds the `/**` pair; the pairs are listed here.
    LanguageConfiguration {
        brackets: Some(vec![("{".into(), "}".into()), ("[".into(), "]".into()), ("(".into(), ")".into())]),
        auto_closing_pairs: Some(vec![
            AutoClosingPairConfig::new("{", "}"),
            AutoClosingPairConfig::new("[", "]"),
            AutoClosingPairConfig::new("(", ")"),
            AutoClosingPairConfig::new("/**", " */"),
        ]),
        ..Default::default()
    }
}

fn test_electric_character(text: &[&str], line_number: usize, column: usize, chr: &str, expected: &str) {
    let mut e = editor(text, Some(electric_char_language()), true);
    move_to(&mut e, line_number, column);
    e.type_text(chr);
    assert_eq!(line(&e, line_number), expected);
}

// upstream: Editor Controller > ElectricCharacter - does nothing if no electric char
#[test]
fn electric_character_does_nothing_if_no_electric_char() {
    test_electric_character(&["  if (a) {", ""], 2, 1, "*", "*");
}

// upstream: Editor Controller > ElectricCharacter - indents in order to match bracket
#[test]
fn electric_character_indents_in_order_to_match_bracket() {
    test_electric_character(&["  if (a) {", ""], 2, 1, "}", "  }");
}

// upstream: Editor Controller > ElectricCharacter - unindents in order to match bracket
#[test]
fn electric_character_unindents_in_order_to_match_bracket() {
    test_electric_character(&["  if (a) {", "    "], 2, 5, "}", "  }");
}

// upstream: Editor Controller > ElectricCharacter - matches with correct bracket
#[test]
fn electric_character_matches_with_correct_bracket() {
    test_electric_character(&["  if (a) {", "    if (b) {", "    }", "    "], 4, 1, "}", "  }    ");
}

// upstream: Editor Controller > ElectricCharacter - does nothing if bracket does not match
#[test]
fn electric_character_does_nothing_if_bracket_does_not_match() {
    test_electric_character(&["  if (a) {", "    if (b) {", "    }", "  }  "], 4, 6, "}", "  }  }");
}

// upstream: Editor Controller > ElectricCharacter - matches bracket even in line with content
#[test]
fn electric_character_matches_bracket_even_in_line_with_content() {
    test_electric_character(&["  if (a) {", "// hello"], 2, 1, "}", "  }// hello");
}

// upstream: Editor Controller > ElectricCharacter - is no-op if bracket is lined up
#[test]
fn electric_character_is_no_op_if_bracket_is_lined_up() {
    test_electric_character(&["  if (a) {", "  "], 2, 3, "}", "  }");
}

// upstream: Editor Controller > ElectricCharacter - is no-op if there is non-whitespace text before
#[test]
fn electric_character_is_no_op_if_there_is_non_whitespace_text_before() {
    test_electric_character(&["  if (a) {", "a"], 2, 2, "}", "a}");
}

// upstream: Editor Controller > ElectricCharacter - is no-op if pairs are all matched before
#[test]
fn electric_character_is_no_op_if_pairs_are_all_matched_before() {
    test_electric_character(&["foo(() => {", "  ( 1 + 2 ) ", "})"], 2, 13, "*", "  ( 1 + 2 ) *");
}

// upstream: Editor Controller > ElectricCharacter - appends text
#[test]
fn electric_character_appends_text() {
    test_electric_character(&["  if (a) {", "/*"], 2, 3, "*", "/** */");
}

// upstream: Editor Controller > ElectricCharacter - appends text 2
#[test]
fn electric_character_appends_text_2() {
    test_electric_character(&["  if (a) {", "  /*"], 2, 5, "*", "  /** */");
}

// upstream: Editor Controller > ElectricCharacter - issue #23711: Replacing selected text with )]} fails to delete old text with backwards-dragged selection
#[test]
fn electric_character_issue_23711_replacing_selected_text_with_bracket() {
    let mut e = editor(&["{", "word"], Some(electric_char_language()), true);
    select(&mut e, (2, 5), (2, 1));
    e.type_text("}");
    assert_eq!(line(&e, 2), "}");
    let _ = Selection::caret(Position::new(1, 1));
}
