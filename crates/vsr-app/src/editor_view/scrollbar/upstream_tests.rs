//! Ports of upstream's scrollbar state tests.
//!
//! upstream: src/vs/base/test/browser/ui/scrollbar/scrollbarState.test.ts @ 50e1350fc78d

use super::state::ScrollbarState;

// upstream: ScrollbarState > inflates slider size
#[test]
fn inflates_slider_size() {
    let mut actual = ScrollbarState::new(0.0, 14.0, 0.0, 339.0, 42423.0, 32787.0);

    assert_eq!(actual.arrow_size(), 0.0);
    assert_eq!(actual.scroll_position(), 32787.0);
    assert_eq!(actual.rectangle_large_size(), 339.0);
    assert_eq!(actual.rectangle_small_size(), 14.0);
    assert!(actual.is_needed());
    assert_eq!(actual.slider_size(), 20.0);
    assert_eq!(actual.slider_position(), 249.0);

    assert_eq!(actual.desired_scroll_position_from_offset(259.0), 32849.0);

    // 259 is greater than 230 so page down, 32787 + 339 =  33126
    assert_eq!(
        actual.desired_scroll_position_from_offset_paged(259.0),
        33126.0
    );

    actual.set_scroll_position(32849.0);
    assert_eq!(actual.arrow_size(), 0.0);
    assert_eq!(actual.scroll_position(), 32849.0);
    assert_eq!(actual.rectangle_large_size(), 339.0);
    assert_eq!(actual.rectangle_small_size(), 14.0);
    assert!(actual.is_needed());
    assert_eq!(actual.slider_size(), 20.0);
    assert_eq!(actual.slider_position(), 249.0);
}

// upstream: ScrollbarState > inflates slider size with arrows
#[test]
fn inflates_slider_size_with_arrows() {
    let mut actual = ScrollbarState::new(12.0, 14.0, 0.0, 339.0, 42423.0, 32787.0);

    assert_eq!(actual.arrow_size(), 12.0);
    assert_eq!(actual.scroll_position(), 32787.0);
    assert_eq!(actual.rectangle_large_size(), 339.0);
    assert_eq!(actual.rectangle_small_size(), 14.0);
    assert!(actual.is_needed());
    assert_eq!(actual.slider_size(), 20.0);
    assert_eq!(actual.slider_position(), 230.0);

    assert_eq!(
        actual.desired_scroll_position_from_offset(240.0 + 12.0),
        32811.0
    );

    // 240 + 12 = 252; greater than 230 so page down, 32787 + 339 =  33126
    assert_eq!(
        actual.desired_scroll_position_from_offset_paged(240.0 + 12.0),
        33126.0
    );

    actual.set_scroll_position(32811.0);
    assert_eq!(actual.arrow_size(), 12.0);
    assert_eq!(actual.scroll_position(), 32811.0);
    assert_eq!(actual.rectangle_large_size(), 339.0);
    assert_eq!(actual.rectangle_small_size(), 14.0);
    assert!(actual.is_needed());
    assert_eq!(actual.slider_size(), 20.0);
    assert_eq!(actual.slider_position(), 230.0);
}
