//! Completion suggestions of the active editor (ADR-0082). A request goes to the extension host's
//! worker (`ToHost::Complete`); the providers' lists come back as `FromHost::Completions` and are
//! kept here as the answer to the newest request. An answer to any other request is dropped.
//!
//! Upstream source: src/vs/editor/contrib/suggest/browser/suggest.ts (`provideSuggestionItems`, the
//! request id stands for the cancellation of the previous request)
//!
//! Without a running host a test can answer in-process through [`SuggestState::fake`].

#[cfg(test)]
mod e2e_tests;

use std::path::{Path, PathBuf};

use vsr_exthost::language_features::dto::CompletionList;
use vsr_exthost::language_features::query::CompletionTrigger;

use crate::Workbench;
use crate::extensions::worker::ToHost;

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
    /// The answer to the newest request, once it arrived.
    pub last: Option<Answer>,
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
}

impl Workbench {
    /// Asks the providers for completions at a 1-based position of `path` (in `char`s). The
    /// answer arrives through [`SuggestState::deliver`]; the returned id names the request.
    #[cfg_attr(not(test), allow(dead_code))] // the trigger machine (ADR-0083) calls this
    pub(crate) fn suggest_request(
        &mut self,
        path: PathBuf,
        line: u32,
        column: u32,
        trigger: CompletionTrigger,
    ) -> u64 {
        let id = self.suggest.request();
        if self.ext.is_running() {
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

    #[cfg(test)]
    fn suggest_answer_in_process(
        &mut self,
        id: u64,
        path: &Path,
        line: u32,
        column: u32,
        trigger: CompletionTrigger,
    ) {
        if let Some(results) = self
            .suggest
            .fake
            .as_ref()
            .map(|f| f(path, line, column, trigger))
        {
            self.suggest.deliver(Answer {
                id,
                path: path.to_path_buf(),
                line,
                column,
                results,
            });
        }
    }

    #[cfg(not(test))]
    fn suggest_answer_in_process(
        &mut self,
        _id: u64,
        _path: &Path,
        _line: u32,
        _column: u32,
        _trigger: CompletionTrigger,
    ) {
    }
}
