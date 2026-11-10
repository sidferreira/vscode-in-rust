//! upstream: src/vs/base/common/filters.ts @ 50e1350fc78d
//! upstream: src/vs/base/test/common/filters.test.ts @ 50e1350fc78d
//!
//! Ports of the fuzzy-score tests of VS Code's filters suite (the `fuzzyScore`, graceful and
//! `anyScore` cases). The prefix, camel case, word and substring filters are not ported yet.
//!
//! Conventions: `assertMatches` becomes `assert_matches` and renders highlights as upstream does
//! (`^` before each highlighted character). `FuzzyScoreOptions` defaults to upstream's
//! `{ boostFullMatch: true, firstMatchCanBeWeak: false }` unless a test sets an option. Each test
//! names its suite and test in a `// upstream:` comment.

use vsr_suggest::filters::{
    FuzzyScore, FuzzyScoreOptions, any_score, create_matches, fuzzy_score, fuzzy_score_graceful,
    fuzzy_score_graceful_aggressive,
};

type Scorer = fn(&str, &str, usize, &str, &str, usize, FuzzyScoreOptions) -> Option<FuzzyScore>;

fn any_scorer(
    pattern: &str,
    pattern_low: &str,
    pattern_pos: usize,
    word: &str,
    word_low: &str,
    word_pos: usize,
    _options: FuzzyScoreOptions,
) -> Option<FuzzyScore> {
    Some(any_score(
        pattern,
        pattern_low,
        pattern_pos,
        word,
        word_low,
        word_pos,
    ))
}

#[derive(Default)]
struct Opts {
    pattern_pos: usize,
    word_pos: usize,
    first_match_can_be_weak: bool,
}

/// Upstream `assertMatches`: the highlighted word (or `None` for no match).
fn assert_matches_with(
    pattern: &str,
    word: &str,
    decorated: Option<&str>,
    filter: Scorer,
    opts: Opts,
) {
    let options = FuzzyScoreOptions {
        first_match_can_be_weak: opts.first_match_can_be_weak,
        boost_full_match: true,
    };
    let result = filter(
        pattern,
        &pattern.to_lowercase(),
        opts.pattern_pos,
        word,
        &word.to_lowercase(),
        opts.word_pos,
        options,
    );
    assert_eq!(
        decorated.is_some(),
        result.is_some(),
        "match presence for {pattern:?} in {word:?}"
    );
    if let Some(decorated) = decorated {
        let chars: Vec<char> = word.chars().collect();
        let mut actual = String::new();
        let mut pos = 0;
        for m in create_matches(result.as_ref()) {
            actual.extend(&chars[pos..m.start]);
            actual.push('^');
            for c in &chars[m.start..m.end] {
                actual.push(*c);
                actual.push('^');
            }
            actual.pop();
            pos = m.end;
        }
        actual.extend(&chars[pos..]);
        assert_eq!(actual, decorated, "highlights for {pattern:?} in {word:?}");
    }
}

fn assert_matches(pattern: &str, word: &str, decorated: Option<&str>, filter: Scorer) {
    assert_matches_with(pattern, word, decorated, filter, Opts::default());
}

/// Upstream `assertTopScore`: the index of the best-scoring word, the first one on a tie.
fn assert_top_score(filter: Scorer, pattern: &str, expected: usize, words: &[&str]) {
    let mut top_score = -(100 * 10);
    let mut top_idx = 0;
    for (i, word) in words.iter().enumerate() {
        let result = filter(
            pattern,
            &pattern.to_lowercase(),
            0,
            word,
            &word.to_lowercase(),
            0,
            FuzzyScoreOptions::default(),
        );
        if let Some(m) = result.filter(|m| m.score > top_score) {
            top_score = m.score;
            top_idx = i;
        }
    }
    assert_eq!(
        top_idx, expected,
        "{pattern} -> actual={} expected={}",
        words[top_idx], words[expected]
    );
}

const FZ: Scorer = fuzzy_score;

// upstream: Filters > fuzzyScore, #23215
#[test]
fn fuzzy_score_issue_23215() {
    assert_matches("tit", "win.tit", Some("win.^t^i^t"), FZ);
    assert_matches("title", "win.title", Some("win.^t^i^t^l^e"), FZ);
    assert_matches(
        "WordCla",
        "WordCharacterClassifier",
        Some("^W^o^r^dCharacter^C^l^assifier"),
        FZ,
    );
    assert_matches(
        "WordCCla",
        "WordCharacterClassifier",
        Some("^W^o^r^d^Character^C^l^assifier"),
        FZ,
    );
}

// upstream: Filters > fuzzyScore, #23332
#[test]
fn fuzzy_score_issue_23332() {
    assert_matches("dete", "\"editor.quickSuggestionsDelay\"", None, FZ);
}

// upstream: Filters > fuzzyScore, #23190
#[test]
fn fuzzy_score_issue_23190() {
    assert_matches(
        "c:\\do",
        "& 'C:\\Documents and Settings'",
        Some("& '^C^:^\\^D^ocuments and Settings'"),
        FZ,
    );
    assert_matches(
        "c:\\do",
        "& 'c:\\Documents and Settings'",
        Some("& '^c^:^\\^D^ocuments and Settings'"),
        FZ,
    );
}

// upstream: Filters > fuzzyScore, #23581
#[test]
fn fuzzy_score_issue_23581() {
    assert_matches(
        "close",
        "css.lint.importStatement",
        Some("^css.^lint.imp^ort^Stat^ement"),
        FZ,
    );
    assert_matches(
        "close",
        "css.colorDecorators.enable",
        Some("^css.co^l^orDecorator^s.^enable"),
        FZ,
    );
    assert_matches(
        "close",
        "workbench.quickOpen.closeOnFocusOut",
        Some("workbench.quickOpen.^c^l^o^s^eOnFocusOut"),
        FZ,
    );
    assert_top_score(
        FZ,
        "close",
        2,
        &[
            "css.lint.importStatement",
            "css.colorDecorators.enable",
            "workbench.quickOpen.closeOnFocusOut",
        ],
    );
}

// upstream: Filters > fuzzyScore, #23458
#[test]
fn fuzzy_score_issue_23458() {
    assert_matches(
        "highlight",
        "editorHoverHighlight",
        Some("editorHover^H^i^g^h^l^i^g^h^t"),
        FZ,
    );
    assert_matches(
        "hhighlight",
        "editorHoverHighlight",
        Some("editor^Hover^H^i^g^h^l^i^g^h^t"),
        FZ,
    );
    assert_matches("dhhighlight", "editorHoverHighlight", None, FZ);
}

// upstream: Filters > fuzzyScore, #23746
#[test]
fn fuzzy_score_issue_23746() {
    assert_matches("-moz", "-moz-foo", Some("^-^m^o^z-foo"), FZ);
    assert_matches("moz", "-moz-foo", Some("-^m^o^z-foo"), FZ);
    assert_matches("moz", "-moz-animation", Some("-^m^o^z-animation"), FZ);
    assert_matches("moza", "-moz-animation", Some("-^m^o^z-^animation"), FZ);
}

// upstream: Filters > fuzzyScore
#[test]
fn fuzzy_score_cases() {
    assert_matches("ab", "abA", Some("^a^bA"), FZ);
    assert_matches("ccm", "cacmelCase", Some("^ca^c^melCase"), FZ);
    assert_matches("bti", "the_black_knight", None, FZ);
    assert_matches("ccm", "camelCase", None, FZ);
    assert_matches("cmcm", "camelCase", None, FZ);
    assert_matches("BK", "the_black_knight", Some("the_^black_^knight"), FZ);
    assert_matches("KeyboardLayout=", "KeyboardLayout", None, FZ);
    assert_matches(
        "LLL",
        "SVisualLoggerLogsList",
        Some("SVisual^Logger^Logs^List"),
        FZ,
    );
    assert_matches("LLLL", "SVilLoLosLi", None, FZ);
    assert_matches("LLLL", "SVisualLoggerLogsList", None, FZ);
    assert_matches("TEdit", "TextEdit", Some("^Text^E^d^i^t"), FZ);
    assert_matches("TEdit", "TextEditor", Some("^Text^E^d^i^tor"), FZ);
    assert_matches("TEdit", "Textedit", Some("^Text^e^d^i^t"), FZ);
    assert_matches("TEdit", "text_edit", Some("^text_^e^d^i^t"), FZ);
    assert_matches(
        "TEditDit",
        "TextEditorDecorationType",
        Some("^Text^E^d^i^tor^Decorat^ion^Type"),
        FZ,
    );
    assert_matches(
        "TEdit",
        "TextEditorDecorationType",
        Some("^Text^E^d^i^torDecorationType"),
        FZ,
    );
    assert_matches("Tedit", "TextEdit", Some("^Text^E^d^i^t"), FZ);
    assert_matches("ba", "?AB?", None, FZ);
    assert_matches("bkn", "the_black_knight", Some("the_^black_^k^night"), FZ);
    assert_matches("bt", "the_black_knight", Some("the_^black_knigh^t"), FZ);
    assert_matches("ccm", "camelCasecm", Some("^camel^Casec^m"), FZ);
    assert_matches("fdm", "findModel", Some("^fin^d^Model"), FZ);
    assert_matches("fob", "foobar", Some("^f^oo^bar"), FZ);
    assert_matches("fobz", "foobar", None, FZ);
    assert_matches("foobar", "foobar", Some("^f^o^o^b^a^r"), FZ);
    assert_matches(
        "form",
        "editor.formatOnSave",
        Some("editor.^f^o^r^matOnSave"),
        FZ,
    );
    assert_matches("g p", "Git: Pull", Some("^Git:^ ^Pull"), FZ);
    assert_matches("gip", "Git: Pull", Some("^G^it: ^Pull"), FZ);
    assert_matches("gp", "Git: Pull", Some("^Git: ^Pull"), FZ);
    assert_matches("gp", "Git_Git_Pull", Some("^Git_Git_^Pull"), FZ);
    assert_matches("is", "ImportStatement", Some("^Import^Statement"), FZ);
    assert_matches("is", "isValid", Some("^i^sValid"), FZ);
    assert_matches("lowrd", "lowWord", Some("^l^o^wWo^r^d"), FZ);
    assert_matches("myvable", "myvariable", Some("^m^y^v^aria^b^l^e"), FZ);
    assert_matches("no", "", None, FZ);
    assert_matches("no", "match", None, FZ);
    assert_matches("ob", "foobar", None, FZ);
    assert_matches(
        "sl",
        "SVisualLoggerLogsList",
        Some("^SVisual^LoggerLogsList"),
        FZ,
    );
    assert_matches(
        "sllll",
        "SVisualLoggerLogsList",
        Some("^SVisua^l^Logger^Logs^List"),
        FZ,
    );
    assert_matches("Three", "HTMLHRElement", None, FZ);
    assert_matches("Three", "Three", Some("^T^h^r^e^e"), FZ);
    assert_matches("fo", "barfoo", None, FZ);
    assert_matches("fo", "bar_foo", Some("bar_^f^oo"), FZ);
    assert_matches("fo", "bar_Foo", Some("bar_^F^oo"), FZ);
    assert_matches("fo", "bar foo", Some("bar ^f^oo"), FZ);
    assert_matches("fo", "bar.foo", Some("bar.^f^oo"), FZ);
    assert_matches("fo", "bar/foo", Some("bar/^f^oo"), FZ);
    assert_matches("fo", "bar\\foo", Some("bar\\^f^oo"), FZ);
}

// upstream: Filters > fuzzyScore (first match can be weak)
#[test]
fn fuzzy_score_first_match_can_be_weak() {
    let weak = Opts {
        first_match_can_be_weak: true,
        ..Opts::default()
    };
    assert_matches_with(
        "Three",
        "HTMLHRElement",
        Some("H^TML^H^R^El^ement"),
        FZ,
        weak,
    );
    assert_matches_with(
        "tor",
        "constructor",
        Some("construc^t^o^r"),
        FZ,
        Opts {
            first_match_can_be_weak: true,
            ..Opts::default()
        },
    );
    assert_matches_with(
        "ur",
        "constructor",
        Some("constr^ucto^r"),
        FZ,
        Opts {
            first_match_can_be_weak: true,
            ..Opts::default()
        },
    );
    assert_top_score(FZ, "tor", 2, &["constructor", "Thor", "cTor"]);
}

// upstream: Filters > fuzzyScore, many matches
#[test]
fn fuzzy_score_many_matches() {
    assert_matches(
        "aaaaaa",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        Some(
            "^a^a^a^a^a^aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ),
        FZ,
    );
}

// upstream: Filters > Freeze when fjfj -> jfjf, https://github.com/microsoft/vscode/issues/91807
#[test]
fn fuzzy_score_does_not_freeze_on_repeated_pattern() {
    assert_matches("jfjfj", "fjfjfjfjfjfjfjfjfjfjfj", None, FZ);
    assert_matches(
        "jfjfjfjfjfjfjfjfjfj",
        "fjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfj",
        None,
        FZ,
    );
    assert_matches(
        "jfjfjfjfjfjfjfjfjfjjfjfjfjfjfjfjfjfjfjjfjfjfjfjfjfjfjfjfjjfjfjfjfjfjfjfjfjfjjfjfjfjfjfjfjfjfjfjjfjfjfjfjfjfjfjfjfj",
        "fjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfjfj",
        None,
        FZ,
    );
}

// upstream: Filters > fuzzyScore, issue #26423
#[test]
fn fuzzy_score_issue_26423() {
    assert_matches("baba", "abababab", None, FZ);
    assert_matches("fsfsfs", "dsafdsafdsafdsafdsafdsafdsafasdfdsa", None, FZ);
    assert_matches(
        "fsfsfsfsfsfsfsf",
        "dsafdsafdsafdsafdsafdsafdsafasdfdsafdsafdsafdsafdsfdsafdsfdfdfasdnfdsajfndsjnafjndsajlknfdsa",
        None,
        FZ,
    );
}

// upstream: Filters > Fuzzy IntelliSense matching vs Haxe metadata completion, #26995
#[test]
fn fuzzy_score_haxe_metadata() {
    assert_matches("f", ":Foo", Some(":^Foo"), FZ);
    assert_matches("f", ":foo", Some(":^foo"), FZ);
}

// upstream: Filters > Separator only match should not be weak #79558
#[test]
fn fuzzy_score_separator_only_match_is_not_weak() {
    assert_matches(".", "foo.bar", Some("foo^.bar"), FZ);
}

// upstream: Filters > Cannot set property '1' of undefined, #26511
#[test]
fn fuzzy_score_long_word_does_not_fail() {
    let word = "a".repeat(122);
    let pattern = "a".repeat(119);
    let _ = fuzzy_score(
        &pattern,
        &pattern.to_lowercase(),
        0,
        &word,
        &word.to_lowercase(),
        0,
        FuzzyScoreOptions::default(),
    );
}

// upstream: Filters > Vscode 1.12 no longer obeys 'sortText' in completion items (from language server), #26096
#[test]
fn fuzzy_score_pattern_with_spaces_and_pattern_pos() {
    let at = |pos| Opts {
        pattern_pos: pos,
        ..Opts::default()
    };
    assert_matches_with("  ", "  group", None, FZ, at(2));
    assert_matches_with("  g", "  group", Some("  ^group"), FZ, at(2));
    assert_matches("g", "  group", Some("  ^group"), FZ);
    assert_matches("g g", "  groupGroup", None, FZ);
    assert_matches("g g", "  group Group", Some("  ^group^ ^Group"), FZ);
    assert_matches_with(" g g", "  group Group", Some("  ^group^ ^Group"), FZ, at(1));
    assert_matches("zz", "zzGroup", Some("^z^zGroup"), FZ);
    assert_matches("zzg", "zzGroup", Some("^z^z^Group"), FZ);
    assert_matches("g", "zzGroup", Some("zz^Group"), FZ);
}

// upstream: Filters > patternPos isn't working correctly #79815
#[test]
fn fuzzy_score_pattern_pos_and_word_pos() {
    let at = |pattern_pos, word_pos, weak| Opts {
        pattern_pos,
        word_pos,
        first_match_can_be_weak: weak,
    };
    assert_matches_with("p", "prop", Some("^prop"), FZ, at(0, 0, false));
    assert_matches_with(":p", "prop", Some("^prop"), FZ, at(1, 0, false));
    assert_matches_with(":p", "prop", None, FZ, at(2, 0, false));
    assert_matches_with(":p", "proP", Some("pro^P"), FZ, at(1, 1, false));
    assert_matches_with(":p", "aprop", Some("a^prop"), FZ, at(1, 0, true));
    assert_matches_with(":p", "aprop", None, FZ, at(1, 0, false));
}

// upstream: Filters > topScore - fuzzyScore
#[test]
fn fuzzy_score_top_score() {
    assert_top_score(
        FZ,
        "cons",
        2,
        &["ArrayBufferConstructor", "Console", "console"],
    );
    assert_top_score(FZ, "Foo", 1, &["foo", "Foo", "foo"]);
    // #24904
    assert_top_score(
        FZ,
        "onMess",
        1,
        &["onmessage", "onMessage", "onThisMegaEscape"],
    );
    assert_top_score(FZ, "CC", 1, &["camelCase", "CamelCase"]);
    assert_top_score(FZ, "cC", 0, &["camelCase", "CamelCase"]);
    assert_top_score(FZ, "p", 4, &["parse", "posix", "pafdsa", "path", "p"]);
    assert_top_score(FZ, "pa", 0, &["parse", "pafdsa", "path"]);
    // issue #14583
    assert_top_score(
        FZ,
        "log",
        3,
        &[
            "HTMLOptGroupElement",
            "ScrollLogicalPosition",
            "SVGFEMorphologyElement",
            "log",
            "logger",
        ],
    );
    assert_top_score(FZ, "e", 2, &["AbstractWorker", "ActiveXObject", "else"]);
    // issue #14446
    assert_top_score(
        FZ,
        "workbench.sideb",
        1,
        &[
            "workbench.editor.defaultSideBySideLayout",
            "workbench.sideBar.location",
        ],
    );
    // issue #11423
    assert_top_score(
        FZ,
        "editor.r",
        2,
        &[
            "diffEditor.renderSideBySide",
            "editor.overviewRulerlanes",
            "editor.renderControlCharacter",
            "editor.renderWhitespace",
        ],
    );
    assert_top_score(FZ, "-mo", 1, &["-ms-ime-mode", "-moz-columns"]);
    // dupe, issue #14861
    assert_top_score(
        FZ,
        "convertModelPosition",
        0,
        &[
            "convertModelPositionToViewPosition",
            "convertViewToModelPosition",
        ],
    );
    // dupe, issue #14942
    assert_top_score(FZ, "is", 0, &["isValidViewletId", "import statement"]);
    assert_top_score(
        FZ,
        "title",
        1,
        &["files.trimTrailingWhitespace", "window.title"],
    );
    assert_top_score(FZ, "const", 1, &["constructor", "const", "cuOnstrul"]);
}

// upstream: Filters > Unexpected suggestion scoring, #28791
#[test]
fn fuzzy_score_unexpected_suggestion_scoring() {
    assert_top_score(FZ, "_lines", 1, &["_lineStarts", "_lines"]);
    assert_top_score(FZ, "_lines", 1, &["_lineS", "_lines"]);
    assert_top_score(FZ, "_lineS", 0, &["_lineS", "_lines"]);
}

// upstream: Filters > HTML closing tag proposal filtered out #38880
#[test]
fn fuzzy_score_html_closing_tag() {
    let at = |pattern_pos| Opts {
        pattern_pos,
        ..Opts::default()
    };
    assert_matches_with("\t\t<", "\t\t</body>", Some("^\t^\t^</body>"), FZ, at(0));
    assert_matches_with("\t\t<", "\t\t</body>", Some("\t\t^</body>"), FZ, at(2));
    assert_matches_with("\t<", "\t</body>", Some("\t^</body>"), FZ, at(1));
}

// upstream: Filters > fuzzyScoreGraceful
#[test]
fn graceful_variants_match_upstream() {
    let graceful: Scorer = fuzzy_score_graceful;
    let aggressive: Scorer = fuzzy_score_graceful_aggressive;
    assert_matches("rlut", "result", None, FZ);
    assert_matches("rlut", "result", Some("^res^u^l^t"), graceful);
    assert_matches("cno", "console", Some("^co^ns^ole"), FZ);
    assert_matches("cno", "console", Some("^co^ns^ole"), graceful);
    assert_matches("cno", "console", Some("^c^o^nsole"), aggressive);
    assert_matches("cno", "co_new", Some("^c^o_^new"), graceful);
    assert_matches("cno", "co_new", Some("^c^o_^new"), aggressive);
}

// upstream: Filters > List highlight filter: Not all characters from match are highlighterd #66923
#[test]
fn fuzzy_score_list_highlight_all_characters() {
    assert_matches(
        "foo",
        "barbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_foo",
        Some("barbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_^f^o^o"),
        FZ,
    );
}

// upstream: Filters > Autocompletion is matched against truncated filterText to 54 characters #74133
#[test]
fn fuzzy_score_truncated_filter_text() {
    assert_matches(
        "foo",
        "ffffffffffffffffffffffffffffbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_foo",
        Some(
            "ffffffffffffffffffffffffffffbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_^f^o^o",
        ),
        FZ,
    );
    assert_matches(
        "Aoo",
        "Affffffffffffffffffffffffffffbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_foo",
        Some(
            "^Affffffffffffffffffffffffffffbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_f^o^o",
        ),
        FZ,
    );
    assert_matches(
        "foo",
        "Gffffffffffffffffffffffffffffbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbarbar_foo",
        None,
        FZ,
    );
}

// upstream: Filters > "Go to Symbol" with the exact method name doesn't work as expected #84787
#[test]
fn fuzzy_score_go_to_symbol_exact_name() {
    let options = FuzzyScoreOptions {
        first_match_can_be_weak: true,
        boost_full_match: true,
    };
    assert!(fuzzy_score(":get", ":get", 1, "get", "get", 0, options).is_some());
}

// upstream: Filters > Wrong highlight after emoji #113404
#[test]
fn fuzzy_score_highlight_after_emoji() {
    assert_matches(
        "di",
        "✨div classname=\"\"></div>",
        Some("✨^d^iv classname=\"\"></div>"),
        FZ,
    );
    assert_matches(
        "di",
        "adiv classname=\"\"></div>",
        Some("adiv classname=\"\"></^d^iv>"),
        FZ,
    );
}

// upstream: Filters > Suggestion is not highlighted #85826
#[test]
fn fuzzy_score_suggestion_is_highlighted() {
    let expected = "^S^e^m^a^n^t^i^c^T^o^k^e^n^sEdits";
    assert_matches("SemanticTokens", "SemanticTokensEdits", Some(expected), FZ);
    assert_matches(
        "SemanticTokens",
        "SemanticTokensEdits",
        Some(expected),
        fuzzy_score_graceful_aggressive,
    );
}

// upstream: Filters > IntelliSense completion not correctly highlighting text in front of cursor #115250
#[test]
fn fuzzy_score_text_in_front_of_cursor() {
    assert_matches("lo", "log", Some("^l^og"), FZ);
    assert_matches(".lo", "log", Some("^l^og"), any_scorer);
    assert_matches(".", "log", Some("log"), any_scorer);
}

// upstream: Filters > anyScore should not require a strong first match
#[test]
fn any_score_does_not_require_strong_first_match() {
    assert_matches("bar", "foobAr", Some("foo^b^A^r"), any_scorer);
    assert_matches("bar", "foobar", Some("foo^b^a^r"), any_scorer);
}

// upstream: Filters > configurable full match boost
#[test]
fn fuzzy_score_configurable_full_match_boost() {
    let prefix = "create";
    let a = "createModelServices";
    let b = "create";
    let weak_boost = FuzzyScoreOptions {
        first_match_can_be_weak: true,
        boost_full_match: true,
    };
    let a_boost = fuzzy_score(prefix, prefix, 0, a, &a.to_lowercase(), 0, weak_boost).unwrap();
    let b_boost = fuzzy_score(prefix, prefix, 0, b, &b.to_lowercase(), 0, weak_boost).unwrap();
    assert!(a_boost.score < b_boost.score);

    // Also with word_start > 0 (https://github.com/microsoft/vscode/issues/187921).
    let word_prefix = "$(symbol-function) ";
    let a_word = format!("{word_prefix}{a}");
    let b_word = format!("{word_prefix}{b}");
    let a_boost = fuzzy_score(
        prefix,
        prefix,
        0,
        &a_word,
        &a_word.to_lowercase(),
        word_prefix.len(),
        weak_boost,
    )
    .unwrap();
    let b_boost = fuzzy_score(
        prefix,
        prefix,
        0,
        &b_word,
        &b_word.to_lowercase(),
        word_prefix.len(),
        weak_boost,
    )
    .unwrap();
    assert!(a_boost.score < b_boost.score);

    let no_boost = FuzzyScoreOptions {
        first_match_can_be_weak: true,
        boost_full_match: false,
    };
    let a_score = fuzzy_score(prefix, prefix, 0, a, &a.to_lowercase(), 0, no_boost).unwrap();
    let b_score = fuzzy_score(prefix, prefix, 0, b, &b.to_lowercase(), 0, no_boost).unwrap();
    assert_eq!(a_score.score, b_score.score);
}

// upstream: Filters > Unexpected suggest highlighting ignores whole word match in favor of matching first letter#147423
#[test]
fn fuzzy_score_whole_word_match_beats_first_letter() {
    assert_matches("i", "machine/{id}", Some("machine/{^id}"), FZ);
    assert_matches("ok", "obobobf{ok}/user", Some("^obobobf{o^k}/user"), FZ);
}
