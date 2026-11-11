//! Completion suggestions of the active editor (ADR-0082). A request goes to the extension host's
//! worker (`ToHost::Complete`); the providers' lists come back as `FromHost::Completions` and are
//! kept here as the answer to the newest request. An answer to any other request is dropped.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggest.ts (`provideSuggestionItems`, the
//! request id stands for the cancellation of the previous request)
//!
//! Without a running host a test can answer in-process through [`SuggestState::fake`]; without a
//! host and without a fake the answer is empty, so the word-based provider alone fills the list.
//!
//! The trigger machine (ADR-0083) lives in `vsr_suggest::trigger`; this module feeds it what happens
//! in the editor (`EditorEvent`), runs the quick suggest timer, asks the providers and merges the
//! word-based suggestions as upstream does: the word-based provider has the lowest score, so it is
//! asked only when no other provider offered anything, and never for a trigger character.

mod accept;
pub mod settings;
#[cfg(test)]
mod e2e_tests;
pub mod widget;

use std::path::{Path, PathBuf};

use gpui_kit::{Context, Task};
use vsr_exthost::language_features::dto::CompletionList;
use vsr_exthost::language_features::query::CompletionTrigger;
use vsr_suggest::adapter::{from_dto, list_to_utf16, utf16_to_char_column};
use vsr_suggest::item::{Position, SuggestItem};
use vsr_suggest::trigger::{
    LineCtx, Reaction, Session, State, TokenKind, TriggerKind, TriggerOptions, WordBased,
};
use vsr_suggest::words::{WORD_PROVIDER, word_items};

use crate::Workbench;
use crate::editor_view::EditorEvent;
use crate::extensions::worker::ToHost;
use crate::model::OpenEditor;

/// What the providers answered for one request.
#[derive(Debug, Clone, PartialEq)]
pub struct Answer {
    pub id: u64,
    pub path: PathBuf,
    /// The request's 1-based position, in `char`s.
    pub line: u32,
    pub column: u32,
    /// `(provider handle, list)` in the registry's order.
    pub results: Vec<(u32, CompletionList)>,
}

/// An in-process provider for tests: the lists at a position of a document.
#[cfg(test)]
pub type FakeSource = Box<dyn Fn(&Path, u32, u32, CompletionTrigger) -> Vec<(u32, CompletionList)>>;

/// The completion state the workbench keeps.
#[derive(Default)]
pub struct SuggestState {
    next_id: u64,
    /// The request whose answer is wanted: the newest one, until it is answered.
    pending: Option<u64>,
    /// The answer to the newest request, once it arrived (taken when it is turned into a model).
    pub last: Option<Answer>,
    /// The trigger state machine of the active editor (ADR-0083).
    pub session: Session,
    /// The document the session runs in.
    path: Option<PathBuf>,
    /// The cursor line when the newest request was made, with its request id: the host's answer
    /// counts `char`s of that text.
    request_line: Option<(u64, i64, String)>,
    /// The quick suggest delay; dropping it cancels it.
    timer: Option<Task<()>>,
    /// The list has something to show (the widget shows it, ADR-0084).
    pub visible: bool,
    /// The `editor.suggest*` settings, read every frame (ADR-0095).
    pub settings: settings::SuggestSettings,
    /// The focused row and the top of the list (ADR-0084).
    pub list: vsr_suggest::list::ListView,
    /// The lists the host still caches for the session, `(provider handle, cache id)`.
    held: Vec<(u32, u64)>,
    /// Items the host resolved, by `(list cache id, index)`; cleared with the session.
    resolved: std::collections::HashMap<(u64, u64), Box<vsr_exthost::language_features::dto::CompletionItem>>,
    /// The resolve request on its way: its id and the item it is for.
    resolving: Option<(u64, (u64, u64))>,
    /// Every request and resolve id come from here.
    next_resolve: u64,
    /// Answers given in-process when no host runs (tests only).
    #[cfg(test)]
    pub fake: Option<FakeSource>,
}

impl SuggestState {
    /// A new request replaces the one waiting. Returns its id.
    pub fn request(&mut self) -> u64 {
        self.next_id += 1;
        self.pending = Some(self.next_id);
        self.next_id
    }

    /// The answer to request `answer.id`. False (and nothing kept) when that request is no longer
    /// the one wanted.
    pub fn deliver(&mut self, answer: Answer) -> bool {
        if self.pending != Some(answer.id) {
            return false;
        }
        self.pending = None;
        self.last = Some(answer);
        true
    }

    /// The lists the host still caches for the session (tests).
    #[cfg(test)]
    pub fn held_lists(&self) -> Vec<(u32, u64)> {
        self.held.clone()
    }

    /// A completion item was resolved by the host (tests).
    #[cfg(test)]
    pub fn has_resolved(&self) -> bool {
        !self.resolved.is_empty()
    }

    /// The id of the request whose answer is wanted, `None` once it came (tests).
    #[cfg(test)]
    pub fn pending_id(&self) -> Option<u64> {
        self.pending
    }

    /// The request on its way is no longer wanted: its answer, when it comes, is dropped.
    fn forget_request(&mut self) {
        self.pending = None;
        self.request_line = None;
        self.last = None;
    }
}

/// The `(provider handle, list cache id)` of the lists in `results`.
pub(crate) fn cache_ids(results: &[(u32, CompletionList)]) -> Vec<(u32, u64)> {
    results.iter().filter_map(|(h, l)| l.cache_id.map(|c| (*h, c))).collect()
}

/// Where the cursor is in the active editor.
struct Cursor {
    path: PathBuf,
    line: i64,
    /// 1-based UTF-16 column.
    column: i64,
    line_text: String,
    /// Nothing is selected.
    empty: bool,
}

impl Cursor {
    fn ctx(&self) -> LineCtx {
        LineCtx::new(self.line, &self.line_text, self.column)
    }
}

/// The kind of token before the cursor (upstream reads the token at `column - 2`).
fn token_kind(e: &OpenEditor, line: i64, column: i64) -> TokenKind {
    use vsr_editor::{StandardTokenType, TokenSource};
    let tokens = crate::syntax::SyntaxTokens(e.syntax());
    let Some(t) = tokens.line_tokens(e.buffer(), line as usize) else {
        return TokenKind::Other;
    };
    match t.type_at_offset((column - 2).max(0) as isize) {
        StandardTokenType::Comment => TokenKind::Comment,
        StandardTokenType::String => TokenKind::String,
        StandardTokenType::RegEx => TokenKind::RegEx,
        _ => TokenKind::Other,
    }
}

impl Workbench {
    /// Asks the providers for completions at a 1-based position of `path` (in `char`s). The
    /// answer arrives through [`SuggestState::deliver`]; the returned id names the request.
    pub(crate) fn suggest_request(
        &mut self,
        path: PathBuf,
        line: u32,
        column: u32,
        trigger: CompletionTrigger,
    ) -> u64 {
        let id = self.suggest.request();
        if self.ext.is_running() && path.is_absolute() {
            // The host's copy of the document must be at the editor's version before it is asked.
            self.ext_sync();
            self.ext_send(ToHost::Complete {
                id,
                path,
                line,
                column,
                trigger,
            });
        } else {
            self.suggest_answer_in_process(id, &path, line, column, trigger);
        }
        id
    }

    /// No host answers: a test's fake source does, otherwise the answer is empty.
    fn suggest_answer_in_process(
        &mut self,
        id: u64,
        path: &Path,
        line: u32,
        column: u32,
        trigger: CompletionTrigger,
    ) {
        #[cfg(test)]
        let results = self
            .suggest
            .fake
            .as_ref()
            .map(|f| f(path, line, column, trigger))
            .unwrap_or_default();
        #[cfg(not(test))]
        let results = {
            let _ = trigger;
            Vec::new()
        };
        self.suggest.deliver(Answer {
            id,
            path: path.to_path_buf(),
            line,
            column,
            results,
        });
    }
}

// ---- the trigger machine's driver (ADR-0083) -------------------------------------------------

impl Workbench {
    fn suggest_cursor(&self) -> Option<Cursor> {
        if !self.editor_surface_active() {
            return None;
        }
        let e = self.editors.active_editor()?;
        let sel = e.editor.selection();
        let line = sel.active.line;
        Some(Cursor {
            path: e.path.clone(),
            line: line as i64,
            column: sel.active.column as i64,
            line_text: e.buffer().line_content(line).to_string(),
            empty: sel.is_empty(),
        })
    }

    /// Ends the session: what is on its way is dropped, the list hides.
    pub(crate) fn suggest_cancel(&mut self, cx: &mut Context<Self>) {
        self.suggest.session.cancel();
        self.suggest_dismissed(cx);
    }

    fn suggest_dismissed(&mut self, cx: &mut Context<Self>) {
        self.suggest.timer = None;
        self.suggest_release_held();
        self.suggest.resolved.clear();
        self.suggest.resolving = None;
        self.suggest.forget_request();
        if std::mem::take(&mut self.suggest.visible) {
            cx.notify();
        }
    }

    /// Called every frame: a session belongs to one document, and to the editor surface.
    pub(crate) fn suggest_sync(&mut self, cx: &mut Context<Self>) {
        let read = settings::SuggestSettings::from_configuration(crate::config::AppConfig::get(cx).configuration());
        if read != self.suggest.settings {
            self.suggest.session.config = read.config;
            self.suggest.settings = read;
        }
        if self.suggest.session.state() != State::Idle || self.suggest.timer.is_some() {
            let here = self.suggest_cursor().map(|c| c.path);
            if here != self.suggest.path {
                self.suggest_cancel(cx);
            }
        }
        self.editor_view.suggest_visible = self.suggest.visible;
        self.editor_view.suggest_widget = self.suggest_widget_data().map(std::rc::Rc::new);
    }

    /// What happened in the editor (upstream's cursor and content listeners of `SuggestModel`).
    pub(crate) fn suggest_editor_event(&mut self, event: EditorEvent, cx: &mut Context<Self>) {
        let Some(cursor) = self.suggest_cursor() else {
            self.suggest_cancel(cx);
            return;
        };
        if !cursor.empty && event != EditorEvent::Other {
            // A selection ends the session.
            self.suggest_cancel(cx);
            return;
        }
        let ctx = cursor.ctx();
        match event {
            EditorEvent::Other => self.suggest_cancel(cx),
            EditorEvent::Typed(text) => {
                // The cursor moved right because of typing: quick suggest.
                self.suggest_arm(&cursor, cx);
                let reaction = self.suggest.session.refilter(&ctx);
                self.suggest_react(reaction, cx);
                // `onDidType`: the typed character may be a trigger character. The host answers
                // for the ones a provider registered; any other character gets an empty answer,
                // which ends an automatic session as upstream's filtering would.
                if let Some(last) = text.chars().last().filter(|c| !c.is_whitespace()) {
                    let options = self.suggest.session.trigger_character(last, &ctx, true);
                    if let Some(options) = options {
                        self.suggest_trigger(options, cx);
                    }
                }
            }
            EditorEvent::DeleteLeft | EditorEvent::Moved | EditorEvent::Edited => {
                let reaction = self.suggest.session.refilter(&ctx);
                self.suggest_react(reaction, cx);
            }
            EditorEvent::CompositionEnd => {
                if self.suggest.session.state() == State::Idle {
                    self.suggest_arm(&cursor, cx);
                } else {
                    let reaction = self.suggest.session.refilter(&ctx);
                    self.suggest_react(reaction, cx);
                }
            }
        }
    }

    /// Starts the quick suggest delay when no session runs.
    fn suggest_arm(&mut self, cursor: &Cursor, cx: &mut Context<Self>) {
        if self.suggest.session.state() != State::Idle {
            return;
        }
        let Some(delay) = self.suggest.session.arm_quick_suggest() else {
            return;
        };
        self.suggest.path = Some(cursor.path.clone());
        self.suggest.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            this.update(cx, |wb, cx| wb.suggest_quick_fire(cx)).ok();
        }));
    }

    /// The quick suggest delay passed.
    fn suggest_quick_fire(&mut self, cx: &mut Context<Self>) {
        let Some(cursor) = self.suggest_cursor() else {
            return;
        };
        if !cursor.empty || self.suggest.path.as_ref() != Some(&cursor.path) {
            return;
        }
        let editors = &self.editors;
        let token = || {
            editors
                .active_editor()
                .map_or(TokenKind::Other, |e| token_kind(e, cursor.line, cursor.column))
        };
        let has_provider = self.ext.is_running() || self.suggest_has_in_process_source();
        let ctx = cursor.ctx();
        if let Some(options) = self.suggest.session.fire_quick_suggest(&ctx, token, has_provider) {
            self.suggest_trigger(options, cx);
        }
    }

    /// Whether something answers without the extension host: the word-based provider, or a test's
    /// fake source.
    fn suggest_has_in_process_source(&self) -> bool {
        #[cfg(test)]
        if self.suggest.fake.is_some() {
            return true;
        }
        self.suggest.session.config.word_based != WordBased::Off
    }

    fn suggest_react(&mut self, reaction: Reaction, cx: &mut Context<Self>) {
        match reaction {
            Reaction::Nothing => {}
            Reaction::Cancelled => self.suggest_dismissed(cx),
            Reaction::Retrigger(options) => self.suggest_trigger(options, cx),
            Reaction::Suggest { frozen } => {
                self.suggest.visible = true;
                if !frozen {
                    self.suggest_refocus();
                }
                self.suggest_focus_changed();
                cx.notify();
            }
        }
    }

    /// Upstream `SuggestModel.trigger`: starts a session at the cursor and asks the providers.
    /// Ctrl+Space calls this with [`TriggerOptions::manual`].
    pub(crate) fn suggest_trigger(&mut self, options: TriggerOptions, cx: &mut Context<Self>) {
        let Some(cursor) = self.suggest_cursor() else {
            return;
        };
        self.suggest.timer = None;
        self.suggest.session.trigger(options, cursor.ctx());
        self.suggest.path = Some(cursor.path.clone());
        let trigger = match options.kind {
            TriggerKind::Invoke => CompletionTrigger::Invoke,
            TriggerKind::Character(c) => CompletionTrigger::Character(c),
            TriggerKind::Incomplete => CompletionTrigger::Incomplete,
        };
        let column = utf16_to_char_column(&cursor.line_text, cursor.column as u32);
        let id = self.suggest_request(cursor.path.clone(), cursor.line as u32, column, trigger);
        self.suggest.request_line = Some((id, cursor.line, cursor.line_text));
        // An answer given in-process is here already.
        self.suggest_on_answer(cx);
    }

    /// The answer to the newest request arrived: the lists become items, the word-based provider
    /// adds its words when nothing else offered any, and the session builds its model.
    pub(crate) fn suggest_on_answer(&mut self, cx: &mut Context<Self>) {
        let Some(answer) = self.suggest.last.take() else {
            return;
        };
        let Some((id, line, line_text)) = self.suggest.request_line.take() else {
            return;
        };
        let (Some(context), Some(options)) = (
            self.suggest.session.context().cloned(),
            self.suggest.session.trigger_options(),
        ) else {
            return;
        };
        let Some(now) = self.suggest_cursor() else {
            return;
        };
        if id != answer.id || self.suggest.path.as_ref() != Some(&now.path) {
            return;
        }
        // The host counts `char`s of the text the request was made against.
        let text = format!("{}{line_text}", "\n".repeat((line - 1).max(0) as usize));
        let position = Position {
            line: context.line,
            column: context.column,
        };
        let mut items: Vec<SuggestItem> = Vec::new();
        let mut answered: Vec<u32> = Vec::new();
        let mut answered_lists: Vec<(u32, u64)> = Vec::new();
        for (handle, mut list) in answer.results {
            list_to_utf16(&mut list, &text);
            answered.push(handle);
            if let Some(cache) = list.cache_id {
                answered_lists.push((handle, cache));
            }
            let incomplete = list.is_incomplete;
            items.extend(
                list.items
                    .iter()
                    .map(|item| from_dto(item, position, handle, incomplete)),
            );
        }
        self.suggest_hold(&answered_lists, &answered);
        let allowed = self.suggest.settings.clone();
        items.retain(|i| allowed.shows(i.completion.kind));
        let word_based = self.suggest.session.config.word_based;
        if items.is_empty()
            && !matches!(options.kind, TriggerKind::Character(_))
            && word_based != WordBased::Off
        {
            answered.push(WORD_PROVIDER);
            items = self.suggest_word_items(&now, word_based);
            items.retain(|i| allowed.shows(i.completion.kind));
        }
        let reaction = self.suggest.session.receive(items, &answered, &now.ctx());
        self.suggest_react(reaction, cx);
    }

    /// `WordBasedCompletionItemProvider`: words of the active document, and per the setting of the
    /// other open documents of the same language or of all.
    fn suggest_word_items(&self, cursor: &Cursor, mode: WordBased) -> Vec<SuggestItem> {
        let Some(active) = self.editors.active_editor() else {
            return Vec::new();
        };
        let mut texts = vec![active.buffer().value()];
        if matches!(mode, WordBased::MatchingDocuments | WordBased::OffWithInlineSuggestions | WordBased::AllDocuments) {
            for e in &self.editors.open {
                if e.path == active.path {
                    continue;
                }
                if mode == WordBased::AllDocuments || e.language_id() == active.language_id() {
                    texts.push(e.buffer().value());
                }
            }
        }
        let refs: Vec<&str> = texts.iter().map(String::as_str).collect();
        word_items(cursor.line, &cursor.line_text, cursor.column, &refs)
    }

    /// Ctrl+Space and Escape.
    pub(crate) fn suggest_command(&mut self, cmd: crate::keymap::Command, window: &mut gpui_kit::Window, cx: &mut Context<Self>) -> bool {
        use crate::keymap::Command;
        match cmd {
            Command::TriggerSuggest => {
                if self.suggest_cursor().is_some_and(|c| c.empty) {
                    self.suggest_trigger(TriggerOptions::manual(), cx);
                }
                true
            }
            Command::HideSuggest => {
                self.suggest_cancel(cx);
                true
            }
            Command::SuggestNext
            | Command::SuggestPrevious
            | Command::SuggestNextPage
            | Command::SuggestPreviousPage
            | Command::AcceptSuggest
            | Command::AcceptSuggestOnEnter => self.suggest_widget_command(cmd, window, cx),
            _ => false,
        }
    }
}
