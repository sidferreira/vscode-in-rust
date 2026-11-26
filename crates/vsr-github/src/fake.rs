//! A scripted [`GhRunner`] for tests (this crate's and the app's): canned answers by argument
//! prefix, every call recorded.
//!
//! Upstream source: none (rscode-native; replaces the GitHub Pull Requests extension for now)

use std::path::Path;
use std::sync::Mutex;

use crate::runner::{GhRunner, Output, SpawnError, Tool};

type Reply = Result<Output, SpawnError>;

/// Answers calls whose arguments start with a registered prefix (the first match wins, later
/// registrations first so a test can override a default). An unmatched call fails the test.
#[derive(Default)]
pub struct FakeRunner {
    replies: Mutex<Vec<(Tool, Vec<String>, Reply)>>,
    calls: Mutex<Vec<(Tool, Vec<String>)>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self::default()
    }

    /// `tool args...` answers `reply`.
    pub fn on(self, tool: Tool, prefix: &[&str], reply: Reply) -> Self {
        self.replies.lock().unwrap().push((tool, prefix.iter().map(|s| s.to_string()).collect(), reply));
        self
    }

    pub fn gh(self, prefix: &[&str], stdout: &str) -> Self {
        self.on(Tool::Gh, prefix, Ok(Output::ok(stdout)))
    }

    pub fn gh_fails(self, prefix: &[&str], code: i32, stderr: &str) -> Self {
        self.on(Tool::Gh, prefix, Ok(Output::fail(code, stderr)))
    }

    pub fn git(self, prefix: &[&str], stdout: &str) -> Self {
        self.on(Tool::Git, prefix, Ok(Output::ok(stdout)))
    }

    pub fn git_fails(self, prefix: &[&str], code: i32, stderr: &str) -> Self {
        self.on(Tool::Git, prefix, Ok(Output::fail(code, stderr)))
    }

    /// Every call so far, as `gh pr list ...` / `git push ...` lines.
    pub fn calls(&self) -> Vec<String> {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .map(|(tool, args)| format!("{} {}", if *tool == Tool::Gh { "gh" } else { "git" }, args.join(" ")))
            .collect()
    }

    /// How many calls started with `prefix` (e.g. `"git push"`).
    pub fn count(&self, prefix: &str) -> usize {
        self.calls().iter().filter(|c| c.starts_with(prefix)).count()
    }
}

impl GhRunner for FakeRunner {
    fn run(&self, tool: Tool, args: &[String], _cwd: &Path) -> Reply {
        self.calls.lock().unwrap().push((tool, args.to_vec()));
        let replies = self.replies.lock().unwrap();
        for (t, prefix, reply) in replies.iter().rev() {
            if *t == tool && args.starts_with(prefix) {
                return reply.clone();
            }
        }
        panic!("FakeRunner: no reply for {tool:?} {args:?}");
    }
}
