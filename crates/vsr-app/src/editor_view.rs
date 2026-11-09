//! Editor surface: a focusable, editable, virtualised view over a `vsr-editor` `Editor`.
//!
//! upstream: src/vs/editor/browser/viewParts/viewCursors/viewCursors.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/viewCursors/viewCursor.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/lineNumbers/lineNumbers.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/currentLineHighlight/currentLineHighlight.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/selections/selections.ts @ 50e1350fc78d
//!
//! This module owns everything about drawing and driving a document: gutter, rows, caret,
//! selection, scroll state, mouse, key actions and the platform text-input handler (typing, IME,
//! dead keys). The model (`OpenEditor` in `model.rs`) stays UI free; key bindings come from
//! `keymap.rs` and column/pixel math from `text_layout.rs`.
//!
//! The view is a plain struct owned by the workbench (not a gpui entity) and reaches the
//! documents through the `EditorHost` trait, so the workbench keeps one source of truth for tabs.
//!
//! Known costs (10k-line file): per keystroke we rescan line lengths for the widest line
//! (`O(lines)`, ~0.3 ms) and `offset_at` for IME queries is `O(lines)`. Rows are virtualised
//! (`uniform_list`), only visible rows are shaped, and text shaping is cached by gpui.
//! Known gaps: multi-cursor, word wrap, syntax colors, horizontal reveal of the caret,
//! drag auto-scroll, IME candidate-window placement precision.

mod decorations;
mod find;

use std::cell::Cell;
use std::collections::HashMap;
use std::ops::Range;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gpui_kit::{
    AnyElement, App, Bounds, ClipboardItem, Context, Entity, FocusHandle, InputHandler,
    IntoElement, KeyBinding, KeyContext, ListHorizontalSizingBehavior, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, ScrollStrategy,
    SharedString, Styled, Task, TextRun, UTF16Selection, UniformListScrollHandle, Window, canvas,
    div, font, point, px, uniform_list,
};
use gpui_kit::InteractiveElement;
use vsr_editor::Position;

use crate::config::settings::{
    CursorBlinking, CursorStyle, EditorSettings, LineHighlight, LineNumbers, OccurrencesHighlight,
};
use crate::keymap::{self, BINDINGS, Command};
use crate::model::{Editors, OpenEditor, SaveTransforms};
use crate::text_layout::{column_for_x, expand_tabs, expanded_byte_for_column, visual_column};
use crate::theme::Theme;
use crate::ui::to_gpui;

/// Half period of the `blink` cursor style.
const BLINK: Duration = Duration::from_millis(500);
/// Key context the bindings are scoped to.
const KEY_CONTEXT: &str = "Editor";

/// The one gpui action: run an editor command. Bindings are generated from `keymap::BINDINGS`.
#[derive(Clone, PartialEq, Debug, gpui_kit::Action)]
#[action(no_json, no_register)]
pub struct Run(pub Command);

/// Register the editor key bindings (call once at startup).
pub fn init(cx: &mut App) {
    cx.bind_keys(BINDINGS.iter().map(|&(keys, cmd)| KeyBinding::new(keys, Run(cmd), Some(KEY_CONTEXT))));
    find::bind(cx); // Find widget keys (ADR-0027)
}

/// What the view needs from its owner: the documents and the view state.
pub trait EditorHost: 'static {
    fn editor_host(&mut self) -> (&mut Editors, &mut EditorView);
}

#[derive(Default)]
pub struct EditorView {
    focus: Option<FocusHandle>,
    /// Per-document scroll state, so switching tabs keeps your place.
    scrolls: HashMap<PathBuf, UniformListScrollHandle>,
    /// Per-document (editor version, index of widest line), for horizontal scrolling.
    widest: HashMap<PathBuf, (u64, usize)>,
    /// List viewport in window coordinates, recorded at paint (for mouse hit-testing).
    bounds: Rc<Cell<Bounds<Pixels>>>,
    gutter: f32,
    /// Instant of the last caret movement/edit: the caret is solid, then blinks.
    activity: Option<Instant>,
    blink_task: Option<Task<()>>,
    /// Active IME composition, as a UTF-16 document range.
    marked: Option<Range<usize>>,
    dragging: bool,
    /// The path the editor was last auto-focused for (focus follows tab changes).
    focused_for: Option<PathBuf>,
    /// The editor settings of the last `element` call (fonts, cursor, line numbers, ...).
    cfg: EditorSettings,
    /// The Find widget (Cmd+F): state, model and text boxes.
    find: find::FindUi,
    /// The bracket pair highlighted at the caret (`editor.matchBrackets`).
    brackets: decorations::BracketCache,
}

impl EditorView {
    pub fn forget(&mut self, path: &PathBuf) {
        self.scrolls.remove(path);
        self.widest.remove(path);
        if self.focused_for.as_ref() == Some(path) {
            self.focused_for = None;
        }
        self.marked = None;
    }

    /// Bring the caret of `e` into view (centred) and make the editor take focus on the next
    /// frame. Used when something else (Search) opens a file at a location.
    pub fn reveal(&mut self, e: &OpenEditor) {
        self.focused_for = None;
        self.activity = Some(Instant::now());
        let line = e.editor.selection().active.line;
        let scroll = self.scrolls.entry(e.path.clone()).or_default();
        scroll.scroll_to_item(line.saturating_sub(1), ScrollStrategy::Center);
    }

    fn focus_handle(&mut self, cx: &mut App) -> FocusHandle {
        self.focus.get_or_insert_with(|| cx.focus_handle()).clone()
    }

    fn caret_visible(&self) -> bool {
        // `smooth`, `phase` and `expand` blink like `blink` for now.
        if self.cfg.cursor_blinking == CursorBlinking::Solid {
            return true;
        }
        let since = self.activity.map_or(Duration::ZERO, |t| t.elapsed());
        (since.as_millis() / BLINK.as_millis()).is_multiple_of(2)
    }

    /// Caret moved or text changed: show the caret solid and bring it into view.
    fn after_change(&mut self, e: &OpenEditor, edited: bool, window: &mut Window) {
        self.activity = Some(Instant::now());
        let line = e.editor.selection().active.line;
        if let Some(s) = self.scrolls.get(&e.path) {
            s.scroll_to_item(line - 1, ScrollStrategy::Nearest);
        }
        if let Some(f) = &self.focus {
            if edited {
                window.notify_text_input_changed(f);
            }
            window.notify_text_selection_changed(f);
        }
    }

    fn rows_per_page(&self) -> usize {
        let h = f32::from(self.bounds.get().size.height);
        ((h / self.cfg.line_height) as usize).max(1)
    }

    /// Build the editor element for `editor`.
    pub fn element<T: EditorHost>(
        &mut self,
        editor: &OpenEditor,
        theme: &Theme,
        settings: &EditorSettings,
        cx: &mut Context<T>,
    ) -> impl IntoElement {
        self.cfg = settings.clone();
        self.find.sync(editor, settings.find);
        let find_widget = self.find.element(theme, cx);
        let cfg = settings.clone();
        let focus = self.focus_handle(cx);
        let lines = editor.buffer().line_count();
        let scroll = self.scrolls.entry(editor.path.clone()).or_default().clone();
        let version = editor.editor.version();
        let widest = match self.widest.get(&editor.path) {
            Some(&(v, i)) if v == version => i,
            _ => {
                let i = widest_line(editor);
                self.widest.insert(editor.path.clone(), (version, i));
                i
            }
        };
        let gutter = gutter_width(lines, &cfg);
        self.gutter = gutter;
        if self.blink_task.is_none() {
            self.blink_task = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(BLINK).await;
                    if this.update(cx, |_, cx| cx.notify()).is_err() {
                        break;
                    }
                }
            }));
        }
        let theme = theme.clone();
        let id: SharedString = editor.path.display().to_string().into();
        let list_focus = focus.clone();
        let list = uniform_list(
            id,
            lines,
            cx.processor(move |this: &mut T, range: Range<usize>, window: &mut Window, cx| {
                let (editors, view) = this.editor_host();
                let Some(e) = editors.active_editor() else { return Vec::new() };
                if view.focused_for.as_ref() != Some(&e.path) {
                    view.focused_for = Some(e.path.clone());
                    list_focus.focus(window, cx);
                }
                let focused = list_focus.is_focused(window);
                let find = view.find.highlights(e);
                let pair = view.brackets.get(&e.path, e.buffer(), e.editor.version(), e.editor.selection().active, cfg.match_brackets);
                rows(e, &theme, &cfg, gutter, range, window, focused && view.caret_visible(), focused, find.as_ref(), pair)
            }),
        )
        .track_scroll(&scroll)
        .with_horizontal_sizing_behavior(ListHorizontalSizingBehavior::Unconstrained)
        .with_width_from_item(Some(widest))
        .size_full();

        let bounds_cell = self.bounds.clone();
        let input_focus = focus.clone();
        let entity = cx.entity();
        let input_surface = canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, cx| {
                bounds_cell.set(bounds);
                window.handle_input(
                    &input_focus,
                    EditorInput { view: entity, bounds },
                    cx,
                );
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();

        let surface = div()
            .id("editor-surface")
            .key_context(KeyContext::parse(KEY_CONTEXT).unwrap_or_default())
            .track_focus(&focus)
            .relative()
            .size_full()
            .on_action(cx.listener(|this: &mut T, a: &Run, window, cx| {
                run_command(this, a.0, window, cx)
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this: &mut T, ev: &MouseDownEvent, window, cx| {
                    mouse_down(this, ev, window, cx)
                }),
            )
            .on_mouse_move(cx.listener(|this: &mut T, ev: &MouseMoveEvent, window, cx| {
                mouse_move(this, ev, window, cx)
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this: &mut T, _: &MouseUpEvent, _, _| {
                    this.editor_host().1.dragging = false;
                }),
            )
            .child(list)
            .child(input_surface);
        // The editor and the Find widget side by side, so the find keys work from both.
        find::register_actions(
            div()
                .id("editor-find-host")
                .key_context(KeyContext::parse(find::KEY_CONTEXT).unwrap_or_default())
                .relative()
                .size_full(),
            cx,
        )
        .child(surface)
        .children(find_widget)
    }
}

// ---- commands ------------------------------------------------------------------------------

fn run_command<T: EditorHost>(this: &mut T, cmd: Command, window: &mut Window, cx: &mut Context<T>) {
    let (editors, view) = this.editor_host();
    let Some(i) = editors.active else { return };
    match cmd {
        Command::CloseTab => {
            if let Some(path) = editors.open.get(i).map(|e| e.path.clone()) {
                editors.close(i);
                view.forget(&path);
            }
        }
        _ => {
            let Some(e) = editors.open.get_mut(i) else { return };
            let page = view.rows_per_page();
            match cmd {
                Command::Save => {
                    let transforms = SaveTransforms {
                        trim_trailing_whitespace: view.cfg.trim_trailing_whitespace,
                        insert_final_newline: view.cfg.insert_final_newline,
                    };
                    if let Err(err) = e.save_with(transforms) {
                        eprintln!("rscode: save {}: {err}", e.path.display());
                    }
                }
                Command::Copy => {
                    let text = e.editor.selected_text();
                    let text = if text.is_empty() {
                        format!("{}\n", e.buffer().line_content(e.editor.selection().active.line))
                    } else {
                        text
                    };
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                Command::Cut => {
                    let text = e.editor.cut();
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                }
                Command::Paste => {
                    if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                        e.editor.paste(&text);
                    }
                }
                _ => {
                    keymap::apply(&mut e.editor, cmd, page);
                }
            }
            view.marked = None;
            view.after_change(e, keymap::is_edit(cmd), window);
        }
    }
    cx.notify();
}

// ---- mouse ---------------------------------------------------------------------------------

fn shape(window: &Window, cfg: &EditorSettings, text: &str) -> Option<gpui_kit::ShapedLine> {
    if text.is_empty() {
        return None;
    }
    let run = TextRun {
        len: text.len(),
        font: editor_font(cfg),
        color: gpui_kit::white(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    Some(window.text_system().shape_line(
        SharedString::from(text.to_string()),
        px(cfg.font_size),
        &[run],
        None,
    ))
}

fn editor_font(cfg: &EditorSettings) -> gpui_kit::Font {
    let mut f = font(SharedString::from(cfg.font_family.clone()));
    f.weight = gpui_kit::FontWeight(cfg.font_weight);
    f.features = font_features(cfg);
    f
}

/// `editor.fontLigatures` as gpui font features (shaping and painting use the same ones).
fn font_features(cfg: &EditorSettings) -> gpui_kit::FontFeatures {
    gpui_kit::FontFeatures(std::sync::Arc::new(cfg.font_features.clone()))
}

/// Document position under window point `p`.
fn hit_test(view: &EditorView, e: &OpenEditor, p: Point<Pixels>, window: &Window) -> Position {
    let bounds = view.bounds.get();
    let (offset_x, offset_y) = view.scrolls.get(&e.path).map_or((0.0, 0.0), |s| {
        let o = s.0.borrow().base_handle.offset();
        (f32::from(o.x), f32::from(o.y))
    });
    let buf = e.buffer();
    let y = f32::from(p.y - bounds.origin.y) - offset_y;
    let line = ((y / view.cfg.line_height).floor().max(0.0) as usize + 1).min(buf.line_count());
    let x = f32::from(p.x - bounds.origin.x) - view.gutter - offset_x;
    let tab = e.editor.options().tab_size;
    let text = buf.line_content(line);
    let expanded = expand_tabs(text, tab);
    let shaped = shape(window, &view.cfg, &expanded);
    let column = column_for_x(text, tab, x, |byte| {
        shaped.as_ref().map_or(0.0, |s| f32::from(s.x_for_index(byte)))
    });
    Position::new(line, column)
}

fn mouse_down<T: EditorHost>(this: &mut T, ev: &MouseDownEvent, window: &mut Window, cx: &mut Context<T>) {
    let (editors, view) = this.editor_host();
    let Some(e) = editors.active.and_then(|i| editors.open.get_mut(i)) else { return };
    if let Some(f) = &view.focus {
        f.focus(window, cx);
    }
    let pos = hit_test(view, e, ev.position, window);
    match ev.click_count {
        0 | 1 => e.editor.set_cursor(pos, ev.modifiers.shift),
        2 => {
            // Double click: select the word under the pointer.
            e.editor.set_cursor(pos, false);
            e.editor.move_word_left(false);
            e.editor.move_word_right(true);
        }
        _ => {
            // Triple click: select the whole line (including its break).
            let line = pos.line;
            e.editor.set_cursor(Position::new(line, 1), false);
            e.editor.set_cursor(Position::new(line + 1, 1), true);
        }
    }
    view.dragging = true;
    view.marked = None;
    view.after_change(e, false, window);
    cx.notify();
}

fn mouse_move<T: EditorHost>(this: &mut T, ev: &MouseMoveEvent, window: &mut Window, cx: &mut Context<T>) {
    let (editors, view) = this.editor_host();
    if !view.dragging {
        return;
    }
    if ev.pressed_button != Some(MouseButton::Left) {
        view.dragging = false;
        return;
    }
    let Some(e) = editors.active.and_then(|i| editors.open.get_mut(i)) else { return };
    let pos = hit_test(view, e, ev.position, window);
    if pos != e.editor.selection().active {
        e.editor.set_cursor(pos, true);
        view.after_change(e, false, window);
        cx.notify();
    }
}

// ---- platform text input (typing, IME, dead keys) -------------------------------------------

struct EditorInput<T: EditorHost> {
    view: Entity<T>,
    bounds: Bounds<Pixels>,
}

impl<T: EditorHost> EditorInput<T> {
    /// Run `f` on the active document and the view state.
    fn with<R>(
        &self,
        cx: &mut App,
        f: impl FnOnce(&mut OpenEditor, &mut EditorView) -> R,
    ) -> Option<R> {
        self.view.update(cx, |this, _| {
            let (editors, view) = this.editor_host();
            let e = editors.active.and_then(|i| editors.open.get_mut(i))?;
            Some(f(e, view))
        })
    }

    /// Replace `range` (UTF-16 document offsets) with `text`; leaves the caret after it.
    fn replace(
        &self,
        range: Option<Range<usize>>,
        text: &str,
        mark: bool,
        selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        let changed = self.with(cx, |e, view| {
            let buf = e.buffer();
            let range = range
                .or_else(|| view.marked.clone())
                .unwrap_or_else(|| selection_offsets(e).0);
            let start = buf.position_at(range.start);
            let end = buf.position_at(range.end);
            e.editor.set_cursor(start, false);
            e.editor.set_cursor(end, true);
            e.editor.type_text(text);
            let len = text.encode_utf16().count();
            view.marked = (mark && len > 0).then(|| range.start..range.start + len);
            if let Some(sel) = selected.filter(|_| mark) {
                let b = e.buffer();
                let (a, z) = (b.position_at(range.start + sel.start), b.position_at(range.start + sel.end));
                e.editor.set_cursor(a, false);
                e.editor.set_cursor(z, true);
            }
            view.after_change(e, true, window);
        });
        if changed.is_some() {
            self.view.update(cx, |_, cx| cx.notify());
        }
    }
}

/// The primary selection as a UTF-16 document range, plus whether the caret is at its start.
fn selection_offsets(e: &OpenEditor) -> (Range<usize>, bool) {
    let sel = e.editor.selection();
    let buf = e.buffer();
    let (a, b) = (buf.offset_at(sel.anchor), buf.offset_at(sel.active));
    (a.min(b)..a.max(b), sel.is_reversed())
}

impl<T: EditorHost> InputHandler for EditorInput<T> {
    fn selected_text_range(
        &mut self,
        _ignore_disabled_input: bool,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<UTF16Selection> {
        self.with(cx, |e, _| {
            let (range, reversed) = selection_offsets(e);
            UTF16Selection { range, reversed }
        })
    }

    fn marked_text_range(&mut self, _window: &mut Window, cx: &mut App) -> Option<Range<usize>> {
        self.with(cx, |_, view| view.marked.clone()).flatten()
    }

    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        adjusted_range: &mut Option<Range<usize>>,
        _window: &mut Window,
        cx: &mut App,
    ) -> Option<String> {
        let (text, adjusted) = self.with(cx, |e, _| {
            let buf = e.buffer();
            let len = buf.len_utf16();
            let (s, z) = (range_utf16.start.min(len), range_utf16.end.min(len));
            let r = vsr_editor::Range::new(buf.position_at(s), buf.position_at(z));
            (buf.value_in_range(r), s..z)
        })?;
        *adjusted_range = Some(adjusted);
        Some(text)
    }

    fn replace_text_in_range(
        &mut self,
        replacement_range: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.replace(replacement_range, text, false, None, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut App,
    ) {
        self.replace(range_utf16, new_text, true, new_selected_range, window, cx);
    }

    fn unmark_text(&mut self, _window: &mut Window, cx: &mut App) {
        self.with(cx, |_, view| view.marked = None);
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<Bounds<Pixels>> {
        let bounds = self.bounds;
        self.with(cx, |e, view| {
            let buf = e.buffer();
            let pos = buf.position_at(range_utf16.start.min(buf.len_utf16()));
            let tab = e.editor.options().tab_size;
            let text = buf.line_content(pos.line);
            let shaped = shape(window, &view.cfg, &expand_tabs(text, tab));
            let x = shaped.map_or(0.0, |s| {
                f32::from(s.x_for_index(expanded_byte_for_column(text, tab, pos.column)))
            });
            let (offset_x, offset_y) = view.scrolls.get(&e.path).map_or((0.0, 0.0), |s| {
                let o = s.0.borrow().base_handle.offset();
                (f32::from(o.x), f32::from(o.y))
            });
            let origin = point(
                bounds.origin.x + px(view.gutter + x + offset_x),
                bounds.origin.y + px((pos.line - 1) as f32 * view.cfg.line_height + offset_y),
            );
            Bounds::new(origin, gpui_kit::size(px(view.cfg.line_cursor_width()), px(view.cfg.line_height)))
        })
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<usize> {
        self.with(cx, |e, view| {
            let pos = hit_test(view, e, point, window);
            e.buffer().offset_at(pos)
        })
    }

    fn element_bounds(&mut self, _window: &mut Window, _cx: &mut App) -> Option<Bounds<Pixels>> {
        Some(self.bounds)
    }
}

// ---- status-bar text -------------------------------------------------------------------------

/// "Ln 3, Col 5" / "Ln 3, Col 5 (12 selected)" like upstream's editor status.
pub fn cursor_status(e: &OpenEditor) -> String {
    let sel = e.editor.selection();
    let tab = e.editor.options().tab_size;
    let p = sel.active;
    let col = visual_column(e.buffer().line_content(p.line), tab, p.column);
    let mut s = format!("Ln {}, Col {}", p.line, col);
    if !sel.is_empty() {
        let (r, _) = selection_offsets(e);
        s.push_str(&format!(" ({} selected)", r.len()));
    }
    s
}

/// "Spaces: 4" / "Tab Size: 4" (`editor.insertSpaces`).
pub fn indent_status(e: &OpenEditor) -> String {
    let o = e.editor.options();
    format!("{}: {}", if o.insert_spaces { "Spaces" } else { "Tab Size" }, o.tab_size)
}

// ---- drawing ---------------------------------------------------------------------------------

fn gutter_width(lines: usize, cfg: &EditorSettings) -> f32 {
    // The 26px right of the numbers are the decorations and folding columns, which stay without
    // line numbers.
    if cfg.line_numbers == LineNumbers::Off {
        return 26.0;
    }
    let digits = lines.to_string().len().max(3);
    // VS Code: lineNumbersMinChars=5 worth of room, number right-aligned.
    digits as f32 * cfg.char_width() + 34.0
}

fn widest_line(e: &OpenEditor) -> usize {
    let b = e.buffer();
    (1..=b.line_count()).max_by_key(|&l| b.line_length(l)).map_or(0, |l| l - 1)
}

/// The part of `selection` that falls on `line`, as (start col, end col, continues past the end).
fn selection_on_line(e: &OpenEditor, line: usize) -> Option<(usize, usize, bool)> {
    let sel = e.editor.selection();
    if sel.is_empty() {
        return None;
    }
    let r = sel.range();
    if line < r.start.line || line > r.end.line {
        return None;
    }
    let from = if line == r.start.line { r.start.column } else { 1 };
    let (to, past) = if line == r.end.line {
        (r.end.column, false)
    } else {
        (e.buffer().line_max_column(line), true)
    };
    Some((from, to, past))
}

/// The occurrences to highlight in this paint (`editor.selectionHighlight`,
/// `editor.occurrencesHighlight`): the selection when it is on one line, else the word at the caret.
fn occurrence_source(e: &OpenEditor, cfg: &EditorSettings) -> Option<decorations::Occurrences> {
    let sel = e.editor.selection();
    let range = sel.range();
    let selection = (cfg.selection_highlight && !sel.is_empty() && range.start.line == range.end.line)
        .then(|| e.editor.selected_text());
    let word = (sel.is_empty() && cfg.occurrences_highlight != OccurrencesHighlight::Off)
        .then(|| e.editor.word_at(sel.active).map(|r| e.buffer().value_in_range(r)))
        .flatten();
    let max = cfg.selection_highlight.then_some(cfg.selection_highlight_max_length);
    decorations::occurrences_for(selection.as_deref(), word.as_deref(), max)
}

/// The gutter text of `line` for `editor.lineNumbers` (lineNumbers.ts: `relative` shows the distance
/// to the caret line and the caret line's own number; `interval` every 10th line and the caret line).
fn line_number_label(mode: LineNumbers, line: usize, caret_line: usize) -> String {
    match mode {
        LineNumbers::Off => String::new(),
        LineNumbers::On => line.to_string(),
        LineNumbers::Relative => {
            if line == caret_line { line.to_string() } else { line.abs_diff(caret_line).to_string() }
        }
        LineNumbers::Interval => {
            if line == caret_line || line.is_multiple_of(10) { line.to_string() } else { String::new() }
        }
    }
}

/// The caret as a positioned box (`viewCursor.ts`): `line` is `editor.cursorWidth` wide (2 when 0),
/// `line-thin` 1px, `block` fills the character cell and redraws it inverted, `block-outline`
/// outlines the cell, `underline` is 2px and `underline-thin` 1px under the cell.
fn caret_element(
    cfg: &EditorSettings,
    t: &Theme,
    x: f32,
    cell_w: f32,
    glyph: Option<&str>,
) -> gpui_kit::Div {
    let lh = cfg.line_height;
    let base = div().absolute().left(px(x));
    let cursor = to_gpui(t.cursor);
    match cfg.cursor_style {
        CursorStyle::Line => base.top_0().w(px(cfg.line_cursor_width())).h(px(lh)).bg(cursor),
        CursorStyle::LineThin => base.top_0().w(px(1.)).h(px(lh)).bg(cursor),
        CursorStyle::Block => base
            .top_0()
            .w(px(cell_w))
            .h(px(lh))
            .bg(cursor)
            .text_color(to_gpui(t.editor_bg))
            .whitespace_nowrap()
            .children(glyph.map(|g| g.to_string())),
        CursorStyle::BlockOutline => base.top_0().w(px(cell_w)).h(px(lh)).border_1().border_color(cursor),
        CursorStyle::Underline => base.top(px(lh - 2.)).w(px(cell_w)).h(px(2.)).bg(cursor),
        CursorStyle::UnderlineThin => base.top(px(lh - 1.)).w(px(cell_w)).h(px(1.)).bg(cursor),
    }
}

#[allow(clippy::too_many_arguments)]
fn rows(
    e: &OpenEditor,
    t: &Theme,
    cfg: &EditorSettings,
    gutter: f32,
    range: Range<usize>,
    window: &Window,
    caret_on: bool,
    focused: bool,
    find: Option<&find::Highlights<'_>>,
    brackets: Option<(Position, Position)>,
) -> Vec<AnyElement> {
    let buf = e.buffer();
    let sel = e.editor.selection();
    let tab = e.editor.options().tab_size;
    let lh = cfg.line_height;
    let space_w = shape(window, cfg, " ").map_or(cfg.char_width(), |s| f32::from(s.width));
    let sel_bg = if focused { t.selection_bg } else { t.inactive_selection_bg };
    let caret_line = sel.active.line;
    let hl = cfg.render_line_highlight;
    // The border box of the current line: content only (`line`, empty selection), gutter only
    // (`gutter`), or the whole row (`all`).
    // `editor.renderLineHighlightOnlyWhenFocus`: the current line is marked only in a focused editor.
    let hl_shown = focused || !cfg.line_highlight_only_when_focus;
    let hl_content = matches!(hl, LineHighlight::Line | LineHighlight::All) && sel.is_empty() && hl_shown;
    let hl_gutter = matches!(hl, LineHighlight::Gutter | LineHighlight::All) && hl_shown;
    let occurrences = occurrence_source(e, cfg);
    // The active indent guide is found once per paint, not per row (it may scan the block).
    let active_guide = (cfg.guides_indentation && cfg.guides_highlight_active)
        .then(|| decorations::active_indent_guide(buf, caret_line, tab))
        .flatten();
    range
        .map(|ix| {
            let line = ix + 1;
            let text = buf.line_content(line);
            let is_active = line == caret_line;
            let expanded = expand_tabs(text, tab);
            let selected = selection_on_line(e, line);
            let has_caret = is_active && caret_on;
            let found = find.map_or_else(Vec::new, |f| f.matches_on_line(line, buf.line_max_column(line)));
            let markers = decorations::whitespace_marks(text, cfg.render_whitespace, selected.map(|(from, to, _)| (from, to)));
            // The bracket boxes of this line (the pair's two brackets, when they are on it).
            let bracket_cols: Vec<usize> = brackets
                .into_iter()
                .flat_map(|(a, b)| [a, b])
                .filter(|p| p.line == line)
                .map(|p| p.column)
                .collect();
            // Occurrences of the selection or of the word at the caret, on this line.
            let occurrence_cols: Vec<(usize, usize)> = occurrences.as_ref().map_or_else(Vec::new, |o| {
                decorations::occurrences_in_line(text, &o.needle, o.whole_word)
                    .into_iter()
                    .filter(|&(from, to)| !(o.is_selection && selected.is_some_and(|(sf, st, _)| (sf, st) == (from, to))))
                    .collect()
            });
            let shaped = (selected.is_some() || has_caret || !found.is_empty() || !markers.is_empty() || !bracket_cols.is_empty() || !occurrence_cols.is_empty())
                .then(|| shape(window, cfg, &expanded))
                .flatten();
            let x_at = |col: usize| -> f32 {
                shaped.as_ref().map_or(0.0, |s| {
                    f32::from(s.x_for_index(expanded_byte_for_column(text, tab, col)))
                })
            };

            let mut text_box = div().relative().flex_none().whitespace_nowrap().h_full();
            if let Some((from, to, past)) = selected {
                let x0 = x_at(from);
                let x1 = x_at(to) + if past { space_w } else { 0.0 };
                if x1 > x0 {
                    let mut sel_box = div().absolute().top_0().left(px(x0)).w(px(x1 - x0)).h(px(lh)).bg(to_gpui(sel_bg));
                    // `editor.roundedSelection`: 3px corners on the selection (upstream `border-radius`).
                    sel_box = if cfg.rounded_selection { sel_box.rounded(px(3.)) } else { sel_box };
                    text_box = text_box.child(sel_box.debug_selector(|| format!("selection:{line}")));
                }
            }
            // Find matches over the selection: the current one in its own color.
            if let Some(f) = find {
                for (from, to, current) in found {
                    let (x0, x1) = (x_at(from), x_at(to));
                    if x1 > x0 {
                        let color = if current { f.colors.match_bg } else { f.colors.match_highlight_bg };
                        text_box = text_box.child(
                            div().absolute().top_0().left(px(x0)).w(px(x1 - x0)).h(px(lh)).bg(to_gpui(color)),
                        );
                    }
                }
            }
            // Whitespace markers (`editor.renderWhitespace`): a middle dot in a space's cell, an arrow
            // at the start of a tab's cells (the tab itself is shown as spaces).
            for m in markers {
                let (x0, x1) = (x_at(m.column), x_at(m.column + 1));
                let glyph = match m.kind {
                    decorations::MarkKind::Space => "\u{b7}",
                    decorations::MarkKind::Tab => "\u{2192}",
                };
                let mark = div()
                    .absolute()
                    .top_0()
                    .left(px(x0))
                    .w(px((x1 - x0).max(0.0)))
                    .h(px(lh))
                    .text_color(to_gpui(t.whitespace_fg))
                    .debug_selector(|| format!("ws:{line}:{}", m.column))
                    .child(glyph);
                text_box = text_box.child(match m.kind {
                    decorations::MarkKind::Space => mark.text_center(),
                    decorations::MarkKind::Tab => mark.text_left(),
                });
            }
            let occurrence_color = match occurrences.as_ref() {
                Some(o) if o.is_selection => t.selection_highlight_bg,
                _ => t.word_highlight_bg,
            };
            for (from, to) in occurrence_cols {
                let (x0, x1) = (x_at(from), x_at(to));
                if x1 > x0 {
                    text_box = text_box.child(
                        div()
                            .absolute()
                            .top_0()
                            .left(px(x0))
                            .w(px(x1 - x0))
                            .h(px(lh))
                            .bg(to_gpui(occurrence_color))
                            .debug_selector(|| format!("occurrence:{line}:{from}")),
                    );
                }
            }
            for col in bracket_cols {
                let (x0, x1) = (x_at(col), x_at(col + 1));
                text_box = text_box.child(
                    div()
                        .absolute()
                        .top_0()
                        .left(px(x0))
                        .w(px((x1 - x0).max(0.0)))
                        .h(px(lh))
                        .bg(to_gpui(t.bracket_match_bg))
                        .border_1()
                        .border_color(to_gpui(t.bracket_match_border))
                        .debug_selector(|| format!("bracket:{line}:{col}")),
                );
            }
            text_box = text_box.text_color(to_gpui(t.editor_fg)).child(expanded);
            if has_caret {
                let col = sel.active.column;
                let x = x_at(col);
                let glyph = text.chars().nth(col - 1).filter(|c| *c != '\t').map(|c| c.to_string());
                let cell = (x_at(col + 1) - x).max(0.0);
                let cell = if glyph.is_some() && cell > 0.5 { cell } else { space_w };
                text_box = text_box.child(caret_element(cfg, t, x, cell, glyph.as_deref()));
            }

            let mut row = div()
                .relative()
                .flex()
                .flex_row()
                .h(px(lh))
                .line_height(px(lh))
                .font_family(SharedString::from(cfg.font_family.clone()))
                .font_weight(gpui_kit::FontWeight(cfg.font_weight))
                .font_features(font_features(cfg))
                .text_size(px(cfg.font_size));
            if let Some(f) = find.filter(|f| f.line_in_scope(line)) {
                // "Find in Selection": the lines of the scope get the range highlight.
                row = row.child(
                    div().absolute().top_0().bottom_0().left(px(gutter)).right_0().bg(to_gpui(f.colors.range_highlight_bg)),
                );
            }
            // Indent guides (`editor.guides.indentation`): one per indent level of the line.
            if cfg.guides_indentation {
                for k in 0..decorations::line_level(buf, line, tab) {
                    let active = active_guide.as_ref().is_some_and(|(ak, span)| *ak == k && span.contains(&line));
                    let color = if active { t.indent_guide_active_fg } else { t.indent_guide_fg };
                    row = row.child(
                        div()
                            .absolute()
                            .top_0()
                            .bottom_0()
                            .left(px(gutter + (k * tab) as f32 * space_w))
                            .w(px(1.))
                            .bg(to_gpui(color))
                            .debug_selector(|| if active { format!("guide-active:{line}:{k}") } else { format!("guide:{line}:{k}") }),
                    );
                }
            }
            // Rulers (`editor.rulers`): a 1px line after `column` characters, in every row.
            for ruler in &cfg.rulers {
                row = row.child(
                    div()
                        .absolute()
                        .top_0()
                        .bottom_0()
                        .left(px(gutter + ruler.column as f32 * space_w))
                        .w(px(1.))
                        .bg(to_gpui(ruler.color.unwrap_or(t.ruler_fg)))
                        .debug_selector(|| format!("ruler:{}", ruler.column)),
                );
            }
            if is_active && (hl_content || hl_gutter) {
                let left = if hl_content && !hl_gutter { gutter } else { 0.0 };
                let mut highlight = div().absolute().top_0().bottom_0().left(px(left)).border_2().border_color(to_gpui(t.line_highlight_border));
                highlight = if hl_content { highlight.right_0() } else { highlight.w(px(gutter)) };
                row = row.child(highlight);
            }
            row.child(
                div()
                    .flex_none()
                    .debug_selector(|| format!("gutter:{line}"))
                    .w(px(gutter))
                    .pr(px(26.))
                    .text_right()
                    .text_color(to_gpui(if is_active {
                        t.line_number_active_fg
                    } else {
                        t.line_number_fg
                    }))
                    .child(line_number_label(cfg.line_numbers, line, caret_line)),
            )
            .child(text_box)
            .into_any_element()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_number_labels_follow_the_setting() {
        assert_eq!(line_number_label(LineNumbers::On, 7, 3), "7");
        assert_eq!(line_number_label(LineNumbers::Off, 7, 3), "");
        assert_eq!(line_number_label(LineNumbers::Relative, 7, 3), "4");
        assert_eq!(line_number_label(LineNumbers::Relative, 3, 3), "3", "the caret line shows its own number");
        assert_eq!(line_number_label(LineNumbers::Relative, 1, 3), "2");
        assert_eq!(line_number_label(LineNumbers::Interval, 10, 3), "10");
        assert_eq!(line_number_label(LineNumbers::Interval, 7, 3), "");
        assert_eq!(line_number_label(LineNumbers::Interval, 3, 3), "3");
    }

    #[test]
    fn gutter_is_narrow_without_line_numbers() {
        let mut cfg = EditorSettings::default();
        assert!(gutter_width(100, &cfg) > gutter_width(100, &{ cfg.line_numbers = LineNumbers::Off; cfg.clone() }));
        cfg.line_numbers = LineNumbers::Off;
        assert_eq!(gutter_width(100_000, &cfg), 26.0);
    }
}

#[cfg(test)]
mod e2e;
#[cfg(test)]
mod find_e2e;
