//! upstream: src/vs/editor/contrib/suggest/browser/suggestModel.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/suggest/test/browser/suggestModel.test.ts @ 50e1350fc78d
//!
//! Ports of the `SuggestModel` suite that apply to the trigger state machine (ADR-0083). Upstream
//! drives a code editor and a provider registry with promises; here a small synchronous driver
//! plays the editor (one line of text, a cursor, typing, delete left) and the provider registry, and
//! feeds the session the way the workbench does. Its log of events stands for `onDidTrigger`,
//! `onDidCancel` and `onDidSuggest`.
//!
//! Not ported: the embedded language boundary and inline completions tests (no such feature), the
//! tests of snippets, completion details and code actions (the widget and the controller, step 3),
//! the `*` selector tests (provider groups are the host's), #99504 and `Set refilter-flag, keep
//! triggerKind` (they read provider filtering and the refilter flag, which the session does not
//! model, ADR-0083). `Handler.Type` of several characters is one typing event, as upstream.

use vsr_suggest::item::{Completion, ItemRange, Position, Span, SuggestItem, kind};
use vsr_suggest::trigger::{
    LineCtx, Reaction, Session, State, TriggerKind, TriggerOptions, should_auto_trigger,
};
use vsr_suggest::words::word_until;

type Label = (String, i64, i64);
type Provide = dyn Fn(&str, i64, TriggerKind) -> Option<(Vec<Label>, bool)>;

struct Provider {
    id: u32,
    trigger_characters: Vec<char>,
    provide: Box<Provide>,
}

#[derive(Debug, Clone, PartialEq)]
enum Event {
    Trigger {
        auto: bool,
    },
    Cancel {
        retrigger: bool,
    },
    Suggest {
        auto: bool,
        frozen: bool,
        labels: Vec<String>,
        incomplete: usize,
    },
}

struct Oracle {
    text: String,
    column: i64,
    session: Session,
    providers: Vec<Provider>,
    log: Vec<Event>,
    timer: bool,
}

/// `label` offered over `start..column` (upstream's `Range.fromPositions(pos.with(undefined, 1), pos)`).
fn label(l: &str, start: i64, column: i64) -> Label {
    (l.to_string(), start, column)
}

fn default_range_start(line: &str, column: i64) -> i64 {
    word_until(line, column).start_column
}

impl Oracle {
    fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
            column: 1,
            session: Session::default(),
            providers: Vec::new(),
            log: Vec::new(),
            timer: false,
        }
    }

    fn register(
        &mut self,
        trigger_characters: &[char],
        provide: impl Fn(&str, i64, TriggerKind) -> Option<(Vec<Label>, bool)> + 'static,
    ) {
        let id = self.providers.len() as u32 + 1;
        self.providers.push(Provider {
            id,
            trigger_characters: trigger_characters.to_vec(),
            provide: Box::new(provide),
        });
    }

    fn ctx(&self) -> LineCtx {
        LineCtx::new(1, &self.text, self.column)
    }

    fn suggests(&self) -> Vec<&Event> {
        self.log
            .iter()
            .filter(|e| matches!(e, Event::Suggest { .. }))
            .collect()
    }

    fn last_suggest(&self) -> (bool, bool, Vec<String>, usize) {
        match self.suggests().last() {
            Some(Event::Suggest {
                auto,
                frozen,
                labels,
                incomplete,
            }) => (*auto, *frozen, labels.clone(), *incomplete),
            _ => panic!("no suggest event in {:?}", self.log),
        }
    }

    fn cancel(&mut self) {
        if self.session.cancel() {
            self.log.push(Event::Cancel { retrigger: false });
        }
    }

    /// `SuggestModel.trigger`: cancel the session, ask the providers, show the answer.
    fn trigger(&mut self, options: TriggerOptions) {
        let ctx = self.ctx();
        if self.session.state() != State::Idle {
            self.log.push(Event::Cancel {
                retrigger: options.retrigger,
            });
        }
        self.session.trigger(options, ctx.clone());
        self.log.push(Event::Trigger { auto: options.auto });

        let mut items = Vec::new();
        let mut answered = Vec::new();
        for p in &self.providers {
            if let TriggerKind::Character(c) = options.kind
                && !p.trigger_characters.contains(&c)
            {
                continue;
            }
            let Some((labels, incomplete)) = (p.provide)(&self.text, self.column, options.kind)
            else {
                continue;
            };
            answered.push(p.id);
            for (l, start, end) in labels {
                let range = Span {
                    start_line: 1,
                    start_column: start,
                    end_line: 1,
                    end_column: end,
                };
                items.push(SuggestItem::new(
                    Position {
                        line: 1,
                        column: self.column,
                    },
                    Completion {
                        sort_text: Some(l.clone()),
                        label: l,
                        kind: kind::PROPERTY,
                        filter_text: None,
                        range: ItemRange::Single(range),
                    },
                    p.id,
                    incomplete,
                ));
            }
        }
        let now = self.ctx();
        let reaction = self.session.receive(items, &answered, &now);
        self.react(reaction);
    }

    fn react(&mut self, reaction: Reaction) {
        match reaction {
            Reaction::Nothing => {}
            Reaction::Cancelled => self.log.push(Event::Cancel { retrigger: false }),
            Reaction::Retrigger(options) => self.trigger(options),
            Reaction::Suggest { frozen } => {
                let auto = self.session.state() == State::Auto;
                let model = self.session.model().expect("a model");
                let incomplete = model.incomplete_providers().len();
                let labels = model.items().iter().map(|i| i.text_label.clone()).collect();
                self.log.push(Event::Suggest {
                    auto,
                    frozen,
                    labels,
                    incomplete,
                });
            }
        }
    }

    fn content_changed(&mut self) {
        let ctx = self.ctx();
        let reaction = self.session.refilter(&ctx);
        self.react(reaction);
    }

    /// The `onDidChangeCursorSelection` of a typing edit that moved the cursor right.
    fn cursor_edit(&mut self) {
        if self.session.state() == State::Idle && self.session.arm_quick_suggest().is_some() {
            self.timer = true;
        }
    }

    /// Lets the quick suggest delay pass.
    fn fire_timer(&mut self) {
        if !std::mem::take(&mut self.timer) {
            return;
        }
        let ctx = self.ctx();
        if let Some(options) = self.session.fire_quick_suggest(
            &ctx,
            || vsr_suggest::trigger::TokenKind::Other,
            !self.providers.is_empty(),
        ) {
            self.trigger(options);
        }
    }

    /// `editor.trigger('keyboard', Handler.Type, { text })`.
    fn type_text(&mut self, text: &str) {
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let at = (self.column - 1) as usize;
        let mut out = String::from_utf16_lossy(&units[..at]);
        out.push_str(text);
        out.push_str(&String::from_utf16_lossy(&units[at..]));
        self.text = out;
        self.column += text.encode_utf16().count() as i64;
        self.cursor_edit();
        self.content_changed();
        // `onDidType`: the last typed character may be a trigger character.
        let last = text.chars().last().expect("text");
        let ctx = self.ctx();
        let supported = self
            .providers
            .iter()
            .any(|p| p.trigger_characters.contains(&last));
        if let Some(options) = self.session.trigger_character(last, &ctx, supported) {
            self.trigger(options);
        }
        self.fire_timer();
    }

    fn delete_left(&mut self) {
        let units: Vec<u16> = self.text.encode_utf16().collect();
        let at = (self.column - 1) as usize;
        let mut out = String::from_utf16_lossy(&units[..at - 1]);
        out.push_str(&String::from_utf16_lossy(&units[at..]));
        self.text = out;
        self.column -= 1;
        self.content_changed();
    }
}

fn always_something(line: &str, column: i64, _: TriggerKind) -> Option<(Vec<Label>, bool)> {
    let word = word_until(line, column);
    Some((vec![label(&word.word, word.start_column, column)], false))
}

// upstream: SuggestModel - Context > Context - shouldAutoTrigger
#[test]
fn context_should_auto_trigger() {
    let line = "Das Pferd frisst keinen Gurkensalat - Philipp Reis 1861.";
    // upstream offsets are 0-based: column = offset + 1.
    assert!(should_auto_trigger(line, 4), "end of word, Das|");
    assert!(!should_auto_trigger(line, 5), "no word Das |");
    assert!(
        should_auto_trigger(line, 2),
        "typing a single character before a word: D|as"
    );
    assert!(!should_auto_trigger(line, 56), "number, 1861|");
}

// upstream: SuggestModel - TriggerAndCancelOracle > events - cancel/trigger
#[test]
fn events_cancel_trigger() {
    let mut o = Oracle::new("abc def");
    o.trigger(TriggerOptions::auto());
    assert_eq!(o.log[0], Event::Trigger { auto: true });
    // Without a provider the answer is empty and an automatic session ends by itself.
    assert_eq!(o.log.last(), Some(&Event::Cancel { retrigger: false }));
    o.cancel();
    o.trigger(TriggerOptions::auto());
    assert_eq!(
        o.log
            .iter()
            .filter(|e| **e == Event::Trigger { auto: true })
            .count(),
        2
    );
    let mut o = Oracle::new("abc def");
    o.trigger(TriggerOptions::manual());
    assert_eq!(o.log[0], Event::Trigger { auto: false });
}

// upstream: SuggestModel - TriggerAndCancelOracle > events - suggest/empty
#[test]
fn events_suggest_empty() {
    let mut o = Oracle::new("abc def");
    o.register(&[], |_, _, _| Some((Vec::new(), false)));
    o.trigger(TriggerOptions::auto());
    assert_eq!(
        o.log.last(),
        Some(&Event::Cancel { retrigger: false }),
        "an automatic session with nothing to show ends"
    );
    let mut o = Oracle::new("abc def");
    o.register(&[], |_, _, _| Some((Vec::new(), false)));
    o.trigger(TriggerOptions::manual());
    let (auto, frozen, labels, _) = o.last_suggest();
    assert!(!auto);
    assert!(!frozen);
    assert!(labels.is_empty());
}

// upstream: SuggestModel - TriggerAndCancelOracle > trigger - on type
#[test]
fn trigger_on_type() {
    let mut o = Oracle::new("abc def");
    o.register(&[], always_something);
    o.column = 4;
    o.type_text("d");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels.len(), 1);
    assert_eq!(o.providers[0].id, 1);
}

// upstream: SuggestModel - TriggerAndCancelOracle > #17400: Keep filtering suggestModel.ts after space
#[test]
fn keep_filtering_after_space() {
    let mut o = Oracle::new("");
    o.register(&[], |line, column, _| {
        Some((
            vec![label("My Table", default_range_start(line, column), column)],
            false,
        ))
    });
    o.trigger(TriggerOptions::auto());
    o.column = 1;
    o.type_text("My");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels, ["My Table"]);
    o.type_text(" ");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels, ["My Table"]);
}

// upstream: SuggestModel - TriggerAndCancelOracle > #21484: Trigger character always force a new completion session
#[test]
fn trigger_character_forces_a_new_session() {
    let mut o = Oracle::new("");
    o.register(&[], |_, column, _| {
        Some((vec![label("foo.bar", 1, column)], false))
    });
    o.register(&['.'], |line, column, _| {
        let dot = line.chars().nth((column - 2) as usize) == Some('.');
        Some((
            vec![label("boom", if dot { column } else { column - 1 }, column)],
            false,
        ))
    });
    o.type_text("foo");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels, ["foo.bar"]);

    o.type_text(".");
    let suggests: Vec<_> = o.suggests().into_iter().cloned().collect();
    let n = suggests.len();
    // SYNC: the list is refiltered first.
    match &suggests[n - 2] {
        Event::Suggest { auto, labels, .. } => {
            assert!(auto);
            assert_eq!(labels, &["foo.bar"]);
        }
        other => panic!("{other:?}"),
    }
    // ASYNC: the trigger character asks the second provider and the first one's items stay.
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels, ["foo.bar", "boom"]);
}

// upstream: SuggestModel - TriggerAndCancelOracle > Intellisense Completion doesn't respect space after equal sign (.html file), #29353 [1/2]
#[test]
fn space_after_equal_sign_1() {
    let mut o = Oracle::new("fo");
    o.register(&[], always_something);
    o.column = 3;
    o.trigger(TriggerOptions::manual());
    let (auto, frozen, labels, _) = o.last_suggest();
    assert!(!auto);
    assert!(!frozen);
    assert_eq!(labels.len(), 1);
    o.type_text("+");
    assert_eq!(o.log.last(), Some(&Event::Cancel { retrigger: false }));
}

// upstream: SuggestModel - TriggerAndCancelOracle > Intellisense Completion doesn't respect space after equal sign (.html file), #29353 [2/2]
#[test]
fn space_after_equal_sign_2() {
    let mut o = Oracle::new("fo");
    o.register(&[], always_something);
    o.column = 3;
    o.trigger(TriggerOptions::manual());
    let (auto, frozen, labels, _) = o.last_suggest();
    assert!(!auto);
    assert!(!frozen);
    assert_eq!(labels.len(), 1);
    o.type_text(" ");
    assert_eq!(o.log.last(), Some(&Event::Cancel { retrigger: false }));
}

// upstream: SuggestModel - TriggerAndCancelOracle > Incomplete suggestion results cause re-triggering when typing w/o further context, #28400 (1/2)
#[test]
fn incomplete_results_cancel_without_context_1() {
    let mut o = Oracle::new("foo");
    o.register(&[], |_, column, _| {
        Some((vec![label("foo", 1, column)], true))
    });
    o.column = 4;
    o.trigger(TriggerOptions::manual());
    let (auto, _, labels, incomplete) = o.last_suggest();
    assert!(!auto);
    assert_eq!(incomplete, 1);
    assert_eq!(labels.len(), 1);
    o.type_text(";");
    assert_eq!(o.log.last(), Some(&Event::Cancel { retrigger: false }));
}

// upstream: SuggestModel - TriggerAndCancelOracle > Incomplete suggestion results cause re-triggering when typing w/o further context, #28400 (2/2)
#[test]
fn incomplete_results_keep_filtering_2() {
    let mut o = Oracle::new("foo");
    o.register(&[], |_, column, _| {
        Some((vec![label("foo;", 1, column)], true))
    });
    o.column = 4;
    o.trigger(TriggerOptions::manual());
    let (auto, _, labels, incomplete) = o.last_suggest();
    assert!(!auto);
    assert_eq!(incomplete, 1);
    assert_eq!(labels.len(), 1);
    // While we cancel incrementally enriching the set of completions we still filter against
    // those that we have until now.
    o.type_text(";");
    let (auto, _, labels, incomplete) = o.last_suggest();
    assert!(!auto);
    assert_eq!(incomplete, 1);
    assert_eq!(labels.len(), 1);
}

// upstream: SuggestModel - TriggerAndCancelOracle > Trigger character is provided in suggest context
#[test]
fn trigger_character_is_provided_in_the_context() {
    let seen = std::rc::Rc::new(std::cell::RefCell::new(None));
    let mut o = Oracle::new("");
    let record = seen.clone();
    o.register(&['.'], move |_, column, kind| {
        *record.borrow_mut() = Some(kind);
        Some((vec![label("foo.bar", 1, column)], false))
    });
    o.type_text("foo.");
    assert_eq!(*seen.borrow(), Some(TriggerKind::Character('.')));
}

// upstream: SuggestModel - TriggerAndCancelOracle > Mac press and hold accent character insertion does not update suggestions, #35269
#[test]
fn accent_replacement_refilters() {
    let mut o = Oracle::new("");
    o.register(&[], |_, column, _| {
        Some((
            vec![label("abc", 1, column), label("\u{e4}bc", 1, column)],
            true,
        ))
    });
    o.type_text("a");
    let (_, _, labels, _) = o.last_suggest();
    assert_eq!(labels, ["abc"]);
    // `executeEdits` replaces the `a` with `ä`: a content change without typing.
    o.text = "\u{e4}".to_string();
    o.content_changed();
    let (_, _, labels, _) = o.last_suggest();
    assert_eq!(labels, ["\u{e4}bc"]);
}

// upstream: SuggestModel - TriggerAndCancelOracle > Backspace should not always cancel code completion, #36491
#[test]
fn backspace_does_not_always_cancel() {
    let mut o = Oracle::new("abc def");
    o.register(&[], always_something);
    o.column = 4;
    o.type_text("d");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels.len(), 1);
    let before = o.suggests().len();
    o.delete_left();
    assert_eq!(o.suggests().len(), before + 1, "the list is refiltered");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto);
    assert_eq!(labels.len(), 1);
}

// upstream: SuggestModel - TriggerAndCancelOracle > Completion list closes unexpectedly when typing a digit after a word separator #169390
#[test]
fn list_stays_when_typing_a_digit_after_a_separator() {
    let counts = std::rc::Rc::new(std::cell::RefCell::new([0, 0]));
    let mut o = Oracle::new("foo");
    let c = counts.clone();
    o.register(&[], move |_, column, _| {
        c.borrow_mut()[0] += 1;
        Some((
            vec![label("foo-20", 1, column), label("foo-hello", 1, column)],
            false,
        ))
    });
    let c = counts.clone();
    o.register(&['2'], move |_, column, kind| {
        c.borrow_mut()[1] += 1;
        if !matches!(kind, TriggerKind::Character(_)) {
            return None;
        }
        Some((vec![label("foo-210", 1, column)], false))
    });
    o.column = 4;
    o.trigger(TriggerOptions::manual());
    let (auto, _, labels, _) = o.last_suggest();
    assert!(!auto);
    assert_eq!(labels, ["foo-20", "foo-hello"]);

    o.type_text("-");
    o.type_text("2");
    let (auto, _, labels, _) = o.last_suggest();
    assert!(auto, "the trigger character starts an automatic session");
    assert_eq!(labels, ["foo-20", "foo-210"]);
    assert_eq!(*counts.borrow(), [1, 2]);
}
