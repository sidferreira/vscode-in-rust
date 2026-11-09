//! The monospace line breaks computer, ported from upstream's tests.
//! upstream: src/vs/editor/test/common/viewModel/monospaceLineBreaksComputer.test.ts @ 50e1350fc78d
//! upstream: src/vs/editor/test/common/viewModel/lineBreakData.test.ts @ 50e1350fc78d
//!
//! Not ported: the nine tests about injected text (rscode has no injected text), and the
//! "from previous line breaks" half of `assertIncrementalLineBreaks` (the resize shortcut
//! `createLineBreaksFromPreviousLineBreaks` is not ported, a resize recomputes with the direct
//! algorithm, whose output the other two assertions check). The `lineBreakData.test.ts` suites
//! `Injected Text 1` and `Injected Text 2` are left out for the same reason; its `Basic` test is here.
//!
//! The helpers mirror the upstream test file: `|` in an annotated text marks a view line break,
//! and the font has half-width 7 and full-width `7 * columnsForFullWidthChar`.

use vsr_editor::wrap::{
    BreakParams, LineBreaksComputer, ModelLineProjectionData, PositionAffinity, WordBreak, WrappingIndent,
};

// The defaults of EditorOptions.wordWrapBreakBeforeCharacters / wordWrapBreakAfterCharacters.
fn default_factory() -> LineBreaksComputer {
    LineBreaksComputer::with_defaults()
}

fn parse_annotated(annotated: &str) -> String {
    annotated.chars().filter(|&c| c != '|').collect()
}

fn to_annotated(text: &str, data: Option<&ModelLineProjectionData>) -> String {
    let Some(data) = data else { return text.to_string() };
    let mut out = String::new();
    let mut previous = 0;
    let mut offset = 0;
    for ch in text.chars() {
        let r = data.translate_to_output_position(offset, PositionAffinity::None);
        if previous != r.output_line_index {
            previous = r.output_line_index;
            out.push('|');
        }
        out.push(ch);
        offset += ch.len_utf16();
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn line_break_data(
    factory: &LineBreaksComputer,
    tab_size: usize,
    break_after: i64,
    columns_for_full_width_char: f64,
    indent: WrappingIndent,
    word_break: WordBreak,
    wrap_on_escaped_line_feeds: bool,
    text: &str,
    use_two_cell: bool,
) -> Option<ModelLineProjectionData> {
    let params = BreakParams {
        tab_size,
        wrapping_column: break_after,
        wrapping_indent: indent,
        word_break,
        wrap_on_escaped_line_feeds,
        half_width: 7.0,
        full_width: 7.0 * columns_for_full_width_char,
        use_two_cell_fullwidth: use_two_cell,
    };
    factory.compute(text, &params)
}

fn assert_line_breaks(
    factory: &LineBreaksComputer,
    tab_size: usize,
    break_after: i64,
    annotated: &str,
    indent: WrappingIndent,
    word_break: WordBreak,
) -> Option<ModelLineProjectionData> {
    let text = parse_annotated(annotated);
    let data = line_break_data(factory, tab_size, break_after, 2.0, indent, word_break, false, &text, false);
    assert_eq!(to_annotated(&text, data.as_ref()), annotated);
    data
}

fn alb(factory: &LineBreaksComputer, tab: usize, after: i64, annotated: &str) {
    assert_line_breaks(factory, tab, after, annotated, WrappingIndent::None, WordBreak::Normal);
}

fn albi(factory: &LineBreaksComputer, tab: usize, after: i64, annotated: &str, indent: WrappingIndent) -> Option<ModelLineProjectionData> {
    assert_line_breaks(factory, tab, after, annotated, indent, WordBreak::Normal)
}

#[allow(clippy::too_many_arguments)]
fn assert_incremental(
    factory: &LineBreaksComputer,
    text: &str,
    tab_size: usize,
    break_after1: i64,
    annotated1: &str,
    break_after2: i64,
    annotated2: &str,
    indent: WrappingIndent,
    columns_for_full_width_char: f64,
    use_two_cell: bool,
) {
    // sanity check the test
    assert_eq!(text, parse_annotated(annotated1));
    assert_eq!(text, parse_annotated(annotated2));
    // the direct mapping for 1 and for 2 (the "from previous" assertions are not ported)
    let d1 = line_break_data(factory, tab_size, break_after1, columns_for_full_width_char, indent, WordBreak::Normal, false, text, use_two_cell);
    assert_eq!(to_annotated(text, d1.as_ref()), annotated1);
    let d2 = line_break_data(factory, tab_size, break_after2, columns_for_full_width_char, indent, WordBreak::Normal, false, text, use_two_cell);
    assert_eq!(to_annotated(text, d2.as_ref()), annotated2);
}

fn every_char_on_its_own_line(text: &str) -> String {
    text.chars().map(String::from).collect::<Vec<_>>().join("|")
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer
#[test]
fn monospace_line_breaks_computer() {
    let factory = LineBreaksComputer::new("(", "\t).");

    // Empty string
    alb(&factory, 4, 5, "");

    // No wrapping if not necessary
    alb(&factory, 4, 5, "aaa");
    alb(&factory, 4, 5, "aaaaa");
    alb(&factory, 4, -1, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");

    // Acts like hard wrapping if no char found
    alb(&factory, 4, 5, "aaaaa|a");

    // Honors wrapping character
    alb(&factory, 4, 5, "aaaaa|.");
    alb(&factory, 4, 5, "aaaaa|a.|aaa.|aa");
    alb(&factory, 4, 5, "aaaaa|a..|aaa.|aa");
    alb(&factory, 4, 5, "aaaaa|a...|aaa.|aa");
    alb(&factory, 4, 5, "aaaaa|a....|aaa.|aa");

    // Honors tabs when computing wrapping position
    alb(&factory, 4, 5, "\t");
    alb(&factory, 4, 5, "\t|aaa");
    alb(&factory, 4, 5, "\t|a\t|aa");
    alb(&factory, 4, 5, "aa\ta");
    alb(&factory, 4, 5, "aa\t|aa");

    // Honors wrapping before characters (& gives it priority)
    alb(&factory, 4, 5, "aaa.|aa");
    alb(&factory, 4, 5, "aaa(.|aa");

    // Honors wrapping after characters (& gives it priority)
    alb(&factory, 4, 5, "aaa))|).aaa");
    alb(&factory, 4, 5, "aaa))|).|aaaa");
    alb(&factory, 4, 5, "aaa)|().|aaa");
    alb(&factory, 4, 5, "aaa|(().|aaa");
    alb(&factory, 4, 5, "aa.|(().|aaa");
    alb(&factory, 4, 5, "aa.|(.).|aaa");
}

#[allow(clippy::too_many_arguments)]
fn inc(factory: &LineBreaksComputer, text: &str, tab: usize, b1: i64, a1: &str, b2: i64, a2: &str, indent: WrappingIndent) {
    assert_incremental(factory, text, tab, b1, a1, b2, a2, indent, 2.0, false);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer incremental 1
#[test]
fn monospace_line_breaks_computer_incremental_1() {
    let factory = default_factory();
    let none = WrappingIndent::None;

    inc(&factory, "just some text and more", 4, 10, "just some |text and |more", 15, "just some text |and more", none);

    inc(
        &factory,
        "Cu scripserit suscipiantur eos, in affert pericula contentiones sed, cetero sanctus et pro. Ius vidit magna regione te, sit ei elaboraret liberavisse. Mundi verear eu mea, eam vero scriptorem in, vix in menandri assueverit. Natum definiebas cu vim. Vim doming vocibus efficiantur id. In indoctum deseruisse voluptatum vim, ad debitis verterem sed.",
        4,
        47,
        "Cu scripserit suscipiantur eos, in affert |pericula contentiones sed, cetero sanctus et |pro. Ius vidit magna regione te, sit ei |elaboraret liberavisse. Mundi verear eu mea, |eam vero scriptorem in, vix in menandri |assueverit. Natum definiebas cu vim. Vim |doming vocibus efficiantur id. In indoctum |deseruisse voluptatum vim, ad debitis verterem |sed.",
        142,
        "Cu scripserit suscipiantur eos, in affert pericula contentiones sed, cetero sanctus et pro. Ius vidit magna regione te, sit ei elaboraret |liberavisse. Mundi verear eu mea, eam vero scriptorem in, vix in menandri assueverit. Natum definiebas cu vim. Vim doming vocibus efficiantur |id. In indoctum deseruisse voluptatum vim, ad debitis verterem sed.",
        none,
    );

    inc(
        &factory,
        "An his legere persecuti, oblique delicata efficiantur ex vix, vel at graecis officiis maluisset. Et per impedit voluptua, usu discere maiorum at. Ut assum ornatus temporibus vis, an sea melius pericula. Ea dicunt oblique phaedrum nam, eu duo movet nobis. His melius facilis eu, vim malorum temporibus ne. Nec no sale regione, meliore civibus placerat id eam. Mea alii fabulas definitionem te, agam volutpat ad vis, et per bonorum nonumes repudiandae.",
        4,
        57,
        "An his legere persecuti, oblique delicata efficiantur ex |vix, vel at graecis officiis maluisset. Et per impedit |voluptua, usu discere maiorum at. Ut assum ornatus |temporibus vis, an sea melius pericula. Ea dicunt |oblique phaedrum nam, eu duo movet nobis. His melius |facilis eu, vim malorum temporibus ne. Nec no sale |regione, meliore civibus placerat id eam. Mea alii |fabulas definitionem te, agam volutpat ad vis, et per |bonorum nonumes repudiandae.",
        58,
        "An his legere persecuti, oblique delicata efficiantur ex |vix, vel at graecis officiis maluisset. Et per impedit |voluptua, usu discere maiorum at. Ut assum ornatus |temporibus vis, an sea melius pericula. Ea dicunt oblique |phaedrum nam, eu duo movet nobis. His melius facilis eu, |vim malorum temporibus ne. Nec no sale regione, meliore |civibus placerat id eam. Mea alii fabulas definitionem |te, agam volutpat ad vis, et per bonorum nonumes |repudiandae.",
        none,
    );

    let same = WrappingIndent::Same;
    inc(&factory, "\t\t\"owner\": \"vscode\",", 4, 14, "\t\t\"owner|\": |\"vscod|e\",", 16, "\t\t\"owner\":| |\"vscode\"|,", same);

    inc(
        &factory,
        "🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇&👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬",
        4,
        51,
        "🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇&|👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬",
        50,
        "🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇|&|👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬",
        same,
    );

    inc(&factory, "🐇👬&🌞🌖", 4, 5, "🐇👬&|🌞🌖", 4, "🐇👬|&|🌞🌖", same);

    inc(
        &factory,
        "\t\tfunc('🌞🏇🍼🌞🏇🍼🐇&👬🌖🌞👬🌖🌞🏇🍼🐇👬', WrappingIndent.Same);",
        4,
        26,
        "\t\tfunc|('🌞🏇🍼🌞🏇🍼🐇&|👬🌖🌞👬🌖🌞🏇🍼🐇|👬', |WrappingIndent.|Same);",
        27,
        "\t\tfunc|('🌞🏇🍼🌞🏇🍼🐇&|👬🌖🌞👬🌖🌞🏇🍼🐇|👬', |WrappingIndent.|Same);",
        same,
    );

    inc(
        &factory,
        "factory, \"xtxtfunc(x\"🌞🏇🍼🌞🏇🍼🐇&👬🌖🌞👬🌖🌞🏇🍼🐇👬x\"",
        4,
        16,
        "factory, |\"xtxtfunc|(x\"🌞🏇🍼🌞🏇🍼|🐇&|👬🌖🌞👬🌖🌞🏇🍼|🐇👬x\"",
        17,
        "factory, |\"xtxtfunc|(x\"🌞🏇🍼🌞🏇🍼🐇|&👬🌖🌞👬🌖🌞🏇🍼|🐇👬x\"",
        same,
    );
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #95686: CRITICAL: loop forever on the monospaceLineBreaksComputer
#[test]
fn issue_95686_loop_forever() {
    let factory = default_factory();
    let text = "\t\t\t\t\t\t<tr dmx-class:table-danger=\"(alt <= 50)\" dmx-class:table-warning=\"(alt <= 200)\" dmx-class:table-primary=\"(alt <= 400)\" dmx-class:table-info=\"(alt <= 800)\" dmx-class:table-success=\"(alt >= 400)\">";
    let first = "\t\t\t\t\t\t<tr dmx-class:table-danger=\"(alt <= 50)\" dmx-class:table-warning=\"(alt <= 200)\" dmx-class:table-primary=\"(alt <= 400)\" dmx-class:table-info=\"(alt <= 800)\" |dmx-class:table-success=\"(alt >= 400)\">";
    // upstream spells the second annotation out: a break between every two characters
    let second = every_char_on_its_own_line(text);
    assert_incremental(&factory, text, 4, 179, first, 1, &second, WrappingIndent::Same, 2.0, false);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #110392: Occasional crash when resize with panel on the right
#[test]
fn issue_110392_crash_on_resize() {
    let factory = default_factory();
    let text = "你好 **hello** **hello** **hello-world** hey there!";
    let second = every_char_on_its_own_line(text);
    assert_incremental(
        &factory,
        text,
        4,
        15,
        "你好 **hello** |**hello** |**hello-world**| hey there!",
        1,
        &second,
        WrappingIndent::Same,
        1.6605405405405405,
        false,
    );
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - CJK and Kinsoku Shori
#[test]
fn cjk_and_kinsoku_shori() {
    let factory = LineBreaksComputer::new("(", "\t)");
    alb(&factory, 4, 5, "aa \u{5b89}|\u{5b89}");
    alb(&factory, 4, 5, "\u{3042} \u{5b89}|\u{5b89}");
    alb(&factory, 4, 5, "\u{3042}\u{3042}|\u{5b89}\u{5b89}");
    alb(&factory, 4, 5, "aa |\u{5b89})\u{5b89}|\u{5b89}");
    alb(&factory, 4, 5, "aa \u{3042}|\u{5b89}\u{3042})|\u{5b89}");
    alb(&factory, 4, 5, "aa |(\u{5b89}aa|\u{5b89}");
}

fn wrap_with(factory: &LineBreaksComputer, text: &str, break_after: i64, columns: f64, two_cell: bool) -> String {
    let d = line_break_data(factory, 4, break_after, columns, WrappingIndent::None, WordBreak::Normal, false, text, two_cell);
    to_annotated(text, d.as_ref())
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - two-cell full-width character width
#[test]
fn two_cell_full_width_character_width() {
    let factory = LineBreaksComputer::new("(", "\t)");
    let text = "\u{3042}\u{3042}\u{3042}\u{3042}";
    assert_eq!(wrap_with(&factory, text, 5, 3.0, false), "\u{3042}|\u{3042}|\u{3042}|\u{3042}"); // lyingFont
    assert_eq!(wrap_with(&factory, text, 5, 3.0, true), "\u{3042}\u{3042}|\u{3042}\u{3042}"); // lyingFontTwoCells
    assert_eq!(wrap_with(&factory, text, 5, 2.0, false), "\u{3042}\u{3042}|\u{3042}\u{3042}"); // gridAlignedFont
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - two-cell width determines whether continuation indentation fits
#[test]
fn two_cell_width_determines_whether_continuation_indentation_fits() {
    let factory = LineBreaksComputer::new("(", "\t)");
    let text = "    \u{3042}\u{3042}\u{3042}";
    let indent = |break_after: i64, columns: f64, two_cell: bool| {
        line_break_data(&factory, 4, break_after, columns, WrappingIndent::Same, WordBreak::Normal, false, text, two_cell)
            .map(|d| d.wrapped_text_indent_length)
    };
    assert_eq!(indent(6, 3.0, false), Some(0)); // wideNatural
    assert_eq!(indent(6, 3.0, true), Some(4)); // wideTwoCells
    assert_eq!(indent(5, 1.0, false), Some(4)); // narrowNatural
    assert_eq!(indent(5, 1.0, true), Some(0)); // narrowTwoCells
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - two-cell width uses shared full-width classification
#[test]
fn two_cell_width_uses_shared_full_width_classification() {
    let factory = LineBreaksComputer::new("(", "\t)");
    let fullwidth_ascii = "\u{ff21}".repeat(4);
    let vai = "\u{a500}".repeat(4);
    assert_eq!(wrap_with(&factory, &fullwidth_ascii, 5, 3.0, false), "\u{ff21}|\u{ff21}|\u{ff21}|\u{ff21}");
    assert_eq!(wrap_with(&factory, &fullwidth_ascii, 5, 3.0, true), "\u{ff21}\u{ff21}|\u{ff21}\u{ff21}");
    assert_eq!(wrap_with(&factory, &vai, 5, 3.0, false), "\u{a500}|\u{a500}|\u{a500}|\u{a500}");
    assert_eq!(wrap_with(&factory, &vai, 5, 3.0, true), "\u{a500}\u{a500}|\u{a500}\u{a500}");
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - two-cell width preserves natural control-character metrics
#[test]
fn two_cell_width_preserves_natural_control_character_metrics() {
    let factory = LineBreaksComputer::new("(", "\t)");
    let control = "\u{1}\u{1}";
    assert_eq!(wrap_with(&factory, control, 4, 3.0, false), "\u{1}|\u{1}");
    assert_eq!(wrap_with(&factory, control, 4, 3.0, true), "\u{1}|\u{1}");
    let with_tab = "\u{1}\t";
    let columns = |two_cell: bool| {
        line_break_data(&factory, 4, 3, 3.0, WrappingIndent::None, WordBreak::Normal, false, with_tab, two_cell)
            .map(|d| d.break_offsets_visible_column)
    };
    assert_eq!(columns(false), Some(vec![3.0, 4.0]));
    assert_eq!(columns(true), Some(vec![3.0, 4.0]));
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - two-cell width treats combining marks independently
#[test]
fn two_cell_width_treats_combining_marks_independently() {
    let factory = LineBreaksComputer::new("(", "\t)");
    let combined = "\u{3042}\u{301}aa";
    assert_eq!(wrap_with(&factory, combined, 4, 3.0, false), "\u{3042}|\u{301}aa");
    assert_eq!(wrap_with(&factory, combined, 4, 3.0, true), "\u{3042}|\u{301}aa");
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - two-cell width supports incremental line breaks
#[test]
fn two_cell_width_supports_incremental_line_breaks() {
    let factory = default_factory();
    // A font that lies about its full-width ratio: each ideograph still occupies exactly two cells.
    assert_incremental(&factory, "ああああああ", 4, 5, "ああ|ああ|ああ", 7, "あああ|あああ", WrappingIndent::None, 3.0, true);
    assert_incremental(&factory, "    ああああ", 4, 8, "    ああ|ああ", 10, "    あああ|あ", WrappingIndent::Same, 3.0, true);
    let text = "你好 **hello** **hello** **hello-world** hey there!";
    let second = every_char_on_its_own_line(text);
    assert_incremental(
        &factory,
        text,
        4,
        15,
        "你好 **hello** |**hello** |**hello-world**| hey there!",
        1,
        &second,
        WrappingIndent::Same,
        1.6605405405405405,
        true,
    );
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - WrappingIndent.Same
#[test]
fn wrapping_indent_same() {
    let factory = LineBreaksComputer::new("", "\t ");
    albi(&factory, 4, 38, " *123456789012345678901234567890123456|7890", WrappingIndent::Same);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #16332: Scroll bar overlaying on top of text
#[test]
fn issue_16332_scroll_bar_overlaying_on_top_of_text() {
    let factory = LineBreaksComputer::new("", "\t ");
    albi(&factory, 4, 24, "a/ very/long/line/of/tex|t/that/expands/beyon|d/your/typical/line/|of/code/", WrappingIndent::Indent);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #35162: wrappingIndent not consistently working
#[test]
fn issue_35162_wrapping_indent_not_consistently_working() {
    let factory = LineBreaksComputer::new("", "\t ");
    let mapper = albi(&factory, 4, 24, "                t h i s |i s |a l |o n |g l |i n |e", WrappingIndent::Indent);
    assert_eq!(mapper.unwrap().wrapped_text_indent_length, "                    ".len());
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #75494: surrogate pairs
#[test]
fn issue_75494_surrogate_pairs() {
    let factory = LineBreaksComputer::new("\t", " ");
    albi(
        &factory,
        4,
        49,
        "🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼|🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼🐇👬🌖🌞🏇🍼|🐇👬",
        WrappingIndent::Same,
    );
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #75494: surrogate pairs overrun 1
#[test]
fn issue_75494_surrogate_pairs_overrun_1() {
    albi(&default_factory(), 4, 4, "🐇👬|&|🌞🌖", WrappingIndent::Same);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #75494: surrogate pairs overrun 2
#[test]
fn issue_75494_surrogate_pairs_overrun_2() {
    albi(&default_factory(), 4, 17, "factory, |\"xtxtfunc|(x\"🌞🏇🍼🌞🏇🍼🐇|&👬🌖🌞👬🌖🌞🏇🍼|🐇👬x\"", WrappingIndent::Same);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > MonospaceLineBreaksComputer - WrappingIndent.DeepIndent
#[test]
fn wrapping_indent_deep_indent() {
    let factory = LineBreaksComputer::new("", "\t ");
    let mapper = albi(&factory, 4, 26, "        W e A r e T e s t |i n g D e |e p I n d |e n t a t |i o n", WrappingIndent::DeepIndent);
    assert_eq!(mapper.unwrap().wrapped_text_indent_length, "                ".len());
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #33366: Word wrap algorithm behaves differently around punctuation
#[test]
fn issue_33366_punctuation() {
    albi(&default_factory(), 4, 23, "this is a line of |text, text that sits |on a line", WrappingIndent::Same);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #152773: Word wrap algorithm behaves differently with bracket followed by comma
#[test]
fn issue_152773_bracket_followed_by_comma() {
    albi(&default_factory(), 4, 24, "this is a line of |(text), text that sits |on a line", WrappingIndent::Same);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #112382: Word wrap doesn't work well with control characters
#[test]
fn issue_112382_control_characters() {
    albi(&default_factory(), 4, 6, "\u{6}\u{6}\u{6}|\u{6}\u{6}\u{6}", WrappingIndent::Same);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > Word break work well with Chinese/Japanese/Korean (CJK) text when setting normal
#[test]
fn word_break_cjk_normal() {
    assert_line_breaks(&default_factory(), 4, 5, "你好|1111", WrappingIndent::Same, WordBreak::Normal);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > Word break work well with Chinese/Japanese/Korean (CJK) text when setting keepAll
#[test]
fn word_break_cjk_keep_all() {
    assert_line_breaks(&default_factory(), 4, 8, "你好1111", WrappingIndent::Same, WordBreak::KeepAll);
}

// upstream: Editor ViewModel - MonospaceLineBreaksComputer > issue #258022: wrapOnEscapedLineFeeds: should work correctly after editor resize
#[test]
fn issue_258022_wrap_on_escaped_line_feeds() {
    let factory = default_factory();
    let text = "\"Short text with\\nescaped newline and an escaped\\\\nbackslash\"";
    let at = |column: i64| {
        let d = line_break_data(&factory, 4, column, 2.0, WrappingIndent::None, WordBreak::Normal, true, text, false);
        to_annotated(text, d.as_ref())
    };
    // the escaped \n triggers a wrap at the initial width and at the resized width (upstream also
    // compares the incremental computation with the fresh one; only the fresh one is ported)
    assert!(at(30).contains("with\\n"), "{}", at(30));
    assert!(at(35).contains("with\\n"), "{}", at(35));
    assert!(at(30).contains("with\\n|"), "the break follows the escaped line feed");
}

// upstream: Editor ViewModel - LineBreakData > Basic
#[test]
fn line_break_data_basic() {
    let data = ModelLineProjectionData::new(vec![100], vec![0.0], 10);
    assert_eq!(data.translate_to_input_offset(0, 50), 50);
    assert_eq!(data.translate_to_input_offset(1, 60), 150);
}
