//! Ports of upstream's overview zone manager tests.
//!
//! upstream: src/vs/editor/test/common/view/overviewZoneManager.test.ts @ 50e1350fc78d
//!
//! The DOM width the upstream tests set (30) changes no zone, so it is left out.

use super::zones::{ColorZone, OverviewRulerZone, OverviewZoneManager};

fn zones() -> Vec<OverviewRulerZone> {
    vec![
        OverviewRulerZone::new(1, 1, 0, "1"),
        OverviewRulerZone::new(10, 10, 0, "2"),
        OverviewRulerZone::new(30, 31, 0, "3"),
        OverviewRulerZone::new(50, 50, 0, "4"),
    ]
}

fn manager(dom_height: f64, pixel_ratio: f64) -> OverviewZoneManager<impl Fn(i64) -> f64> {
    const LINE_COUNT: f64 = 50.0;
    const LINE_HEIGHT: f64 = 20.0;
    let mut manager = OverviewZoneManager::new(|line: i64| LINE_HEIGHT * line as f64);
    manager.set_dom_height(dom_height);
    manager.set_outer_height(LINE_COUNT * LINE_HEIGHT);
    manager.set_line_height(LINE_HEIGHT);
    manager.set_pixel_ratio(pixel_ratio);
    manager.set_zones(zones());
    manager
}

// upstream: Editor View - OverviewZoneManager > pixel ratio 1, dom height 600
#[test]
fn pixel_ratio_1_dom_height_600() {
    // one line = 12, but cap is at 6
    assert_eq!(
        manager(600.0, 1.0).resolve_color_zones(),
        vec![
            ColorZone {
                from: 12,
                to: 24,
                color_id: 1
            },
            ColorZone {
                from: 120,
                to: 132,
                color_id: 2
            },
            ColorZone {
                from: 360,
                to: 384,
                color_id: 3
            },
            ColorZone {
                from: 588,
                to: 600,
                color_id: 4
            },
        ]
    );
}

// upstream: Editor View - OverviewZoneManager > pixel ratio 1, dom height 300
#[test]
fn pixel_ratio_1_dom_height_300() {
    // one line = 6, cap is at 6
    assert_eq!(
        manager(300.0, 1.0).resolve_color_zones(),
        vec![
            ColorZone {
                from: 6,
                to: 12,
                color_id: 1
            },
            ColorZone {
                from: 60,
                to: 66,
                color_id: 2
            },
            ColorZone {
                from: 180,
                to: 192,
                color_id: 3
            },
            ColorZone {
                from: 294,
                to: 300,
                color_id: 4
            },
        ]
    );
}

// upstream: Editor View - OverviewZoneManager > pixel ratio 2, dom height 300
#[test]
fn pixel_ratio_2_dom_height_300() {
    // one line = 6, cap is at 12
    assert_eq!(
        manager(300.0, 2.0).resolve_color_zones(),
        vec![
            ColorZone {
                from: 12,
                to: 24,
                color_id: 1
            },
            ColorZone {
                from: 120,
                to: 132,
                color_id: 2
            },
            ColorZone {
                from: 360,
                to: 384,
                color_id: 3
            },
            ColorZone {
                from: 588,
                to: 600,
                color_id: 4
            },
        ]
    );
}
