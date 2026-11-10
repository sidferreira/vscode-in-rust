//! The editor's vertical and horizontal scrollbars (`editor.scrollbar.*`, ADR-0054).
//!
//! upstream: src/vs/base/browser/ui/scrollbar/scrollbarState.ts @ 50e1350fc78d
//! upstream: src/vs/base/browser/ui/scrollbar/abstractScrollbar.ts @ 50e1350fc78d
//! upstream: src/vs/base/browser/ui/scrollbar/verticalScrollbar.ts @ 50e1350fc78d
//! upstream: src/vs/base/browser/ui/scrollbar/horizontalScrollbar.ts @ 50e1350fc78d
//!
//! `state` is the slider math. This module draws a bar for each axis from the list's scroll
//! handle: the extents come from gpui's measurement at the last paint (`ScrollHandle::bounds` and
//! `max_offset`), so the slider follows the list's real scroll range, including the room of
//! `editor.scrollBeyondLastLine` and `editor.scrollBeyondLastColumn`. A click on the track jumps the
//! slider's centre to the click (or pages with `editor.scrollbar.scrollByPage`); dragging the slider
//! moves the list by the pointer's distance. Not ported: the arrow buttons (arrowSize is 0), the
//! fade of the bars after scrolling (`scrollbarVisibilityController` fades only with `auto` and
//! `mouseover` in upstream), and `ignoreHorizontalScrollbarInContentHeight`, which needs the scroll
//! height to include the bar's height, while the list measures whole rows.

pub mod state;
#[cfg(test)]
mod upstream_tests;

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{
    AnyElement, App, Bounds, DispatchPhase, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Point, SharedString, Size,
    Styled, UniformListScrollHandle, Window, canvas, div, point, px,
};

use self::state::ScrollbarState;
use crate::config::scrollbar::ScrollbarVisibility;
use crate::config::settings::EditorSettings;
use crate::theme::Rgba8;
use crate::ui::to_gpui;

/// Which bar: the vertical one scrolls the list's rows, the horizontal one its width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Vertical,
    Horizontal,
}

/// `scrollbarSlider.*` colours.
#[derive(Clone, Copy, Debug)]
pub struct SliderColors {
    pub background: Rgba8,
    pub hover: Rgba8,
}

/// The drag in progress: where the pointer went down along the axis and the state it started from.
#[derive(Clone, Copy)]
struct Drag {
    start: f64,
    initial: ScrollbarState,
}

/// What the bar's event handlers share with the frame that built them.
#[derive(Default)]
pub struct BarShared {
    /// The painted track, in window coordinates.
    track: Cell<Bounds<Pixels>>,
    drag: Cell<Option<Drag>>,
}

/// The two bars of one editor (shared between frames, so a drag survives a repaint).
#[derive(Default)]
pub struct Bars {
    pub vertical: Rc<BarShared>,
    pub horizontal: Rc<BarShared>,
}

/// The `editor.scrollbar.*` settings the bars read.
pub struct BarSettings {
    pub vertical: ScrollbarVisibility,
    pub horizontal: ScrollbarVisibility,
    pub vertical_size: f32,
    pub horizontal_size: f32,
    pub by_page: bool,
}

pub fn settings(cfg: &EditorSettings) -> BarSettings {
    BarSettings {
        vertical: cfg.scrollbar_vertical,
        horizontal: cfg.scrollbar_horizontal,
        vertical_size: cfg.scrollbar_vertical_size,
        horizontal_size: cfg.scrollbar_horizontal_size,
        by_page: cfg.scrollbar_scroll_by_page,
    }
}

/// The two bars for this frame: the viewport is the list's last paint, the scroll range its layout.
pub fn inputs(
    handle: &UniformListScrollHandle,
    viewport: Size<Pixels>,
    s: &BarSettings,
) -> (BarInput, BarInput) {
    let (vw, vh) = (f32::from(viewport.width), f32::from(viewport.height));
    let vertical = BarInput {
        axis: Axis::Vertical,
        visibility: s.vertical,
        thickness: s.vertical_size,
        viewport: vh,
        content: vh + max_offset_along(handle, Axis::Vertical) as f32,
        position: offset_along(handle, Axis::Vertical) as f32,
        by_page: s.by_page,
    };
    let horizontal = BarInput {
        axis: Axis::Horizontal,
        visibility: s.horizontal,
        thickness: s.horizontal_size,
        viewport: vw,
        content: vw + max_offset_along(handle, Axis::Horizontal) as f32,
        position: offset_along(handle, Axis::Horizontal) as f32,
        by_page: s.by_page,
    };
    (vertical, horizontal)
}

/// One bar as the editor sees it this frame.
pub struct BarInput {
    pub axis: Axis,
    pub visibility: ScrollbarVisibility,
    /// The bar's thickness: its width for the vertical bar, its height for the horizontal one.
    pub thickness: f32,
    /// The viewport length along the axis.
    pub viewport: f32,
    /// The content length along the axis (viewport plus the list's scroll range).
    pub content: f32,
    /// The scroll offset along the axis, from the top or the left.
    pub position: f32,
    pub by_page: bool,
}

impl BarInput {
    /// Whether the bar is drawn: `hidden` never, `visible` always, `auto` when the content overflows.
    pub fn shown(&self) -> bool {
        match self.visibility {
            ScrollbarVisibility::Hidden => false,
            ScrollbarVisibility::Visible => true,
            ScrollbarVisibility::Auto => self.content > self.viewport,
        }
    }

    /// The slider math for this frame.
    pub fn state(&self) -> ScrollbarState {
        ScrollbarState::new(
            0.0,
            f64::from(self.thickness),
            0.0,
            f64::from(self.viewport),
            f64::from(self.content),
            f64::from(self.position),
        )
    }
}

/// The scroll offset along `axis`, from the list's handle (gpui keeps it negative).
fn offset_along(handle: &UniformListScrollHandle, axis: Axis) -> f64 {
    let state = handle.0.borrow();
    let o = state.base_handle.offset();
    f64::from(f32::from(match axis {
        Axis::Vertical => -o.y,
        Axis::Horizontal => -o.x,
    }))
}

/// The largest scroll offset along `axis`, from the list's last layout.
fn max_offset_along(handle: &UniformListScrollHandle, axis: Axis) -> f64 {
    let state = handle.0.borrow();
    let m = state.base_handle.max_offset();
    f64::from(f32::from(match axis {
        Axis::Vertical => m.y,
        Axis::Horizontal => m.x,
    }))
}

/// Moves the list to `position` along `axis`, clamped to its scroll range.
fn set_offset_along(handle: &UniformListScrollHandle, axis: Axis, position: f64) {
    let max = max_offset_along(handle, axis);
    let value = position.clamp(0.0, max.max(0.0)) as f32;
    let state = handle.0.borrow();
    let base = &state.base_handle;
    let o = base.offset();
    match axis {
        Axis::Vertical => base.set_offset(point(o.x, px(-value))),
        Axis::Horizontal => base.set_offset(point(px(-value), o.y)),
    }
}

/// The pointer's position along `axis`, relative to the track's origin.
fn along(axis: Axis, track: Bounds<Pixels>, p: Point<Pixels>) -> f64 {
    f64::from(f32::from(match axis {
        Axis::Vertical => p.y - track.origin.y,
        Axis::Horizontal => p.x - track.origin.x,
    }))
}

/// The bar's state from the list's live scroll values (a click or a drag starts from what is on screen).
fn live_state(
    handle: &UniformListScrollHandle,
    axis: Axis,
    thickness: f32,
    track_len: f64,
) -> ScrollbarState {
    ScrollbarState::new(
        0.0,
        f64::from(thickness),
        0.0,
        track_len,
        track_len + max_offset_along(handle, axis),
        offset_along(handle, axis),
    )
}

/// Draws one bar, or nothing when it is hidden (or `auto` and not needed).
pub fn bar(
    id: &'static str,
    handle: &UniformListScrollHandle,
    input: &BarInput,
    colors: SliderColors,
    shared: Rc<BarShared>,
) -> Option<AnyElement> {
    if !input.shown() {
        return None;
    }
    let axis = input.axis;
    let thickness = input.thickness;
    let by_page = input.by_page;
    let shown = input.state();
    let (slider_pos, slider_size) = (shown.slider_position() as f32, shown.slider_size() as f32);

    let slider_id = SharedString::from(format!("{id}-slider"));
    let slider = match axis {
        Axis::Vertical => div()
            .absolute()
            .left_0()
            .top(px(slider_pos))
            .w_full()
            .h(px(slider_size)),
        Axis::Horizontal => div()
            .absolute()
            .top_0()
            .left(px(slider_pos))
            .h_full()
            .w(px(slider_size)),
    }
    .id(slider_id)
    .debug_selector(move || format!("{id}-slider"))
    .bg(to_gpui(colors.background))
    .hover(move |s| s.bg(to_gpui(colors.hover)));

    // Records the track's bounds at paint, and follows the pointer anywhere in the window while the
    // slider is dragged (the listeners are registered each paint, as the minimap's are).
    let paint_shared = shared.clone();
    let paint_handle = handle.clone();
    let track_canvas = canvas(
        |bounds, _, _| bounds,
        move |_, bounds, window: &mut Window, _cx: &mut App| {
            paint_shared.track.set(bounds);
            let move_shared = paint_shared.clone();
            let move_handle = paint_handle.clone();
            window.on_mouse_event(
                move |ev: &MouseMoveEvent, phase, window: &mut Window, _cx: &mut App| {
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    let Some(drag) = move_shared.drag.get() else {
                        return;
                    };
                    if ev.pressed_button == Some(MouseButton::Left) {
                        let pointer = along(axis, move_shared.track.get(), ev.position);
                        let desired = drag
                            .initial
                            .desired_scroll_position_from_delta(pointer - drag.start);
                        set_offset_along(&move_handle, axis, desired);
                    } else {
                        move_shared.drag.set(None);
                    }
                    window.refresh();
                },
            );
            let up_shared = paint_shared.clone();
            window.on_mouse_event(
                move |_: &MouseUpEvent, phase, window: &mut Window, _cx: &mut App| {
                    if phase == DispatchPhase::Bubble && up_shared.drag.take().is_some() {
                        window.refresh();
                    }
                },
            );
        },
    )
    .absolute()
    .size_full();

    let down_handle = handle.clone();
    let down_shared = shared.clone();
    let track = match axis {
        Axis::Vertical => div().w(px(thickness)).h_full(),
        Axis::Horizontal => div().h(px(thickness)).w_full(),
    }
    .id(id)
    .debug_selector(move || id.to_string())
    .relative()
    .flex_none()
    .child(track_canvas)
    .child(slider)
    .on_mouse_down(
        MouseButton::Left,
        move |ev: &MouseDownEvent, window: &mut Window, _cx: &mut App| {
            let track = down_shared.track.get();
            let track_len = f64::from(f32::from(match axis {
                Axis::Vertical => track.size.height,
                Axis::Horizontal => track.size.width,
            }));
            let now = live_state(&down_handle, axis, thickness, track_len);
            let offset = along(axis, track, ev.position);
            let (pos, size) = (now.slider_position(), now.slider_size());
            if offset >= pos && offset <= pos + size {
                down_shared.drag.set(Some(Drag {
                    start: offset,
                    initial: now,
                }));
            } else {
                // A click on the track: a page, or a jump that centres the slider on the click.
                let desired = if by_page {
                    now.desired_scroll_position_from_offset_paged(offset)
                } else {
                    now.desired_scroll_position_from_offset(offset)
                };
                set_offset_along(&down_handle, axis, desired);
                // The slider is now where the list is, and a drag from here moves it with the pointer.
                let moved = live_state(&down_handle, axis, thickness, track_len);
                down_shared.drag.set(Some(Drag {
                    start: offset,
                    initial: moved,
                }));
            }
            window.refresh();
        },
    );
    Some(track.into_any_element())
}
