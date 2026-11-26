//! Snapshot comparison: per-channel threshold plus a maximum ratio of differing pixels
//! (Zed's visual-test numbers: tolerance 2, 99% match), a diff image on failure, and
//! `UPDATE_SNAPSHOTS=1` to rewrite the reference.

use std::fs;
use std::path::PathBuf;

use image::{Rgba, RgbaImage};

use super::snapshot_dir;

/// A pixel differs when any channel differs by more than this.
pub const CHANNEL_TOLERANCE: u8 = 2;
/// The share of differing pixels allowed before the snapshot fails.
pub const MAX_DIFF_RATIO: f64 = 0.01;

#[derive(Debug, PartialEq)]
pub enum Outcome {
    Match { ratio: f64 },
    Mismatch { ratio: f64, diff: RgbaImage },
    SizeMismatch { expected: (u32, u32), actual: (u32, u32) },
}

/// Compares two images. The diff image shows differing pixels in red over a dimmed copy of `actual`.
pub fn compare(expected: &RgbaImage, actual: &RgbaImage, tolerance: u8, max_ratio: f64) -> Outcome {
    if expected.dimensions() != actual.dimensions() {
        return Outcome::SizeMismatch { expected: expected.dimensions(), actual: actual.dimensions() };
    }
    let (w, h) = actual.dimensions();
    let mut diff = RgbaImage::new(w, h);
    let mut bad = 0u64;
    for (x, y, a) in actual.enumerate_pixels() {
        let e = expected.get_pixel(x, y);
        let differs = (0..4).any(|i| e.0[i].abs_diff(a.0[i]) > tolerance);
        if differs {
            bad += 1;
            diff.put_pixel(x, y, Rgba([255, 0, 0, 255]));
        } else {
            diff.put_pixel(x, y, Rgba([a.0[0] / 3 + 170, a.0[1] / 3 + 170, a.0[2] / 3 + 170, 255]));
        }
    }
    let ratio = bad as f64 / (w as u64 * h as u64).max(1) as f64;
    if ratio <= max_ratio { Outcome::Match { ratio } } else { Outcome::Mismatch { ratio, diff } }
}

fn reference_path(name: &str) -> PathBuf {
    snapshot_dir().join(format!("{name}.png"))
}

fn failure_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/ui-snapshots");
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// Asserts `actual` equals the reference image of `name`. With `UPDATE_SNAPSHOTS=1` (or when no
/// reference exists yet and `UPDATE_SNAPSHOTS` is set) the reference is written instead.
pub fn assert_snapshot(name: &str, actual: &RgbaImage) {
    let path = reference_path(name);
    let update = std::env::var_os("UPDATE_SNAPSHOTS").is_some_and(|v| v != "0" && !v.is_empty());
    if update {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        actual.save(&path).unwrap();
        eprintln!("ui-snapshots: wrote {}", path.display());
        return;
    }
    let expected = match image::open(&path) {
        Ok(img) => img.to_rgba8(),
        Err(e) => {
            let new = failure_dir().join(format!("{name}.actual.png"));
            actual.save(&new).unwrap();
            panic!("no reference image {} ({e}); run with UPDATE_SNAPSHOTS=1 to create it (candidate: {})", path.display(), new.display());
        }
    };
    match compare(&expected, actual, CHANNEL_TOLERANCE, MAX_DIFF_RATIO) {
        Outcome::Match { .. } => {}
        Outcome::Mismatch { ratio, diff } => {
            let dir = failure_dir();
            actual.save(dir.join(format!("{name}.actual.png"))).unwrap();
            diff.save(dir.join(format!("{name}.diff.png"))).unwrap();
            panic!("snapshot {name} differs in {:.2}% of pixels (max {:.2}%); see {}", ratio * 100., MAX_DIFF_RATIO * 100., dir.display());
        }
        Outcome::SizeMismatch { expected, actual: a } => {
            let new = failure_dir().join(format!("{name}.actual.png"));
            actual.save(&new).unwrap();
            panic!("snapshot {name}: size {a:?} but the reference is {expected:?} (candidate: {})", new.display());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(c))
    }

    #[test]
    fn identical_and_within_tolerance_match() {
        let a = solid(10, 10, [10, 20, 30, 255]);
        assert!(matches!(compare(&a, &a, 2, 0.0), Outcome::Match { ratio } if ratio == 0.0));
        let b = solid(10, 10, [12, 18, 32, 255]);
        assert!(matches!(compare(&a, &b, 2, 0.0), Outcome::Match { .. }));
    }

    #[test]
    fn beyond_tolerance_counts_pixels_and_marks_the_diff() {
        let a = solid(10, 10, [0, 0, 0, 255]);
        let mut b = a.clone();
        for x in 0..5 {
            b.put_pixel(x, 0, Rgba([0, 0, 40, 255]));
        }
        match compare(&a, &b, 2, 0.01) {
            Outcome::Mismatch { ratio, diff } => {
                assert!((ratio - 0.05).abs() < 1e-9);
                assert_eq!(diff.get_pixel(0, 0), &Rgba([255, 0, 0, 255]));
                assert_ne!(diff.get_pixel(9, 9), &Rgba([255, 0, 0, 255]));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(compare(&a, &b, 2, 0.06), Outcome::Match { .. }), "the ratio allowance applies");
    }

    #[test]
    fn different_sizes_are_reported() {
        let a = solid(2, 2, [0; 4]);
        let b = solid(3, 2, [0; 4]);
        assert_eq!(compare(&a, &b, 2, 1.0), Outcome::SizeMismatch { expected: (2, 2), actual: (3, 2) });
    }
}
