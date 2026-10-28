//! The drawer's time axis: seconds and pixels, the beat grid, snapping, and which region lies
//! under the pointer. Pure, so it is tested without a window.

use daw388_project::regions::Region;

/// How close (in pixels) to a region's edge the pointer grabs the edge instead of the region.
pub(crate) const EDGE_GRIP: f32 = 6.0;
/// How far (in pixels) snapping reaches.
pub(crate) const SNAP_PIXELS: f32 = 8.0;
/// The widest zoom in pixels per second.
pub(crate) const ZOOM_MIN: f64 = 2.0;
/// The closest zoom in pixels per second.
pub(crate) const ZOOM_MAX: f64 = 20_000.0;

/// Where time sits on screen: x `left` shows `start` seconds, `zoom` pixels per second.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Timeline {
    /// The x of the left edge of the lanes.
    pub left: f32,
    /// The time at that edge, in seconds.
    pub start: f64,
    /// Pixels per second.
    pub zoom: f64,
}

impl Timeline {
    /// The x of a time.
    pub fn x(&self, seconds: f64) -> f32 {
        self.left + ((seconds - self.start) * self.zoom) as f32
    }

    /// The time at an x.
    pub fn seconds(&self, x: f32) -> f64 {
        self.start + f64::from(x - self.left) / self.zoom
    }

    /// The zoom that shows `length` seconds (at least 10) in `width` pixels.
    pub fn fit(length: f64, width: f32) -> f64 {
        (f64::from(width.max(1.0)) / length.max(10.0)).clamp(ZOOM_MIN, ZOOM_MAX)
    }

    /// The new start after zooming to `zoom` with the time under `x` staying put.
    pub fn zoomed_start(&self, zoom: f64, x: f32) -> f64 {
        (self.seconds(x) - f64::from(x - self.left) / zoom).max(0.0)
    }
}

/// A beat and a bar in seconds. The tempo counts quarter notes; in x/8 a beat is an eighth,
/// as the metronome does.
pub(crate) fn beat_and_bar(bpm: f64, beats: u8, unit: u8) -> (f64, f64) {
    let beat = 60.0 / bpm.max(1.0) * 4.0 / f64::from(unit.max(1));
    (beat, beat * f64::from(beats.max(1)))
}

/// The beats from `from` to `to` (bar 1 starts at 0:00): their time and their bar number when a
/// bar starts there.
pub(crate) fn beats(from: f64, to: f64, beat: f64, per_bar: u8) -> Vec<(f64, Option<u32>)> {
    let per_bar = i64::from(per_bar.max(1));
    let first = (from.max(0.0) / beat).ceil() as i64;
    let last = (to.max(0.0) / beat).floor() as i64;
    (first..=last).map(|n| (n as f64 * beat, (n % per_bar == 0).then(|| (n / per_bar + 1) as u32))).collect()
}

/// The nearest of `marks` within `reach` of `t`, else `t`.
pub(crate) fn snap(t: f64, marks: &[f64], reach: f64) -> f64 {
    marks.iter().copied().filter(|m| (m - t).abs() <= reach).min_by(|a, b| (a - t).abs().total_cmp(&(b - t).abs())).unwrap_or(t)
}

/// A shift `delta` adjusted so that the one of `edges` that is closest to a mark lands on it.
pub(crate) fn snap_shift(edges: &[f64], delta: f64, marks: &[f64], reach: f64) -> f64 {
    let mut best: Option<f64> = None;
    for edge in edges {
        let moved = edge + delta;
        let adjust = snap(moved, marks, reach) - moved;
        if adjust != 0.0 && best.is_none_or(|b| adjust.abs() < b.abs()) {
            best = Some(adjust);
        }
    }
    delta + best.unwrap_or(0.0)
}

/// Which edge of a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Edge {
    Start,
    End,
}

/// What lies under the pointer in a lane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Hit {
    /// A region, and which edge when the pointer is on one.
    Region {
        index: usize,
        edge: Option<Edge>,
    },
    Empty,
}

/// What lies under `x` on a track. A region narrower than three grips has no edges to grab.
pub(crate) fn hit(line: &Timeline, regions: &[Region], x: f32) -> Hit {
    for (index, region) in regions.iter().enumerate() {
        let (left, right) = (line.x(region.start), line.x(region.end()));
        if x < left || x > right {
            continue;
        }
        let wide = right - left > 3.0 * EDGE_GRIP;
        let edge = if wide && x - left <= EDGE_GRIP {
            Some(Edge::Start)
        } else if wide && right - x <= EDGE_GRIP {
            Some(Edge::End)
        } else {
            None
        };
        return Hit::Region { index, edge };
    }
    Hit::Empty
}

/// The lane under `y`, with lanes of `height` from `top`.
pub(crate) fn lane_at(top: f32, height: f32, lanes: usize, y: f32) -> Option<usize> {
    let lane = ((y - top) / height).floor();
    (lane >= 0.0 && (lane as usize) < lanes).then_some(lane as usize)
}

/// A step for the seconds ruler: at least `min_pixels` apart.
pub(crate) fn seconds_step(zoom: f64, min_pixels: f64) -> f64 {
    [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0].into_iter().find(|s| s * zoom >= min_pixels).unwrap_or(600.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: f64, length: f64) -> Region {
        Region { source: 1, start, offset: 0.0, length, name: String::new() }
    }

    #[test]
    fn x_and_seconds_round_trip() {
        let line = Timeline { left: 100.0, start: 2.0, zoom: 50.0 };
        assert_eq!(line.x(4.0), 200.0);
        assert_eq!(line.seconds(200.0), 4.0);
    }

    #[test]
    fn fit_shows_the_song_with_a_minimum() {
        assert_eq!(Timeline::fit(100.0, 1000.0), 10.0);
        assert_eq!(Timeline::fit(1.0, 1000.0), 100.0);
    }

    #[test]
    fn zooming_keeps_the_time_under_the_pointer() {
        let line = Timeline { left: 100.0, start: 2.0, zoom: 50.0 };
        let zoomed = Timeline { start: line.zoomed_start(100.0, 300.0), zoom: 100.0, ..line };
        assert!((zoomed.seconds(300.0) - line.seconds(300.0)).abs() < 1e-9);
    }

    #[test]
    fn beats_in_4_4_and_6_8() {
        assert_eq!(beat_and_bar(120.0, 4, 4), (0.5, 2.0));
        assert_eq!(beat_and_bar(120.0, 6, 8), (0.25, 1.5));
        let grid = beats(0.0, 2.0, 0.5, 4);
        assert_eq!(grid.len(), 5);
        assert_eq!((grid[0], grid[4]), ((0.0, Some(1)), (2.0, Some(2))));
        assert_eq!(grid[1].1, None);
    }

    #[test]
    fn snap_takes_the_nearest_mark_in_reach() {
        assert_eq!(snap(1.04, &[1.0, 1.1], 0.05), 1.0);
        assert_eq!(snap(1.5, &[1.0], 0.05), 1.5);
    }

    #[test]
    fn snap_shift_uses_the_closest_edge() {
        assert!((snap_shift(&[1.0, 3.0], 0.97, &[2.0, 4.01], 0.05) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn hit_finds_regions_and_edges() {
        let line = Timeline { left: 0.0, start: 0.0, zoom: 100.0 };
        let regions = [r(1.0, 1.0), r(3.0, 0.1)];
        assert_eq!(hit(&line, &regions, 103.0), Hit::Region { index: 0, edge: Some(Edge::Start) });
        assert_eq!(hit(&line, &regions, 150.0), Hit::Region { index: 0, edge: None });
        assert_eq!(hit(&line, &regions, 198.0), Hit::Region { index: 0, edge: Some(Edge::End) });
        assert_eq!(hit(&line, &regions, 305.0), Hit::Region { index: 1, edge: None });
        assert_eq!(hit(&line, &regions, 250.0), Hit::Empty);
    }

    #[test]
    fn lane_at_counts_from_the_top() {
        assert_eq!(lane_at(10.0, 20.0, 8, 55.0), Some(2));
        assert_eq!(lane_at(10.0, 20.0, 8, 5.0), None);
        assert_eq!(lane_at(10.0, 20.0, 8, 175.0), None);
    }

    #[test]
    fn seconds_step_grows_with_zooming_out() {
        assert_eq!(seconds_step(100.0, 60.0), 1.0);
        assert_eq!(seconds_step(1.0, 60.0), 60.0);
    }
}
