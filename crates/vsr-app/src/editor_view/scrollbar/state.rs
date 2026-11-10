//! The scrollbar's slider math, UI-free.
//!
//! upstream: src/vs/base/browser/ui/scrollbar/scrollbarState.ts @ 50e1350fc78d
//!
//! Everything the scrollbar draws or does with a click or drag comes from these numbers: the slider
//! size and position, whether the bar is needed, and the scroll position a click or a drag asks for.
//! Values are whole pixels, rounded the way `Math.round` rounds (half up), as upstream does.

/// The minimal size of the slider (such that it can still be clickable) -- it is artificially enlarged.
const MINIMUM_SLIDER_SIZE: f64 = 20.0;

/// `Math.round` (half up), so the numbers match upstream's test expectations.
fn js_round(x: f64) -> f64 {
    (x + 0.5).floor()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollbarState {
    /// For the vertical scrollbar: the width. For the horizontal scrollbar: the height.
    scrollbar_size: f64,
    /// The thickness of the other scrollbar of the pair (the corner it leaves free).
    opposite_scrollbar_size: f64,
    /// The length of the arrows at either end (rscode draws none, so this is 0 in practice).
    arrow_size: f64,
    /// The viewport length along the bar's axis.
    visible_size: f64,
    /// The content length along the bar's axis.
    scroll_size: f64,
    /// The scroll offset along the bar's axis.
    scroll_position: f64,

    computed_available_size: f64,
    computed_is_needed: bool,
    computed_slider_size: f64,
    computed_slider_ratio: f64,
    computed_slider_position: f64,
}

// The setters and getters are the upstream API that the ported tests check; the bars build a new
// state for each frame and use only the slider math.
#[cfg_attr(not(test), allow(dead_code))]
impl ScrollbarState {
    pub fn new(
        arrow_size: f64,
        scrollbar_size: f64,
        opposite_scrollbar_size: f64,
        visible_size: f64,
        scroll_size: f64,
        scroll_position: f64,
    ) -> Self {
        let mut s = Self {
            scrollbar_size: js_round(scrollbar_size),
            opposite_scrollbar_size: js_round(opposite_scrollbar_size),
            arrow_size: js_round(arrow_size),
            visible_size,
            scroll_size,
            scroll_position,
            computed_available_size: 0.0,
            computed_is_needed: false,
            computed_slider_size: 0.0,
            computed_slider_ratio: 0.0,
            computed_slider_position: 0.0,
        };
        s.refresh();
        s
    }

    /// `setScrollPosition`. Returns whether the value changed.
    pub fn set_scroll_position(&mut self, scroll_position: f64) -> bool {
        let v = js_round(scroll_position);
        if self.scroll_position != v {
            self.scroll_position = v;
            self.refresh();
            return true;
        }
        false
    }

    pub fn arrow_size(&self) -> f64 {
        self.arrow_size
    }

    pub fn scroll_position(&self) -> f64 {
        self.scroll_position
    }

    /// The length of the track (`visibleSize` - `oppositeScrollbarSize`, floored at 0).
    pub fn rectangle_large_size(&self) -> f64 {
        self.computed_available_size
    }

    /// The thickness of the bar.
    pub fn rectangle_small_size(&self) -> f64 {
        self.scrollbar_size
    }

    /// Whether the content is longer than the viewport, so the slider has something to move through.
    pub fn is_needed(&self) -> bool {
        self.computed_is_needed
    }

    pub fn slider_size(&self) -> f64 {
        self.computed_slider_size
    }

    pub fn slider_position(&self) -> f64 {
        self.computed_slider_position
    }

    /// `scrollPosition` that puts `offset` at the centre of the slider (a click on the track, jumping).
    pub fn desired_scroll_position_from_offset(&self, offset: f64) -> f64 {
        if !self.computed_is_needed {
            return 0.0;
        }
        let desired_slider_position = offset - self.arrow_size - self.computed_slider_size / 2.0;
        js_round(desired_slider_position / self.computed_slider_ratio)
    }

    /// `scrollPosition` one page up or down, depending on which side of the slider `offset` is.
    pub fn desired_scroll_position_from_offset_paged(&self, offset: f64) -> f64 {
        if !self.computed_is_needed {
            return 0.0;
        }
        let corrected_offset = offset - self.arrow_size;
        if corrected_offset < self.computed_slider_position {
            self.scroll_position - self.visible_size
        } else {
            self.scroll_position + self.visible_size
        }
    }

    /// `scrollPosition` that moves the slider by `delta` (a drag).
    pub fn desired_scroll_position_from_delta(&self, delta: f64) -> f64 {
        if !self.computed_is_needed {
            return 0.0;
        }
        let desired_slider_position = self.computed_slider_position + delta;
        js_round(desired_slider_position / self.computed_slider_ratio)
    }

    fn refresh(&mut self) {
        let opposite = self.opposite_scrollbar_size;
        let arrow = self.arrow_size;
        let visible = self.visible_size;
        let scroll = self.scroll_size;

        let available = (visible - opposite).max(0.0);
        let representable = (available - 2.0 * arrow).max(0.0);
        let needed = scroll > 0.0 && scroll > visible;

        self.computed_available_size = js_round(available);
        self.computed_is_needed = needed;
        if !needed {
            self.computed_slider_size = js_round(representable);
            self.computed_slider_ratio = 0.0;
            self.computed_slider_position = 0.0;
            return;
        }

        // The slider is enlarged to the minimum so the mouse can still grab it.
        let slider = js_round(MINIMUM_SLIDER_SIZE.max((visible * representable / scroll).floor()));
        // The slider moves from 0 to `representable - slider` as `scrollPosition` moves from 0 to
        // `scroll - visible`.
        let ratio = (representable - slider) / (scroll - visible);
        self.computed_slider_size = slider;
        self.computed_slider_ratio = ratio;
        self.computed_slider_position = js_round(self.scroll_position * ratio);
    }
}
