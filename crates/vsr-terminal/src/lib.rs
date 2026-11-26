//! The integrated terminal core (ADR-0045): a shell on a PTY (`portable-pty`) whose output feeds a
//! terminal emulator grid (`alacritty_terminal`). No UI: the app draws a [`Snapshot`].
//!
//! Upstream source: src/vs/platform/terminal/node/terminalProcess.ts (spawn, resize, input) and
//! src/vs/workbench/contrib/terminal/browser/terminalInstance.ts (what an instance exposes).

mod keys;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use alacritty_terminal::event::{Event, EventListener};
use alacritty_terminal::grid::{Dimensions, Scroll};
use alacritty_terminal::index::{Column, Line, Point, Side};
use alacritty_terminal::selection::{Selection, SelectionType};
use alacritty_terminal::term::{Config as TermConfig, Term, TermMode};
use alacritty_terminal::vte::ansi::{CursorShape, Processor};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};

pub use alacritty_terminal::term::cell::Flags as CellFlags;
pub use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
pub use keys::{Key, Modifiers, encode_key};

/// What a [`Terminal`] reports to its owner, from the reader thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TerminalEvent {
    /// New output was processed: draw again.
    Output,
    /// The shell set the title (OSC 0/2); `None` resets it.
    Title(Option<String>),
    Bell,
    /// The shell exited with this code (`None` when it was killed by a signal).
    Exit(Option<u32>),
}

/// How to start a terminal.
#[derive(Debug, Clone)]
pub struct TerminalConfig {
    /// The shell; `None` is `$SHELL`, else `/bin/zsh`.
    pub shell: Option<PathBuf>,
    /// Arguments; `None` is a login shell (`-l`), as VS Code starts it on macOS.
    pub args: Option<Vec<String>>,
    pub cwd: Option<PathBuf>,
    /// Extra environment on top of the inherited one.
    pub env: Vec<(String, String)>,
    pub cols: u16,
    pub rows: u16,
    /// `terminal.integrated.scrollback`.
    pub scrollback: usize,
}

impl Default for TerminalConfig {
    fn default() -> Self {
        Self { shell: None, args: None, cwd: None, env: Vec::new(), cols: 80, rows: 24, scrollback: 1000 }
    }
}

/// The shell to start: `shell`, else `$SHELL`, else `/bin/zsh`.
pub fn default_shell(shell: Option<&PathBuf>) -> PathBuf {
    shell
        .cloned()
        .or_else(|| std::env::var_os("SHELL").filter(|s| !s.is_empty()).map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("/bin/zsh"))
}

struct Size {
    cols: usize,
    rows: usize,
    history: usize,
}

impl Dimensions for Size {
    fn total_lines(&self) -> usize {
        self.rows + self.history
    }
    fn screen_lines(&self) -> usize {
        self.rows
    }
    fn columns(&self) -> usize {
        self.cols
    }
}

type Callback = Arc<dyn Fn(TerminalEvent) + Send + Sync>;

/// The emulator's event sink: answers queries the shell makes (cursor position, device attributes) on
/// the PTY, and forwards what the owner cares about.
#[derive(Clone)]
struct Listener {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    callback: Callback,
}

impl EventListener for Listener {
    fn send_event(&self, event: Event) {
        match event {
            Event::PtyWrite(text) => {
                if let Ok(mut w) = self.writer.lock() {
                    let _ = w.write_all(text.as_bytes());
                    let _ = w.flush();
                }
            }
            Event::Title(t) => (self.callback)(TerminalEvent::Title(Some(t))),
            Event::ResetTitle => (self.callback)(TerminalEvent::Title(None)),
            Event::Bell => (self.callback)(TerminalEvent::Bell),
            _ => {}
        }
    }
}

/// A cell of the visible grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub flags: CellFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cursor {
    pub row: usize,
    pub col: usize,
    pub shape: CursorShape,
}

/// What the view needs to draw one frame.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub cols: usize,
    pub rows: usize,
    /// `rows` rows of `cols` cells.
    pub cells: Vec<Vec<Cell>>,
    /// `None` when hidden or scrolled out of view.
    pub cursor: Option<Cursor>,
    /// Lines the view is scrolled up from the bottom (0: following the output).
    pub display_offset: usize,
    pub history_size: usize,
    /// The selection as (row, col) of its start and end in the visible grid, clipped to it, and whether it is a block.
    pub selection: Option<((usize, usize), (usize, usize), bool)>,
    pub application_cursor: bool,
    pub bracketed_paste: bool,
    pub alt_screen: bool,
}

impl Snapshot {
    /// The text of row `row`, trailing blanks trimmed.
    pub fn row_text(&self, row: usize) -> String {
        let s: String = self.cells[row]
            .iter()
            .filter(|c| !c.flags.contains(CellFlags::WIDE_CHAR_SPACER))
            .map(|c| c.ch)
            .collect();
        s.trim_end().to_string()
    }

    /// All visible rows joined with newlines, trailing blank rows dropped.
    pub fn text(&self) -> String {
        let mut rows: Vec<String> = (0..self.rows).map(|r| self.row_text(r)).collect();
        while rows.last().is_some_and(|r| r.is_empty()) {
            rows.pop();
        }
        rows.join("\n")
    }
}

type Shared = Arc<Mutex<Term<Listener>>>;

pub struct Terminal {
    term: Shared,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    exit: Arc<Mutex<Option<Option<u32>>>>,
    title: Arc<Mutex<Option<String>>>,
    scrollback: usize,
    pid: Option<u32>,
}

impl Terminal {
    /// Spawns the shell. `on_event` runs on background threads.
    pub fn spawn(config: &TerminalConfig, on_event: impl Fn(TerminalEvent) + Send + Sync + 'static) -> Result<Terminal, String> {
        let pty = native_pty_system();
        let pair = pty
            .openpty(PtySize { rows: config.rows, cols: config.cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| e.to_string())?;
        let shell = default_shell(config.shell.as_ref());
        let mut cmd = CommandBuilder::new(&shell);
        match &config.args {
            Some(args) => cmd.args(args),
            None => cmd.arg("-l"),
        }
        if let Some(cwd) = &config.cwd {
            cmd.cwd(cwd);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "vscode");
        for (k, v) in &config.env {
            cmd.env(k, v);
        }
        let mut child = pair.slave.spawn_command(cmd).map_err(|e| format!("{}: {e}", shell.display()))?;
        drop(pair.slave);
        let pid = child.process_id();
        let killer = child.clone_killer();
        let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
        let writer: Arc<Mutex<Box<dyn Write + Send>>> = Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| e.to_string())?));

        let callback: Callback = Arc::new(on_event);
        let title = Arc::new(Mutex::new(None));
        let title_cb = title.clone();
        let user_cb = callback.clone();
        let tracking: Callback = Arc::new(move |e| {
            if let TerminalEvent::Title(t) = &e {
                *title_cb.lock().unwrap() = t.clone();
            }
            user_cb(e)
        });
        let listener = Listener { writer: writer.clone(), callback: tracking.clone() };
        let size = Size { cols: config.cols as usize, rows: config.rows as usize, history: 0 };
        let term_config = TermConfig { scrolling_history: config.scrollback, ..TermConfig::default() };
        let term: Shared = Arc::new(Mutex::new(Term::new(term_config, &size, listener)));

        let reader_term = term.clone();
        let reader_cb = tracking.clone();
        std::thread::Builder::new()
            .name("terminal-reader".into())
            .spawn(move || {
                let mut processor: Processor = Processor::new();
                let mut buf = [0u8; 65536];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            {
                                let mut t = reader_term.lock().unwrap();
                                processor.advance(&mut *t, &buf[..n]);
                            }
                            reader_cb(TerminalEvent::Output);
                        }
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        let exit = Arc::new(Mutex::new(None));
        let exit_slot = exit.clone();
        let exit_cb = tracking;
        std::thread::Builder::new()
            .name("terminal-wait".into())
            .spawn(move || {
                let code = child.wait().ok().map(|s| s.exit_code());
                // Give the reader a moment to drain what the shell printed before it exited.
                std::thread::sleep(std::time::Duration::from_millis(50));
                *exit_slot.lock().unwrap() = Some(code);
                exit_cb(TerminalEvent::Exit(code));
            })
            .map_err(|e| e.to_string())?;

        Ok(Terminal {
            term,
            writer,
            master: Mutex::new(pair.master),
            killer: Mutex::new(killer),
            exit,
            title,
            scrollback: config.scrollback,
            pid,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// The title the shell set, if any.
    pub fn title(&self) -> Option<String> {
        self.title.lock().unwrap().clone()
    }

    /// `Some(code)` once the shell has exited.
    pub fn exit_status(&self) -> Option<Option<u32>> {
        *self.exit.lock().unwrap()
    }

    /// Sends bytes to the shell (keyboard input, pasted text).
    pub fn write(&self, bytes: &[u8]) {
        if self.exit_status().is_some() {
            return;
        }
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(bytes);
            let _ = w.flush();
        }
    }

    /// Pastes text, in bracketed paste mode when the shell asked for it.
    pub fn paste(&self, text: &str) {
        let bracketed = self.term.lock().unwrap().mode().contains(TermMode::BRACKETED_PASTE);
        if bracketed {
            self.write(format!("\x1b[200~{text}\x1b[201~").as_bytes());
        } else {
            self.write(text.replace("\r\n", "\r").replace('\n', "\r").as_bytes());
        }
    }

    /// Sends a key as the shell expects it; false when the key has no terminal encoding.
    pub fn send_key(&self, key: &Key, mods: Modifiers) -> bool {
        let app_cursor = self.term.lock().unwrap().mode().contains(TermMode::APP_CURSOR);
        match encode_key(key, mods, app_cursor) {
            Some(bytes) => {
                self.write(&bytes);
                true
            }
            None => false,
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) {
        let (cols, rows) = (cols.max(2), rows.max(1));
        let _ = self.master.lock().unwrap().resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 });
        let history = self.term.lock().unwrap().grid().history_size();
        self.term.lock().unwrap().resize(Size { cols: cols as usize, rows: rows as usize, history: history.min(self.scrollback) });
    }

    /// Kills the shell.
    pub fn kill(&self) {
        let _ = self.killer.lock().unwrap().kill();
    }

    /// Scrolls the view: positive lines go up into the history.
    pub fn scroll(&self, lines: i32) {
        self.term.lock().unwrap().scroll_display(Scroll::Delta(lines));
    }

    pub fn scroll_to_bottom(&self) {
        self.term.lock().unwrap().scroll_display(Scroll::Bottom);
    }

    /// Whether the shell uses the alternate screen or asked for mouse reporting.
    pub fn alt_screen(&self) -> bool {
        self.term.lock().unwrap().mode().contains(TermMode::ALT_SCREEN)
    }

    pub fn start_selection(&self, row: usize, col: usize, kind: SelectionKind) {
        let mut t = self.term.lock().unwrap();
        let point = viewport_point(&t, row, col);
        let ty = match kind {
            SelectionKind::Simple => SelectionType::Simple,
            SelectionKind::Word => SelectionType::Semantic,
            SelectionKind::Line => SelectionType::Lines,
            SelectionKind::Block => SelectionType::Block,
        };
        t.selection = Some(Selection::new(ty, point, Side::Left));
    }

    pub fn update_selection(&self, row: usize, col: usize) {
        let mut t = self.term.lock().unwrap();
        let point = viewport_point(&t, row, col);
        if let Some(sel) = t.selection.as_mut() {
            sel.update(point, Side::Right);
        }
    }

    pub fn clear_selection(&self) {
        self.term.lock().unwrap().selection = None;
    }

    /// The selected text.
    pub fn selection_text(&self) -> Option<String> {
        self.term.lock().unwrap().selection_to_string().filter(|s| !s.is_empty())
    }

    /// Selects the whole buffer (visible grid and scrollback).
    pub fn select_all(&self) {
        let mut t = self.term.lock().unwrap();
        let start = Point::new(t.grid().topmost_line(), Column(0));
        let end = Point::new(t.grid().bottommost_line(), t.grid().last_column());
        let mut sel = Selection::new(SelectionType::Simple, start, Side::Left);
        sel.update(end, Side::Right);
        t.selection = Some(sel);
    }

    /// Copies the visible grid, cursor and mode flags.
    pub fn snapshot(&self) -> Snapshot {
        let t = self.term.lock().unwrap();
        let content = t.renderable_content();
        let (cols, rows) = (t.columns(), t.screen_lines());
        let offset = content.display_offset;
        let blank = Cell { ch: ' ', fg: Color::Named(NamedColor::Foreground), bg: Color::Named(NamedColor::Background), flags: CellFlags::empty() };
        let mut cells = vec![vec![blank; cols]; rows];
        for indexed in content.display_iter {
            let row = indexed.point.line.0 + offset as i32;
            if row < 0 || row as usize >= rows {
                continue;
            }
            let c = &indexed.cell;
            cells[row as usize][indexed.point.column.0] = Cell { ch: c.c, fg: c.fg, bg: c.bg, flags: c.flags };
        }
        let cursor_row = content.cursor.point.line.0 + offset as i32;
        let cursor = (content.cursor.shape != CursorShape::Hidden && cursor_row >= 0 && (cursor_row as usize) < rows)
            .then(|| Cursor { row: cursor_row as usize, col: content.cursor.point.column.0, shape: content.cursor.shape });
        let selection = content.selection.map(|s| {
            let clip = |p: Point, last: bool| {
                let row = p.line.0 + offset as i32;
                if row < 0 {
                    (0, 0)
                } else if row as usize >= rows {
                    (rows - 1, cols - 1)
                } else {
                    let _ = last;
                    (row as usize, p.column.0)
                }
            };
            (clip(s.start, false), clip(s.end, true), s.is_block)
        });
        Snapshot {
            cols,
            rows,
            cells,
            cursor,
            display_offset: offset,
            history_size: t.grid().history_size(),
            selection,
            application_cursor: content.mode.contains(TermMode::APP_CURSOR),
            bracketed_paste: content.mode.contains(TermMode::BRACKETED_PASTE),
            alt_screen: content.mode.contains(TermMode::ALT_SCREEN),
        }
    }
}

/// How a selection starts: a drag, a double click (word), a triple click (line), or Alt-drag (block).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionKind {
    Simple,
    Word,
    Line,
    Block,
}

fn viewport_point<T>(t: &Term<T>, row: usize, col: usize) -> Point {
    let offset = t.grid().display_offset() as i32;
    Point::new(Line(row as i32 - offset), Column(col.min(t.columns().saturating_sub(1))))
}

impl Drop for Terminal {
    fn drop(&mut self) {
        if self.exit_status().is_none() {
            self.kill();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    use super::*;

    fn sh(script: &str) -> (Terminal, mpsc::Receiver<TerminalEvent>) {
        let (tx, rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let config = TerminalConfig {
            shell: Some("/bin/sh".into()),
            args: Some(vec!["-c".into(), script.into()]),
            ..TerminalConfig::default()
        };
        let t = Terminal::spawn(&config, move |e| {
            let _ = tx.lock().unwrap().send(e);
        })
        .expect("spawn");
        (t, rx)
    }

    /// Waits until the shell has exited and its output was processed.
    fn wait_exit(t: &Terminal, rx: &mpsc::Receiver<TerminalEvent>) -> Option<u32> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(code) = t.exit_status() {
                std::thread::sleep(Duration::from_millis(100));
                return code;
            }
            assert!(Instant::now() < deadline, "the shell did not exit");
            let _ = rx.recv_timeout(Duration::from_millis(50));
        }
    }

    fn wait_for_text(t: &Terminal, text: &str) -> Snapshot {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let s = t.snapshot();
            if s.text().contains(text) {
                return s;
            }
            assert!(Instant::now() < deadline, "never saw {text:?}; screen: {:?}", s.text());
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn output_reaches_the_grid() {
        let (t, rx) = sh("printf 'hello\\r\\nworld'");
        assert_eq!(wait_exit(&t, &rx), Some(0));
        let s = t.snapshot();
        assert_eq!(s.row_text(0), "hello");
        assert_eq!(s.row_text(1), "world");
        assert_eq!(s.cursor.map(|c| (c.row, c.col)), Some((1, 5)));
    }

    #[test]
    fn colours_and_styles_are_in_the_cells() {
        let (t, rx) = sh("printf '\\033[1;31mR\\033[0m \\033[38;2;1;2;3mT\\033[0m'");
        wait_exit(&t, &rx);
        let s = t.snapshot();
        assert_eq!(s.cells[0][0].fg, Color::Named(NamedColor::Red));
        assert!(s.cells[0][0].flags.contains(CellFlags::BOLD));
        assert_eq!(s.cells[0][2].fg, Color::Spec(Rgb { r: 1, g: 2, b: 3 }));
    }

    #[test]
    fn the_exit_code_is_reported() {
        let (t, rx) = sh("exit 7");
        assert_eq!(wait_exit(&t, &rx), Some(7));
    }

    #[test]
    fn the_working_directory_is_the_configured_one() {
        let dir = std::env::temp_dir().join(format!("vsr-terminal-cwd-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let canonical = dir.canonicalize().unwrap();
        let (tx, _rx) = mpsc::channel();
        let tx = Mutex::new(tx);
        let config = TerminalConfig {
            shell: Some("/bin/sh".into()),
            args: Some(vec!["-c".into(), "pwd".into()]),
            cwd: Some(canonical.clone()),
            ..TerminalConfig::default()
        };
        let t = Terminal::spawn(&config, move |e| {
            let _ = tx.lock().unwrap().send(e);
        })
        .unwrap();
        wait_for_text(&t, canonical.to_str().unwrap());
    }

    #[test]
    fn input_reaches_the_shell_and_the_shell_echoes_it() {
        let (t, _rx) = sh("read line; printf 'got:%s' \"$line\"");
        t.write(b"abc\r");
        wait_for_text(&t, "got:abc");
    }

    #[test]
    fn resize_changes_the_grid_and_the_shells_idea_of_it() {
        let (t, _rx) = sh("sleep 0.3; stty size");
        t.resize(100, 30);
        let s = t.snapshot();
        assert_eq!((s.cols, s.rows), (100, 30));
        wait_for_text(&t, "30 100");
    }

    #[test]
    fn scrollback_keeps_lines_and_scrolling_shows_them() {
        let (t, rx) = sh("i=1; while [ $i -le 60 ]; do echo line$i; i=$((i+1)); done");
        wait_exit(&t, &rx);
        let s = t.snapshot();
        assert!(s.history_size >= 30, "{}", s.history_size);
        assert!(s.text().contains("line60"));
        assert!(!s.text().contains("line1\n"));
        t.scroll(1000);
        let top = t.snapshot();
        assert!(top.display_offset > 0);
        assert_eq!(top.row_text(0), "line1");
        t.scroll_to_bottom();
        assert_eq!(t.snapshot().display_offset, 0);
    }

    #[test]
    fn a_selection_is_copied() {
        let (t, rx) = sh("printf 'hello world'");
        wait_exit(&t, &rx);
        t.start_selection(0, 0, SelectionKind::Simple);
        t.update_selection(0, 4);
        assert_eq!(t.selection_text().as_deref(), Some("hello"));
        assert_eq!(t.snapshot().selection, Some(((0, 0), (0, 4), false)));
        t.clear_selection();
        assert_eq!(t.selection_text(), None);
        t.start_selection(0, 7, SelectionKind::Word);
        assert_eq!(t.selection_text().as_deref(), Some("world"));
    }

    #[test]
    fn kill_ends_the_shell() {
        let (t, rx) = sh("sleep 30");
        t.kill();
        wait_exit(&t, &rx);
        assert!(t.exit_status().is_some());
    }

    #[test]
    fn the_title_is_tracked() {
        let (t, rx) = sh("printf '\\033]0;my title\\007'");
        wait_exit(&t, &rx);
        assert_eq!(t.title().as_deref(), Some("my title"));
    }

    #[test]
    fn the_default_shell_comes_from_the_environment_or_falls_back() {
        assert_eq!(default_shell(Some(&"/bin/bash".into())), PathBuf::from("/bin/bash"));
        assert!(default_shell(None).is_absolute());
    }
}
