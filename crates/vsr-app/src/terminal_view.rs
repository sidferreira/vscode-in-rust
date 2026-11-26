//! The terminal in the panel (ADR-0045, ADR-0112): a view per terminal that draws a `vsr_terminal`
//! snapshot, sends the keyboard to the shell and handles scrolling, selection and copy; plus the
//! workbench commands (new, kill, toggle) and the `terminal.integrated.*` settings the view reads.
//!
//! upstream: src/vs/workbench/contrib/terminal/browser/terminalInstance.ts @ 50e1350fc78d
//! upstream: src/vs/workbench/contrib/terminal/browser/terminalActions.ts @ 50e1350fc78d

use std::cell::Cell;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::prelude::FluentBuilder;
use gpui_kit::*;
use vsr_settings::Configuration;
use vsr_terminal::{
    CellFlags, Color, Key, Modifiers as TermMods, NamedColor, SelectionKind, Snapshot, Terminal, TerminalConfig, TerminalEvent,
    encode_key,
};

use crate::commands::{Handler, Scope};
use crate::config::AppConfig;
use crate::icons;
use crate::panel::PanelView;
use crate::theme::{Rgba8, Theme};
use crate::theme_service::ThemeService;
use crate::ui::to_gpui;
use crate::Workbench;

actions!(
    rscode,
    [NewTerminal, KillTerminal, ToggleTerminal, TerminalCopy, TerminalPaste, TerminalSelectAll, TerminalScrollToTop, TerminalScrollToBottom, TerminalScrollPageUp, TerminalScrollPageDown]
);

/// How often a terminal view looks for new output.
const POLL: Duration = Duration::from_millis(16);

/// The key context of a focused terminal.
pub const KEY_CONTEXT: &str = "Terminal";

/// Binds the terminal's keys. Ctrl+G is a workbench key (Go to Line) the shell wants (readline abort):
/// the terminal context unbinds it.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("ctrl-`", ToggleTerminal, None),
        KeyBinding::new("ctrl-shift-`", NewTerminal, None),
        KeyBinding::new("cmd-c", TerminalCopy, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-v", TerminalPaste, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-a", TerminalSelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-home", TerminalScrollToTop, Some(KEY_CONTEXT)),
        KeyBinding::new("cmd-end", TerminalScrollToBottom, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-pageup", TerminalScrollPageUp, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-pagedown", TerminalScrollPageDown, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-g", NoAction, Some(KEY_CONTEXT)),
    ]);
}

/// Attach the terminal's action handlers to the workbench root.
pub fn bind_actions(el: Div, cx: &mut Context<Workbench>) -> Div {
    el.on_action(cx.listener(|this, _: &NewTerminal, window, cx| this.new_terminal(window, cx)))
        .on_action(cx.listener(|this, _: &KillTerminal, _, cx| this.kill_terminal(cx)))
        .on_action(cx.listener(|this, _: &ToggleTerminal, window, cx| this.toggle_terminal(window, cx)))
}

/// The terminal commands (ids and titles of upstream's terminalActions.ts).
pub fn register_commands(r: &mut vsr_commands::CommandsRegistry<Handler>) {
    use vsr_commands::CommandDescriptor as D;
    let wb = |a: Box<dyn Action>| Handler::Action { action: a, scope: Scope::Workbench };
    r.register(D::new("workbench.action.terminal.toggleTerminal", "Toggle Terminal").category("View").f1(), wb(Box::new(ToggleTerminal)));
    r.register(D::new("workbench.action.terminal.new", "Create New Terminal").category("Terminal").f1(), wb(Box::new(NewTerminal)));
    r.register(D::new("workbench.action.terminal.kill", "Kill the Active Terminal Instance").category("Terminal").f1(), wb(Box::new(KillTerminal)));
}

// ---- settings --------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CursorStyle {
    #[default]
    Block,
    Line,
    Underline,
}

/// The `terminal.integrated.*` settings the terminal reads.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminalSettings {
    pub font_size: f32,
    /// `terminal.integrated.fontFamily`; empty means `editor.fontFamily`.
    pub font_family: String,
    pub line_height: f32,
    pub font_weight: f32,
    pub font_weight_bold: f32,
    pub cursor_style: CursorStyle,
    pub cursor_width: f32,
    pub scrollback: usize,
    pub copy_on_selection: bool,
    pub mac_option_is_meta: bool,
    pub draw_bold_text_in_bright_colors: bool,
    pub wheel_sensitivity: f32,
    pub fast_scroll_sensitivity: f32,
    pub word_separators: String,
    pub ignore_bracketed_paste_mode: bool,
    pub hide_on_last_closed: bool,
    pub cwd: String,
    pub default_profile: Option<String>,
}

/// `normal`, `bold` or a number from 1 to 1000 (as a string or a number).
fn font_weight(c: &Configuration, key: &str, default: f32) -> f32 {
    match c.get_value(key) {
        Some(serde_json::Value::String(s)) => match s.as_str() {
            "normal" => 400.0,
            "bold" => 700.0,
            n => n.parse::<f32>().ok().filter(|w| w.is_finite()).map_or(default, |w| w.clamp(1.0, 1000.0)),
        },
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|w| w as f32).filter(|w| w.is_finite()).map_or(default, |w| w.clamp(1.0, 1000.0)),
        _ => default,
    }
}

impl TerminalSettings {
    pub fn from_configuration(c: &Configuration) -> Self {
        let p = "terminal.integrated.";
        let key = |k: &str| format!("{p}{k}");
        Self {
            font_size: c.get_f64(&key("fontSize")).unwrap_or(14.0).clamp(6.0, 100.0) as f32,
            font_family: c.get_str(&key("fontFamily")).unwrap_or("").trim().to_string(),
            line_height: c.get_f64(&key("lineHeight")).unwrap_or(1.0).max(1.0) as f32,
            font_weight: font_weight(c, &key("fontWeight"), 400.0),
            font_weight_bold: font_weight(c, &key("fontWeightBold"), 700.0),
            cursor_style: c.get_enum(&key("cursorStyle")).unwrap_or_default(),
            cursor_width: c.get_f64(&key("cursorWidth")).unwrap_or(1.0).max(1.0) as f32,
            scrollback: c.get_f64(&key("scrollback")).unwrap_or(1000.0).max(0.0) as usize,
            copy_on_selection: c.get_bool(&key("copyOnSelection")).unwrap_or(false),
            mac_option_is_meta: c.get_bool(&key("macOptionIsMeta")).unwrap_or(false),
            draw_bold_text_in_bright_colors: c.get_bool(&key("drawBoldTextInBrightColors")).unwrap_or(true),
            wheel_sensitivity: c.get_f64(&key("mouseWheelScrollSensitivity")).unwrap_or(1.0).max(0.01) as f32,
            fast_scroll_sensitivity: c.get_f64(&key("fastScrollSensitivity")).unwrap_or(5.0).max(0.01) as f32,
            word_separators: c.get_str(&key("wordSeparators")).unwrap_or(" ()[]{}',\"`─‘’“”|").to_string(),
            ignore_bracketed_paste_mode: c.get_bool(&key("ignoreBracketedPasteMode")).unwrap_or(false),
            hide_on_last_closed: c.get_bool(&key("hideOnLastClosed")).unwrap_or(true),
            cwd: c.get_str(&key("cwd")).unwrap_or("").to_string(),
            // The registered enum lists the profiles upstream detects; a profile the user defines is not in it.
            default_profile: c.user().get(&key("defaultProfile.osx")).and_then(|v| v.as_str()).or_else(|| c.get_str(&key("defaultProfile.osx"))).map(str::to_string),
        }
    }
}

/// The shell to start and its arguments and environment: the default profile of
/// `terminal.integrated.profiles.osx` (`path`, `args`, `env`), else `$SHELL -l`; plus
/// `terminal.integrated.env.osx` on top. `None` for the shell means the default.
pub fn launch_config(c: &Configuration, root: &Path, scrollback: usize, word_separators: &str) -> TerminalConfig {
    let settings = TerminalSettings::from_configuration(c);
    let mut config = TerminalConfig { scrollback, word_separators: Some(word_separators.to_string()), ..TerminalConfig::default() };
    let mut env: Vec<(String, String)> = Vec::new();
    let profile = settings.default_profile.as_deref().and_then(|name| c.get_value("terminal.integrated.profiles.osx")?.get(name));
    if let Some(profile) = profile {
        let path = match profile.get("path") {
            Some(serde_json::Value::String(s)) => Some(s.clone()),
            Some(serde_json::Value::Array(a)) => a.iter().filter_map(|v| v.as_str()).find(|p| Path::new(p).exists()).map(str::to_string),
            _ => None,
        };
        config.shell = path.map(PathBuf::from);
        config.args = match profile.get("args") {
            Some(serde_json::Value::Array(a)) => Some(a.iter().filter_map(|v| v.as_str().map(str::to_string)).collect()),
            Some(serde_json::Value::String(s)) => Some(vec![s.clone()]),
            _ => None,
        };
        collect_env(profile.get("env"), &mut env);
    }
    collect_env(c.get_value("terminal.integrated.env.osx"), &mut env);
    config.env = env;
    config.cwd = Some(if settings.cwd.is_empty() { root.to_path_buf() } else { PathBuf::from(&settings.cwd) });
    config
}

/// `{ "NAME": "value" }`; a null value removes the variable (here: sets it empty).
fn collect_env(value: Option<&serde_json::Value>, out: &mut Vec<(String, String)>) {
    if let Some(serde_json::Value::Object(map)) = value {
        for (k, v) in map {
            out.push((k.clone(), v.as_str().unwrap_or("").to_string()));
        }
    }
}

// ---- colours -----------------------------------------------------------------------------------------

/// `(name, dark, light)` of the 16 `terminal.ansi*` colours (terminalColorRegistry.ts).
const ANSI_DEFAULTS: [(&str, Rgba8, Rgba8); 16] = {
    const fn c(hex: u32) -> Rgba8 {
        Rgba8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 0xFF)
    }
    [
        ("ansiBlack", c(0x000000), c(0x000000)),
        ("ansiRed", c(0xcd3131), c(0xcd3131)),
        ("ansiGreen", c(0x0DBC79), c(0x107C10)),
        ("ansiYellow", c(0xe5e510), c(0x949800)),
        ("ansiBlue", c(0x2472c8), c(0x0451a5)),
        ("ansiMagenta", c(0xbc3fbc), c(0xbc05bc)),
        ("ansiCyan", c(0x11a8cd), c(0x0598bc)),
        ("ansiWhite", c(0xe5e5e5), c(0x555555)),
        ("ansiBrightBlack", c(0x666666), c(0x666666)),
        ("ansiBrightRed", c(0xf14c4c), c(0xf14c4c)),
        ("ansiBrightGreen", c(0x23d18b), c(0x14CE14)),
        ("ansiBrightYellow", c(0xf5f543), c(0xb5ba00)),
        ("ansiBrightBlue", c(0x3b8eea), c(0x3b8eea)),
        ("ansiBrightMagenta", c(0xd670d6), c(0xd670d6)),
        ("ansiBrightCyan", c(0x29b8db), c(0x29b8db)),
        ("ansiBrightWhite", c(0xe5e5e5), c(0xa5a5a5)),
    ]
};

/// The colours of one terminal, from the theme's `terminal.*` colours.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub fg: Rgba8,
    pub bg: Rgba8,
    pub cursor: Rgba8,
    pub selection: Rgba8,
    pub inactive_selection: Rgba8,
    pub ansi: [Rgba8; 16],
}

impl Palette {
    pub fn new(theme: &Theme, color_theme: Option<&vsr_theme::ColorTheme>) -> Self {
        let luma = |c: Rgba8| 0.299 * c.0 as f32 + 0.587 * c.1 as f32 + 0.114 * c.2 as f32;
        let dark = luma(theme.editor_bg) < 128.0;
        let set = |c: Rgba8, fallback: Rgba8| if c.3 > 0 { c } else { fallback };
        let fg = set(theme.terminal_fg, if dark { Rgba8::rgb(0xCC, 0xCC, 0xCC) } else { Rgba8::rgb(0x33, 0x33, 0x33) });
        let mut ansi = [Rgba8::rgb(0, 0, 0); 16];
        for (i, (name, d, l)) in ANSI_DEFAULTS.iter().enumerate() {
            let themed = color_theme.and_then(|t| t.color(&format!("terminal.{name}"))).map(|c| c.to_rgba8()).filter(|c| c.3 > 0);
            ansi[i] = themed.unwrap_or(if dark { *d } else { *l });
        }
        Self {
            fg,
            bg: theme.terminal_bg,
            cursor: set(theme.terminal_cursor_fg, fg),
            selection: theme.terminal_selection_bg,
            inactive_selection: set(theme.terminal_inactive_selection_bg, theme.terminal_selection_bg),
            ansi,
        }
    }

    /// The colour of a cell colour; `None` for the default background (the view paints it).
    pub fn resolve(&self, color: Color, bold: bool, bright_bold: bool) -> Option<Rgba8> {
        match color {
            Color::Spec(rgb) => Some(Rgba8::rgb(rgb.r, rgb.g, rgb.b)),
            Color::Indexed(i) => Some(self.indexed(i as usize)),
            Color::Named(n) => {
                let i = n as usize;
                match n {
                    NamedColor::Foreground | NamedColor::BrightForeground => Some(self.fg),
                    NamedColor::Background => None,
                    NamedColor::Cursor => Some(self.cursor),
                    NamedColor::DimForeground => Some(dim(self.fg)),
                    _ if i < 8 => Some(self.ansi[if bold && bright_bold { i + 8 } else { i }]),
                    _ if i < 16 => Some(self.ansi[i]),
                    _ if (259..=266).contains(&i) => Some(dim(self.ansi[i - 259])),
                    _ => Some(self.fg),
                }
            }
        }
    }

    fn indexed(&self, i: usize) -> Rgba8 {
        match i {
            0..=15 => self.ansi[i],
            16..=231 => {
                let n = i - 16;
                let level = |v: usize| if v == 0 { 0 } else { (55 + v * 40) as u8 };
                Rgba8::rgb(level(n / 36), level(n / 6 % 6), level(n % 6))
            }
            _ => {
                let g = (8 + (i.min(255) - 232) * 10) as u8;
                Rgba8::rgb(g, g, g)
            }
        }
    }
}

fn dim(c: Rgba8) -> Rgba8 {
    Rgba8(c.0, c.1, c.2, (c.3 as f32 * 0.66) as u8)
}

// ---- the view ----------------------------------------------------------------------------------------

/// What a terminal view tells the workbench.
pub enum TerminalViewEvent {
    /// The shell ended: `Some(code)` (non-zero keeps the view with a message), `None` when killed.
    Exited(Option<u32>),
    TitleChanged,
}

impl EventEmitter<TerminalViewEvent> for TerminalView {}

pub struct TerminalView {
    terminal: Option<Arc<Terminal>>,
    /// Why the shell did not start.
    error: Option<String>,
    focus: FocusHandle,
    title: Option<String>,
    shell_name: String,
    exit: Option<Option<u32>>,
    /// The grid area as painted last, to map the mouse to cells and to size the PTY.
    bounds: Rc<Cell<Bounds<Pixels>>>,
    cell: Rc<Cell<(f32, f32)>>,
    selecting: bool,
    /// The font family the last frame used (tests).
    #[cfg(test)]
    pub used_family: std::cell::RefCell<String>,
    /// The poll task's inbox: gone once the task ended (tests).
    #[cfg(test)]
    pub inbox_probe: std::sync::Weak<Mutex<Vec<TerminalEvent>>>,
}

impl TerminalView {
    pub fn new(root: &Path, cx: &mut Context<Self>) -> Self {
        let config = AppConfig::get(cx).configuration();
        let settings = TerminalSettings::from_configuration(config);
        let launch = launch_config(config, root, settings.scrollback, &settings.word_separators);
        let shell = vsr_terminal::default_shell(launch.shell.as_ref());
        let shell_name = shell.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let inbox: Arc<Mutex<Vec<TerminalEvent>>> = Arc::default();
        #[cfg(test)]
        let inbox_probe = Arc::downgrade(&inbox);
        let outbox = inbox.clone();
        let mut view = Self {
            terminal: None,
            error: None,
            focus: cx.focus_handle(),
            title: None,
            shell_name,
            exit: None,
            bounds: Rc::new(Cell::new(Bounds::default())),
            cell: Rc::new(Cell::new((0.0, 0.0))),
            selecting: false,
            #[cfg(test)]
            used_family: Default::default(),
            #[cfg(test)]
            inbox_probe,
        };
        match Terminal::spawn(&launch, move |e| {
            if let Ok(mut q) = outbox.lock() {
                q.push(e);
            }
        }) {
            Ok(t) => view.terminal = Some(Arc::new(t)),
            Err(e) => view.error = Some(e),
        }
        // The shell's threads cannot wake a gpui task (the test scheduler refuses it), so the view polls
        // the inbox on a short timer: 16 ms, a frame.
        // No shell, no events: nothing to poll for.
        if view.terminal.is_some() {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(POLL).await;
                    let events = std::mem::take(&mut *inbox.lock().unwrap_or_else(|e| e.into_inner()));
                    let exited = events.iter().any(|e| matches!(e, TerminalEvent::Exit(_)));
                    // Every tick checks that the view is still alive, so the timer stops with it.
                    let alive = this
                        .update(cx, |this, cx| {
                            if events.is_empty() {
                                return;
                            }
                            for e in events {
                                this.on_event(e, cx);
                            }
                            cx.notify();
                        })
                        .is_ok();
                    if !alive || exited {
                        break;
                    }
                }
            })
            .detach();
        }
        view
    }

    fn on_event(&mut self, event: TerminalEvent, cx: &mut Context<Self>) {
        match event {
            TerminalEvent::Title(t) => {
                self.title = t;
                cx.emit(TerminalViewEvent::TitleChanged);
            }
            TerminalEvent::Exit(code) => {
                self.exit = Some(code);
                cx.emit(TerminalViewEvent::Exited(code));
            }
            TerminalEvent::Output | TerminalEvent::Bell => {}
        }
    }

    pub fn title(&self) -> String {
        self.title.clone().filter(|t| !t.is_empty()).unwrap_or_else(|| self.shell_name.clone())
    }

    pub fn focus_handle(&self) -> FocusHandle {
        self.focus.clone()
    }

    #[cfg(test)]
    pub fn is_selecting(&self) -> bool {
        self.selecting
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        self.focus.focus(window, cx);
    }

    /// One cell's width and height in pixels as painted last.
    #[cfg(test)]
    pub fn cell_size(&self) -> (f32, f32) {
        self.cell.get()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.terminal.as_ref().map(|t| t.snapshot())
    }

    pub fn kill(&self) {
        if let Some(t) = &self.terminal {
            t.kill();
        }
    }

    fn copy(&self, cx: &mut App) {
        if let Some(text) = self.terminal.as_ref().and_then(|t| t.selection_text()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }

    fn paste(&self, cx: &mut App) {
        let settings = TerminalSettings::from_configuration(AppConfig::get(cx).configuration());
        if let (Some(t), Some(item)) = (&self.terminal, cx.read_from_clipboard())
            && let Some(text) = item.text()
        {
            t.scroll_to_bottom();
            t.paste(&text, !settings.ignore_bracketed_paste_mode);
        }
    }

    /// The (row, col) under a window position.
    fn cell_at(&self, p: Point<Pixels>) -> (usize, usize) {
        let b = self.bounds.get();
        let (cw, lh) = self.cell.get();
        let col = ((f32::from(p.x) - f32::from(b.origin.x)) / cw.max(1.0)).floor().max(0.0) as usize;
        let row = ((f32::from(p.y) - f32::from(b.origin.y)) / lh.max(1.0)).floor().max(0.0) as usize;
        // A drag past the grid selects up to its last cell, never outside it.
        let (cols, rows) = self.terminal.as_ref().map_or((1, 1), |t| {
            let s = t.size();
            (s.0.max(1), s.1.max(1))
        });
        (row.min(rows - 1), col.min(cols - 1))
    }

    /// The pointer moved: extends the selection while the left button is held, and notices a release it
    /// did not see.
    fn drag_selection(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        if ev.pressed_button != Some(MouseButton::Left) {
            self.selecting = false;
        } else if self.selecting {
            let (row, col) = self.cell_at(ev.position);
            if let Some(t) = &self.terminal {
                t.update_selection(row, col);
            }
            cx.notify();
        }
    }

    /// The left button was released (anywhere): ends the drag, and copies on selection when asked.
    fn finish_selection(&mut self, cx: &mut Context<Self>) {
        if self.selecting {
            self.selecting = false;
            let settings = TerminalSettings::from_configuration(AppConfig::get(cx).configuration());
            if settings.copy_on_selection {
                self.copy(cx);
            }
        }
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let Some(t) = &self.terminal else { return };
        let settings = TerminalSettings::from_configuration(AppConfig::get(cx).configuration());
        let ks = &ev.keystroke;
        if ks.modifiers.platform {
            return; // Cmd keys belong to the workbench
        }
        if self.exit.is_some() {
            return;
        }
        let mods = TermMods { shift: ks.modifiers.shift, alt: ks.modifiers.alt, ctrl: ks.modifiers.control };
        let key = match ks.key.as_str() {
            "enter" => Key::Enter,
            "tab" => Key::Tab,
            "backspace" => Key::Backspace,
            "escape" => Key::Escape,
            "up" => Key::Up,
            "down" => Key::Down,
            "left" => Key::Left,
            "right" => Key::Right,
            "home" => Key::Home,
            "end" => Key::End,
            "pageup" => Key::PageUp,
            "pagedown" => Key::PageDown,
            "delete" => Key::Delete,
            "insert" => Key::Insert,
            "space" => Key::Char(' '),
            k if k.len() > 1 && k.starts_with('f') && k[1..].parse::<u8>().is_ok() => Key::F(k[1..].parse().unwrap_or(0)),
            k if k.chars().count() == 1 => Key::Char(k.chars().next().unwrap_or(' ')),
            _ => return,
        };
        t.scroll_to_bottom();
        let text_key = matches!(key, Key::Char(_));
        if text_key && !mods.ctrl && !(mods.alt && settings.mac_option_is_meta) {
            // Typed text (Option composes characters unless it acts as Meta).
            if let Some(text) = ks.key_char.as_deref().filter(|s| !s.is_empty()) {
                t.write(text.as_bytes());
            }
            return;
        }
        let key = match key {
            Key::Char(c) if mods.shift && !mods.ctrl => Key::Char(c.to_ascii_uppercase()),
            k => k,
        };
        let app_cursor = t.snapshot().application_cursor;
        if let Some(bytes) = encode_key(&key, mods, app_cursor) {
            t.write(&bytes);
        }
        cx.stop_propagation();
    }
}

/// A run of cells with one style in a row.
struct Span {
    range: Range<usize>,
    style: HighlightStyle,
}

fn in_selection(sel: &Option<vsr_terminal::SelectionSpan>, row: usize, col: usize) -> bool {
    let Some((a, b, block)) = sel else { return false };
    if *block {
        row >= a.0.min(b.0) && row <= a.0.max(b.0) && col >= a.1.min(b.1) && col <= a.1.max(b.1)
    } else {
        (row, col) >= *a && (row, col) <= *b
    }
}

/// The styled text of one row: its string and the highlight runs.
pub(crate) fn row_runs(snap: &Snapshot, row: usize, palette: &Palette, settings: &TerminalSettings, focused: bool) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let mut text = String::new();
    let mut spans: Vec<Span> = Vec::new();
    for (col, cell) in snap.cells[row].iter().enumerate() {
        if cell.flags.contains(CellFlags::WIDE_CHAR_SPACER) {
            continue;
        }
        let bold = cell.flags.contains(CellFlags::BOLD);
        let inverse = cell.flags.contains(CellFlags::INVERSE);
        let mut fg = palette.resolve(cell.fg, bold, settings.draw_bold_text_in_bright_colors).unwrap_or(palette.fg);
        let mut bg = palette.resolve(cell.bg, false, false);
        if inverse {
            let (f, b) = (bg.unwrap_or(palette.bg), fg);
            fg = f;
            bg = Some(b);
        }
        if cell.flags.contains(CellFlags::HIDDEN) {
            fg = bg.unwrap_or(palette.bg);
        }
        if cell.flags.contains(CellFlags::DIM) {
            fg = dim(fg);
        }
        if in_selection(&snap.selection, row, col) {
            bg = Some(if focused { palette.selection } else { palette.inactive_selection });
        }
        let style = HighlightStyle {
            color: Some(to_gpui(fg)),
            background_color: bg.map(to_gpui),
            font_weight: Some(FontWeight(if bold { settings.font_weight_bold } else { settings.font_weight })),
            font_style: cell.flags.contains(CellFlags::ITALIC).then_some(FontStyle::Italic),
            underline: cell.flags.intersects(CellFlags::ALL_UNDERLINES).then(|| UnderlineStyle {
                thickness: px(1.),
                color: Some(to_gpui(fg)),
                wavy: cell.flags.contains(CellFlags::UNDERCURL),
            }),
            strikethrough: cell.flags.contains(CellFlags::STRIKEOUT).then(|| StrikethroughStyle { thickness: px(1.), color: Some(to_gpui(fg)) }),
            ..Default::default()
        };
        let start = text.len();
        text.push(if cell.ch == '\0' { ' ' } else { cell.ch });
        match spans.last_mut() {
            Some(last) if last.style == style => last.range.end = text.len(),
            _ => spans.push(Span { range: start..text.len(), style }),
        }
    }
    (text, spans.into_iter().map(|s| (s.range, s.style)).collect())
}

impl Render for TerminalView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = ThemeService::theme(cx);
        let palette = Palette::new(&theme, ThemeService::color_theme(cx));
        let app_config = AppConfig::get(cx);
        let settings = TerminalSettings::from_configuration(app_config.configuration());
        let editor_family = crate::config::settings::EditorSettings::from_configuration(app_config.configuration()).font_family;
        let family = if settings.font_family.is_empty() { editor_family } else { settings.font_family.clone() };
        let focused = self.focus.is_focused(window);

        // Cell metrics: the advance of one character as shaped, else estimated like the editor does.
        #[cfg(test)]
        self.used_family.replace(family.clone());
        let font = gpui_kit::font(family.clone());
        let size = settings.font_size;
        let ts = window.text_system();
        let measured = ts.advance(ts.resolve_font(&font), px(size), 'm').ok().map(|s| f32::from(s.width)).filter(|w| *w > 0.0);
        let cw = measured.unwrap_or(size * 0.602);
        let lh = (size * settings.line_height).round().max(size);
        self.cell.set((cw, lh));

        let mut grid = div()
            .id("terminal-grid")
            .debug_selector(|| "terminal-grid".to_string())
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(to_gpui(palette.bg))
            .text_color(to_gpui(palette.fg))
            .font_family(family)
            .text_size(px(size))
            .line_height(px(lh));

        if let Some(error) = &self.error {
            return grid.p(px(8.)).child(format!("The terminal process failed to launch: {error}")).into_any_element();
        }
        let Some(term) = self.terminal.clone() else { return grid.into_any_element() };

        // Size the PTY to the area as painted.
        let bounds = self.bounds.clone();
        let entity = cx.entity();
        let tracked = term.clone();
        let canvas_el = canvas(
            move |b, _, _| b,
            move |b, _, window, cx| {
                // A release outside the grid must end the selection too.
                // The drag and its release are tracked at the window, where the pointer may leave the grid.
                let view = entity.clone();
                window.on_mouse_event(move |ev: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture && ev.button == MouseButton::Left {
                        view.update(cx, |this, cx| this.finish_selection(cx));
                    }
                });
                let view = entity.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, _, cx| {
                    if phase == DispatchPhase::Capture {
                        let ev = ev.clone();
                        view.update(cx, |this, cx| this.drag_selection(&ev, cx));
                    }
                });
                bounds.set(b);
                let cols = (f32::from(b.size.width) / cw).floor().max(2.0) as usize;
                let rows = (f32::from(b.size.height) / lh).floor().max(1.0) as usize;
                let snap = tracked.snapshot();
                if (snap.cols, snap.rows) != (cols, rows) {
                    tracked.resize(cols as u16, rows as u16);
                    entity.update(cx, |_, cx| cx.notify());
                }
            },
        )
        .absolute()
        .size_full();

        let snap = term.snapshot();
        for row in 0..snap.rows {
            let (text, runs) = row_runs(&snap, row, &palette, &settings, focused);
            let line = div().absolute().top(px(row as f32 * lh)).left_0().h(px(lh)).whitespace_nowrap();
            grid = grid.child(if runs.is_empty() { line.child(text) } else { line.child(StyledText::new(text).with_highlights(runs)) });
        }
        if let Some(c) = snap.cursor {
            let (x, y) = (c.col as f32 * cw, c.row as f32 * lh);
            let colour = to_gpui(palette.cursor);
            let mut cursor = div().absolute().debug_selector(|| "terminal-cursor".to_string());
            use vsr_terminal::CursorShape as S;
            cursor = match (focused, settings.cursor_style, c.shape) {
                (false, _, _) | (_, _, S::HollowBlock) => cursor.left(px(x)).top(px(y)).w(px(cw)).h(px(lh)).border_1().border_color(colour),
                (_, CursorStyle::Line, _) | (_, _, S::Beam) => cursor.left(px(x)).top(px(y)).w(px(settings.cursor_width)).h(px(lh)).bg(colour),
                (_, CursorStyle::Underline, _) | (_, _, S::Underline) => {
                    cursor.left(px(x)).top(px(y + lh - settings.cursor_width)).w(px(cw)).h(px(settings.cursor_width)).bg(colour)
                }
                _ => cursor.left(px(x)).top(px(y)).w(px(cw)).h(px(lh)).bg(colour).opacity(0.6),
            };
            grid = grid.child(cursor);
        }
        if let Some(code) = self.exit.filter(|c| *c != Some(0)) {
            let msg = match code {
                Some(c) => format!("The terminal process \"{}\" terminated with exit code: {c}.", self.shell_name),
                None => format!("The terminal process \"{}\" was terminated.", self.shell_name),
            };
            grid = grid.child(div().absolute().bottom_0().left_0().w_full().px(px(8.)).bg(to_gpui(theme.panel_bg)).debug_selector(|| "terminal-exit-message".to_string()).child(msg));
        }

        let wheel_term = term.clone();
        grid.child(canvas_el)
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .on_action(cx.listener(|this, _: &TerminalCopy, _, cx| this.copy(cx)))
            .on_action(cx.listener(|this, _: &TerminalPaste, _, cx| this.paste(cx)))
            .on_action(cx.listener(|this, _: &TerminalSelectAll, _, cx| {
                if let Some(t) = &this.terminal {
                    t.select_all();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TerminalScrollToTop, _, cx| {
                if let Some(t) = &this.terminal {
                    t.scroll(i32::MAX / 2);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TerminalScrollToBottom, _, cx| {
                if let Some(t) = &this.terminal {
                    t.scroll_to_bottom();
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TerminalScrollPageUp, _, cx| {
                if let Some(t) = &this.terminal {
                    t.scroll(t.snapshot().rows as i32);
                }
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &TerminalScrollPageDown, _, cx| {
                if let Some(t) = &this.terminal {
                    t.scroll(-(t.snapshot().rows as i32));
                }
                cx.notify();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, ev: &MouseDownEvent, window, cx| {
                    this.focus.focus(window, cx);
                    let (row, col) = this.cell_at(ev.position);
                    if let Some(t) = &this.terminal {
                        let kind = match (ev.click_count, ev.modifiers.alt) {
                            (_, true) => SelectionKind::Block,
                            (2, _) => SelectionKind::Word,
                            (c, _) if c >= 3 => SelectionKind::Line,
                            _ => SelectionKind::Simple,
                        };
                        t.start_selection(row, col, kind);
                        this.selecting = true;
                    }
                    cx.notify();
                }),
            )
            .on_scroll(cx.listener(move |this, ev: &ScrollEvent, _, cx| {
                let settings = TerminalSettings::from_configuration(AppConfig::get(cx).configuration());
                let (_, lh) = this.cell.get();
                let mut lines = ev.delta.pixel_delta(px(lh.max(1.0))).y / px(lh.max(1.0));
                lines *= settings.wheel_sensitivity;
                if ev.modifiers.alt {
                    lines *= settings.fast_scroll_sensitivity;
                }
                let n = lines.round() as i32;
                if n == 0 {
                    return;
                }
                if wheel_term.alt_screen() {
                    // Full-screen programs (less, vim) get the arrow keys, as xterm does without mouse reporting.
                    let key = if n > 0 { Key::Up } else { Key::Down };
                    let app = wheel_term.snapshot().application_cursor;
                    for _ in 0..n.abs() {
                        if let Some(b) = encode_key(&key, TermMods::default(), app) {
                            wheel_term.write(&b);
                        }
                    }
                } else {
                    wheel_term.scroll(n);
                }
                cx.notify();
            }))
            .into_any_element()
    }
}

// ---- workbench commands ---------------------------------------------------------------------------------

/// A terminal in the panel and the subscription that watches it.
pub struct TerminalInstance {
    pub view: Entity<TerminalView>,
    _sub: Subscription,
}

impl Workbench {
    /// `workbench.action.terminal.new`: a terminal in the workspace folder, shown and focused.
    pub(crate) fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let view = cx.new(|cx| TerminalView::new(&root, cx));
        let sub = cx.subscribe(&view, |this, view, event: &TerminalViewEvent, cx| this.on_terminal_event(&view, event, cx));
        view.update(cx, |v, cx| v.focus(window, cx));
        self.panel.terminals.push(TerminalInstance { view, _sub: sub });
        self.panel.active_terminal = self.panel.terminals.len() - 1;
        let opens = self.panel_settings(cx).opens_maximized;
        self.panel.show(opens);
        self.panel.active = PanelView::Terminal;
        cx.notify();
    }

    /// `workbench.action.terminal.kill`: ends the active terminal and removes it.
    pub(crate) fn kill_terminal(&mut self, cx: &mut Context<Self>) {
        let i = self.panel.active_terminal;
        if i < self.panel.terminals.len() {
            self.remove_terminal(i, cx);
        }
    }

    pub(crate) fn remove_terminal(&mut self, i: usize, cx: &mut Context<Self>) {
        if i >= self.panel.terminals.len() {
            return;
        }
        let inst = self.panel.terminals.remove(i);
        inst.view.read(cx).kill();
        // The shown terminal stays the same one unless it is the removed one (then its successor, or the
        // new last one).
        let active = &mut self.panel.active_terminal;
        if i < *active {
            *active -= 1;
        }
        *active = (*active).min(self.panel.terminals.len().saturating_sub(1));
        // `panel_sync` moves the focus on at the next frame when the removed terminal had it.
        self.panel.refocus = Some(inst.view.read(cx).focus_handle());
        if self.panel.terminals.is_empty() {
            let settings = TerminalSettings::from_configuration(AppConfig::get(cx).configuration());
            if settings.hide_on_last_closed && self.panel.active == PanelView::Terminal {
                self.panel.hide();
            }
        }
        cx.notify();
    }

    /// Ctrl+`: shows the panel on the terminal (creating one when there is none), or hides it when it
    /// already shows the terminal.
    pub(crate) fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.panel.visible && self.panel.active == PanelView::Terminal {
            self.panel.hide();
            cx.notify();
        } else if self.panel.terminals.is_empty() {
            self.new_terminal(window, cx);
        } else {
            let opens = self.panel_settings(cx).opens_maximized;
            self.panel.show(opens);
            self.panel.active = PanelView::Terminal;
            if let Some(i) = self.panel.terminals.get(self.panel.active_terminal) {
                i.view.update(cx, |v, cx| v.focus(window, cx));
            }
            cx.notify();
        }
    }

    fn on_terminal_event(&mut self, view: &Entity<TerminalView>, event: &TerminalViewEvent, cx: &mut Context<Self>) {
        match event {
            TerminalViewEvent::TitleChanged => cx.notify(),
            // A clean exit closes the terminal (as upstream); a failure keeps it with its message.
            TerminalViewEvent::Exited(code) => {
                if *code == Some(0)
                    && let Some(i) = self.panel.terminals.iter().position(|t| &t.view == view)
                {
                    self.remove_terminal(i, cx);
                } else {
                    cx.notify();
                }
            }
        }
    }

    /// The terminal body of the panel: the active terminal and, with several, the list of them.
    pub(crate) fn terminal_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme.clone();
        let Some(active) = self.panel.terminals.get(self.panel.active_terminal) else {
            return div().size_full().px(px(20.)).py(px(8.)).text_color(to_gpui(t.description_fg)).child("No terminal is open.").into_any_element();
        };
        let list = (self.panel.terminals.len() > 1).then(|| {
            let items: Vec<_> = self
                .panel
                .terminals
                .iter()
                .enumerate()
                .map(|(i, inst)| {
                    let selected = i == self.panel.active_terminal;
                    div()
                        .id(SharedString::from(format!("terminal-tab-{i}")))
                        .debug_selector(move || format!("terminal-tab:{i}"))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .h(px(22.))
                        .px(px(8.))
                        .cursor_pointer()
                        .text_size(px(12.))
                        .when(selected, |d| d.bg(to_gpui(t.list_active_bg)))
                        .hover(|s| s.bg(to_gpui(t.list_hover_bg)))
                        .child(div().font_family(icons::FAMILY).child(icons::glyph(icons::TERMINAL)))
                        .child(div().flex_1().min_w_0().overflow_hidden().child(inst.view.read(cx).title()))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                            this.panel.active_terminal = i;
                            if let Some(inst) = this.panel.terminals.get(i) {
                                inst.view.update(cx, |v, cx| v.focus(window, cx));
                            }
                            cx.notify();
                        }))
                })
                .collect();
            div()
                .flex_none()
                .w(px(160.))
                .h_full()
                .border_l_1()
                .border_color(to_gpui(t.panel_border))
                .debug_selector(|| "terminal-tabs".to_string())
                .children(items)
        });
        div()
            .size_full()
            .flex()
            .child(div().flex_1().min_w_0().h_full().p_0().child(active.view.clone()))
            .children(list)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use vsr_settings::Configuration;
    use vsr_terminal::{Color, NamedColor};

    use super::{Palette, TerminalSettings, launch_config};
    use crate::theme::{Rgba8, Theme};

    fn config(json: &str) -> Configuration {
        Configuration::new(&vsr_settings::SettingsRegistry::load_default(), vsr_settings::UserSettings::parse(json).unwrap())
    }

    #[test]
    fn indexed_colours_follow_the_xterm_cube() {
        let p = Palette::new(&Theme::dark_modern(), None);
        assert_eq!(p.indexed(16), Rgba8::rgb(0, 0, 0));
        assert_eq!(p.indexed(196), Rgba8::rgb(255, 0, 0));
        assert_eq!(p.indexed(231), Rgba8::rgb(255, 255, 255));
        assert_eq!(p.indexed(232), Rgba8::rgb(8, 8, 8));
        assert_eq!(p.indexed(255), Rgba8::rgb(238, 238, 238));
    }

    #[test]
    fn bold_ansi_foregrounds_use_the_bright_colour_when_asked() {
        let p = Palette::new(&Theme::dark_modern(), None);
        let red = Color::Named(NamedColor::Red);
        assert_eq!(p.resolve(red, false, true), Some(p.ansi[1]));
        assert_eq!(p.resolve(red, true, true), Some(p.ansi[9]));
        assert_eq!(p.resolve(red, true, false), Some(p.ansi[1]));
        assert_eq!(p.resolve(Color::Named(NamedColor::Background), false, false), None);
    }

    #[test]
    fn the_palette_uses_the_dark_defaults_on_a_dark_theme() {
        let p = Palette::new(&Theme::dark_modern(), None);
        assert_eq!(p.ansi[2], Rgba8::rgb(0x0D, 0xBC, 0x79));
        assert_eq!(p.fg, Rgba8::rgb(0xCC, 0xCC, 0xCC));
    }

    #[test]
    fn a_profile_chooses_the_shell_args_and_env() {
        let c = config(
            r#"{ "terminal.integrated.defaultProfile.osx": "mine",
                 "terminal.integrated.profiles.osx": { "mine": { "path": "/bin/sh", "args": ["-c", "true"], "env": { "A": "1" } } },
                 "terminal.integrated.env.osx": { "B": "2" } }"#,
        );
        let l = launch_config(&c, Path::new("/tmp"), 1000, " ");
        assert_eq!(l.shell, Some(PathBuf::from("/bin/sh")));
        assert_eq!(l.args, Some(vec!["-c".to_string(), "true".to_string()]));
        assert_eq!(l.env, vec![("A".to_string(), "1".to_string()), ("B".to_string(), "2".to_string())]);
        assert_eq!(l.cwd, Some(PathBuf::from("/tmp")));
    }

    #[test]
    fn font_weights_accept_names_and_numbers() {
        let c = config(r#"{ "terminal.integrated.fontWeight": "300", "terminal.integrated.fontWeightBold": 900 }"#);
        let s = TerminalSettings::from_configuration(&c);
        assert_eq!((s.font_weight, s.font_weight_bold), (300.0, 900.0));
    }

    #[test]
    fn non_finite_font_weights_are_rejected() {
        let c = config(r#"{ "terminal.integrated.fontWeight": "nan", "terminal.integrated.fontWeightBold": "inf" }"#);
        let s = TerminalSettings::from_configuration(&c);
        assert_eq!((s.font_weight, s.font_weight_bold), (400.0, 700.0));
    }
}
