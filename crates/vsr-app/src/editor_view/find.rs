//! The editor Find widget (Cmd+F, Alt+Cmd+F): the overlay at the top right of the editor, the
//! highlights it draws in the text, and the commands behind its keys.
//!
//! upstream: src/vs/editor/contrib/find/browser/findWidget.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findWidget.css @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findController.ts @ 50e1350fc78d
//! upstream: src/vs/editor/contrib/find/browser/findOptionsWidget.ts @ 50e1350fc78d
//!
//! The search itself is `vsr-find` (ADR-0027); this module owns what is gpui specific. The widget
//! is not an entity of its own: `FindUi` lives in `EditorView`, because every command needs the
//! active document (`Editors`) next to the widget state, and both are reached through
//! `EditorHost`. Two `InputState`s (find and replace) are the text boxes.
//!
//! Key contexts: the editor surface and the widget sit side by side under one `EditorFind`
//! element, so the find keys work from either. Enter in the Find box comes from the input's
//! `PressEnter` event (the input propagates Enter, a second binding would fire twice).
//!
//! Not implemented: "Select All Occurrences of Find Match" (one cursor), the find history
//! (`editor.find.history`), the global find clipboard, the options-only widget shown when the
//! widget is closed, the Tab order between the buttons, resizing the widget, and the overview
//! ruler marks.

use std::path::PathBuf;
use std::time::Instant;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    AnyElement, App, AppContext, BoxShadow, Context, Div, ElementId, Entity, FocusHandle, InteractiveElement, IntoElement,
    Focusable, KeyBinding, MouseButton, ParentElement, ScrollStrategy, Stateful, StatefulInteractiveElement, Styled, Subscription,
    Window, actions, div, point, px,
};
use vsr_find::{FindModel, FindState, SeedMode, scope_from_selection, seed_search_string};
use vsr_text::Range as TextRange;

use super::{EditorHost, EditorView};
use crate::config::settings::{AutoFindInSelection, FindSettings, SeedSearchString};
use crate::icons;
use crate::model::OpenEditor;
use crate::theme::{Rgba8, Theme, hex};
use crate::ui::to_gpui;

actions!(
    rscode_find,
    [
        Find,
        FindReplace,
        FindNext,
        FindPrevious,
        CloseFind,
        ToggleMatchCase,
        ToggleWholeWord,
        ToggleRegex,
        ToggleFindInSelection,
        TogglePreserveCase,
        ReplaceOne,
        ReplaceAll,
        UseSelectionForFind,
    ]
);

/// Codicons only the Find widget uses (`codiconsLibrary.ts`).
pub(super) mod ic {
    pub const ARROW_UP: char = '\u{EAA1}';
    pub const ARROW_DOWN: char = '\u{EA9A}';
    pub const SELECTION: char = '\u{EB85}';
    pub const REPLACE: char = '\u{EB3D}';
    pub const REPLACE_ALL: char = '\u{EB3C}';
    pub const PRESERVE_CASE: char = '\u{EB2E}';
}

/// Key context of the element holding both the editor and the widget.
pub(super) const KEY_CONTEXT: &str = "EditorFind";

/// The macOS default bindings of `findController.ts` / `findModel.ts`.
pub(super) fn bind(cx: &mut App) {
    let c = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-f", Find, c),
        KeyBinding::new("alt-cmd-f", FindReplace, c),
        KeyBinding::new("cmd-g", FindNext, c),
        KeyBinding::new("f3", FindNext, c),
        KeyBinding::new("shift-cmd-g", FindPrevious, c),
        KeyBinding::new("shift-f3", FindPrevious, c),
        KeyBinding::new("cmd-e", UseSelectionForFind, c),
        KeyBinding::new("escape", CloseFind, c),
        KeyBinding::new("shift-escape", CloseFind, c),
        KeyBinding::new("alt-cmd-c", ToggleMatchCase, c),
        KeyBinding::new("alt-cmd-w", ToggleWholeWord, c),
        KeyBinding::new("alt-cmd-r", ToggleRegex, c),
        KeyBinding::new("alt-cmd-l", ToggleFindInSelection, c),
        KeyBinding::new("alt-cmd-p", TogglePreserveCase, c),
        KeyBinding::new("cmd-shift-1", ReplaceOne, c),
        KeyBinding::new("alt-cmd-enter", ReplaceAll, c),
    ]);
}

/// The colors of the widget and the highlights (`editorColors.ts`, dark_modern).
#[derive(Debug, Clone, Copy)]
pub(super) struct FindColors {
    pub widget_bg: Rgba8,        // editorWidget.background (dark_modern)
    pub widget_border: Rgba8,    // widget.border (dark_modern)
    pub widget_shadow: Rgba8,    // widget.shadow (registry dark)
    pub disabled_fg: Rgba8,      // disabledForeground
    pub match_bg: Rgba8,         // editor.findMatchBackground (dark_modern)
    pub match_highlight_bg: Rgba8, // editor.findMatchHighlightBackground
    pub range_highlight_bg: Rgba8, // editor.findRangeHighlightBackground
}

impl FindColors {
    pub(super) fn dark_modern() -> Self {
        let c = |s: &str| hex(s).expect("valid built-in color");
        Self {
            widget_bg: c("#202020"),
            widget_border: c("#313131"),
            widget_shadow: c("#0000005C"),
            disabled_fg: c("#CCCCCC80"),
            match_bg: c("#9E6A03"),
            match_highlight_bg: c("#EA5C0055"),
            range_highlight_bg: c("#3A3D4166"),
        }
    }
}

/// What the text rows draw for the find widget.
pub(super) struct Highlights<'a> {
    pub matches: &'a [TextRange],
    /// The match navigation last landed on.
    pub current: Option<TextRange>,
    /// The "find in selection" scopes.
    pub scopes: Vec<TextRange>,
    pub colors: FindColors,
}

/// The part of `r` on `line` as (start column, end column).
pub(super) fn range_on_line(r: &TextRange, line: usize, line_max_column: usize) -> Option<(usize, usize)> {
    if line < r.start.line || line > r.end.line {
        return None;
    }
    let from = if line == r.start.line { r.start.column } else { 1 };
    let to = if line == r.end.line { r.end.column } else { line_max_column };
    Some((from, to))
}

impl Highlights<'_> {
    /// The matches on `line`: (start column, end column, is the current match).
    pub(super) fn matches_on_line(&self, line: usize, line_max_column: usize) -> Vec<(usize, usize, bool)> {
        let first = self.matches.partition_point(|m| m.end.line < line);
        self.matches[first..]
            .iter()
            .take_while(|m| m.start.line <= line)
            .filter_map(|m| {
                let (a, b) = range_on_line(m, line, line_max_column)?;
                (b > a).then_some((a, b, self.current == Some(*m)))
            })
            .collect()
    }

    pub(super) fn line_in_scope(&self, line: usize) -> bool {
        self.scopes.iter().any(|s| s.start.line <= line && line <= s.end.line)
    }
}

/// The widget state (one per editor view).
#[derive(Default)]
pub(super) struct FindUi {
    open: bool,
    replace_open: bool,
    find_input: Option<Entity<InputState>>,
    replace_input: Option<Entity<InputState>>,
    /// Containers tracking focus, to style the box that has the focus (`in_focus`).
    find_box_focus: Option<FocusHandle>,
    replace_box_focus: Option<FocusHandle>,
    _subs: Vec<Subscription>,
    /// The options kept while the widget is closed (and the search string to start from).
    state: FindState,
    /// Present while the widget is open.
    model: Option<FindModel>,
    model_path: Option<PathBuf>,
    settings: FindSettings,
}

impl FindUi {
    #[cfg(test)]
    pub(super) fn is_open(&self) -> bool {
        self.open
    }

    /// "3 of 12" / "No results" (empty before the widget was ever opened).
    #[cfg(test)]
    pub(super) fn label(&self) -> String {
        self.model.as_ref().map_or_else(String::new, FindModel::label)
    }

    #[cfg(test)]
    pub(super) fn find_input(&self) -> Option<&Entity<InputState>> {
        self.find_input.as_ref()
    }

    #[cfg(test)]
    pub(super) fn replace_input(&self) -> Option<&Entity<InputState>> {
        self.replace_input.as_ref()
    }

    #[cfg(test)]
    pub(super) fn model(&self) -> Option<&FindModel> {
        self.model.as_ref()
    }

    fn state(&self) -> &FindState {
        self.model.as_ref().map_or(&self.state, FindModel::state)
    }

    /// Per frame, before drawing: apply the settings, keep the model in step with the document
    /// (a different tab gets a fresh model with the same options, no scope).
    pub(super) fn sync(&mut self, e: &OpenEditor, settings: FindSettings) {
        self.settings = settings;
        if !self.open {
            return;
        }
        if self.model_path.as_ref() != Some(&e.path) {
            let mut state = self.state().clone();
            state.search_scope = None;
            self.model = Some(self.new_model(state, e));
            self.model_path = Some(e.path.clone());
            return;
        }
        if let Some(m) = &mut self.model {
            m.cursor_move_on_type = settings.cursor_move_on_type;
            m.set_loop(settings.loop_);
            m.sync(&e.editor);
        }
    }

    fn new_model(&self, mut state: FindState, e: &OpenEditor) -> FindModel {
        state.loop_ = self.settings.loop_;
        let mut m = FindModel::new(state, &e.editor);
        m.cursor_move_on_type = self.settings.cursor_move_on_type;
        m
    }

    pub(super) fn highlights(&self, e: &OpenEditor) -> Option<Highlights<'_>> {
        if !self.open || self.model_path.as_ref() != Some(&e.path) {
            return None;
        }
        let m = self.model.as_ref()?;
        Some(Highlights {
            matches: m.matches(),
            current: m.highlighted(),
            scopes: m.scopes(e.buffer()).unwrap_or_default(),
            colors: FindColors::dark_modern(),
        })
    }
}

// ---- commands --------------------------------------------------------------------------------

fn with_editor<T: EditorHost, R>(this: &mut T, f: impl FnOnce(&mut OpenEditor, &mut EditorView) -> R) -> Option<R> {
    let (editors, view) = this.editor_host();
    let e = editors.active.and_then(|i| editors.open.get_mut(i))?;
    Some(f(e, view))
}

/// Creates the two text boxes (once) and listens to them.
fn ensure_inputs<T: EditorHost>(this: &mut T, window: &mut Window, cx: &mut Context<T>) {
    if this.editor_host().1.find.find_input.is_some() {
        return;
    }
    let find = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
    let replace = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
    let subs = vec![
        cx.subscribe_in(&find, window, |this: &mut T, st, ev: &InputEvent, window, cx| {
            on_find_input(this, st, ev, window, cx)
        }),
        cx.subscribe_in(&replace, window, |this: &mut T, st, ev: &InputEvent, window, cx| {
            on_replace_input(this, st, ev, window, cx)
        }),
    ];
    let ui = &mut this.editor_host().1.find;
    ui.find_input = Some(find);
    ui.replace_input = Some(replace);
    ui._subs = subs;
    ui.find_box_focus = Some(cx.focus_handle());
    ui.replace_box_focus = Some(cx.focus_handle());
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    FindInput,
    ReplaceInput,
    None,
}

/// `FindController._start`: reveal the widget, seed the search string, set the scope.
fn open<T: EditorHost>(this: &mut T, replace: bool, focus: Focus, seed: bool, window: &mut Window, cx: &mut Context<T>) {
    ensure_inputs(this, window, cx);
    let Some((find_text, find_input, replace_input)) = with_editor(this, |e, view| {
        let ui = &mut view.find;
        let was_open = ui.open;
        let cfg = ui.settings;
        let mut state = ui.state().clone();
        let mode = match cfg.seed_search_string_from_selection {
            SeedSearchString::Never => SeedMode::Never,
            SeedSearchString::Always => SeedMode::Always,
            SeedSearchString::Selection => SeedMode::Selection,
        };
        if seed && let Some(s) = seed_search_string(&e.editor, mode, state.is_regex) {
            state.search_string = s;
        }
        let multiline = e.editor.selection().anchor.line != e.editor.selection().active.line;
        let auto_scope = match cfg.auto_find_in_selection {
            AutoFindInSelection::Always => true,
            AutoFindInSelection::Never => false,
            AutoFindInSelection::Multiline => multiline,
        };
        if auto_scope && let Some(scope) = scope_from_selection(&e.editor) {
            state.search_scope = Some(scope);
        }
        if replace {
            ui.replace_open = true;
        } else if !was_open {
            ui.replace_open = false;
        }
        ui.open = true;
        let text = state.search_string.clone();
        ui.model = Some(ui.new_model(state, e));
        ui.model_path = Some(e.path.clone());
        (text, ui.find_input.clone(), ui.replace_input.clone())
    }) else {
        return;
    };
    let (Some(find_input), Some(replace_input)) = (find_input, replace_input) else { return };
    find_input.update(cx, |s, cx| {
        if s.value().as_ref() != find_text.as_str() {
            s.set_value(find_text.clone(), window, cx);
        }
    });
    let replace_text = with_editor(this, |_, v| v.find.state().replace_string.clone()).unwrap_or_default();
    replace_input.update(cx, |s, cx| {
        if s.value().as_ref() != replace_text.as_str() {
            s.set_value(replace_text.clone(), window, cx);
        }
    });
    match focus {
        Focus::FindInput => find_input.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        }),
        Focus::ReplaceInput => replace_input.update(cx, |s, cx| {
            s.focus(window, cx);
            s.select_all(window, cx);
        }),
        Focus::None => {}
    }
    cx.notify();
}

fn close<T: EditorHost>(this: &mut T, window: &mut Window, cx: &mut Context<T>) {
    let (_, view) = this.editor_host();
    if !view.find.open {
        return;
    }
    let mut state = view.find.state().clone();
    state.search_scope = None;
    view.find.state = state;
    view.find.open = false;
    view.find.model = None;
    view.find.model_path = None;
    if let Some(f) = &view.focus {
        f.focus(window, cx);
    }
    cx.notify();
}

/// After a move: scroll the match into view (centred, when it is outside), show the caret.
fn finish<T: EditorHost>(this: &mut T, edited: bool, window: &mut Window, cx: &mut Context<T>) {
    with_editor(this, |e, view| {
        let reveal = view.find.model.as_mut().and_then(FindModel::take_reveal);
        if let Some(r) = reveal {
            view.reveal_range_if_outside(e, r.start.line, r.end.line);
        }
        view.activity = Some(Instant::now());
        if let Some(f) = &view.focus {
            if edited {
                window.notify_text_input_changed(f);
            }
            window.notify_text_selection_changed(f);
        }
    });
    cx.notify();
}

fn search_text_changed<T: EditorHost>(this: &mut T, text: String, window: &mut Window, cx: &mut Context<T>) {
    with_editor(this, |e, view| {
        if let Some(m) = &mut view.find.model {
            if m.state().search_string != text {
                m.change(&mut e.editor, true, |s| s.search_string = text);
            }
        } else {
            view.find.state.search_string = text;
        }
    });
    finish(this, false, window, cx);
}

fn on_find_input<T: EditorHost>(
    this: &mut T,
    st: &Entity<InputState>,
    ev: &InputEvent,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    let find_on_type = this.editor_host().1.find.settings.find_on_type;
    match ev {
        InputEvent::Change if find_on_type => {
            let text = st.read(cx).value().to_string();
            search_text_changed(this, text, window, cx);
        }
        InputEvent::PressEnter { shift, .. } => {
            if !find_on_type {
                let text = st.read(cx).value().to_string();
                let changed = with_editor(this, |_, v| v.find.state().search_string != text).unwrap_or(false);
                if changed {
                    search_text_changed(this, text, window, cx);
                    return;
                }
            }
            navigate(this, !*shift, true, window, cx);
        }
        _ => {}
    }
}

fn on_replace_input<T: EditorHost>(
    this: &mut T,
    st: &Entity<InputState>,
    ev: &InputEvent,
    window: &mut Window,
    cx: &mut Context<T>,
) {
    match ev {
        InputEvent::Change => {
            let text = st.read(cx).value().to_string();
            with_editor(this, |_, view| {
                if let Some(m) = &mut view.find.model {
                    m.set_replace_string(text); // no new search needed
                } else {
                    view.find.state.replace_string = text;
                }
            });
        }
        InputEvent::PressEnter { secondary, .. } => {
            if *secondary {
                replace_all(this, window, cx);
            } else {
                replace_one(this, window, cx);
            }
        }
        _ => {}
    }
}

/// Next (`forward`) or previous match. `from_input`: Enter in the Find box, which may close the
/// widget (`editor.find.closeOnResult`).
fn navigate<T: EditorHost>(this: &mut T, forward: bool, from_input: bool, window: &mut Window, cx: &mut Context<T>) {
    if !this.editor_host().1.find.open {
        // `matchFindAction`: with no widget, start a find (seeding when there is no search yet).
        let seed = with_editor(this, |_, v| v.find.state().search_string.is_empty()).unwrap_or(false);
        open(this, false, Focus::None, seed, window, cx);
    }
    let landed = with_editor(this, |e, view| {
        let m = view.find.model.as_mut()?;
        Some(if forward { m.move_to_next_match(&mut e.editor) } else { m.move_to_prev_match(&mut e.editor) })
    })
    .flatten()
    .unwrap_or(false);
    finish(this, false, window, cx);
    if landed && from_input && this.editor_host().1.find.settings.close_on_result {
        close(this, window, cx);
    }
}

fn replace_one<T: EditorHost>(this: &mut T, window: &mut Window, cx: &mut Context<T>) {
    with_editor(this, |e, view| {
        if let Some(m) = &mut view.find.model {
            m.replace(&mut e.editor);
        }
    });
    finish(this, true, window, cx);
}

fn replace_all<T: EditorHost>(this: &mut T, window: &mut Window, cx: &mut Context<T>) {
    with_editor(this, |e, view| {
        if let Some(m) = &mut view.find.model {
            m.replace_all(&mut e.editor);
        }
    });
    finish(this, true, window, cx);
}

/// Which option a toggle flips.
#[derive(Clone, Copy)]
enum Toggle {
    MatchCase,
    WholeWord,
    Regex,
    PreserveCase,
    Scope,
}

/// `move_cursor`: a click moves to the next match from where the search started (like typing);
/// the keyboard shortcut only updates the matches.
fn toggle<T: EditorHost>(this: &mut T, which: Toggle, move_cursor: bool, window: &mut Window, cx: &mut Context<T>) {
    with_editor(this, |e, view| {
        let flip = |s: &mut FindState| match which {
            Toggle::MatchCase => s.match_case = !s.match_case,
            Toggle::WholeWord => s.whole_word = !s.whole_word,
            Toggle::Regex => s.is_regex = !s.is_regex,
            Toggle::PreserveCase => s.preserve_case = !s.preserve_case,
            Toggle::Scope => {}
        };
        let scope = match which {
            Toggle::Scope => {
                let has = view.find.state().search_scope.is_some();
                Some(if has { None } else { scope_from_selection(&e.editor) }).filter(|s| has || s.is_some())
            }
            _ => None,
        };
        let apply = |s: &mut FindState| {
            flip(s);
            if let Some(scope) = scope {
                s.search_scope = scope;
            }
        };
        match &mut view.find.model {
            Some(m) => m.change(&mut e.editor, move_cursor || matches!(which, Toggle::Scope), apply),
            None => apply(&mut view.find.state),
        }
    });
    finish(this, false, window, cx);
}

fn toggle_replace<T: EditorHost>(this: &mut T, cx: &mut Context<T>) {
    let ui = &mut this.editor_host().1.find;
    ui.replace_open = !ui.replace_open;
    cx.notify();
}

/// `StartFindReplaceAction` (Alt+Cmd+F): reveal the replace row. The search string is seeded only
/// from a non-empty single-line selection (and not while the Find box has the focus); the Replace
/// box gets the focus when the Find box already had it or was just seeded.
fn start_replace<T: EditorHost>(this: &mut T, window: &mut Window, cx: &mut Context<T>) {
    let find_focused = this
        .editor_host()
        .1
        .find
        .find_input
        .clone()
        .is_some_and(|i| i.read(cx).focus_handle(cx).is_focused(window));
    let seeded = with_editor(this, |e, view| {
        let sel = e.editor.selection();
        !sel.is_empty()
            && sel.anchor.line == sel.active.line
            && view.find.settings.seed_search_string_from_selection != SeedSearchString::Never
    })
    .unwrap_or(false)
        && !find_focused;
    let focus = if find_focused || seeded { Focus::ReplaceInput } else { Focus::FindInput };
    open(this, true, focus, seeded, window, cx);
}

/// `UseSelectionForFind` (Cmd+E): the selection (or word) becomes the search string; the widget
/// opens without taking focus.
fn use_selection<T: EditorHost>(this: &mut T, window: &mut Window, cx: &mut Context<T>) {
    let text = with_editor(this, |e, view| {
        let is_regex = view.find.state().is_regex;
        seed_search_string(&e.editor, SeedMode::Always, is_regex)
    })
    .flatten();
    let Some(text) = text else { return };
    open(this, false, Focus::None, false, window, cx);
    if let Some(input) = this.editor_host().1.find.find_input.clone() {
        input.update(cx, |s, cx| s.set_value(text.clone(), window, cx));
    }
    search_text_changed(this, text, window, cx);
}

/// Attach the find actions to the `EditorFind` element.
pub(super) fn register_actions<T: EditorHost>(el: Stateful<Div>, cx: &mut Context<T>) -> Stateful<Div> {
    el.on_action(cx.listener(|this: &mut T, _: &Find, w, cx| open(this, false, Focus::FindInput, true, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &FindReplace, w, cx| start_replace(this, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &FindNext, w, cx| navigate(this, true, false, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &FindPrevious, w, cx| navigate(this, false, false, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &CloseFind, w, cx| close(this, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &ToggleMatchCase, w, cx| toggle(this, Toggle::MatchCase, false, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &ToggleWholeWord, w, cx| toggle(this, Toggle::WholeWord, false, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &ToggleRegex, w, cx| toggle(this, Toggle::Regex, false, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &ToggleFindInSelection, w, cx| toggle(this, Toggle::Scope, true, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &TogglePreserveCase, w, cx| toggle(this, Toggle::PreserveCase, false, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &ReplaceOne, w, cx| replace_one(this, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &ReplaceAll, w, cx| replace_all(this, w, cx)))
        .on_action(cx.listener(|this: &mut T, _: &UseSelectionForFind, w, cx| use_selection(this, w, cx)))
}

impl EditorView {
    /// Scroll so that lines `first..=last` (1-based) are visible, centring the first when it is
    /// outside the viewport (`revealRangeInCenterIfOutsideViewport`).
    fn reveal_range_if_outside(&mut self, e: &OpenEditor, first: usize, last: usize) {
        let rows = self.rows_per_page();
        let Some(scroll) = self.scrolls.get(&e.path) else { return };
        let offset_y = f32::from(scroll.0.borrow().base_handle.offset().y);
        let top = (-offset_y / self.cfg.line_height).max(0.0).floor() as usize;
        let (a, z) = (first.saturating_sub(1), last.saturating_sub(1));
        if a < top || z >= top + rows {
            scroll.scroll_to_item(a, ScrollStrategy::Center);
        }
    }
}

// ---- drawing ---------------------------------------------------------------------------------

const WIDGET_WIDTH: f32 = 419.0;
const INPUT_WIDTH: f32 = 221.0;

/// A toggle inside an input (Match Case, ...): `monaco-custom-toggle` (same recipe as the Search view).
fn option_toggle<T: EditorHost>(
    t: &Theme,
    id: &'static str,
    glyph: char,
    tip: &'static str,
    active: bool,
    cx: &mut Context<T>,
    on: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex_none()
        .size(px(20.))
        .ml(px(2.))
        .rounded(px(3.))
        .border_1()
        .border_color(if active { to_gpui(t.input_option_active_border) } else { gpui_kit::transparent_black() })
        .when(active, |d| d.bg(to_gpui(t.input_option_active_bg)))
        .when(!active, |d| d.hover(|s| s.bg(to_gpui(t.input_option_hover_bg))))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(icons::FAMILY)
        .text_size(px(16.))
        .text_color(to_gpui(if active { t.input_option_active_fg } else { t.sidebar_fg }))
        .child(icons::glyph(glyph))
        .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx))
        .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, w, cx| on(this, w, cx)))
}

/// A `.button` of the widget: 22px, rounded; disabled buttons are dimmed and inert.
#[allow(clippy::too_many_arguments)]
fn button<T: EditorHost>(
    t: &Theme,
    colors: &FindColors,
    id: impl Into<ElementId>,
    glyph: char,
    tip: &'static str,
    enabled: bool,
    checked: bool,
    cx: &mut Context<T>,
    on: impl Fn(&mut T, &mut Window, &mut Context<T>) + 'static,
) -> Stateful<Div> {
    let el = div()
        .id(id)
        .flex_none()
        .size(px(22.))
        .ml(px(3.))
        .rounded(px(5.))
        .border_1()
        .border_color(if checked { to_gpui(t.input_option_active_border) } else { gpui_kit::transparent_black() })
        .when(checked, |d| d.bg(to_gpui(t.input_option_active_bg)))
        .flex()
        .items_center()
        .justify_center()
        .font_family(icons::FAMILY)
        .text_size(px(16.))
        .text_color(to_gpui(if enabled { t.foreground } else { colors.disabled_fg }))
        .child(icons::glyph(glyph))
        .tooltip(move |w, cx| Tooltip::new(tip).build(w, cx));
    if enabled {
        el.cursor_pointer()
            .when(!checked, |d| d.hover(|s| s.bg(to_gpui(t.toolbar_hover_bg))))
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, w, cx| on(this, w, cx)))
    } else {
        el
    }
}

/// The box around a text input: `monaco-inputbox` with the option toggles at its right.
fn input_box(
    t: &Theme,
    colors: &FindColors,
    id: &'static str,
    state: &Entity<InputState>,
    focus: &FocusHandle,
    invalid: bool,
    toggles: Vec<Stateful<Div>>,
) -> Stateful<Div> {
    let _ = colors;
    div()
        .id(id)
        .track_focus(focus)
        .flex()
        .items_center()
        .flex_none()
        .w(px(INPUT_WIDTH))
        .h(px(25.))
        .px(px(3.))
        .rounded(px(4.))
        .border_1()
        .border_color(to_gpui(if invalid { t.validation_error_border } else { t.input_border }))
        .in_focus(|s| s.border_color(to_gpui(if invalid { t.validation_error_border } else { t.focus_border })))
        .bg(to_gpui(t.settings_text_input_bg))
        .child(div().flex_1().min_w_0().child(Input::new(state).appearance(false).bordered(false)))
        .children(toggles)
}

impl FindUi {
    /// The widget element, when it is open and has a model.
    pub(super) fn element<T: EditorHost>(&self, t: &Theme, cx: &mut Context<T>) -> Option<AnyElement> {
        if !self.open {
            return None;
        }
        let model = self.model.as_ref()?;
        let (find_input, replace_input) = (self.find_input.as_ref()?, self.replace_input.as_ref()?);
        let (find_focus, replace_focus) = (self.find_box_focus.as_ref()?, self.replace_box_focus.as_ref()?);
        let colors = FindColors::dark_modern();
        let st = model.state();
        let info = model.match_info();
        let has_text = !st.search_string.is_empty();
        let no_results = has_text && (info.count == 0 || model.error().is_some());
        let can_nav = has_text && info.count > 0;
        let label = model.label();

        let toggles = vec![
            option_toggle(t, "find-case", icons::CASE_SENSITIVE, "Match Case (⌥⌘C)", st.match_case, cx, |this: &mut T, w, cx| {
                toggle(this, Toggle::MatchCase, true, w, cx)
            }),
            option_toggle(t, "find-word", icons::WHOLE_WORD, "Match Whole Word (⌥⌘W)", st.whole_word, cx, |this: &mut T, w, cx| {
                toggle(this, Toggle::WholeWord, true, w, cx)
            }),
            option_toggle(t, "find-regex", icons::REGEX, "Use Regular Expression (⌥⌘R)", st.is_regex, cx, |this: &mut T, w, cx| {
                toggle(this, Toggle::Regex, true, w, cx)
            }),
        ];
        let count = div()
            .id("find-count")
            .debug_selector(|| "find-count".to_string())
            .flex_none()
            .min_w(px(69.))
            .h(px(25.))
            .ml(px(3.))
            .pl(px(2.))
            .flex()
            .items_center()
            .justify_center()
            .text_size(px(12.))
            .text_color(to_gpui(if no_results { t.error_fg } else { t.foreground }))
            .child(label);
        let find_row = div()
            .flex()
            .items_center()
            .ml(px(17.))
            .mt(px(3.))
            .child(input_box(t, &colors, "find-box", find_input, find_focus, no_results, toggles))
            .child(count)
            .child(button(t, &colors, "find-prev", ic::ARROW_UP, "Previous Match (⇧⌘G)", can_nav, false, cx, |this: &mut T, w, cx| {
                navigate(this, false, false, w, cx)
            }))
            .child(button(t, &colors, "find-next", ic::ARROW_DOWN, "Next Match (⌘G)", can_nav, false, cx, |this: &mut T, w, cx| {
                navigate(this, true, false, w, cx)
            }))
            .child(button(
                t,
                &colors,
                "find-scope",
                ic::SELECTION,
                "Find in Selection (⌥⌘L)",
                true,
                st.search_scope.is_some(),
                cx,
                |this: &mut T, w, cx| toggle(this, Toggle::Scope, true, w, cx),
            ))
            .child(div().flex_1())
            .child(button(t, &colors, "find-close", icons::CLOSE, "Close (Escape)", true, false, cx, |this: &mut T, w, cx| {
                close(this, w, cx)
            }));

        let replace_row = self.replace_open.then(|| {
            let preserve = vec![option_toggle(
                t,
                "find-preserve-case",
                ic::PRESERVE_CASE,
                "Preserve Case (⌥⌘P)",
                st.preserve_case,
                cx,
                |this: &mut T, w, cx| toggle(this, Toggle::PreserveCase, true, w, cx),
            )];
            let can_replace = info.count > 0;
            div()
                .flex()
                .items_center()
                .ml(px(17.))
                .mt(px(3.))
                .child(input_box(t, &colors, "replace-box", replace_input, replace_focus, false, preserve))
                .child(div().w(px(3. + 69.)).flex_none())
                .child(button(t, &colors, "find-replace", ic::REPLACE, "Replace (Enter)", can_replace, false, cx, |this: &mut T, w, cx| {
                    replace_one(this, w, cx)
                }))
                .child(button(
                    t,
                    &colors,
                    "find-replace-all",
                    ic::REPLACE_ALL,
                    "Replace All (⌘Enter)",
                    can_replace,
                    false,
                    cx,
                    |this: &mut T, w, cx| replace_all(this, w, cx),
                ))
        });

        // The chevron that reveals the replace row: the left edge of the widget, full height.
        let chevron = div()
            .id("find-toggle-replace")
            .debug_selector(|| "find-toggle-replace".to_string())
            .absolute()
            .top_0()
            .left_0()
            .w(px(18.))
            .h_full()
            .rounded(px(3.))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .font_family(icons::FAMILY)
            .text_size(px(16.))
            .text_color(to_gpui(t.foreground))
            .hover(|s| s.bg(to_gpui(t.toolbar_hover_bg)))
            .child(icons::glyph(if self.replace_open { icons::CHEVRON_DOWN } else { icons::CHEVRON_RIGHT }))
            .tooltip(|w, cx| Tooltip::new("Toggle Replace").build(w, cx))
            .on_mouse_down(MouseButton::Left, cx.listener(|this: &mut T, _, _, cx| toggle_replace(this, cx)));

        let shadow = BoxShadow {
            color: to_gpui(colors.widget_shadow),
            offset: point(px(0.), px(0.)),
            blur_radius: px(8.),
            spread_radius: px(2.),
            inset: false,
        };
        Some(
            div()
                .id("find-widget")
                .debug_selector(|| "find-widget".to_string())
                .key_context("FindWidget")
                .absolute()
                .top(px(4.))
                .right(px(14.))
                .w(px(WIDGET_WIDTH))
                .pl(px(9.))
                .pr(px(4.))
                .pb(px(3.))
                .rounded(px(6.))
                .border_1()
                .border_color(to_gpui(colors.widget_border))
                .bg(to_gpui(colors.widget_bg))
                .text_color(to_gpui(t.foreground))
                .shadow(vec![shadow])
                .occlude()
                .child(chevron)
                .child(find_row)
                .children(replace_row)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use vsr_text::Position;

    use super::*;

    fn r(l1: usize, c1: usize, l2: usize, c2: usize) -> TextRange {
        TextRange::new(Position::new(l1, c1), Position::new(l2, c2))
    }

    #[test]
    fn matches_on_line_clips_multiline_matches_and_marks_the_current_one() {
        let matches = [r(1, 2, 1, 4), r(1, 6, 2, 3), r(2, 5, 2, 5), r(4, 1, 4, 3)];
        let h = Highlights {
            matches: &matches,
            current: Some(matches[3]),
            scopes: vec![r(2, 1, 3, 1)],
            colors: FindColors::dark_modern(),
        };
        assert_eq!(h.matches_on_line(1, 9), [(2, 4, false), (6, 9, false)], "a match continuing on the next line fills the rest");
        assert_eq!(h.matches_on_line(2, 8), [(1, 3, false)], "its tail; the empty match draws nothing");
        assert_eq!(h.matches_on_line(3, 5), []);
        assert_eq!(h.matches_on_line(4, 5), [(1, 3, true)]);
        assert!(!h.line_in_scope(1) && h.line_in_scope(2) && h.line_in_scope(3) && !h.line_in_scope(4));
    }

    /// The widget's icons are in the bundled codicon font. Uses the REAL CoreText text system (the
    /// test platform's accepts anything), without a window.
    #[test]
    fn find_icons_exist_in_the_bundled_codicon_font() {
        use gpui::PlatformTextSystem;
        let ts = gpui_apple::AppleTextSystem::new();
        ts.add_fonts(vec![Cow::Borrowed(include_bytes!("../../assets/codicon.ttf"))]).expect("add_fonts");
        let id = ts.font_id(&gpui::font(icons::FAMILY)).expect("codicon family loads");
        for c in [ic::ARROW_UP, ic::ARROW_DOWN, ic::SELECTION, ic::REPLACE, ic::REPLACE_ALL, ic::PRESERVE_CASE] {
            assert!(ts.glyph_for_char(id, c).is_some(), "no glyph for {c:?}");
        }
    }
}
