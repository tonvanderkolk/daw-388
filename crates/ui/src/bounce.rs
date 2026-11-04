//! Bounce Setup: the 388's routing for a live mix of some tracks onto one track or a pair, set
//! in one go. It only sets keys the console already has; recording stays REC and PLAY.

use super::*;
use crate::link::pair_of;

/// Where the bounce goes: one track (0-based) or a pair (0 for tracks 1 and 2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Target {
    Track(usize),
    Pair(usize),
}

impl Target {
    /// The tracks that record.
    pub(crate) fn tracks(self) -> Vec<usize> {
        match self {
            Target::Track(t) => vec![t],
            Target::Pair(p) => vec![2 * p, 2 * p + 1],
        }
    }

    /// The ASSIGN key that feeds the target's buses: 0 for 1-2, up to 3 for 7-8.
    pub(crate) fn assign(self) -> usize {
        match self {
            Target::Track(t) => t / 2,
            Target::Pair(p) => p,
        }
    }
}

/// The two check boxes of the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct BounceOptions {
    /// Mute the sources in MONITOR, so the mix is heard once, through the target.
    pub silence_sources: bool,
    /// Switch LINK on for a target pair.
    pub link_target: bool,
    /// TO PGM onto the target, so the plate and the echo are recorded too.
    pub include_effects: bool,
}

impl Default for BounceOptions {
    fn default() -> Self {
        Self { silence_sources: true, link_target: true, include_effects: true }
    }
}

/// What the window holds while it is open.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct BounceDraft {
    pub sources: [bool; TRACKS],
    pub target: Option<Target>,
    pub options: BounceOptions,
    /// The target is one track rather than a pair.
    pub mono: bool,
}

/// Track numbers as people say them: "1 to 6", "1, 2 and 4" (`tracks` 0-based and sorted).
pub(crate) fn track_list(tracks: &[usize]) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut i = 0;
    while i < tracks.len() {
        let mut j = i;
        while j + 1 < tracks.len() && tracks[j + 1] == tracks[j] + 1 {
            j += 1;
        }
        if j - i >= 2 {
            parts.push(format!("{} to {}", tracks[i] + 1, tracks[j] + 1));
        } else {
            parts.extend(tracks[i..=j].iter().map(|t| (t + 1).to_string()));
        }
        i = j + 1;
    }
    match parts.split_last() {
        Some((last, rest)) if !rest.is_empty() => format!("{} and {last}", rest.join(", ")),
        _ => parts.concat(),
    }
}

/// One line that says what Apply will set up, such as "Tracks 1 to 6 onto 7 and 8 (stereo)".
pub(crate) fn bounce_summary(sources: &[bool; TRACKS], target: Target) -> String {
    let from: Vec<usize> = (0..TRACKS).filter(|t| sources[*t] && !target.tracks().contains(t)).collect();
    let word = if from.len() == 1 { "Track" } else { "Tracks" };
    let onto = match target {
        Target::Track(t) => format!("track {} (mono)", t + 1),
        Target::Pair(p) => format!("{} and {} (stereo)", 2 * p + 1, 2 * p + 2),
    };
    format!("{word} {} onto {onto}", track_list(&from))
}

/// The sources after a click: a linked pair follows the side that changed, and the target's
/// tracks are never sources.
pub(crate) fn choose_sources(before: [bool; TRACKS], mut now: [bool; TRACKS], links: [bool; 4], target: Option<Target>) -> [bool; TRACKS] {
    for (pair, linked) in links.into_iter().enumerate() {
        let (l, r) = (2 * pair, 2 * pair + 1);
        if linked && now[l] != now[r] {
            let value = if now[l] != before[l] { now[l] } else { now[r] };
            (now[l], now[r]) = (value, value);
        }
    }
    for t in target.map(Target::tracks).unwrap_or_default() {
        now[t] = false;
    }
    now
}

impl DawApp {
    /// Set the console for the bounce: 388 mode, the sources on RMX into the target's buses,
    /// the target armed and heard in MONITOR. Every channel comes off L-R; the rest of the other tracks stays as it is.
    pub(crate) fn bounce_setup(&mut self, sources: [bool; TRACKS], target: Target, options: BounceOptions) {
        self.set_routing(Routing::Classic);
        let to = target.tracks();
        let key = target.assign();
        // A mono target on a linked pair: arming one side would arm the other.
        if let Target::Track(t) = target
            && self.mixer.links[pair_of(t)]
        {
            self.set_link(pair_of(t), false);
        }
        // While one channel is on L-R, MONITOR does not reach STEREO: the bounce is heard
        // through MONITOR, so every channel comes off L-R (the other tracks are heard there too).
        for t in 0..TRACKS {
            if self.tracks[t].channel.lr {
                self.tracks[t].channel.lr = false;
                self.send_channel(t);
            }
        }
        for t in (0..TRACKS).filter(|t| sources[*t] && !to.contains(t)) {
            let track = &mut self.tracks[t];
            track.channel.source = ChannelSource::Rmx;
            track.channel.assign = std::array::from_fn(|k| k == key);
            track.channel.lr = false;
            track.channel.direct = false;
            if let Target::Track(n) = target {
                // The odd-numbered bus (0-based even) is left.
                track.pan = if n.is_multiple_of(2) { -1.0 } else { 1.0 };
            }
            if options.silence_sources {
                track.monitor.mute = true;
            }
            if self.tracks[t].armed {
                self.toggle_arm(t);
            }
            self.send_channel(t);
            self.send_monitor(t);
        }
        for &t in &to {
            let track = &mut self.tracks[t];
            track.channel.assign = [false; 4];
            track.channel.lr = false;
            track.channel.direct = false;
            track.monitor.mute = false;
            track.monitor.pan = match target {
                Target::Pair(_) if t.is_multiple_of(2) => -1.0,
                Target::Pair(_) => 1.0,
                Target::Track(_) => 0.0,
            };
            if !self.tracks[t].armed {
                self.toggle_arm(t);
            }
            self.send_channel(t);
            self.send_monitor(t);
        }
        // The returns onto the target's buses, or back to the stereo bus. Onto one track both
        // returns pan to its side, as the sources do.
        self.mixer.effects.to_pgm = options.include_effects.then_some(key);
        if options.include_effects
            && let Target::Track(n) = target
        {
            let side = if n.is_multiple_of(2) { -1.0 } else { 1.0 };
            for rtn in &mut self.mixer.effects.rtn {
                rtn.pan = side;
            }
        }
        self.send_mixer();
        if let Target::Pair(p) = target
            && options.link_target
            && !self.mixer.links[p]
        {
            self.set_link(p, true);
        }
        // What was set here is the new state of every linked pair, not a change to copy over:
        // a linked source pair onto one track has both pans on the same side on purpose.
        self.link_seen = None;
        self.link_touched = None;
        self.mark_dirty();
        self.status = "Bounce ready: press REC and PLAY".into();
    }

    /// What the window should say before Apply: audio on the target, a closed PGM master, the
    /// switch to 388.
    pub(crate) fn bounce_notes(&self, target: Target, effects: bool) -> Vec<String> {
        let to = target.tracks();
        let mut notes = Vec::new();
        for &t in &to {
            if !self.tracks[t].regions.is_empty() {
                notes.push(format!("Track {} has audio; recording replaces it where you record.", t + 1));
            }
            let (name, db) = if t.is_multiple_of(2) { ("ODD", self.mixer.odd_db) } else { ("EVEN", self.mixer.even_db) };
            if db <= MIN_DB {
                notes.push(format!("PGM MASTER {name} is closed: track {} records silence.", t + 1));
            }
        }
        if effects && self.mixer.effects.rtn.iter().all(|r| r.gain_db <= MIN_DB) {
            notes.push("EFFECT RTN 1 and 2 are closed: no reverb or echo on the bounce.".to_string());
        }
        if self.mixer.mode == Routing::Simple {
            notes.push("Apply switches the console to 388.".to_string());
        }
        notes
    }

    pub(crate) fn open_bounce(&mut self) {
        self.bounce = Some(BounceDraft::default());
    }

    /// Apply: only with a source and a target; then the window closes.
    pub(crate) fn apply_bounce(&mut self) {
        let Some(draft) = self.bounce.clone() else { return };
        let Some(target) = draft.target else { return };
        if !draft.sources.iter().any(|s| *s) {
            return;
        }
        self.bounce = None;
        self.bounce_setup(draft.sources, target, draft.options);
    }

    pub(crate) fn bounce_window(&mut self, ctx: &egui::Context) {
        use skin::paper::{self, check, note, plain_button, plain_cards, primary_button, section_title, switch_pair};
        let Some(mut draft) = self.bounce.clone() else { return };
        // Only tracks with audio can be mixed; an empty one shows a dash, as on the strip.
        let filled: [bool; TRACKS] = std::array::from_fn(|i| !self.tracks[i].regions.is_empty() || self.tracks[i].unresolved.is_some());
        let names: [String; TRACKS] = std::array::from_fn(|i| if filled[i] { self.tracks[i].name.clone() } else { "–".to_string() });
        let excluded = draft.target.map(Target::tracks).unwrap_or_default();
        let notes = draft.target.map(|t| self.bounce_notes(t, draft.options.include_effects)).unwrap_or_default();
        let (mut open, mut apply, mut cancel) = (true, false, false);
        let before = draft.sources;
        paper::paper_window(ctx, "bounce-window", "Bounce Setup", "", &mut open, 400.0, |ui| {
            note(
                ui,
                "Mix tracks live onto one track or a stereo pair. Apply sets the routing; then press REC and PLAY and mix with the faders.",
            );
            section_title(ui, "Mix these tracks");
            egui::Grid::new("bounce-sources").num_columns(2).spacing([24.0, 4.0]).show(ui, |ui| {
                for (index, name) in names.iter().enumerate() {
                    ui.add_enabled_ui(filled[index] && !excluded.contains(&index), |ui| {
                        let label = format!("{:02}  {}", index + 1, shorten(name, 18));
                        check(ui, &mut draft.sources[index], &label).on_hover_text(name);
                    });
                    if index % 2 == 1 {
                        ui.end_row();
                    }
                }
            });
            section_title(ui, "Onto");
            let mut mono = draft.mono;
            if switch_pair(ui, [(false, "A stereo pair"), (true, "One track")], &mut mono, 240.0) {
                draft.mono = mono;
                draft.target = None;
            }
            ui.add_space(6.0);
            if draft.mono {
                let numbers: Vec<String> = (1..=TRACKS).map(|n| n.to_string()).collect();
                let tracks: Vec<(Option<Target>, &str, Option<&str>)> =
                    numbers.iter().enumerate().map(|(t, n)| (Some(Target::Track(t)), n.as_str(), None)).collect();
                plain_cards(ui, &tracks, &mut draft.target);
            } else {
                let pairs = [
                    (Some(Target::Pair(0)), "1 + 2", None),
                    (Some(Target::Pair(1)), "3 + 4", None),
                    (Some(Target::Pair(2)), "5 + 6", None),
                    (Some(Target::Pair(3)), "7 + 8", None),
                ];
                plain_cards(ui, &pairs, &mut draft.target);
            }
            section_title(ui, "Options");
            check(ui, &mut draft.options.silence_sources, "Silence the sources in MONITOR");
            check(ui, &mut draft.options.include_effects, "Include reverb and echo (EFFECT RTN to the target)");
            if !draft.mono {
                check(ui, &mut draft.options.link_target, "Link the target pair");
            }
            if !notes.is_empty() {
                ui.add_space(6.0);
            }
            for text in &notes {
                note(ui, text);
            }
            ui.add_space(10.0);
            let ready = draft.target.is_some() && draft.sources.iter().any(|s| *s);
            if let Some(target) = draft.target.filter(|_| ready) {
                ui.label(RichText::new(bounce_summary(&draft.sources, target)).strong().size(12.5).color(paper::INK));
                ui.add_space(8.0);
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_enabled_ui(ready, |ui| apply = primary_button(ui, "Apply").clicked());
                cancel = plain_button(ui, "Cancel").clicked();
            });
        });
        draft.sources = choose_sources(before, draft.sources, self.mixer.links, draft.target);
        if cancel || !open {
            self.bounce = None;
        } else {
            self.bounce = Some(draft);
            if apply {
                self.apply_bounce();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_linked_pair_is_chosen_together_and_targets_are_left_out() {
        let links = [true, false, false, false];
        let before = [false; TRACKS];
        let mut now = before;
        now[1] = true;
        let fixed = choose_sources(before, now, links, Some(Target::Track(5)));
        assert_eq!(fixed[..2], [true, true], "1-2 is linked");
        let mut now = fixed;
        now[0] = false;
        assert_eq!(choose_sources(fixed, now, links, None)[..2], [false, false]);
        let all = [true; TRACKS];
        let fixed = choose_sources(all, all, links, Some(Target::Pair(3)));
        assert!(!fixed[6] && !fixed[7], "a target is no source");
    }

    #[test]
    fn the_summary_says_what_apply_sets_up() {
        assert_eq!(track_list(&[0, 1, 2, 3, 4, 5]), "1 to 6");
        assert_eq!(track_list(&[0, 1, 3]), "1, 2 and 4");
        assert_eq!(track_list(&[0, 2, 3, 4, 7]), "1, 3 to 5 and 8");
        let sources: [bool; TRACKS] = std::array::from_fn(|t| t < 6);
        assert_eq!(bounce_summary(&sources, Target::Pair(3)), "Tracks 1 to 6 onto 7 and 8 (stereo)");
        let one: [bool; TRACKS] = std::array::from_fn(|t| t == 0);
        assert_eq!(bounce_summary(&one, Target::Track(2)), "Track 1 onto track 3 (mono)");
    }

    #[test]
    fn targets_and_their_assign_key() {
        assert_eq!(Target::Track(3).tracks(), vec![3]);
        assert_eq!(Target::Track(3).assign(), 1, "track 4 is on ASSIGN 3-4");
        assert_eq!(Target::Pair(3).tracks(), vec![6, 7]);
        assert_eq!(Target::Pair(3).assign(), 3);
    }
}
