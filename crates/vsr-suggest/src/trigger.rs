//! The trigger state machine of completion suggestions (ADR-0083).
//!
//! upstream: src/vs/editor/contrib/suggest/browser/suggestModel.ts @ 50e1350fc78d
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggestModel.ts (`SuggestModel`,
//! `LineContext`), src/vs/editor/contrib/suggest/browser/suggest.ts (`QuickSuggestionsOptions`).
//!
//! UI-free and clock-free. The caller (the workbench) tells the session what happened in the editor
//! and does what the session answers: ask the providers, start the quick suggest timer, show or hide
//! the list. Positions are 1-based lines and UTF-16 columns, as in upstream.
//!
//! Differences from upstream, all recorded in ADR-0083:
//! - A retrigger asks every provider again (the host's `Complete` takes no provider filter), except
//!   that items of providers that did not answer a character or incomplete retrigger are reused, which
//!   is what `providerItemsToReuse` does.
//! - No inline completions (`offWhenInlineCompletions` behaves as `on`), no snippet mode.

use std::time::Duration;

use crate::filters::FuzzyScoreOptions;
use crate::item::SuggestItem;
use crate::model::{CompletionModel, LineContext, NoWordDistance, SuggestOptions};
use crate::sort::{SnippetSortOrder, suggestion_comparator};
use crate::words::{Word, is_numeric, word_at, word_until};

/// `editor.quickSuggestions` value for one kind of token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickValue {
    On,
    Inline,
    Off,
    OffWhenInlineCompletions,
}

/// `editor.quickSuggestions`: whether typing in code, comments and strings opens the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QuickSuggestions {
    pub other: QuickValue,
    pub comments: QuickValue,
    pub strings: QuickValue,
}

impl Default for QuickSuggestions {
    /// Upstream's default: code only.
    fn default() -> Self {
        Self {
            other: QuickValue::On,
            comments: QuickValue::Off,
            strings: QuickValue::Off,
        }
    }
}

/// The kind of token the cursor is in (`StandardTokenType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenKind {
    Other,
    Comment,
    String,
    RegEx,
}

impl QuickSuggestions {
    /// Upstream `QuickSuggestionsOptions.isAllOff`.
    pub fn is_all_off(&self) -> bool {
        self.other == QuickValue::Off
            && self.comments == QuickValue::Off
            && self.strings == QuickValue::Off
    }

    /// Upstream `QuickSuggestionsOptions.isAllOn`.
    pub fn is_all_on(&self) -> bool {
        self.other == QuickValue::On
            && self.comments == QuickValue::On
            && self.strings == QuickValue::On
    }

    /// Upstream `QuickSuggestionsOptions.valueFor`.
    pub fn value_for(&self, token: TokenKind) -> QuickValue {
        match token {
            TokenKind::Comment => self.comments,
            TokenKind::String => self.strings,
            TokenKind::Other | TokenKind::RegEx => self.other,
        }
    }
}

/// `editor.wordBasedSuggestions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WordBased {
    Off,
    OffWithInlineSuggestions,
    CurrentDocument,
    MatchingDocuments,
    AllDocuments,
}

/// The settings the trigger machine and the word-based provider read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SuggestConfig {
    pub quick: QuickSuggestions,
    /// `editor.quickSuggestionsDelay`.
    pub quick_delay: Duration,
    /// `editor.suggestOnTriggerCharacters`.
    pub on_trigger_characters: bool,
    pub word_based: WordBased,
    pub snippet_order: SnippetSortOrder,
    pub options: SuggestOptions,
}

impl Default for SuggestConfig {
    /// Upstream's defaults. `wordBasedSuggestions` is `offWithInlineSuggestions`, which without an
    /// inline completions provider is the same as `matchingDocuments`.
    fn default() -> Self {
        Self {
            quick: QuickSuggestions::default(),
            quick_delay: Duration::from_millis(10),
            on_trigger_characters: true,
            word_based: WordBased::OffWithInlineSuggestions,
            snippet_order: SnippetSortOrder::Inline,
            options: SuggestOptions::default(),
        }
    }
}

/// `CompletionTriggerKind`, with the character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TriggerKind {
    Invoke,
    Character(char),
    /// The previous list was incomplete and the user typed on.
    Incomplete,
}

/// Upstream `SuggestTriggerOptions`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TriggerOptions {
    /// Triggered by typing rather than asked for (Ctrl+Space).
    pub auto: bool,
    pub shy: bool,
    pub refilter: bool,
    /// Replaces a session in progress: the list stays up while the new answer is on its way.
    pub retrigger: bool,
    pub kind: TriggerKind,
}

impl TriggerOptions {
    pub fn auto() -> Self {
        Self {
            auto: true,
            shy: false,
            refilter: false,
            retrigger: false,
            kind: TriggerKind::Invoke,
        }
    }

    /// Ctrl+Space.
    pub fn manual() -> Self {
        Self {
            auto: false,
            ..Self::auto()
        }
    }
}

/// The cursor line when a request was made or the list refiltered (upstream `LineContext`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineCtx {
    pub line: i64,
    pub column: i64,
    pub leading_line_content: String,
    pub leading_word: Word,
    /// Upstream `LineContext.shouldAutoTrigger` for this position.
    pub should_auto_trigger: bool,
}

impl LineCtx {
    /// The context at `column` of line `line` whose text is `line_text`.
    pub fn new(line: i64, line_text: &str, column: i64) -> Self {
        let units: Vec<u16> = line_text.encode_utf16().collect();
        let end = ((column - 1).max(0) as usize).min(units.len());
        let leading_line_content = String::from_utf16_lossy(&units[..end]);
        Self {
            line,
            column,
            leading_line_content,
            leading_word: word_until(line_text, column),
            should_auto_trigger: should_auto_trigger(line_text, column),
        }
    }
}

/// Upstream `LineContext.shouldAutoTrigger`: at the end of a word, or right after the first
/// character of one, and not a number.
pub fn should_auto_trigger(line_text: &str, column: i64) -> bool {
    let Some(word) = word_at(line_text, column) else {
        return false;
    };
    if word.end_column != column && word.start_column + 1 != column {
        return false;
    }
    !is_numeric(&word.word)
}

/// Upstream `State`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    Idle,
    Manual,
    Auto,
}

/// What a context change did to the session.
#[derive(Clone, Debug, PartialEq)]
pub enum Reaction {
    /// Nothing to show or hide.
    Nothing,
    /// The session ended (the list hides).
    Cancelled,
    /// Ask the providers again with these options (call [`Session::trigger`]).
    Retrigger(TriggerOptions),
    /// Show the model's items. `frozen`: the list was asked for and now matches nothing, so it
    /// keeps what it had.
    Suggest { frozen: bool },
}

fn leading_whitespace(s: &str) -> &str {
    let end = s
        .char_indices()
        .find(|(_, c)| *c != ' ' && *c != '\t')
        .map_or(s.len(), |(i, _)| i);
    &s[..end]
}

/// The trigger state of one editor (upstream `SuggestModel`).
#[derive(Default)]
pub struct Session {
    pub config: SuggestConfig,
    trigger_state: Option<TriggerOptions>,
    context: Option<LineCtx>,
    model: Option<CompletionModel>,
    /// Items of the session being replaced, for providers that will not answer the retrigger.
    reuse: Vec<SuggestItem>,
    quick_pending: bool,
}

impl Session {
    pub fn new(config: SuggestConfig) -> Self {
        Self {
            config,
            ..Self::default()
        }
    }

    pub fn state(&self) -> State {
        match self.trigger_state {
            None => State::Idle,
            Some(o) if o.auto => State::Auto,
            Some(_) => State::Manual,
        }
    }

    /// The options the session was triggered with.
    pub fn trigger_options(&self) -> Option<TriggerOptions> {
        self.trigger_state
    }

    /// The context the request was made at.
    pub fn context(&self) -> Option<&LineCtx> {
        self.context.as_ref()
    }

    /// The model, once the answer arrived.
    pub fn model(&mut self) -> Option<&mut CompletionModel> {
        self.model.as_mut()
    }

    /// True while a quick suggest timer is meant to run.
    pub fn quick_pending(&self) -> bool {
        self.quick_pending
    }

    /// Upstream `cancel`. True when a session was active.
    pub fn cancel(&mut self) -> bool {
        self.quick_pending = false;
        self.reuse.clear();
        let was = self.trigger_state.is_some();
        self.trigger_state = None;
        self.model = None;
        self.context = None;
        was
    }

    /// Upstream `trigger`: starts a session (cancelling the one in progress) and remembers where the
    /// request is made. The caller asks the providers; the answer goes to [`Session::receive`].
    pub fn trigger(&mut self, options: TriggerOptions, ctx: LineCtx) {
        // A character or incomplete retrigger keeps what providers that stay silent offered.
        let keep = if options.retrigger && options.kind != TriggerKind::Invoke {
            self.model
                .as_ref()
                .map(|m| m.all_items().to_vec())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        self.cancel();
        self.reuse = keep;
        self.trigger_state = Some(options);
        self.context = Some(ctx);
    }

    /// The answer to the request: `items` of the providers that answered (`answered`, including those
    /// with nothing to offer) at the cursor `ctx` now. Upstream's `.then` of `trigger`.
    pub fn receive(
        &mut self,
        mut items: Vec<SuggestItem>,
        answered: &[u32],
        ctx: &LineCtx,
    ) -> Reaction {
        let Some(context) = &self.context else {
            return Reaction::Nothing;
        };
        let column = context.column;
        for kept in std::mem::take(&mut self.reuse) {
            if !answered.contains(&kept.provider) {
                items.push(kept);
            }
        }
        items.sort_by(suggestion_comparator(self.config.snippet_order));
        self.model = Some(CompletionModel::new(
            items,
            column,
            LineContext {
                leading_line_content: ctx.leading_line_content.clone(),
                character_count_delta: ctx.column - column,
            },
            Box::new(NoWordDistance),
            self.config.options,
            self.config.snippet_order,
            FuzzyScoreOptions::default(),
        ));
        self.on_new_context(ctx)
    }

    /// The cursor moved right because of an edit (typing), or the text changed: with no session this
    /// starts the quick suggest delay. Returns the delay when a timer should run, `None` when quick
    /// suggestions are off. Upstream `_doTriggerQuickSuggest` up to the timer.
    pub fn arm_quick_suggest(&mut self) -> Option<Duration> {
        if self.config.quick.is_all_off() {
            return None;
        }
        self.cancel();
        self.quick_pending = true;
        Some(self.config.quick_delay)
    }

    /// The quick suggest timer fired. `token` reads the kind of token before the cursor (only
    /// asked when the setting differs by token); `has_provider` is whether any completion provider
    /// applies (the word-based one counts). Returns the options to trigger with.
    pub fn fire_quick_suggest(
        &mut self,
        ctx: &LineCtx,
        token: impl FnOnce() -> TokenKind,
        has_provider: bool,
    ) -> Option<TriggerOptions> {
        if !self.quick_pending {
            return None;
        }
        self.quick_pending = false;
        if self.trigger_state.is_some() || !ctx.should_auto_trigger {
            return None;
        }
        let config = self.config.quick;
        if config.is_all_off() {
            return None;
        }
        if !config.is_all_on() {
            match config.value_for(token()) {
                QuickValue::Off | QuickValue::Inline => return None,
                // No inline completions in rscode: nothing to wait for.
                QuickValue::On | QuickValue::OffWhenInlineCompletions => {}
            }
        }
        has_provider.then(TriggerOptions::auto)
    }

    /// Upstream `checkTriggerCharacter`: the last typed character may trigger when a provider
    /// asked for it (`supported`) and this is not a quick suggest case.
    pub fn trigger_character(
        &self,
        typed: char,
        ctx: &LineCtx,
        supported: bool,
    ) -> Option<TriggerOptions> {
        if !self.config.on_trigger_characters || ctx.should_auto_trigger || !supported {
            return None;
        }
        Some(TriggerOptions {
            auto: true,
            shy: false,
            refilter: false,
            retrigger: self.model.is_some(),
            kind: TriggerKind::Character(typed),
        })
    }

    /// Upstream `_refilterCompletionItems`: the text or the cursor changed while a session is
    /// active.
    pub fn refilter(&mut self, ctx: &LineCtx) -> Reaction {
        if self.trigger_state.is_none() {
            return Reaction::Nothing;
        }
        self.on_new_context(ctx)
    }

    /// Upstream `_onNewContext`.
    fn on_new_context(&mut self, ctx: &LineCtx) -> Reaction {
        let Some(context) = self.context.clone() else {
            // Still in the quick suggest delay.
            return Reaction::Nothing;
        };
        let auto = self.trigger_state.is_some_and(|o| o.auto);
        let retrigger = |kind| {
            Reaction::Retrigger(TriggerOptions {
                auto,
                shy: false,
                refilter: false,
                retrigger: true,
                kind,
            })
        };

        if ctx.line != context.line {
            // Enter while the answer was on its way.
            self.cancel();
            return Reaction::Cancelled;
        }
        if leading_whitespace(&ctx.leading_line_content)
            != leading_whitespace(&context.leading_line_content)
        {
            // The word was outdented.
            self.cancel();
            return Reaction::Cancelled;
        }
        if ctx.column < context.column {
            // Typed, then moved left: ask again if still on a word.
            return if !ctx.leading_word.word.is_empty() {
                retrigger(TriggerKind::Invoke)
            } else {
                self.cancel();
                Reaction::Cancelled
            };
        }
        let Some(model) = self.model.as_mut() else {
            // Not answered yet.
            return Reaction::Nothing;
        };

        if !ctx.leading_word.word.is_empty()
            && ctx.leading_word.start_column > context.leading_word.start_column
        {
            // Started a new word while the list shows.
            return if ctx.should_auto_trigger {
                retrigger(TriggerKind::Invoke)
            } else {
                Reaction::Nothing
            };
        }

        if ctx.column > context.column
            && !model.incomplete_providers().is_empty()
            && !ctx.leading_word.word.is_empty()
        {
            // Typed on and the list is incomplete: ask the incomplete providers for more.
            return retrigger(TriggerKind::Incomplete);
        }

        let old = model.line_context().clone();
        let mut frozen = false;
        model.set_line_context(LineContext {
            leading_line_content: ctx.leading_line_content.clone(),
            character_count_delta: ctx.column - context.column,
        });
        if model.items().is_empty() {
            if ctx.should_auto_trigger
                && context.leading_word.end_column < ctx.leading_word.start_column
            {
                // Heading into a new word.
                return retrigger(TriggerKind::Invoke);
            }
            if !auto {
                // Asked for: keep what it showed.
                model.set_line_context(old);
                frozen = !model.items().is_empty();
                if frozen && ctx.leading_word.word.is_empty() {
                    self.cancel();
                    return Reaction::Cancelled;
                }
            } else {
                self.cancel();
                return Reaction::Cancelled;
            }
        }
        Reaction::Suggest { frozen }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx(text: &str, column: i64) -> LineCtx {
        LineCtx::new(1, text, column)
    }

    fn fired(config: SuggestConfig, token: TokenKind) -> bool {
        let mut s = Session::new(config);
        assert!(s.arm_quick_suggest().is_some());
        s.fire_quick_suggest(&ctx("ab", 3), || token, true)
            .is_some()
    }

    #[test]
    fn quick_suggestions_follow_the_token_kind() {
        let defaults = SuggestConfig::default();
        assert!(fired(defaults, TokenKind::Other), "code: on");
        assert!(!fired(defaults, TokenKind::Comment), "comments: off");
        assert!(!fired(defaults, TokenKind::String), "strings: off");
        let mut strings_on = defaults;
        strings_on.quick.strings = QuickValue::On;
        assert!(fired(strings_on, TokenKind::String));
        let mut inline = defaults;
        inline.quick.other = QuickValue::Inline;
        assert!(!fired(inline, TokenKind::Other), "inline is not the list's");
    }

    #[test]
    fn all_off_never_arms_and_the_delay_is_the_setting() {
        let off = QuickSuggestions {
            other: QuickValue::Off,
            comments: QuickValue::Off,
            strings: QuickValue::Off,
        };
        let config = SuggestConfig { quick: off, ..SuggestConfig::default() };
        assert_eq!(Session::new(config).arm_quick_suggest(), None);
        let config = SuggestConfig { quick_delay: Duration::from_millis(250), ..SuggestConfig::default() };
        assert_eq!(Session::new(config).arm_quick_suggest(), Some(Duration::from_millis(250)));
    }

    #[test]
    fn a_quick_suggest_needs_a_word_and_no_session() {
        let mut s = Session::default();
        s.arm_quick_suggest();
        assert!(
            s.fire_quick_suggest(&ctx("a ", 3), || TokenKind::Other, true)
                .is_none(),
            "no word"
        );
        s.arm_quick_suggest();
        assert!(
            s.fire_quick_suggest(&ctx("a 12", 5), || TokenKind::Other, true)
                .is_none(),
            "a number"
        );
        s.arm_quick_suggest();
        assert!(
            s.fire_quick_suggest(&ctx("ab", 3), || TokenKind::Other, false)
                .is_none(),
            "no provider"
        );
        // The timer does nothing once the session was cancelled.
        s.arm_quick_suggest();
        s.cancel();
        assert!(
            s.fire_quick_suggest(&ctx("ab", 3), || TokenKind::Other, true)
                .is_none()
        );
    }

    #[test]
    fn trigger_characters_respect_the_setting_and_quick_suggest_cases() {
        let mut s = Session::default();
        assert!(s.trigger_character('.', &ctx("a.", 3), true).is_some());
        assert!(
            s.trigger_character('.', &ctx("a.", 3), false).is_none(),
            "no provider asked for it"
        );
        assert!(
            s.trigger_character('b', &ctx("ab", 3), true).is_none(),
            "a word: quick suggest's case"
        );
        s.config.on_trigger_characters = false;
        assert!(s.trigger_character('.', &ctx("a.", 3), true).is_none());
    }
}
