//! A track's regions as one buffer for the engine, with short fades so an edit never clicks.

use crate::regions::{Region, SourceId};
use std::collections::HashMap;
use std::sync::Arc;

/// Length of a fade at a free edge, and of the crossfade where two regions meet.
pub const FADE_SECONDS: f64 = 0.003;

/// Sources at the render's sample rate.
pub type RateSources = HashMap<SourceId, Arc<Vec<f32>>>;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Edge {
    /// Silence on the other side: a fade.
    Free,
    /// The next region continues the same source exactly: nothing.
    Seamless,
    /// Another region: a crossfade, this many samples on either side of the joint.
    Cross(i64),
}

/// The regions (sorted, not overlapping) as one buffer from 0:00 to the end of the last region.
/// A region whose source is missing from `sources` is silent.
pub fn render_track(regions: &[Region], sources: &RateSources, rate: u32) -> Vec<f32> {
    let rate_f = f64::from(rate);
    // Both edges are rounded on their own, so regions that meet still meet at any rate.
    let at = |seconds: f64| (seconds * rate_f).round().max(0.0) as i64;
    let bounds: Vec<(i64, i64)> = regions.iter().map(|r| (at(r.start), at(r.end()))).collect();
    let len = bounds.iter().map(|b| b.1).max().unwrap_or(0);
    let mut out = vec![0.0f32; len as usize];
    let fade = at(FADE_SECONDS).max(1);
    let meets = |i: usize| i > 0 && i < regions.len() && bounds[i - 1].1 == bounds[i].0;
    let seamless = |i: usize| {
        meets(i) && {
            let (a, b) = (&regions[i - 1], &regions[i]);
            a.source == b.source && ((a.offset + a.length - b.offset) * rate_f).abs() < 0.5
        }
    };
    // Source sample = tape sample + shift. A seamless continuation keeps the shift of the region
    // before it, so a split plays back bit for bit.
    let mut shifts: Vec<i64> = regions.iter().map(|r| ((r.offset - r.start) * rate_f).round() as i64).collect();
    for i in 1..regions.len() {
        if seamless(i) {
            shifts[i] = shifts[i - 1];
        }
    }
    // The edge in front of region `i` (and so behind region `i - 1`).
    let edge = |i: usize| {
        if seamless(i) {
            Edge::Seamless
        } else if meets(i) {
            let shortest = (bounds[i - 1].1 - bounds[i - 1].0).min(bounds[i].1 - bounds[i].0);
            Edge::Cross((fade / 2).min(shortest / 2))
        } else {
            Edge::Free
        }
    };
    for (i, region) in regions.iter().enumerate() {
        let Some(source) = sources.get(&region.source) else { continue };
        let (a, b) = bounds[i];
        if b <= a {
            continue;
        }
        let free = fade.min((b - a) / 2);
        let (edge_in, edge_out) = (edge(i), edge(i + 1));
        let lo = if let Edge::Cross(h) = edge_in { a - h } else { a };
        let hi = if let Edge::Cross(h) = edge_out { b + h } else { b };
        for pos in lo.max(0)..hi.min(len) {
            let gain_in = match edge_in {
                Edge::Seamless => 1.0,
                Edge::Free => ramp(pos - a, free),
                Edge::Cross(h) => ramp(pos - (a - h), 2 * h),
            };
            let gain_out = match edge_out {
                Edge::Seamless => 1.0,
                Edge::Free => ramp(b - 1 - pos, free),
                Edge::Cross(h) => ramp(b + h - 1 - pos, 2 * h),
            };
            let sample = usize::try_from(pos + shifts[i]).ok().and_then(|k| source.get(k)).copied().unwrap_or(0.0);
            out[pos as usize] += sample * gain_in * gain_out;
        }
    }
    out
}

/// A linear ramp from 0 to 1 over `over` samples, sampled in the middle of each step.
fn ramp(step: i64, over: i64) -> f32 {
    if over <= 0 { 1.0 } else { ((step as f32 + 0.5) / over as f32).clamp(0.0, 1.0) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(source: SourceId, start: f64, offset: f64, length: f64) -> Region {
        Region { source, start, offset, length, name: String::new() }
    }

    fn sources(list: &[(SourceId, Vec<f32>)]) -> RateSources {
        list.iter().map(|(id, s)| (*id, Arc::new(s.clone()))).collect()
    }

    #[test]
    fn nothing_renders_to_nothing() {
        assert!(render_track(&[], &HashMap::new(), 1000).is_empty());
    }

    #[test]
    fn silence_before_and_between_regions() {
        let out = render_track(&[r(1, 1.0, 0.0, 0.5), r(1, 2.0, 0.0, 0.5)], &sources(&[(1, vec![1.0; 2000])]), 1000);
        assert_eq!(out.len(), 2500);
        assert!(out[..1000].iter().chain(&out[1500..2000]).all(|s| *s == 0.0));
        assert_eq!((out[1100], out[2250]), (1.0, 1.0));
    }

    #[test]
    fn free_edges_fade() {
        let out = render_track(&[r(1, 1.0, 0.0, 0.5)], &sources(&[(1, vec![1.0; 2000])]), 1000);
        assert!(out[1000] > 0.0 && out[1000] < 0.5);
        assert!(out[1002] < 1.0);
        assert_eq!(out[1003], 1.0);
        assert!(out[1499] < 0.5);
    }

    #[test]
    fn meeting_regions_crossfade_without_a_dip() {
        let out =
            render_track(&[r(1, 1.0, 1.0, 1.0), r(2, 2.0, 1.0, 1.0)], &sources(&[(1, vec![1.0; 10_000]), (2, vec![1.0; 10_000])]), 1000);
        assert!(out[1003..=2996].iter().all(|s| (s - 1.0).abs() < 1e-6));
    }

    #[test]
    fn a_crossfade_fades_to_zero_where_a_source_ends() {
        let out =
            render_track(&[r(1, 1.0, 1.0, 1.0), r(2, 2.0, 1.0, 1.0)], &sources(&[(1, vec![1.0; 2000]), (2, vec![0.0; 10_000])]), 1000);
        assert_eq!(out[1990], 1.0);
        assert!((out[1999] - 0.75).abs() < 1e-6);
        assert!(out.iter().all(|s| *s <= 1.0));
    }

    #[test]
    fn a_split_region_plays_bit_for_bit() {
        let source = sources(&[(1, (0..5000).map(|i| i as f32 / 5000.0).collect())]);
        let whole = [r(1, 0.5, 0.25, 3.0)];
        assert_eq!(render_track(&whole, &source, 1000), render_track(&crate::regions::split(&whole, 1.7371), &source, 1000));
    }

    #[test]
    fn edges_meet_at_another_rate() {
        let source = sources(&[(1, vec![1.0; 200_000]), (2, vec![1.0; 200_000])]);
        let out = render_track(&[r(1, 0.0, 1.0, 1.0 / 3.0), r(2, 1.0 / 3.0, 1.0, 1.0)], &source, 44_100);
        assert_eq!(out.len(), (4.0 / 3.0 * 44_100.0f64).round() as usize);
        assert!(out[200..out.len() - 200].iter().all(|s| (s - 1.0).abs() < 1e-6));
    }

    #[test]
    fn a_missing_source_is_silent_but_counts() {
        assert_eq!(render_track(&[r(7, 0.0, 0.0, 1.0)], &HashMap::new(), 1000), vec![0.0; 1000]);
    }
}
