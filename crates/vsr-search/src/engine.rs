//! The text search engine: a parallel directory walk (`ignore`) feeding `grep-searcher`.
//!
//! upstream: src/vs/workbench/services/search/node/ripgrepTextSearchEngine.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/services/search/node/ripgrepSearchUtils.ts @ 50e1350fc78d
//!
//! Where upstream spawns `rg --json` and parses its output, this calls the same libraries
//! in-process. Results are reported one file at a time, in completion order (the view sorts).

use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use grep_matcher::{LineTerminator, Matcher};
use grep_regex::{RegexMatcher, RegexMatcherBuilder};
use grep_searcher::{BinaryDetection, Searcher, SearcherBuilder, Sink, SinkMatch};
use ignore::{WalkBuilder, WalkState};

use crate::patterns::{FileFilter, parse_search_paths};
use crate::preview::{PreviewOptions, SearchRange, TextSearchMatch, VIEW_PREVIEW_OPTIONS};
use crate::query::{TextSearchQuery, regex_source};
use crate::settings::{DEFAULT_MAX_SEARCH_RESULTS, SearchSettings};

/// Everything about a search except the query text.
#[derive(Clone, Debug)]
pub struct SearchOptions {
    /// Folder to search (the workspace root).
    pub root: PathBuf,
    /// "files to include" entries (already split on commas; see [`crate::split_pattern_input`]).
    pub includes: Vec<String>,
    /// "files to exclude" entries.
    pub excludes: Vec<String>,
    /// `search.useIgnoreFiles` (the "Use Exclude Settings and Ignore Files" toggle turns this
    /// and `exclude_globs_from_settings` off together).
    pub use_ignore_files: bool,
    pub use_parent_ignore_files: bool,
    pub use_global_ignore_files: bool,
    pub follow_symlinks: bool,
    /// `None` = unlimited.
    pub max_results: Option<usize>,
    /// Files bigger than this are skipped.
    pub max_file_size: Option<u64>,
    /// Enabled `files.exclude` + `search.exclude` globs (`!x` entries are exceptions).
    pub exclude_globs_from_settings: Vec<String>,
    /// `search.smartCase`.
    pub smart_case: bool,
    pub ignore_glob_case: bool,
    pub preview: PreviewOptions,
    /// Walker threads (0 = automatic).
    pub threads: usize,
}

impl SearchOptions {
    /// Defaults: upstream settings defaults, nothing included or excluded by the user.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::from_settings(root, &SearchSettings::default())
    }

    pub fn from_settings(root: impl Into<PathBuf>, s: &SearchSettings) -> Self {
        Self {
            root: root.into(),
            includes: Vec::new(),
            excludes: Vec::new(),
            use_ignore_files: s.use_ignore_files,
            use_parent_ignore_files: s.use_parent_ignore_files,
            use_global_ignore_files: s.use_global_ignore_files,
            follow_symlinks: s.follow_symlinks,
            max_results: s.max_results,
            max_file_size: None,
            exclude_globs_from_settings: s.exclude_globs(false),
            smart_case: s.smart_case,
            ignore_glob_case: false,
            preview: VIEW_PREVIEW_OPTIONS,
            threads: 0,
        }
    }
}

impl Default for SearchOptions {
    fn default() -> Self {
        let mut o = Self::new(".");
        o.max_results = Some(DEFAULT_MAX_SEARCH_RESULTS);
        o
    }
}

/// One result chunk: a line (or several, for a multiline match) with all its matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextMatch {
    /// 1-based line of the first range.
    pub line: usize,
    /// Preview text (one line, elided around long lines; multi-line text for multiline matches).
    pub preview: String,
    /// Match ranges in the file: 1-based lines and UTF-16 columns, end exclusive.
    pub ranges: Vec<SearchRange>,
    /// The same ranges inside `preview`: 0-based line (relative to the first) and UTF-16 column.
    pub preview_ranges: Vec<SearchRange>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileMatch {
    /// Absolute path.
    pub path: PathBuf,
    pub matches: Vec<TextMatch>,
}

impl FileMatch {
    /// Number of match ranges (what "N results" counts).
    pub fn result_count(&self) -> usize {
        self.matches.iter().map(|m| m.ranges.len()).sum()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SearchComplete {
    pub limit_hit: bool,
    pub cancelled: bool,
    pub files_searched: usize,
    pub duration: Duration,
}

#[derive(Debug)]
pub enum SearchError {
    EmptyPattern,
    /// The pattern is not a valid regex (message from the regex engine).
    InvalidRegex(String),
    Io(io::Error),
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::EmptyPattern => write!(f, "empty search pattern"),
            SearchError::InvalidRegex(m) => write!(f, "{m}"),
            SearchError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for SearchError {}

/// Build the matcher for `query` (exposed for tests and the replace feature later).
pub fn build_matcher(query: &TextSearchQuery, smart_case: bool) -> Result<RegexMatcher, SearchError> {
    if query.pattern.is_empty() {
        return Err(SearchError::EmptyPattern);
    }
    let multiline = query.effective_multiline();
    let mut b = RegexMatcherBuilder::new();
    b.case_insensitive(!query.effective_case_sensitive(smart_case))
        .multi_line(true)
        .crlf(true)
        .unicode(true);
    if multiline {
        // `crlf(true)` sets a CRLF terminator, which would forbid `\n` in the pattern.
        b.line_terminator(None);
    }
    b.build(&regex_source(query)).map_err(|e| SearchError::InvalidRegex(e.to_string()))
}

fn utf16_len_lossy(bytes: &[u8]) -> usize {
    String::from_utf8_lossy(bytes).encode_utf16().count()
}

/// Lines (`\n` count) and the UTF-16 length of the last line of `bytes`
/// (upstream `getNumLinesAndLastNewlineLength`).
fn lines_and_last_len(bytes: &[u8]) -> (usize, usize) {
    match bytes.iter().rposition(|&b| b == b'\n') {
        Some(last) => (bytes.iter().filter(|&&b| b == b'\n').count(), utf16_len_lossy(&bytes[last + 1..])),
        None => (0, utf16_len_lossy(bytes)),
    }
}

/// Source ranges (0-based line and column) of the submatches of one chunk
/// (upstream `createTextSearchMatch`).
fn submatch_ranges(
    bytes: &[u8],
    first_line0: usize,
    subs: &[(usize, usize)],
    mut room: impl FnMut() -> bool,
) -> Vec<SearchRange> {
    let (mut prev_end, mut prev_end_col, mut prev_end_line) = (0usize, 0usize, first_line0);
    let mut ranges = Vec::new();
    for &(start, end) in subs {
        if !room() {
            break;
        }
        let (n_between, last_between) = lines_and_last_len(&bytes[prev_end..start]);
        let start_col = if n_between > 0 { last_between } else { last_between + prev_end_col };
        let (n_match, last_match) = lines_and_last_len(&bytes[start..end]);
        let start_line = n_between + prev_end_line;
        let end_line = n_match + start_line;
        let end_col = if n_match > 0 { last_match } else { last_match + start_col };
        prev_end = end;
        prev_end_col = end_col;
        prev_end_line = end_line;
        ranges.push(SearchRange::new(start_line, start_col, end_line, end_col));
    }
    ranges
}

struct Shared {
    cancel: AtomicBool, // set when the limit is hit
    limit_hit: AtomicBool,
    results: AtomicUsize,
    files_searched: AtomicUsize,
    max_results: Option<usize>,
    preview: PreviewOptions,
}

struct Collector<'a> {
    matcher: &'a RegexMatcher,
    shared: &'a Shared,
    external_cancel: &'a AtomicBool,
    matches: Vec<TextMatch>,
}

impl Collector<'_> {
    fn stopped(&self) -> bool {
        self.shared.cancel.load(Ordering::Relaxed) || self.external_cancel.load(Ordering::Relaxed)
    }
}

impl Sink for Collector<'_> {
    type Error = io::Error;

    fn matched(&mut self, _s: &Searcher, mat: &SinkMatch<'_>) -> Result<bool, io::Error> {
        if self.stopped() {
            return Ok(false);
        }
        let bytes = mat.bytes();
        let first_line0 = mat.line_number().unwrap_or(1).saturating_sub(1) as usize;
        let mut subs: Vec<(usize, usize)> = Vec::new();
        self.matcher
            .find_iter(bytes, |m| {
                subs.push((m.start(), m.end()));
                true
            })
            .map_err(|e| io::Error::other(e.to_string()))?;
        // Some patterns match a line without reporting a submatch (upstream #100569).
        if subs.is_empty() {
            let text = String::from_utf8_lossy(bytes);
            subs.push(if text.is_empty() { (0, 0) } else { (0, text.chars().next().map_or(0, char::len_utf8)) });
        }
        let shared = self.shared;
        let ranges = submatch_ranges(bytes, first_line0, &subs, || {
            let n = shared.results.fetch_add(1, Ordering::Relaxed) + 1;
            match shared.max_results {
                Some(max) if n > max => {
                    shared.limit_hit.store(true, Ordering::Relaxed);
                    shared.cancel.store(true, Ordering::Relaxed);
                    false
                }
                Some(max) => {
                    if n >= max {
                        shared.limit_hit.store(true, Ordering::Relaxed);
                        shared.cancel.store(true, Ordering::Relaxed);
                    }
                    true
                }
                None => true,
            }
        });
        if ranges.is_empty() {
            return Ok(false);
        }
        let text = String::from_utf8_lossy(bytes);
        let m = TextSearchMatch::new(&text, &ranges, Some(self.shared.preview));
        let to_view = |r: &SearchRange| SearchRange::new(r.start_line + 1, r.start_column + 1, r.end_line + 1, r.end_column + 1);
        self.matches.push(TextMatch {
            line: ranges[0].start_line + 1,
            preview: m.preview_text,
            ranges: m.range_locations.iter().map(|p| to_view(&p.source)).collect(),
            preview_ranges: m.range_locations.iter().map(|p| p.preview).collect(),
        });
        Ok(!self.stopped())
    }
}

/// One folder to walk plus its filter.
struct Folder {
    dir: PathBuf,
    filter: Arc<FileFilter>,
}

fn plan_folders(options: &SearchOptions) -> Vec<Folder> {
    let parsed_inc = parse_search_paths(&options.root, &options.includes);
    let parsed_exc = parse_search_paths(&options.root, &options.excludes);
    let settings_excludes: &[String] = &options.exclude_globs_from_settings;
    let mk = |dir: &Path, folder_includes: &[String]| {
        let mut excludes = parsed_exc.patterns.clone();
        // Exclude search paths that live inside this folder become folder-relative globs.
        for sp in &parsed_exc.search_paths {
            if let Ok(rel) = sp.dir.strip_prefix(dir) {
                let rel = rel.to_string_lossy().replace('\\', "/");
                if sp.patterns.is_empty() {
                    if !rel.is_empty() {
                        excludes.push(rel.clone());
                        excludes.push(format!("{rel}/**"));
                    }
                } else {
                    excludes.extend(sp.patterns.iter().map(|p| if rel.is_empty() { p.clone() } else { format!("{rel}/{p}") }));
                }
            }
        }
        Folder {
            dir: dir.to_path_buf(),
            filter: Arc::new(FileFilter::new(
                &parsed_inc.patterns,
                folder_includes,
                &excludes,
                settings_excludes,
                options.ignore_glob_case,
            )),
        }
    };
    if parsed_inc.search_paths.is_empty() {
        vec![mk(&options.root, &[])]
    } else {
        parsed_inc.search_paths.iter().map(|sp| mk(&sp.dir, &sp.patterns)).collect()
    }
}

fn walker(dir: &Path, options: &SearchOptions, filter: &Arc<FileFilter>) -> WalkBuilder {
    let mut wb = WalkBuilder::new(dir);
    let local = options.use_ignore_files;
    wb.hidden(false) // `--hidden`
        .require_git(false) // `--no-require-git`
        .follow_links(options.follow_symlinks)
        .max_filesize(options.max_file_size)
        .git_ignore(local)
        .git_exclude(local)
        .ignore(local)
        .git_global(local && options.use_global_ignore_files)
        .parents(local && options.use_parent_ignore_files)
        .threads(options.threads);
    if local {
        wb.add_custom_ignore_filename(".rgignore");
    }
    let f = Arc::clone(filter);
    let base = dir.to_path_buf();
    wb.filter_entry(move |e| {
        if e.depth() == 0 || !e.file_type().is_some_and(|t| t.is_dir()) {
            return true;
        }
        match e.path().strip_prefix(&base) {
            Ok(rel) => !f.prune_dir(&rel.to_string_lossy()),
            Err(_) => true,
        }
    });
    wb
}

/// Run a streaming text search. `on_file` is called for each file with matches, in completion
/// order, on the calling thread. Returns when the walk is complete, the result limit is hit, or
/// `cancel` is set.
///
/// The walk runs on `ignore`'s worker threads, each with one `Searcher` it reuses for every file.
/// Matches travel to the calling thread over a channel: the workers never wait on a callback lock,
/// and `on_file` needs no `Send` bound.
pub fn search(
    query: &TextSearchQuery,
    options: &SearchOptions,
    cancel: &AtomicBool,
    mut on_file: impl FnMut(FileMatch),
) -> Result<SearchComplete, SearchError> {
    let started = Instant::now();
    let matcher = build_matcher(query, options.smart_case)?;
    let multiline = query.effective_multiline();
    let shared = Shared {
        cancel: AtomicBool::new(false),
        limit_hit: AtomicBool::new(false),
        results: AtomicUsize::new(0),
        files_searched: AtomicUsize::new(0),
        max_results: options.max_results,
        preview: options.preview,
    };
    let seen = Mutex::new(std::collections::HashSet::<PathBuf>::new());
    let folders = plan_folders(options);
    let dedupe = folders.len() > 1;
    let (tx, rx) = std::sync::mpsc::channel::<FileMatch>();

    std::thread::scope(|scope| {
        let (matcher, shared, seen) = (&matcher, &shared, &seen);
        let walk = scope.spawn(move || {
            for folder in &folders {
                if cancel.load(Ordering::Relaxed) || shared.cancel.load(Ordering::Relaxed) {
                    break;
                }
                let dir = folder.dir.clone();
                let filter = Arc::clone(&folder.filter);
                walker(&dir, options, &folder.filter).build_parallel().run(|| {
                    let tx = tx.clone();
                    let mut searcher = SearcherBuilder::new()
                        .line_number(true)
                        .multi_line(multiline)
                        .binary_detection(BinaryDetection::quit(b'\x00'))
                        .line_terminator(LineTerminator::crlf())
                        .build();
                    let (dir, filter) = (&dir, &filter);
                    Box::new(move |entry| {
                        if cancel.load(Ordering::Relaxed) || shared.cancel.load(Ordering::Relaxed) {
                            return WalkState::Quit;
                        }
                        let Ok(entry) = entry else { return WalkState::Continue };
                        if !entry.file_type().is_some_and(|t| t.is_file()) {
                            return WalkState::Continue;
                        }
                        let path = entry.path();
                        let rel = path.strip_prefix(dir).unwrap_or(path).to_string_lossy();
                        if !filter.accept_file(&rel) {
                            return WalkState::Continue;
                        }
                        if dedupe && !seen.lock().unwrap_or_else(|e| e.into_inner()).insert(path.to_path_buf()) {
                            return WalkState::Continue;
                        }
                        shared.files_searched.fetch_add(1, Ordering::Relaxed);
                        let mut sink = Collector { matcher, shared, external_cancel: cancel, matches: Vec::new() };
                        // I/O errors (permissions, races with deletion) skip the file, like ripgrep.
                        let _ = searcher.search_path(matcher, path, &mut sink);
                        if !sink.matches.is_empty() {
                            // The receiver outlives the walk; a send error only means it stopped.
                            let _ = tx.send(FileMatch { path: path.to_path_buf(), matches: sink.matches });
                        }
                        WalkState::Continue
                    })
                });
            }
            // `tx` is dropped here, and each worker's clone with its visitor: `rx` ends.
        });
        for file in rx {
            on_file(file);
        }
        walk.join().unwrap_or_else(|panic| std::panic::resume_unwind(panic));
    });

    Ok(SearchComplete {
        limit_hit: shared.limit_hit.load(Ordering::Relaxed),
        cancelled: cancel.load(Ordering::Relaxed),
        files_searched: shared.files_searched.load(Ordering::Relaxed),
        duration: started.elapsed(),
    })
}

/// Convenience for callers and tests that want everything at once, sorted by path.
pub fn search_collect(
    query: &TextSearchQuery,
    options: &SearchOptions,
) -> Result<(Vec<FileMatch>, SearchComplete), SearchError> {
    let cancel = AtomicBool::new(false);
    let out = Mutex::new(Vec::new());
    let done = search(query, options, &cancel, |f| out.lock().unwrap_or_else(|e| e.into_inner()).push(f))?;
    let mut files = out.into_inner().unwrap_or_else(|e| e.into_inner());
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok((files, done))
}
