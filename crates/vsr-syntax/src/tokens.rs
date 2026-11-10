//! Incremental, per-line tokenization of a document.
//!
//! upstream: src/vs/editor/common/model/tokens/tokenizationTextModelPart.ts @ 50e1350fc78d
//! upstream: src/vs/editor/common/model/textModelTokens.ts @ 50e1350fc78d
//!
//! Like upstream's `TokenizationStateStore`, every line keeps the rule stack the tokenizer had
//! before and after it. After an edit only the changed lines are re-tokenized, then following
//! lines until one starts in the same state it had before (it and everything after it is still
//! right). Tokenizing a big file runs on a background thread (`BackgroundTokenizer`); the editor
//! shows the old tokens of a line until its new ones arrive, which is also what upstream does.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use vsr_textmate::{Grammar, StateStack, Token, TokenType};

/// The lines of a document, 0-based.
pub trait LineSource {
    fn line_count(&self) -> usize;
    fn line(&self, index: usize) -> &str;
}

impl LineSource for Vec<&str> {
    fn line_count(&self) -> usize {
        self.len()
    }
    fn line(&self, index: usize) -> &str {
        self[index]
    }
}

impl LineSource for Vec<String> {
    fn line_count(&self) -> usize {
        self.len()
    }
    fn line(&self, index: usize) -> &str {
        &self[index]
    }
}

struct LineEntry {
    text: String,
    tokens: Option<Arc<[Token]>>,
    start_state: Option<StateStack>,
    end_state: Option<StateStack>,
}

impl LineEntry {
    fn dirty(text: &str) -> Self {
        Self { text: text.to_string(), tokens: None, start_state: None, end_state: None }
    }
}

pub struct DocumentTokens {
    grammar: Arc<Grammar>,
    lines: Vec<LineEntry>,
    /// Every line before this index has tokens computed from the correct start state.
    valid_upto: usize,
    max_line_length: usize,
    /// How many lines were tokenized (not skipped by convergence) since creation.
    tokenized_lines: u64,
}

impl DocumentTokens {
    pub fn new(grammar: Arc<Grammar>, max_line_length: usize) -> Self {
        Self { grammar, lines: Vec::new(), valid_upto: 0, max_line_length, tokenized_lines: 0 }
    }

    pub fn grammar(&self) -> &Arc<Grammar> {
        &self.grammar
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Lines that were actually tokenized so far (for tests and measurements).
    pub fn tokenized_lines(&self) -> u64 {
        self.tokenized_lines
    }

    /// `editor.maxTokenizationLineLength`: longer lines are not tokenized.
    pub fn set_max_line_length(&mut self, n: usize) {
        if n != self.max_line_length {
            self.max_line_length = n;
            self.invalidate_all();
        }
    }

    pub fn invalidate_all(&mut self) {
        for l in &mut self.lines {
            l.tokens = None;
            l.start_state = None;
            l.end_state = None;
        }
        self.valid_upto = 0;
    }

    /// Brings the stored lines in line with `src`: the first and last differing lines bound the
    /// edit, lines inside are marked for re-tokenizing. Returns the first changed line.
    pub fn sync(&mut self, src: &dyn LineSource) -> Option<usize> {
        let new_len = src.line_count();
        let old_len = self.lines.len();
        let max = old_len.min(new_len);
        let mut p = 0;
        while p < max && self.lines[p].text == src.line(p) {
            p += 1;
        }
        if p == old_len && p == new_len {
            return None;
        }
        let mut s = 0;
        while s < max - p && self.lines[old_len - 1 - s].text == src.line(new_len - 1 - s) {
            s += 1;
        }
        let fresh: Vec<LineEntry> = (p..new_len - s).map(|i| LineEntry::dirty(src.line(i))).collect();
        self.lines.splice(p..old_len - s, fresh);
        self.valid_upto = self.valid_upto.min(p);
        Some(p)
    }

    pub fn is_up_to_date(&self) -> bool {
        self.valid_upto >= self.lines.len()
    }

    /// Tokenizes pending lines, at most `max_lines` and until `deadline`. Returns whether the
    /// whole document is now up to date.
    pub fn tokenize_some(&mut self, max_lines: usize, deadline: Option<Instant>) -> bool {
        let n = self.lines.len();
        let mut done = 0;
        while self.valid_upto < n {
            if done >= max_lines || deadline.is_some_and(|d| Instant::now() >= d) {
                return false;
            }
            let i = self.valid_upto;
            let start = if i == 0 { StateStack::initial() } else { self.lines[i - 1].end_state.clone().expect("lines before valid_upto are tokenized") };
            if self.lines[i].tokens.is_some() && self.lines[i].start_state.as_ref() == Some(&start) {
                // Converged: this line and the ones after it up to the next changed line are right.
                let next = self.lines[i + 1..].iter().position(|l| l.tokens.is_none()).map_or(n, |k| i + 1 + k);
                self.valid_upto = next;
                continue;
            }
            let entry = &mut self.lines[i];
            if entry.text.len() > self.max_line_length {
                // Too long to tokenize: one token, and the state carries over unchanged.
                entry.tokens = Some(Arc::from(vec![Token { start: 0, scope: 0 }]));
                entry.end_state = Some(start.clone());
            } else {
                let r = self.grammar.tokenize_line(&entry.text, &start);
                entry.tokens = Some(Arc::from(r.tokens));
                entry.end_state = Some(r.end_state);
            }
            entry.start_state = Some(start);
            self.valid_upto = i + 1;
            self.tokenized_lines += 1;
            done += 1;
        }
        true
    }

    /// Whether the line's tokens were computed from the right start state (upstream
    /// `hasAccurateTokensForLine`).
    pub fn is_accurate(&self, line: usize) -> bool {
        line < self.valid_upto
    }

    /// The tokens of a 0-based line, if it has been tokenized (possibly before a later edit).
    pub fn tokens(&self, line: usize) -> Option<Arc<[Token]>> {
        self.lines.get(line)?.tokens.clone()
    }

    /// The tokens of a range of lines (one lock when shared).
    pub fn tokens_in(&self, lines: std::ops::Range<usize>) -> Vec<Option<Arc<[Token]>>> {
        lines.map(|l| self.tokens(l)).collect()
    }

    /// The type of the token a `ch` inserted at byte `at` of the 0-based `line` would be in, from
    /// the line's start state (upstream `getTokenTypeIfInsertingCharacter`). `None` when the line
    /// is not accurate or too long. Typing a quote asks this to tell whether it ends a string.
    pub fn token_type_if_inserting(&self, line: usize, at: usize, ch: char) -> Option<TokenType> {
        if !self.is_accurate(line) {
            return None;
        }
        let start = if line == 0 { StateStack::initial() } else { self.lines[line - 1].end_state.clone()? };
        let text = &self.lines.get(line)?.text;
        if at > text.len() || !text.is_char_boundary(at) || text.len() + ch.len_utf8() > self.max_line_length {
            return None;
        }
        let mut modified = String::with_capacity(text.len() + ch.len_utf8());
        modified.push_str(&text[..at]);
        modified.push(ch);
        modified.push_str(&text[at..]);
        let r = self.grammar.tokenize_line(&modified, &start);
        let ix = r.tokens.partition_point(|t| t.start as usize <= at).saturating_sub(1);
        r.tokens.get(ix).map(|t| self.grammar.token_type(t.scope))
    }

    /// The standard token type at a byte offset of a line (`Other` when not tokenized).
    pub fn token_type_at(&self, line: usize, byte: usize) -> TokenType {
        let Some(tokens) = self.tokens(line) else { return TokenType::Other };
        let ix = tokens.partition_point(|t| t.start as usize <= byte).saturating_sub(1);
        tokens.get(ix).map_or(TokenType::Other, |t| self.grammar.token_type(t.scope))
    }
}

/// A document's tokens plus a worker thread that tokenizes in the background.
pub struct BackgroundTokenizer {
    shared: Arc<Shared>,
}

struct Shared {
    doc: Mutex<DocumentTokens>,
    wake: Condvar,
    stop: AtomicBool,
    /// Bumped whenever the worker finished a batch (a repaint is due).
    progress: AtomicU64,
}

impl BackgroundTokenizer {
    pub fn new(grammar: Arc<Grammar>, max_line_length: usize) -> Self {
        let shared = Arc::new(Shared {
            doc: Mutex::new(DocumentTokens::new(grammar, max_line_length)),
            wake: Condvar::new(),
            stop: AtomicBool::new(false),
            progress: AtomicU64::new(0),
        });
        let worker = shared.clone();
        std::thread::Builder::new()
            .name("syntax-tokenizer".into())
            .spawn(move || worker.run())
            .expect("spawn the tokenizer thread");
        Self { shared }
    }

    /// Syncs the stored lines with `src` and tokenizes the changed lines now (up to `budget`),
    /// leaving the rest to the worker.
    pub fn update(&self, src: &dyn LineSource, budget: Duration) {
        let mut doc = self.shared.doc.lock().expect("tokenizer lock");
        let changed = doc.sync(src);
        if changed.is_some() {
            doc.tokenize_some(usize::MAX, Some(Instant::now() + budget));
        }
        let done = doc.is_up_to_date();
        drop(doc);
        if !done {
            self.shared.wake.notify_one();
        }
    }

    pub fn set_max_line_length(&self, n: usize) {
        self.shared.doc.lock().expect("tokenizer lock").set_max_line_length(n);
        self.shared.wake.notify_one();
    }

    pub fn with<R>(&self, f: impl FnOnce(&DocumentTokens) -> R) -> R {
        f(&self.shared.doc.lock().expect("tokenizer lock"))
    }

    /// True while lines still wait for the worker.
    pub fn is_busy(&self) -> bool {
        !self.shared.doc.lock().expect("tokenizer lock").is_up_to_date()
    }

    /// Increases whenever the worker produced tokens since the last look.
    pub fn progress(&self) -> u64 {
        self.shared.progress.load(Ordering::Relaxed)
    }

    /// Blocks until the worker has caught up (tests, measurements).
    pub fn wait_idle(&self, timeout: Duration) -> bool {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            if !self.is_busy() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        !self.is_busy()
    }
}

impl Drop for BackgroundTokenizer {
    fn drop(&mut self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        self.shared.wake.notify_all();
    }
}

impl Shared {
    fn run(&self) {
        let mut doc = self.doc.lock().expect("tokenizer lock");
        loop {
            if self.stop.load(Ordering::Relaxed) {
                return;
            }
            if doc.is_up_to_date() {
                doc = self.wake.wait_timeout(doc, Duration::from_millis(250)).expect("tokenizer lock").0;
                continue;
            }
            // A short slice, so the editor thread is never blocked for long.
            doc.tokenize_some(usize::MAX, Some(Instant::now() + Duration::from_millis(4)));
            self.progress.fetch_add(1, Ordering::Relaxed);
            drop(doc);
            std::thread::yield_now();
            doc = self.doc.lock().expect("tokenizer lock");
        }
    }
}
