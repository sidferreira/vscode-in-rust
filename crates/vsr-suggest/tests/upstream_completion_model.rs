//! upstream: src/vs/editor/contrib/suggest/browser/completionModel.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/suggest/browser/suggest.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/suggest/test/browser/completionModel.test.ts @ 50e1350fc78d
//!
//! Ports of VS Code's `CompletionModel` suite: filtering, sorting by score and distance, snippet
//! placement, incremental refiltering. Word distance is `WordDistance.None` (locality bonus is
//! off), as in most upstream cases.
//!
//! Conventions: `createSuggestItem(label, overwriteBefore, kind, incomplete, position, sortText,
//! filterText)` becomes `create_suggest_item`. Each call gets a new provider id, as upstream does
//! with a new provider object per call. A label object (`{ label: 'ZZZ' }`) is its label text.

use std::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

use vsr_suggest::filters::FuzzyScoreOptions;
use vsr_suggest::item::{Completion, ItemRange, Position, Span, SuggestItem, kind};
use vsr_suggest::model::{CompletionModel, LineContext, NoWordDistance, SuggestOptions};
use vsr_suggest::sort::{SnippetSortOrder, suggestion_comparator};

static NEXT_PROVIDER: AtomicU32 = AtomicU32::new(1);

const DEFAULT_POSITION: Position = Position { line: 1, column: 1 };

/// Upstream `createSuggestItem`: a single range that starts `overwrite_before` characters before
/// the position.
fn create_suggest_item(
    label: &str,
    overwrite_before: i64,
    kind: u32,
    incomplete: bool,
    position: Position,
    sort_text: Option<&str>,
    filter_text: Option<&str>,
) -> SuggestItem {
    let completion = Completion {
        label: label.to_string(),
        kind,
        filter_text: filter_text.map(str::to_string),
        sort_text: sort_text.map(str::to_string),
        range: ItemRange::Single(Span {
            start_line: position.line,
            start_column: position.column - overwrite_before,
            end_line: position.line,
            end_column: position.column,
        }),
    };
    let provider = NEXT_PROVIDER.fetch_add(1, AtomicOrdering::Relaxed);
    SuggestItem::new(position, completion, provider, incomplete)
}

fn item(label: &str, overwrite_before: i64) -> SuggestItem {
    create_suggest_item(
        label,
        overwrite_before,
        kind::PROPERTY,
        false,
        DEFAULT_POSITION,
        None,
        None,
    )
}

fn model_with(items: Vec<SuggestItem>, column: i64, leading: &str, delta: i64) -> CompletionModel {
    CompletionModel::new(
        items,
        column,
        LineContext {
            leading_line_content: leading.to_string(),
            character_count_delta: delta,
        },
        Box::new(NoWordDistance),
        SuggestOptions::default(),
        SnippetSortOrder::Inline,
        FuzzyScoreOptions::default(),
    )
}

fn labels(model: &mut CompletionModel) -> Vec<String> {
    model
        .items()
        .iter()
        .map(|i| i.completion.label.clone())
        .collect()
}

fn setup() -> CompletionModel {
    model_with(
        vec![item("foo", 3), item("Foo", 3), item("foo", 2)],
        1,
        "foo",
        0,
    )
}

// upstream: CompletionModel > filtering - cached
#[test]
fn filtering_cached() {
    let mut model = setup();
    model.items();
    let computed = model.computations();
    model.items();
    assert_eq!(
        model.computations(),
        computed,
        "a cached view is not refiltered"
    );

    // still the same context
    model.set_line_context(LineContext {
        leading_line_content: "foo".to_string(),
        character_count_delta: 0,
    });
    model.items();
    assert_eq!(
        model.computations(),
        computed,
        "an unchanged context keeps the view"
    );

    // different context, refilter
    model.set_line_context(LineContext {
        leading_line_content: "foo1".to_string(),
        character_count_delta: 1,
    });
    model.items();
    assert_eq!(model.computations(), computed + 1);
}

// upstream: CompletionModel > complete/incomplete
#[test]
fn complete_and_incomplete() {
    let mut model = setup();
    assert_eq!(model.incomplete_providers().len(), 0);

    let mut incomplete_model = model_with(
        vec![
            create_suggest_item("foo", 3, kind::PROPERTY, true, DEFAULT_POSITION, None, None),
            item("foo", 2),
        ],
        1,
        "foo",
        0,
    );
    assert_eq!(incomplete_model.incomplete_providers().len(), 1);
}

// upstream: CompletionModel > Fuzzy matching of snippets stopped working with inline snippet suggestions #49895
#[test]
fn fuzzy_matching_of_snippets_with_incomplete_items() {
    let at_column_two = Position { line: 1, column: 2 };
    let complete = |label: &str| {
        create_suggest_item(label, 1, kind::PROPERTY, false, at_column_two, None, None)
    };
    let items = vec![
        complete("foobar1"),
        complete("foobar2"),
        complete("foobar3"),
        complete("foobar4"),
        complete("foobar5"),
        create_suggest_item(
            "foofoo1",
            1,
            kind::PROPERTY,
            true,
            at_column_two,
            None,
            None,
        ),
    ];
    let mut model = model_with(items, 2, "f", 0);
    assert_eq!(model.incomplete_providers().len(), 1);
    assert_eq!(model.items().len(), 6);
}

// upstream: CompletionModel > proper current word when length=0, #16380
#[test]
fn proper_current_word_when_length_is_zero() {
    let mut model = model_with(
        vec![
            item("    </div", 4),
            item("a", 0),
            item("p", 0),
            item("    </tag", 4),
            item("    XYZ", 4),
        ],
        1,
        "   <",
        0,
    );
    assert_eq!(model.items().len(), 4);
    assert_eq!(labels(&mut model), vec!["    </div", "    </tag", "a", "p"]);
}

fn snippet_model(
    items: Vec<SuggestItem>,
    leading: &str,
    order: SnippetSortOrder,
) -> CompletionModel {
    CompletionModel::new(
        items,
        1,
        LineContext {
            leading_line_content: leading.to_string(),
            character_count_delta: 0,
        },
        Box::new(NoWordDistance),
        SuggestOptions::default(),
        order,
        FuzzyScoreOptions::default(),
    )
}

// upstream: CompletionModel > keep snippet sorting with prefix: top, #25495
#[test]
fn snippet_sorting_top() {
    let mut model = snippet_model(
        vec![
            item_of_kind("Snippet1", 1, kind::SNIPPET),
            item_of_kind("tnippet2", 1, kind::SNIPPET),
            item_of_kind("semver", 1, kind::PROPERTY),
        ],
        "s",
        SnippetSortOrder::Top,
    );
    let items = model.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].completion.label, "Snippet1");
    assert_eq!(items[1].completion.label, "semver");
    assert!(
        items[0].score.score < items[1].score.score,
        "snippet really promoted"
    );
}

// upstream: CompletionModel > keep snippet sorting with prefix: bottom, #25495
#[test]
fn snippet_sorting_bottom() {
    let mut model = snippet_model(
        vec![
            item_of_kind("snippet1", 1, kind::SNIPPET),
            item_of_kind("tnippet2", 1, kind::SNIPPET),
            item_of_kind("Semver", 1, kind::PROPERTY),
        ],
        "s",
        SnippetSortOrder::Bottom,
    );
    let items = model.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].completion.label, "Semver");
    assert_eq!(items[1].completion.label, "snippet1");
    assert!(
        items[0].score.score < items[1].score.score,
        "snippet really demoted"
    );
}

// upstream: CompletionModel > keep snippet sorting with prefix: inline, #25495
#[test]
fn snippet_sorting_inline() {
    let mut model = snippet_model(
        vec![
            item_of_kind("snippet1", 1, kind::SNIPPET),
            item_of_kind("tnippet2", 1, kind::SNIPPET),
            item_of_kind("Semver", 1, kind::PROPERTY),
        ],
        "s",
        SnippetSortOrder::Inline,
    );
    let items = model.items();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].completion.label, "snippet1");
    assert_eq!(items[1].completion.label, "Semver");
    assert!(
        items[0].score.score > items[1].score.score,
        "snippet really demoted"
    );
}

fn item_of_kind(label: &str, overwrite_before: i64, kind: u32) -> SuggestItem {
    create_suggest_item(
        label,
        overwrite_before,
        kind,
        false,
        DEFAULT_POSITION,
        None,
        None,
    )
}

// upstream: CompletionModel > filterText seems ignored in autocompletion, #26874
#[test]
fn filter_text_is_used_for_filtering() {
    let item1 = create_suggest_item(
        "Map - java.util",
        1,
        kind::PROPERTY,
        false,
        DEFAULT_POSITION,
        None,
        Some("Map"),
    );
    let item2 = item("Map - java.util", 1);
    let mut model = model_with(vec![item1, item2], 1, "M", 0);
    assert_eq!(model.items().len(), 2);
    model.set_line_context(LineContext {
        leading_line_content: "Map ".to_string(),
        character_count_delta: 3,
    });
    assert_eq!(model.items().len(), 1);
}

// upstream: CompletionModel > Vscode 1.12 no longer obeys 'sortText' in completion items (from language server), #26096
#[test]
fn sort_text_of_language_server_items() {
    let at_column_three = Position { line: 1, column: 3 };
    let item1 = create_suggest_item(
        "<- groups",
        2,
        kind::PROPERTY,
        false,
        at_column_three,
        Some("00002"),
        Some("  groups"),
    );
    let item2 = create_suggest_item(
        "source",
        0,
        kind::PROPERTY,
        false,
        at_column_three,
        Some("00001"),
        Some("source"),
    );
    let mut items = vec![item1, item2];
    items.sort_by(suggestion_comparator(SnippetSortOrder::Inline));
    let mut model = model_with(items, 3, "  ", 0);
    assert_eq!(model.items().len(), 2);
    assert_eq!(labels(&mut model), vec!["source", "<- groups"]);
}

// upstream: CompletionModel > Completion item sorting broken when using label details #153026
// (the label details object of the upstream test is its label text here)
#[test]
fn completion_item_sorting_with_label_details() {
    let zzz = item_of_kind("ZZZ", 0, kind::OPERATOR);
    let aaa = item_of_kind("AAA", 0, kind::OPERATOR);
    let iii = item_of_kind("III", 0, kind::OPERATOR);
    let mut items = [zzz, aaa, iii];
    items.sort_by(suggestion_comparator(SnippetSortOrder::Inline));
    let order: Vec<&str> = items.iter().map(|i| i.completion.label.as_str()).collect();
    assert_eq!(order, vec!["AAA", "III", "ZZZ"]);
}

// upstream: CompletionModel > Score only filtered items when typing more, score all when typing less
#[test]
fn score_only_filtered_items_when_typing_more() {
    let mut model = model_with(
        vec![
            item("console", 0),
            item("co_new", 0),
            item("bar", 0),
            item("car", 0),
            item("foo", 0),
        ],
        1,
        "",
        0,
    );
    assert_eq!(model.items().len(), 5);

    // narrow down once
    model.set_line_context(LineContext {
        leading_line_content: "c".to_string(),
        character_count_delta: 1,
    });
    assert_eq!(model.items().len(), 3);

    // query gets longer, narrow down the narrow-down'ed-set from before
    model.set_line_context(LineContext {
        leading_line_content: "cn".to_string(),
        character_count_delta: 2,
    });
    assert_eq!(model.items().len(), 2);

    // query gets shorter, refilter everything
    model.set_line_context(LineContext {
        leading_line_content: String::new(),
        character_count_delta: 0,
    });
    assert_eq!(model.items().len(), 5);
}

// upstream: CompletionModel > Have more relaxed suggest matching algorithm #15419
#[test]
fn relaxed_matching() {
    let mut model = model_with(
        vec![
            item("result", 0),
            item("replyToUser", 0),
            item("randomLolut", 0),
            item("car", 0),
            item("foo", 0),
        ],
        1,
        "",
        0,
    );
    model.set_line_context(LineContext {
        leading_line_content: "rlut".to_string(),
        character_count_delta: 4,
    });
    assert_eq!(model.items().len(), 3);
    assert_eq!(
        labels(&mut model),
        vec!["result", "replyToUser", "randomLolut"],
    );
}

// upstream: CompletionModel > Emmet suggestion not appearing at the top of the list in jsx files, #39518
#[test]
fn emmet_suggestion_at_top() {
    let mut model = model_with(
        vec![
            item("from", 0),
            item("form", 0),
            item("form:get", 0),
            item("testForeignMeasure", 0),
            item("fooRoom", 0),
        ],
        1,
        "",
        0,
    );
    model.set_line_context(LineContext {
        leading_line_content: "form".to_string(),
        character_count_delta: 4,
    });
    assert_eq!(model.items().len(), 5);
    let first_three: Vec<String> = labels(&mut model).into_iter().take(3).collect();
    assert_eq!(first_three, vec!["form", "form:get", "from"]);
}
