//! Regions on the tracks: which stretch of which source plays where on the tape.
//!
//! Regions on a track are sorted by start and never overlap: whatever lands on a stretch replaces
//! what was there. The sources stay as they are, so an edge can be pulled back later. Every
//! function is pure and returns new lists, so undo can keep the old ones.

/// Identifies a source (an audio file) in the project's source table.
pub type SourceId = u64;

/// Shorter regions do not exist: an edit that would make one leaves it out.
pub const MIN_LENGTH: f64 = 0.001;

/// Whether a length counts as a region. A little slack, so a region trimmed to exactly
/// [`MIN_LENGTH`] survives the rounding of `end - (end - MIN_LENGTH)`.
fn long_enough(length: f64) -> bool {
    length >= MIN_LENGTH - 1e-9
}

/// A stretch of a source on a track. All times are in seconds.
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    pub source: SourceId,
    /// Where it starts on the tape.
    pub start: f64,
    /// Where it starts in its source.
    pub offset: f64,
    pub length: f64,
    pub name: String,
}

impl Region {
    /// Where it ends on the tape.
    pub fn end(&self) -> f64 {
        self.start + self.length
    }

    /// The part between `from` and `to` on the tape, if it is long enough.
    fn slice(&self, from: f64, to: f64) -> Option<Region> {
        let (from, to) = (from.max(self.start), to.min(self.end()));
        long_enough(to - from).then(|| Region { start: from, offset: self.offset + (from - self.start), length: to - from, ..self.clone() })
    }

    fn shifted(&self, by: f64) -> Region {
        Region { start: self.start + by, ..self.clone() }
    }
}

/// A file that fills a track from 0:00 but whose length is not known yet: an older project that
/// is opening, or "8 audiobestanden laden". It becomes a region once its source has loaded.
#[derive(Debug, Clone, PartialEq)]
pub struct Unresolved {
    pub source: SourceId,
    pub name: String,
}

/// What was cut or copied.
#[derive(Debug, Clone, PartialEq)]
pub enum Clipboard {
    /// Regions with their track; `start` counts from the earliest of them.
    Regions(Vec<(usize, Region)>),
    /// A stretch over all tracks: per track its regions, `start` counting from the stretch's start.
    Range { length: f64, tracks: Vec<Vec<Region>> },
}

/// The end of a track: the end of its last region (0 when empty).
pub fn track_end(track: &[Region]) -> f64 {
    track.iter().map(Region::end).fold(0.0, f64::max)
}

/// The track without anything between `from` and `to`; nothing moves.
pub fn clear_span(track: &[Region], from: f64, to: f64) -> Vec<Region> {
    let mut out = Vec::with_capacity(track.len() + 1);
    for region in track {
        if region.end() <= from || region.start >= to {
            out.push(region.clone());
        } else {
            out.extend(region.slice(region.start, from));
            out.extend(region.slice(to, region.end()));
        }
    }
    out
}

fn insert_sorted(track: &mut Vec<Region>, region: Region) {
    let at = track.partition_point(|r| r.start < region.start);
    track.insert(at, region);
}

/// Put `region` on the track; what it lands on is cut away.
pub fn place(track: &[Region], region: Region) -> Vec<Region> {
    if !long_enough(region.length) {
        return track.to_vec();
    }
    let mut out = clear_span(track, region.start, region.end());
    insert_sorted(&mut out, region);
    out
}

/// The track without the region at `index`.
pub fn remove(track: &[Region], index: usize) -> Vec<Region> {
    track.iter().enumerate().filter(|(i, _)| *i != index).map(|(_, r)| r.clone()).collect()
}

/// Split the region under `at` in two. On an edge, or so close that a part would be shorter
/// than [`MIN_LENGTH`], nothing changes.
pub fn split(track: &[Region], at: f64) -> Vec<Region> {
    let mut out = Vec::with_capacity(track.len() + 1);
    for region in track {
        match (region.slice(region.start, at), region.slice(at, region.end())) {
            (Some(first), Some(second)) => {
                out.push(first);
                out.push(second);
            }
            _ => out.push(region.clone()),
        }
    }
    out
}

/// Move the start of the region at `index`, no earlier than its source and 0:00; what the
/// longer region now covers is cut away.
pub fn trim_start(track: &[Region], index: usize, new_start: f64) -> Vec<Region> {
    let Some(region) = track.get(index) else { return track.to_vec() };
    let earliest = (region.start - region.offset).max(0.0);
    let new_start = new_start.max(earliest).min(region.end() - MIN_LENGTH);
    let trimmed =
        Region { start: new_start, offset: region.offset + (new_start - region.start), length: region.end() - new_start, ..region.clone() };
    place(&remove(track, index), trimmed)
}

/// Move the end of the region at `index`, no further than the end of its source
/// (`source_length` seconds long).
pub fn trim_end(track: &[Region], index: usize, new_end: f64, source_length: f64) -> Vec<Region> {
    let Some(region) = track.get(index) else { return track.to_vec() };
    let latest = region.start + (source_length - region.offset);
    let new_end = new_end.min(latest).max(region.start + MIN_LENGTH);
    let trimmed = Region { length: new_end - region.start, ..region.clone() };
    place(&remove(track, index), trimmed)
}

/// Fit the regions of `source` to its real length (`duration` seconds): a region that runs past
/// the end gets shorter, one that starts past the end is left out.
pub fn fit_to_source(track: &[Region], source: SourceId, duration: f64) -> Vec<Region> {
    track
        .iter()
        .filter_map(|r| {
            if r.source != source {
                return Some(r.clone());
            }
            let length = r.length.min(duration - r.offset);
            long_enough(length).then(|| Region { length, ..r.clone() })
        })
        .collect()
}

/// Move the selected regions (`(track, index)`) together by whole tracks and by time. They are
/// all taken off first and then placed, so they never cut each other. The group stops at track 1,
/// the last track and 0:00.
pub fn move_regions(tracks: &[Vec<Region>], selection: &[(usize, usize)], track_delta: isize, time_delta: f64) -> Vec<Vec<Region>> {
    let mut selection = selection.to_vec();
    selection.sort_unstable();
    selection.dedup();
    let picked: Vec<(usize, Region)> =
        selection.iter().filter_map(|&(t, i)| tracks.get(t).and_then(|track| track.get(i)).map(|r| (t, r.clone()))).collect();
    if picked.is_empty() {
        return tracks.to_vec();
    }
    let lowest = picked.iter().map(|(t, _)| *t as isize).min().unwrap_or(0);
    let highest = picked.iter().map(|(t, _)| *t as isize).max().unwrap_or(0);
    let track_delta = track_delta.clamp(-lowest, tracks.len() as isize - 1 - highest);
    let earliest = picked.iter().map(|(_, r)| r.start).fold(f64::INFINITY, f64::min);
    let time_delta = time_delta.max(-earliest);
    let mut out: Vec<Vec<Region>> = tracks
        .iter()
        .enumerate()
        .map(|(t, track)| {
            track.iter().enumerate().filter(|(i, _)| selection.binary_search(&(t, *i)).is_err()).map(|(_, r)| r.clone()).collect()
        })
        .collect();
    for (t, region) in picked {
        let to = (t as isize + track_delta) as usize;
        out[to] = place(&out[to], region.shifted(time_delta));
    }
    out
}

/// The selected regions as a clipboard.
pub fn copy_regions(tracks: &[Vec<Region>], selection: &[(usize, usize)]) -> Clipboard {
    let picked: Vec<(usize, Region)> =
        selection.iter().filter_map(|&(t, i)| tracks.get(t).and_then(|track| track.get(i)).map(|r| (t, r.clone()))).collect();
    let earliest = picked.iter().map(|(_, r)| r.start).fold(f64::INFINITY, f64::min);
    Clipboard::Regions(picked.into_iter().map(|(t, r)| (t, r.shifted(-earliest))).collect())
}

/// Paste regions at `at`, each on its own track; they replace what they land on.
pub fn paste_regions(tracks: &[Vec<Region>], at: f64, clipboard: &Clipboard) -> Vec<Vec<Region>> {
    let mut out = tracks.to_vec();
    if let Clipboard::Regions(items) = clipboard {
        for (t, region) in items {
            if let Some(track) = out.get_mut(*t) {
                *track = place(track, region.shifted(at));
            }
        }
    }
    out
}

/// The stretch from `from` to `to` on every track, as a clipboard.
pub fn copy_range(tracks: &[Vec<Region>], from: f64, to: f64) -> Clipboard {
    let pieces = tracks.iter().map(|track| track.iter().filter_map(|r| r.slice(from, to)).map(|r| r.shifted(-from)).collect()).collect();
    Clipboard::Range { length: (to - from).max(0.0), tracks: pieces }
}

/// A splice: the stretch from `from` to `to` comes out of every track and everything after it
/// moves forward. Returns the tracks and what was cut.
pub fn cut_range(tracks: &[Vec<Region>], from: f64, to: f64) -> (Vec<Vec<Region>>, Clipboard) {
    let clipboard = copy_range(tracks, from, to);
    if !long_enough(to - from) {
        return (tracks.to_vec(), clipboard);
    }
    let length = to - from;
    let out = tracks
        .iter()
        .map(|track| clear_span(track, from, to).into_iter().map(|r| if r.start >= from { r.shifted(-length) } else { r }).collect())
        .collect();
    (out, clipboard)
}

/// The stretch from `from` to `to` cleared on every track, without closing the gap.
pub fn clear_range(tracks: &[Vec<Region>], from: f64, to: f64) -> Vec<Vec<Region>> {
    tracks.iter().map(|track| clear_span(track, from, to)).collect()
}

/// Insert a cut or copied stretch at `at` on every track: what lies after `at` (a region over
/// `at` is split first) moves on by the stretch's length.
pub fn insert_range(tracks: &[Vec<Region>], at: f64, clipboard: &Clipboard) -> Vec<Vec<Region>> {
    let Clipboard::Range { length, tracks: pieces } = clipboard else { return tracks.to_vec() };
    tracks
        .iter()
        .enumerate()
        .map(|(t, track)| {
            let mut out: Vec<Region> = split(track, at).into_iter().map(|r| if r.start >= at { r.shifted(*length) } else { r }).collect();
            for piece in pieces.get(t).into_iter().flatten() {
                out = place(&out, piece.shifted(at));
            }
            out
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(source: SourceId, start: f64, offset: f64, length: f64) -> Region {
        Region { source, start, offset, length, name: format!("r{source}") }
    }

    /// (source, start, offset, length) per region, rounded to microseconds for comparing.
    fn shape(track: &[Region]) -> Vec<(SourceId, i64, i64, i64)> {
        let us = |s: f64| (s * 1e6).round() as i64;
        track.iter().map(|r| (r.source, us(r.start), us(r.offset), us(r.length))).collect()
    }

    const S: i64 = 1_000_000;

    #[test]
    fn place_over_the_middle_splits_what_was_there() {
        let track = place(&[r(1, 0.0, 0.0, 10.0)], r(2, 4.0, 0.0, 2.0));
        assert_eq!(shape(&track), vec![(1, 0, 0, 4 * S), (2, 4 * S, 0, 2 * S), (1, 6 * S, 6 * S, 4 * S)]);
    }

    #[test]
    fn place_over_the_start_and_the_end_shortens() {
        let track = place(&[r(1, 0.0, 0.0, 5.0), r(3, 6.0, 0.0, 4.0)], r(2, 3.0, 0.0, 5.0));
        assert_eq!(shape(&track), vec![(1, 0, 0, 3 * S), (2, 3 * S, 0, 5 * S), (3, 8 * S, 2 * S, 2 * S)]);
    }

    #[test]
    fn place_over_whole_regions_removes_them() {
        let track = place(&[r(1, 2.0, 0.0, 1.0), r(3, 4.0, 0.0, 1.0)], r(2, 1.0, 0.0, 6.0));
        assert_eq!(shape(&track), vec![(2, S, 0, 6 * S)]);
    }

    #[test]
    fn place_keeps_the_track_sorted() {
        let track = place(&place(&[], r(1, 5.0, 0.0, 1.0)), r(2, 1.0, 0.0, 1.0));
        assert_eq!(shape(&track), vec![(2, S, 0, S), (1, 5 * S, 0, S)]);
    }

    #[test]
    fn split_in_the_middle_and_on_an_edge() {
        let whole = [r(1, 0.0, 0.0, 10.0)];
        assert_eq!(shape(&split(&whole, 4.0)), vec![(1, 0, 0, 4 * S), (1, 4 * S, 4 * S, 6 * S)]);
        for at in [0.0, 10.0, 0.0005] {
            assert_eq!(split(&whole, at), whole.to_vec(), "split at {at}");
        }
    }

    #[test]
    fn trim_start_pulls_back_to_the_source_and_zero() {
        assert_eq!(shape(&trim_start(&[r(1, 3.0, 2.0, 4.0)], 0, 0.0)), vec![(1, S, 0, 6 * S)]);
        assert_eq!(shape(&trim_start(&[r(1, 1.0, 2.0, 4.0)], 0, 0.0)), vec![(1, 0, S, 5 * S)]);
        let shortest = trim_start(&[r(1, 0.0, 0.0, 10.0)], 0, 20.0);
        assert_eq!(shortest.len(), 1, "trimming to the minimum keeps the region");
        assert!((shortest[0].length - MIN_LENGTH).abs() < 1e-9 && (shortest[0].end() - 10.0).abs() < 1e-9);
        let shortest = trim_end(&[r(1, 0.0, 0.0, 10.0)], 0, -5.0, 100.0);
        assert_eq!(shortest.len(), 1);
        assert!((shortest[0].length - MIN_LENGTH).abs() < 1e-9);
    }

    #[test]
    fn trim_end_stops_at_the_end_of_the_source() {
        assert_eq!(shape(&trim_end(&[r(1, 0.0, 2.0, 3.0)], 0, 10.0, 6.0)), vec![(1, 0, 2 * S, 4 * S)]);
    }

    #[test]
    fn trimming_cuts_away_what_the_region_now_covers() {
        let track = trim_end(&[r(1, 0.0, 0.0, 4.0), r(2, 4.0, 0.0, 4.0)], 0, 6.0, 100.0);
        assert_eq!(shape(&track), vec![(1, 0, 0, 6 * S), (2, 6 * S, 2 * S, 2 * S)]);
    }

    #[test]
    fn fit_to_source_shortens_and_drops() {
        let track = fit_to_source(&[r(1, 0.0, 0.0, 10.0), r(1, 20.0, 8.0, 5.0), r(2, 30.0, 0.0, 5.0)], 1, 6.0);
        assert_eq!(shape(&track), vec![(1, 0, 0, 6 * S), (2, 30 * S, 0, 5 * S)]);
    }

    #[test]
    fn moving_regions_on_several_tracks_keeps_them_together() {
        let tracks = vec![vec![r(1, 0.0, 0.0, 2.0)], vec![r(2, 1.0, 0.0, 2.0)], vec![]];
        let moved = move_regions(&tracks, &[(0, 0), (1, 0)], 1, 3.0);
        assert!(moved[0].is_empty());
        assert_eq!(shape(&moved[1]), vec![(1, 3 * S, 0, 2 * S)]);
        assert_eq!(shape(&moved[2]), vec![(2, 4 * S, 0, 2 * S)]);
    }

    #[test]
    fn moving_stops_at_the_first_and_last_track_and_at_zero() {
        let tracks = vec![vec![], vec![], vec![r(1, 1.0, 0.0, 1.0)]];
        let moved = move_regions(&tracks, &[(2, 0)], 5, -10.0);
        assert_eq!(shape(&moved[2]), vec![(1, 0, 0, S)]);
        let moved = move_regions(&tracks, &[(2, 0)], -5, 0.0);
        assert_eq!(shape(&moved[0]), vec![(1, S, 0, S)]);
    }

    #[test]
    fn moving_regions_do_not_cut_each_other() {
        let tracks = vec![vec![r(1, 0.0, 0.0, 2.0), r(2, 2.0, 0.0, 2.0)]];
        let moved = move_regions(&tracks, &[(0, 0), (0, 1)], 0, 1.0);
        assert_eq!(shape(&moved[0]), vec![(1, S, 0, 2 * S), (2, 3 * S, 0, 2 * S)]);
    }

    #[test]
    fn copy_and_paste_regions_keep_their_spacing() {
        let tracks = vec![vec![r(1, 2.0, 0.0, 1.0)], vec![], vec![], vec![r(2, 5.0, 0.0, 1.0)]];
        let pasted = paste_regions(&tracks, 10.0, &copy_regions(&tracks, &[(0, 0), (3, 0)]));
        assert_eq!(shape(&pasted[0]), vec![(1, 2 * S, 0, S), (1, 10 * S, 0, S)]);
        assert_eq!(shape(&pasted[3]), vec![(2, 5 * S, 0, S), (2, 13 * S, 0, S)]);
    }

    #[test]
    fn a_splice_closes_the_gap_on_every_track() {
        let tracks = vec![vec![r(1, 0.0, 0.0, 10.0)], vec![r(2, 6.0, 0.0, 2.0)], vec![]];
        let (cut, clipboard) = cut_range(&tracks, 4.0, 7.0);
        assert_eq!(shape(&cut[0]), vec![(1, 0, 0, 4 * S), (1, 4 * S, 7 * S, 3 * S)]);
        assert_eq!(shape(&cut[1]), vec![(2, 4 * S, S, S)]);
        assert!(cut[2].is_empty());
        let Clipboard::Range { length, tracks: pieces } = clipboard else { panic!("a range") };
        assert!((length - 3.0).abs() < 1e-9);
        assert_eq!(shape(&pieces[0]), vec![(1, 0, 4 * S, 3 * S)]);
        assert_eq!(shape(&pieces[1]), vec![(2, 2 * S, 0, S)]);
    }

    #[test]
    fn inserting_a_stretch_moves_the_rest_on() {
        let clipboard = Clipboard::Range { length: 2.0, tracks: vec![vec![r(9, 0.0, 0.0, 2.0)]] };
        let inserted = insert_range(&[vec![r(1, 0.0, 0.0, 10.0)]], 4.0, &clipboard);
        assert_eq!(shape(&inserted[0]), vec![(1, 0, 0, 4 * S), (9, 4 * S, 0, 2 * S), (1, 6 * S, 4 * S, 6 * S)]);
    }

    #[test]
    fn cut_then_insert_at_the_same_place_restores_the_tracks() {
        let (cut, clipboard) = cut_range(&[vec![r(1, 0.0, 0.0, 10.0)]], 4.0, 7.0);
        let back = insert_range(&cut, 4.0, &clipboard);
        assert_eq!(shape(&back[0]), vec![(1, 0, 0, 4 * S), (1, 4 * S, 4 * S, 3 * S), (1, 7 * S, 7 * S, 3 * S)]);
    }

    #[test]
    fn clear_range_leaves_a_gap() {
        let cleared = clear_range(&[vec![r(1, 0.0, 0.0, 10.0)]], 4.0, 7.0);
        assert_eq!(shape(&cleared[0]), vec![(1, 0, 0, 4 * S), (1, 7 * S, 7 * S, 3 * S)]);
    }

    #[test]
    fn track_end_is_the_last_region() {
        assert_eq!(track_end(&[]), 0.0);
        assert!((track_end(&[r(1, 2.0, 0.0, 3.0)]) - 5.0).abs() < 1e-12);
    }
}
