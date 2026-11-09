//! The editor minimap (ADR-0031): the document scaled down beside the editor, with a slider over
//! the visible part that you click or drag to scroll.
//!
//! upstream: src/vs/editor/browser/viewParts/minimap/minimap.ts @ 50e1350fc78d
//! upstream: src/vs/editor/browser/viewParts/minimap/minimap.css @ 50e1350fc78d
//! upstream: src/vs/platform/theme/common/colors/minimapColors.ts @ 50e1350fc78d
//!
//! `layout` is the pure math (width, line height, slider, sampling), `raster` draws the lines
//! into a pixel buffer, and this module wires them to gpui: it is rebuilt with every render of the
//! editor from the document, the scroll offset and the last painted size. Only the lines that fit
//! the minimap canvas are drawn (so a 10k-line file costs the same as a 100-line one), and the
//! pixel buffer is reused until something it shows changes.
//!
//! Not ported: section headers (`showRegionSectionHeaders`, `showMarkSectionHeaders`,
//! `markSectionHeaderRegex`, `sectionHeaderFontSize`, `sectionHeaderLetterSpacing`: they need
//! language comment tokens and `#region` folding markers), per-token colours (no tokenizer yet),
//! decorations (selection, find matches, errors), the shadow when scrolled horizontally, and
//! fade transitions.

pub mod layout;
mod prebaked;
pub mod raster;
#[cfg(test)]
mod e2e;
#[cfg(test)]
mod upstream_tests;

use std::cell::{Cell, RefCell};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use gpui_kit::{
    AnyElement, App, Bounds, Context, Corners, DispatchPhase, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, RenderImage, ScrollEvent,
    StatefulInteractiveElement, Styled, Task, UniformListScrollHandle, Window, canvas, div, point, px, size,
};
use vsr_editor::EditorOptions;

use self::layout::{
    MinimapLayout, MinimapLayoutInfo, MinimapLayoutInput, RenderMinimap, SliderOptions, Viewport,
    compute_contained_minimap_line_count, compute_minimap_layout, sampling_lines,
};
use self::raster::{RasterParams, render_lines};
use crate::config::minimap::{MinimapAutohide, MinimapSide, MinimapSize, ShowSlider};
use crate::config::settings::EditorSettings;
use crate::model::OpenEditor;
use crate::theme::{Rgba8, Theme};
use crate::ui::to_gpui;

use super::EditorHost;

/// How long the slider (`showSlider: mouseover`) and the minimap (`autohide: scroll`) stay visible
/// after the editor scrolled (`_hideSliderAfterScrollScheduler` / `_hideDelayedScheduler`).
const SCROLL_VISIBLE: Duration = Duration::from_millis(500);

/// `minimapSlider.*` colours: `scrollbarSlider.*` at half their alpha (minimapColors.ts).
struct SliderColors {
    background: Rgba8,
    hover: Rgba8,
    active: Rgba8,
}

impl SliderColors {
    fn for_background(bg: Rgba8) -> Self {
        if luminance(bg) > 0.5 {
            Self { background: Rgba8(0x64, 0x64, 0x64, 0x33), hover: Rgba8(0x64, 0x64, 0x64, 0x59), active: Rgba8(0, 0, 0, 0x4D) }
        } else {
            Self { background: Rgba8(0x79, 0x79, 0x79, 0x33), hover: Rgba8(0x64, 0x64, 0x64, 0x59), active: Rgba8(0xBF, 0xBF, 0xBF, 0x33) }
        }
    }
}

/// Relative luminance, like `Color.getRelativeLuminance`.
fn luminance(c: Rgba8) -> f64 {
    let lin = |v: u8| {
        let s = f64::from(v) / 255.0;
        if s <= 0.03928 { s / 12.92 } else { ((s + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * lin(c.0) + 0.7152 * lin(c.1) + 0.0722 * lin(c.2)
}

/// What the last build decided; tests and event handlers read it.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub info: MinimapLayoutInfo,
    pub layout: MinimapLayout,
    pub side: MinimapSide,
    pub size: MinimapSize,
    /// Editor width the layout was computed for (text area plus minimap).
    pub outer_width: f32,
    /// Editor height (the viewport).
    pub outer_height: f32,
    pub scroll_top: f32,
    pub pixel_ratio: f64,
    pub line_count: usize,
    pub line_height: f32,
    /// `editor.minimap.autohide` / `showSlider` resolved to what is on screen now.
    pub minimap_visible: bool,
    pub slider_visible: bool,
}

#[derive(Clone, Copy)]
struct Drag {
    initial_y: f32,
    layout: MinimapLayout,
}

/// State the event handlers share with the build (they run outside the editor's `&mut self`).
struct Shared {
    hover: Cell<bool>,
    drag: Cell<Option<Drag>>,
    pixel_ratio: Cell<f32>,
    /// The painted minimap canvas, in window coordinates.
    bounds: Cell<Bounds<Pixels>>,
    live: RefCell<Option<Live>>,
    /// Images that are no longer shown; dropped from the atlas at the next paint.
    retired: RefCell<Vec<Arc<RenderImage>>>,
}

struct Live {
    frame: Frame,
    scroll: UniformListScrollHandle,
}

impl Live {
    fn max_scroll(&self) -> f32 {
        (self.frame.line_count as f32 * self.frame.line_height - self.frame.outer_height).max(0.0)
    }

    fn set_scroll_top(&self, top: f32) {
        let top = top.clamp(0.0, self.max_scroll());
        let state = self.scroll.0.borrow();
        let x = state.base_handle.offset().x;
        state.base_handle.set_offset(point(x, px(-top)));
    }
}

impl Shared {
    fn start_drag(&self, initial_y: f32) {
        if let Some(live) = self.live.borrow().as_ref() {
            self.drag.set(Some(Drag { initial_y, layout: live.frame.layout }));
        }
    }

    /// `handlePointerMove`: the slider follows the pointer by the same distance.
    fn drag_to(&self, y: f32) {
        let (Some(d), live) = (self.drag.get(), self.live.borrow()) else { return };
        let Some(live) = live.as_ref() else { return };
        live.set_scroll_top(d.layout.desired_scroll_top_from_delta(f64::from(y - d.initial_y)) as f32);
    }

    /// `_pointerDown` on the minimap (not on the slider).
    fn pointer_down(&self, y: f32) {
        let origin = f32::from(self.bounds.get().origin.y);
        let slider_center = {
            let live = self.live.borrow();
            let Some(live) = live.as_ref() else { return };
            let f = &live.frame;
            if f.size == MinimapSize::Proportional {
                let local = f64::from(y - origin);
                let line = f
                    .layout
                    .line_at_y(local, f.info.canvas_inner_height, f.info.canvas_outer_height, f.info.line_height, f.line_count as f64)
                    .max(1.0);
                // Reveal the line in the centre of the editor.
                let lh = f.line_height;
                live.set_scroll_top((line as f32 - 1.0) * lh + lh / 2.0 - f.outer_height / 2.0);
                return;
            }
            origin + f.layout.slider_top as f32 + f.layout.slider_height as f32 / 2.0
        };
        // fill / fit: pretend the click happened in the centre of the slider and drag from there.
        self.start_drag(slider_center);
        self.drag_to(y);
    }
}

/// The minimap's part of `EditorView`.
pub struct MinimapView {
    shared: Rc<Shared>,
    image: RefCell<Option<(u64, Arc<RenderImage>)>>,
    frame: Option<Frame>,
    /// Width of the minimap in the last build, so the editor's full width can be recovered from
    /// the (narrower) text area.
    last_width: f32,
    seen_scroll_top: f32,
    /// The editor scrolled within `SCROLL_VISIBLE` (cleared by a timer).
    scrolled: bool,
    was_visible: bool,
    hide_task: Option<Task<()>>,
}

impl Default for MinimapView {
    fn default() -> Self {
        Self {
            shared: Rc::new(Shared {
                hover: Cell::new(false),
                drag: Cell::new(None),
                pixel_ratio: Cell::new(1.0),
                bounds: Cell::new(Bounds::default()),
                live: RefCell::new(None),
                retired: RefCell::new(Vec::new()),
            }),
            image: RefCell::new(None),
            frame: None,
            last_width: 0.0,
            seen_scroll_top: 0.0,
            scrolled: false,
            was_visible: true,
            hide_task: None,
        }
    }
}

/// The minimap element(s) and the side they go on.
pub struct Built {
    pub left: Option<AnyElement>,
    pub right: Option<AnyElement>,
    /// A zero-sized element that asks for another frame when the layout used stale sizes.
    pub check: Option<AnyElement>,
}

/// What a build reads from the editor.
pub struct BuildInput<'a> {
    pub editor: &'a OpenEditor,
    pub theme: &'a Theme,
    pub cfg: &'a EditorSettings,
    pub scroll: &'a UniformListScrollHandle,
    /// Width of line numbers and decorations left of the text.
    pub gutter: f32,
    /// The editor text area as last painted (empty before the first paint).
    pub text_bounds: &'a Rc<Cell<Bounds<Pixels>>>,
}

impl MinimapView {
    /// The last frame built (`None` when the minimap is off).
    #[cfg(test)]
    pub fn frame(&self) -> Option<&Frame> {
        self.frame.as_ref()
    }

    pub fn build<T: EditorHost>(&mut self, i: BuildInput, cx: &mut Context<T>) -> Built {
        let none = Built { left: None, right: None, check: None };
        let m = i.cfg.minimap;
        if !m.enabled {
            self.last_width = 0.0;
            self.frame = None;
            *self.shared.live.borrow_mut() = None;
            return none;
        }
        let pr = f64::from(self.shared.pixel_ratio.get());
        let lh = i.cfg.line_height;
        let buf = i.editor.buffer();
        let line_count = buf.line_count();

        // Sizes come from the last paint; the first frame guesses and the check below asks again.
        let tb = i.text_bounds.get();
        let (tw, th) = (f32::from(tb.size.width), f32::from(tb.size.height));
        let known = tw > 0.0 && th > 0.0;
        let (outer_w, outer_h) = if known { (tw + self.last_width, th) } else { (1000.0, 700.0) };

        let mut info = compute_minimap_layout(&MinimapLayoutInput {
            outer_width: f64::from(outer_w),
            outer_height: f64::from(outer_h),
            line_height: f64::from(lh),
            typical_halfwidth_char_width: f64::from(i.cfg.char_width()),
            pixel_ratio: pr,
            scroll_beyond_last_line: false,
            padding_top: 0.0,
            padding_bottom: 0.0,
            minimap: &m,
            vertical_scrollbar_width: 0.0,
            view_line_count: line_count as f64,
            remaining_width: f64::from(outer_w - i.gutter),
        });
        let sampling = info.is_sampling.then(|| {
            let c = compute_contained_minimap_line_count(
                line_count as f64, false, 0.0, 0.0, f64::from(outer_h), f64::from(lh), pr,
            );
            sampling_lines(c.minimap_line_count.max(0.0) as usize, line_count)
        });
        // The minimap cannot be wider than the editor leaves room for.
        info.width = info.width.min(f64::from(outer_w)).max(0.0);

        // The viewport, from the scroll handle (the list scrolls it).
        let offset_y = -f32::from(i.scroll.0.borrow().base_handle.offset().y);
        let scroll_top = offset_y.max(0.0);
        let scroll_height = (line_count as f32 * lh).max(outer_h);
        let start = ((scroll_top / lh).floor() as usize + 1).min(line_count);
        let end = (((scroll_top + outer_h) / lh).ceil() as usize).clamp(start, line_count);
        let viewport = Viewport {
            start_line: start as f64,
            end_line: end as f64,
            start_line_vertical_offset: f64::from((start - 1) as f32 * lh),
            start_line_height: f64::from(lh),
            height: f64::from(outer_h),
            contains_whitespace_gaps: false,
            line_count: sampling.as_ref().map_or(line_count, Vec::len) as f64,
            real_line_count: line_count as f64,
            scroll_top: f64::from(scroll_top),
            scroll_height: f64::from(scroll_height),
        };
        let slider_options = SliderOptions {
            pixel_ratio: pr,
            minimap_line_height: info.line_height,
            canvas_inner_height: info.canvas_inner_height,
            line_height: f64::from(lh),
            minimap_height_is_editor_height: info.height_is_editor_height,
            minimap_height: f64::from(outer_h),
            padding_top: 0.0,
            padding_bottom: 0.0,
            scroll_beyond_last_line: false,
        };
        let previous = self.frame.as_ref().map(|f| f.layout);
        let layout = MinimapLayout::create(&slider_options, &viewport, previous.as_ref());

        // Visibility (autohide, showSlider).
        if (scroll_top - self.seen_scroll_top).abs() > 0.5 {
            self.seen_scroll_top = scroll_top;
            self.scrolled = true;
            self.hide_task = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(SCROLL_VISIBLE).await;
                this.update(cx, |this, cx| {
                    this.editor_host().1.minimap.scrolled = false;
                    cx.notify();
                })
                .ok();
            }));
        }
        let scrolled_recently = self.scrolled;
        let hover = self.shared.hover.get();
        let dragging = self.shared.drag.get().is_some();
        let minimap_visible = match m.autohide {
            MinimapAutohide::None => true,
            MinimapAutohide::Mouseover => hover || dragging,
            MinimapAutohide::Scroll => scrolled_recently || ((hover || dragging) && self.was_visible),
        };
        self.was_visible = minimap_visible;
        let slider_visible = match m.show_slider {
            ShowSlider::Always => true,
            ShowSlider::Mouseover => hover || dragging || scrolled_recently,
        };

        let frame = Frame {
            info,
            layout,
            side: m.side,
            size: m.size,
            outer_width: outer_w,
            outer_height: outer_h,
            scroll_top,
            pixel_ratio: pr,
            line_count,
            line_height: lh,
            minimap_visible,
            slider_visible,
        };
        self.last_width = info.width as f32;
        self.frame = Some(frame.clone());
        *self.shared.live.borrow_mut() = Some(Live { frame: frame.clone(), scroll: i.scroll.clone() });

        // Pixels: redrawn only when something they show changed.
        let bg = i.theme.editor_bg;
        let fg = i.theme.editor_fg;
        let params = RasterParams {
            render: info.render,
            font_scale: info.scale as usize,
            line_height: info.line_height as usize,
            char_width: info.scale as usize,
            canvas_width: info.canvas_inner_width as usize,
            canvas_height: info.canvas_inner_height as usize,
            tab_size: tab_size(i.editor.editor.options()),
            fg: [fg.0, fg.1, fg.2],
            fg_alpha: 255,
            bg: [bg.0, bg.1, bg.2],
            lighter_font: luminance(bg) > 0.5,
            top_padding_lines: layout.top_padding_line_count as usize,
        };
        let key = {
            let mut h = DefaultHasher::new();
            (i.editor.path.as_path(), i.editor.editor.version()).hash(&mut h);
            (layout.start_line.to_bits(), layout.end_line.to_bits(), line_count).hash(&mut h);
            (params.render == RenderMinimap::Text, params.font_scale, params.line_height, params.char_width).hash(&mut h);
            (params.canvas_width, params.canvas_height, params.tab_size, params.top_padding_lines).hash(&mut h);
            (params.fg, params.bg, sampling.as_ref().map(Vec::len)).hash(&mut h);
            h.finish()
        };
        let cached = self.image.borrow().as_ref().filter(|(k, _)| *k == key).map(|(_, img)| img.clone());
        let image = cached.unwrap_or_else(|| {
            let (first, last) = (layout.start_line as usize, layout.end_line as usize);
            let lines = (first..=last.max(first)).filter(|_| last >= first).map(|n| {
                let model_line = sampling.as_ref().map_or(n, |s| s.get(n - 1).copied().unwrap_or(line_count));
                buf.line_content(model_line.clamp(1, line_count))
            });
            let canvas = render_lines(&params, lines);
            let frame = image::Frame::new(
                image::RgbaImage::from_raw(canvas.width as u32, canvas.height as u32, canvas.data)
                    .unwrap_or_else(|| image::RgbaImage::new(1, 1)),
            );
            let img = Arc::new(RenderImage::new(vec![frame]));
            if let Some((_, old)) = self.image.borrow_mut().replace((key, img.clone())) {
                self.shared.retired.borrow_mut().push(old);
            }
            img
        });

        let element = self.element(&frame, &i, image, outer_w, outer_h);
        let check = self.check_element(i.text_bounds, i.scroll.clone(), info.width as f32, outer_w, outer_h, scroll_top, known);
        let (left, right) = match m.side {
            MinimapSide::Left => (Some(element), None),
            MinimapSide::Right => (None, Some(element)),
        };
        Built { left, right, check: Some(check) }
    }

    /// A zero-sized element that, once the frame is laid out, asks for another one when the build
    /// assumed other sizes than the ones painted (first frame, resize, scrolled by `scroll_to_item`,
    /// pixel ratio).
    #[allow(clippy::too_many_arguments)]
    fn check_element(
        &self,
        text_bounds: &Rc<Cell<Bounds<Pixels>>>,
        scroll: UniformListScrollHandle,
        minimap_width: f32,
        used_w: f32,
        used_h: f32,
        used_top: f32,
        used_known: bool,
    ) -> AnyElement {
        let shared = self.shared.clone();
        let text_bounds = text_bounds.clone();
        let used_pr = shared.pixel_ratio.get();
        canvas(
            |bounds, _, _| bounds,
            move |_, _, window, cx| {
                let pr = window.scale_factor();
                let tb = text_bounds.get();
                let (tw, th) = (f32::from(tb.size.width), f32::from(tb.size.height));
                let top = (-f32::from(scroll.0.borrow().base_handle.offset().y)).max(0.0);
                let sized = tw > 0.0 && th > 0.0;
                let stale = (pr - used_pr).abs() > 0.01
                    || (sized && (!used_known || (tw + minimap_width - used_w).abs() > 0.5 || (th - used_h).abs() > 0.5))
                    || (top - used_top).abs() > 0.5;
                if stale {
                    shared.pixel_ratio.set(pr);
                    let view = window.current_view();
                    window.defer(cx, move |_, cx| cx.notify(view));
                }
            },
        )
        .absolute()
        .size_0()
        .into_any_element()
    }

    fn element(&self, f: &Frame, i: &BuildInput, image: Arc<RenderImage>, _outer_w: f32, _outer_h: f32) -> AnyElement {
        let shared = self.shared.clone();
        let colors = SliderColors::for_background(i.theme.editor_bg);
        let info = f.info;
        let width = info.width as f32;
        let canvas_size = size(px(info.canvas_outer_width as f32), px(info.canvas_outer_height as f32));
        let interactive = f.minimap_visible || i.cfg.minimap.autohide != MinimapAutohide::Scroll;

        let paint_shared = shared.clone();
        let canvas_el = canvas(
            |bounds, _, _| bounds,
            move |_, bounds, window, _cx| {
                paint_shared.bounds.set(bounds);
                paint_shared.pixel_ratio.set(window.scale_factor());
                for old in paint_shared.retired.borrow_mut().drain(..) {
                    window.drop_image(old).ok();
                }
                let _ = window.paint_image(bounds, bounds, Corners::default(), image.clone(), 0, false);
                // While dragging the slider, follow the pointer anywhere in the window.
                let s = paint_shared.clone();
                window.on_mouse_event(move |ev: &MouseMoveEvent, phase, window: &mut Window, _cx: &mut App| {
                    if phase == DispatchPhase::Bubble && s.drag.get().is_some() {
                        if ev.pressed_button == Some(MouseButton::Left) {
                            s.drag_to(f32::from(ev.position.y));
                        } else {
                            s.drag.set(None);
                        }
                        window.refresh();
                    }
                });
                let s = paint_shared.clone();
                window.on_mouse_event(move |_: &MouseUpEvent, phase, window: &mut Window, _cx: &mut App| {
                    if phase == DispatchPhase::Bubble && s.drag.take().is_some() {
                        window.refresh();
                    }
                });
            },
        )
        .absolute()
        .top_0()
        .left_0()
        .w(canvas_size.width)
        .h(canvas_size.height);

        let mut root = div()
            .id("minimap")
            .debug_selector(|| "minimap".to_string())
            .relative()
            .flex_none()
            .w(px(width))
            .h_full()
            .overflow_hidden()
            .bg(to_gpui(i.theme.editor_bg))
            .opacity(if f.minimap_visible { 1.0 } else { 0.0 })
            .child(canvas_el);

        if interactive {
            let s = shared.clone();
            let h = shared.clone();
            let w = shared.clone();
            root = root
                .on_hover(move |hovered, window, _| {
                    h.hover.set(*hovered);
                    window.refresh();
                })
                .on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, _| {
                    s.pointer_down(f32::from(ev.position.y));
                    window.refresh();
                })
                .on_scroll(move |ev: &ScrollEvent, window, _| {
                    let live = w.live.borrow();
                    if let Some(live) = live.as_ref() {
                        let delta = ev.delta.pixel_delta(px(live.frame.line_height));
                        live.set_scroll_top(live.scroll_top_now() - f32::from(delta.y));
                        window.refresh();
                    }
                });
        }

        if f.layout.slider_needed && f.layout.slider_height > 0.0 {
            let dragging = shared.drag.get().is_some();
            let s = shared.clone();
            let mut slider = div()
                .id("minimap-slider")
                .debug_selector(|| "minimap-slider".to_string())
                .absolute()
                .left_0()
                .top(px(f.layout.slider_top as f32))
                .w(px(width))
                .h(px(f.layout.slider_height as f32))
                .bg(to_gpui(if dragging { colors.active } else { colors.background }))
                .opacity(if f.slider_visible { 1.0 } else { 0.0 })
                .hover(move |st| st.bg(to_gpui(colors.hover)));
            if interactive {
                slider = slider.on_mouse_down(MouseButton::Left, move |ev: &MouseDownEvent, window, cx| {
                    cx.stop_propagation();
                    s.start_drag(f32::from(ev.position.y));
                    window.refresh();
                });
            }
            root = root.child(slider);
        }
        root.into_any_element()
    }
}

impl Live {
    fn scroll_top_now(&self) -> f32 {
        (-f32::from(self.scroll.0.borrow().base_handle.offset().y)).max(0.0)
    }
}

fn tab_size(o: EditorOptions) -> usize {
    o.tab_size.max(1)
}
