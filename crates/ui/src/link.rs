//! LINK: two tracks of a pair (1-2, 3-4, 5-6, 7-8) as one stereo channel. What they share is
//! in [`Linked`]; [`link_from`] turns one side into the other.

use crate::{DawApp, Routing, TRACKS, palette};
use daw388_engine::Command;
use daw388_project::session::{ChannelEntry, MonitorEntry};

/// The settings a linked pair shares, as one track has them. SOLO and REC FUNCTION are linked
/// too, but go through their own engine commands (see `DawApp::sync_links`); the name is not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Linked {
    pub fader_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub input: Option<u16>,
    pub channel: ChannelEntry,
    pub monitor: MonitorEntry,
}

/// The pair a track belongs to (0 for tracks 1 and 2).
pub(crate) fn pair_of(track: usize) -> usize {
    track / 2
}

/// The other track of its pair.
pub(crate) fn partner(track: usize) -> usize {
    track ^ 1
}

/// The other side of a stereo pan: `-pan`, without a negative zero in `project.json`.
pub(crate) fn mirror(pan: f32) -> f32 {
    if pan == 0.0 { 0.0 } else { -pan }
}

/// What the partner of a track with `from` gets. `left` is whether `from` is the left (odd-
/// numbered) track. The input runs on (IN 1 gives IN 2, and back); without that input
/// (`input_channels` known and too few) the partner gets none.
pub(crate) fn link_from(from: Linked, left: bool, input_channels: u16) -> Linked {
    let input = from
        .input
        .and_then(|n| if left { n.checked_add(1) } else { n.checked_sub(1) })
        .filter(|&n| n >= 1 && (input_channels == 0 || n <= input_channels));
    Linked { pan: mirror(from.pan), input, monitor: MonitorEntry { pan: mirror(from.monitor.pan), ..from.monitor }, ..from }
}

impl DawApp {
    fn linked(&self, track: usize) -> Linked {
        let t = &self.tracks[track];
        Linked { fader_db: t.fader_db, pan: t.pan, mute: t.mute, input: t.input, channel: t.channel, monitor: t.monitor }
    }

    fn input_channels(&self) -> u16 {
        self.output.as_ref().and_then(|o| o.input.as_ref()).map_or(0, |i| i.channels)
    }

    /// Give `to` what `from` has, as its partner, and tell the engine.
    fn copy_side(&mut self, from: usize, to: usize) {
        let l = link_from(self.linked(from), from.is_multiple_of(2), self.input_channels());
        let t = &mut self.tracks[to];
        (t.fader_db, t.pan, t.mute, t.input, t.channel, t.monitor) = (l.fader_db, l.pan, l.mute, l.input, l.channel, l.monitor);
        if self.tracks[to].solo != self.tracks[from].solo {
            let solo = self.tracks[from].solo;
            self.tracks[to].solo = solo;
            self.engine_send(Command::SetSolo(to, solo));
        }
        if self.tracks[to].armed != self.tracks[from].armed {
            self.toggle_arm(to);
        }
        self.send_channel(to);
        self.send_monitor(to);
    }

    /// Once a frame, after the views: a linked pair where one side changed gets that change on
    /// the other side too. So no knob needs to know about LINK.
    pub(crate) fn sync_links(&mut self) {
        let now: [Linked; TRACKS] = std::array::from_fn(|t| self.linked(t));
        if let Some(seen) = self.link_seen {
            let touched = self.link_touched;
            for pair in 0..TRACKS / 2 {
                if !self.mixer.links[pair] {
                    continue;
                }
                let (l, r) = (2 * pair, 2 * pair + 1);
                let keys = |t: usize| (self.tracks[t].solo, self.tracks[t].armed);
                // SOLO and REC are not in the snapshot: the key that was pressed says which side.
                let from = if now[l] != seen[l] {
                    Some(l)
                } else if now[r] != seen[r] {
                    Some(r)
                } else if keys(l) != keys(r) {
                    Some(if touched == Some(r) { r } else { l })
                } else {
                    None
                };
                if let Some(from) = from {
                    self.copy_side(from, partner(from));
                    self.mark_dirty();
                }
            }
        }
        self.link_touched = None;
        self.link_seen = Some(std::array::from_fn(|t| self.linked(t)));
    }

    /// The region on the linked partner that starts where `index` on `track` starts and is as
    /// long: its other side.
    pub(crate) fn counterpart(&self, track: usize, index: usize) -> Option<(usize, usize)> {
        if !self.mixer.links[pair_of(track)] {
            return None;
        }
        let region = self.tracks[track].regions.get(index)?;
        let other = partner(track);
        let at = self.tracks[other]
            .regions
            .iter()
            .position(|r| (r.start - region.start).abs() < 1e-9 && (r.length - region.length).abs() < 1e-9)?;
        Some((other, at))
    }

    /// A move or copy by `track_delta` tracks: a selection on a linked pair only goes by whole
    /// pairs onto linked pairs; otherwise it stays on its tracks.
    pub(crate) fn linked_delta(&self, tracks: &[usize], track_delta: isize) -> isize {
        let linked = |t: usize| self.mixer.links[pair_of(t)];
        if !tracks.iter().any(|&t| linked(t)) {
            return track_delta;
        }
        let fits = track_delta % 2 == 0
            && tracks.iter().all(|&t| {
                let to = t as isize + track_delta;
                (0..TRACKS as isize).contains(&to) && (!linked(t) || linked(to as usize))
            });
        if fits { track_delta } else { 0 }
    }

    /// A thin line between the numbers of a linked pair under the faders.
    pub(crate) fn paint_links(&self, ui: &egui::Ui) {
        for pair in 0..TRACKS / 2 {
            let (a, b) = (self.strip_numbers[2 * pair], self.strip_numbers[2 * pair + 1]);
            if self.mixer.links[pair] && a.is_positive() && b.is_positive() {
                let y = a.center().y;
                let stroke = egui::Stroke::new(1.5, palette::LED_GREEN.gamma_multiply(0.7));
                ui.painter().line_segment([egui::pos2(a.right() + 6.0, y), egui::pos2(b.left() - 6.0, y)], stroke);
            }
        }
    }

    /// The LINK key of a pair. On: the right side takes the left's settings; a centred pan on
    /// the left becomes hard left, so the pair is stereo (in 388 mode also in MONITOR). Off:
    /// everything stays as it is.
    pub(crate) fn set_link(&mut self, pair: usize, on: bool) {
        self.mixer.links[pair] = on;
        let l = 2 * pair;
        if on {
            if self.tracks[l].pan == 0.0 {
                self.tracks[l].pan = -1.0;
            }
            if self.mixer.mode == Routing::Classic && self.tracks[l].monitor.pan == 0.0 {
                self.tracks[l].monitor.pan = -1.0;
            }
            self.copy_side(l, l + 1);
            self.send_channel(l);
            self.send_monitor(l);
        }
        self.link_seen = None;
        self.mark_dirty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn left() -> Linked {
        Linked {
            fader_db: -3.0,
            pan: -0.6,
            mute: true,
            input: Some(1),
            channel: ChannelEntry { trim_db: 12.0, ..ChannelEntry::default() },
            monitor: MonitorEntry { gain_db: -6.0, pan: -1.0, mute: false },
        }
    }

    #[test]
    fn the_partner_gets_the_same_with_pan_mirrored() {
        let right = link_from(left(), true, 2);
        assert_eq!(right.fader_db, -3.0);
        assert!(right.mute);
        assert_eq!(right.channel, left().channel);
        assert_eq!(right.pan, 0.6);
        assert_eq!(right.monitor.pan, 1.0);
        assert_eq!(right.monitor.gain_db, -6.0);
        assert_eq!(right.input, Some(2), "IN 1 gives IN 2");
    }

    #[test]
    fn the_input_runs_back_and_stops_at_the_last_one() {
        let right = Linked { input: Some(2), ..left() };
        assert_eq!(link_from(right, false, 2).input, Some(1), "IN 2 gives IN 1");
        assert_eq!(link_from(right, true, 2).input, None, "there is no IN 3");
        assert_eq!(link_from(right, true, 0).input, Some(3), "no input device: not checked");
        let first = Linked { input: Some(1), ..left() };
        assert_eq!(link_from(first, false, 2).input, None);
    }

    #[test]
    fn a_centred_pan_stays_a_plain_zero() {
        let centre = Linked { pan: 0.0, monitor: MonitorEntry::default(), ..left() };
        let other = link_from(centre, true, 2);
        assert_eq!(other.pan.to_bits(), 0.0f32.to_bits(), "no -0.0");
        assert_eq!(other.monitor, MonitorEntry::default(), "the simple mode still sees a default monitor");
    }

    #[test]
    fn pairs_and_partners() {
        assert_eq!((pair_of(0), partner(0)), (0, 1));
        assert_eq!((pair_of(5), partner(5)), (2, 4));
        assert_eq!((pair_of(7), partner(7)), (3, 6));
    }
}
