//! MIDI learn: a controller's faders, knobs and keys move the faders, PAN, MUTE and SOLO of each
//! track, the STEREO MASTER and PLAY, STOP, REC and RTZ. Pickup keeps a fader from jumping, and
//! feedback sends every change back, so motor faders and lamps follow the screen.

use crate::{DawApp, PITCH_PERCENT, TRACKS, fader_db, fader_position, palette};
use daw388_engine::Command;
pub use daw388_project::preferences::{MidiBinding as Binding, MidiControl as Control, MidiKind, MidiSource as Source};
use std::collections::{HashMap, HashSet};

/// A CC of 0 this soon after a press is that key's release (a momentary key); later, it is a
/// press of a key that toggles itself (as on a BCF2000) and means off.
const RELEASE_WITHIN: f64 = 0.35;

/// A MIDI message as the platform hands it over: the input it came from and its bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MidiMessage {
    pub port: String,
    pub bytes: Vec<u8>,
}

/// A control change or note with its value (0 for a note-off), or `None` for anything else.
pub fn parse(message: &MidiMessage) -> Option<(Source, u8)> {
    let [status, number, value, ..] = message.bytes[..] else { return None };
    let channel = (status & 0x0F) + 1;
    let (kind, value) = match status & 0xF0 {
        0xB0 => (MidiKind::Cc, value),
        0x90 => (MidiKind::Note, value),
        0x80 => (MidiKind::Note, 0),
        _ => return None,
    };
    if number > 127 || value > 127 {
        return None;
    }
    Some((Source { port: message.port.clone(), channel, kind, number }, value))
}

/// What a control is called in the menus and in Settings.
pub fn label(control: Control) -> String {
    match control {
        Control::Fader(t) => format!("Fader {}", t + 1),
        Control::Pan(t) => format!("PAN {}", t + 1),
        Control::Mute(t) => format!("MUTE {}", t + 1),
        Control::Solo(t) => format!("SOLO {}", t + 1),
        Control::Master => "STEREO MASTER".into(),
        Control::Pitch => "PITCH".into(),
        Control::Play => "PLAY".into(),
        Control::Stop => "STOP".into(),
        Control::Rec => "REC".into(),
        Control::Rtz => "RTZ".into(),
    }
}

/// "BCF2000, ch 1, CC 83" or "MK-225C, ch 10, note 36".
pub fn describe(source: &Source) -> String {
    let kind = match source.kind {
        MidiKind::Cc => "CC",
        MidiKind::Note => "note",
    };
    format!("{}, ch {}, {kind} {}", source.port, source.channel, source.number)
}

/// Whether a control moves continuously (a fader or a knob) rather than being pressed.
fn continuous(control: Control) -> bool {
    matches!(control, Control::Fader(_) | Control::Pan(_) | Control::Master | Control::Pitch)
}

/// A continuous control's value on the 0 to 127 scale of a CC: a fader by its position (so 0 dB
/// sits where it sits on screen), PAN with the middle at 63.5.
pub fn to_cc(control: Control, value: f32) -> f32 {
    match control {
        Control::Pan(_) => (value.clamp(-1.0, 1.0) * 63.5 + 63.5).clamp(0.0, 127.0),
        Control::Pitch => (value.clamp(-PITCH_PERCENT, PITCH_PERCENT) / PITCH_PERCENT * 63.5 + 63.5).clamp(0.0, 127.0),
        _ => fader_position(value) * 127.0,
    }
}

/// The value a CC sets: the inverse of [`to_cc`], with 64 exactly in the middle for PAN.
pub fn from_cc(control: Control, cc: u8) -> f32 {
    match control {
        Control::Pan(_) => match cc {
            64 => 0.0,
            0 => -1.0,
            127 => 1.0,
            _ => ((f32::from(cc) - 63.5) / 63.5).clamp(-1.0, 1.0),
        },
        Control::Pitch => from_cc(Control::Pan(0), cc) * PITCH_PERCENT,
        _ => fader_db(f32::from(cc) / 127.0),
    }
}

/// Pickup for one binding: a controller that stands elsewhere than the screen does nothing until
/// it reaches the screen's value (within two steps, or by passing it).
#[derive(Debug, Clone, Copy, Default)]
pub struct Pickup {
    picked: bool,
    last: Option<u8>,
}

impl Pickup {
    /// Whether `incoming` may move a control that stands at `screen` (both on the CC scale).
    pub fn admit(&mut self, incoming: u8, screen: f32) -> bool {
        let value = f32::from(incoming);
        let crossed = self.last.is_some_and(|last| {
            let last = f32::from(last);
            (last - screen) * (value - screen) <= 0.0
        });
        self.last = Some(incoming);
        if self.picked || (value - screen).abs() <= 2.0 || crossed {
            self.picked = true;
        }
        self.picked
    }

    /// The screen moved on its own: wait for the controller again.
    pub fn drop_it(&mut self) {
        self.picked = false;
        self.last = None;
    }
}

/// The bindings, what is being learnt, pickup and what feedback sent last.
#[derive(Debug, Default)]
pub struct Midi {
    pub bindings: Vec<Binding>,
    pub learning: Option<Control>,
    pickups: HashMap<Control, Pickup>,
    /// What feedback last sent per control, on the CC scale; missing: not sent yet.
    sent: HashMap<Control, u8>,
    /// When each CC key was last pressed, and the keys found to be momentary (they send 0 when
    /// let go).
    pressed_at: HashMap<Source, f64>,
    momentary: HashSet<Source>,
}

/// What a key's message means.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Press {
    /// Pressed: toggle MUTE or SOLO, or do the transport key.
    Down,
    /// A toggling key went to off.
    Off,
    /// Let go, or nothing.
    Ignore,
}

impl Midi {
    /// What a message from a key means, `now` seconds into the app's run. A note-on is a press,
    /// a note-off nothing. A CC above 63 is a press; a CC of 0 or so is the release of a
    /// momentary key (it came soon after the press, or the key did that before), or else the
    /// "off" of a key that toggles on the controller itself.
    pub fn press(&mut self, source: &Source, value: u8, now: f64) -> Press {
        if source.kind == MidiKind::Note {
            return if value > 0 { Press::Down } else { Press::Ignore };
        }
        if value > 63 {
            self.pressed_at.insert(source.clone(), now);
            return Press::Down;
        }
        if self.momentary.contains(source) {
            return Press::Ignore;
        }
        if self.pressed_at.get(source).is_some_and(|at| now - at < RELEASE_WITHIN) {
            self.momentary.insert(source.clone());
            return Press::Ignore;
        }
        Press::Off
    }
}

impl Midi {
    pub fn new(bindings: Vec<Binding>) -> Self {
        Self { bindings, ..Self::default() }
    }

    /// While learning: a CC or a pressed note becomes the binding of the control being learnt;
    /// the same source bound to something else moves here. Returns the control learnt.
    pub fn learn(&mut self, source: &Source, value: u8) -> Option<Control> {
        let control = self.learning?;
        if source.kind == MidiKind::Note && value == 0 {
            return None;
        }
        self.bindings.retain(|b| &b.source != source && b.control != control);
        self.bindings.push(Binding { control, source: source.clone() });
        self.learning = None;
        self.pickups.remove(&control);
        self.sent.remove(&control);
        Some(control)
    }

    pub fn forget(&mut self, control: Control) {
        self.bindings.retain(|b| b.control != control);
        self.pickups.remove(&control);
        self.sent.remove(&control);
    }

    pub fn forget_all(&mut self) {
        self.bindings.clear();
        self.pickups.clear();
        self.sent.clear();
    }

    pub fn bound(&self, control: Control) -> Option<&Binding> {
        self.bindings.iter().find(|b| b.control == control)
    }

    pub fn control_of(&self, source: &Source) -> Option<Control> {
        self.bindings.iter().find(|b| &b.source == source).map(|b| b.control)
    }

    /// Send everything again on the next frame (a project opened: motor faders follow).
    pub fn resend(&mut self) {
        self.sent.clear();
    }
}

impl DawApp {
    /// A continuous control's value now.
    fn control_value(&self, control: Control) -> f32 {
        match control {
            Control::Fader(t) => self.tracks[t].fader_db,
            Control::Pan(t) => self.tracks[t].pan,
            Control::Master => self.master_db,
            Control::Pitch => self.pitch_percent,
            Control::Mute(t) => f32::from(u8::from(self.tracks[t].mute)) * 127.0,
            Control::Solo(t) => f32::from(u8::from(self.tracks[t].solo)) * 127.0,
            _ => 0.0,
        }
    }

    /// What feedback sends for a control, or `None` for a transport key.
    fn feedback_value(&self, control: Control) -> Option<u8> {
        match control {
            Control::Fader(_) | Control::Pan(_) | Control::Master | Control::Pitch => {
                Some(to_cc(control, self.control_value(control)).round().clamp(0.0, 127.0) as u8)
            }
            Control::Mute(_) | Control::Solo(_) => Some(self.control_value(control) as u8),
            _ => None,
        }
    }

    /// Take the MIDI that came in, before the views: learn, or move what it is bound to.
    pub(crate) fn poll_midi(&mut self) {
        let messages = self.platform.midi_poll();
        let mut learnt = false;
        for message in messages {
            let Some((source, value)) = parse(&message) else { continue };
            if let Some(control) = self.midi.learn(&source, value) {
                self.status = format!("MIDI: {} ← {}", label(control), describe(&source));
                learnt = true;
                continue;
            }
            if let Some(control) = self.midi.control_of(&source) {
                self.apply_midi(control, &source, value);
            }
        }
        if learnt {
            self.store_midi();
        }
    }

    fn store_midi(&mut self) {
        self.preferences.midi = self.midi.bindings.clone();
        self.store_preferences();
    }

    fn apply_midi(&mut self, control: Control, source: &Source, value: u8) {
        if control == Control::Pitch {
            // As the knob: only in VARI, and the speed is fixed while recording.
            let recording = self.engine.as_ref().is_some_and(|e| e.rec_mode() != daw388_engine::RecMode::Off);
            if !self.vari || recording {
                return;
            }
        }
        if continuous(control) {
            let screen = to_cc(control, self.control_value(control));
            if !self.midi.pickups.entry(control).or_default().admit(value, screen) {
                return;
            }
            let new = from_cc(control, value);
            match control {
                Control::Fader(t) if t < TRACKS => {
                    self.tracks[t].fader_db = new;
                    self.send_channel(t);
                }
                Control::Pan(t) if t < TRACKS => {
                    self.tracks[t].pan = new;
                    self.send_channel(t);
                }
                Control::Master => {
                    self.master_db = new;
                    self.engine_send(Command::SetMaster(crate::db_to_gain(new)));
                }
                Control::Pitch => {
                    self.pitch_percent = new;
                    self.send_pitch();
                }
                _ => return,
            }
            // It came from the controller: do not send it back.
            self.midi.sent.insert(control, value);
            self.mark_dirty();
            return;
        }
        let press = self.midi.press(source, value, self.now);
        if press == Press::Ignore {
            return;
        }
        let off = press == Press::Off;
        match control {
            Control::Mute(t) if t < TRACKS => {
                if off && !self.tracks[t].mute {
                    return;
                }
                self.tracks[t].mute = !self.tracks[t].mute;
                self.send_channel(t);
                self.mark_dirty();
            }
            Control::Solo(t) if t < TRACKS => {
                if off && !self.tracks[t].solo {
                    return;
                }
                let solo = !self.tracks[t].solo;
                self.tracks[t].solo = solo;
                self.engine_send(Command::SetSolo(t, solo));
                self.link_touched = Some(t);
                self.mark_dirty();
            }
            Control::Play => self.play(),
            Control::Stop => self.stop(),
            Control::Rec => self.record(),
            Control::Rtz => self.rtz(),
            _ => {}
        }
    }

    /// Once a frame after the views: send every bound control that changed, so motor faders and
    /// lamps follow. A change that did not come from the controller makes pickup wait again.
    pub(crate) fn midi_feedback(&mut self) {
        let bindings = self.midi.bindings.clone();
        for binding in bindings {
            let Some(value) = self.feedback_value(binding.control) else { continue };
            if self.midi.sent.get(&binding.control) == Some(&value) {
                continue;
            }
            let first = !self.midi.sent.contains_key(&binding.control);
            self.midi.sent.insert(binding.control, value);
            if !first && let Some(pickup) = self.midi.pickups.get_mut(&binding.control) {
                pickup.drop_it();
            }
            let source = &binding.source;
            let channel = source.channel.saturating_sub(1) & 0x0F;
            let status = match source.kind {
                MidiKind::Cc => 0xB0,
                MidiKind::Note => 0x90,
            } | channel;
            self.platform.midi_send(&source.port, &[status, source.number, value]);
        }
    }

    /// The MIDI menu on a control (right-click), its blink while learning and its dot when bound.
    pub(crate) fn midi_target(&mut self, response: &egui::Response, control: Control) {
        let bound = self.midi.bound(control).map(|b| describe(&b.source));
        let mut learn = false;
        let mut forget = false;
        response.context_menu(|ui| {
            ui.label(egui::RichText::new(label(control)).strong());
            if ui.button("MIDI Learn").clicked() {
                learn = true;
                ui.close();
            }
            if let Some(source) = &bound {
                ui.label(egui::RichText::new(source).small());
                if ui.button("Forget MIDI").clicked() {
                    forget = true;
                    ui.close();
                }
            }
        });
        let painter = response.ctx.layer_painter(response.layer_id);
        if self.midi.learning == Some(control) {
            if (self.now * 3.0).fract() < 0.6 {
                painter.rect_stroke(response.rect.expand(2.0), 3.0, egui::Stroke::new(1.5, palette::LED), egui::StrokeKind::Outside);
            }
            response.ctx.request_repaint();
        }
        if bound.is_some() {
            painter.circle_filled(response.rect.right_top() + egui::vec2(1.0, -1.0), 2.0, palette::LED_GREEN);
        }
        if learn {
            self.midi.learning = Some(control);
            self.status = format!("{}: move a control on your MIDI controller (Esc cancels)", label(control));
        }
        if forget {
            self.midi.forget(control);
            self.store_midi();
            self.status = format!("{}: MIDI forgotten", label(control));
        }
    }

    /// Esc while learning.
    pub(crate) fn cancel_midi_learn(&mut self) -> bool {
        if self.midi.learning.take().is_some() {
            self.status = "MIDI Learn cancelled".into();
            return true;
        }
        false
    }

    /// Settings, MIDI: the inputs found and the bindings.
    pub(crate) fn midi_settings(&mut self, ui: &mut egui::Ui) {
        use crate::skin::paper::{self, note, plain_button, section_title};
        section_title(ui, "MIDI");
        let inputs = self.platform.midi_inputs();
        if inputs.is_empty() {
            note(ui, "No MIDI inputs found.");
        } else {
            note(ui, &format!("Inputs: {}", inputs.join(", ")));
        }
        note(ui, "Right-click (Ctrl-click) a fader, PAN, MUTE, SOLO, the STEREO MASTER, PITCH or a transport key and choose MIDI Learn.");
        let mut forget = None;
        let mut all = false;
        for binding in &self.midi.bindings {
            ui.horizontal(|ui| {
                let text = format!("{} ← {}", label(binding.control), describe(&binding.source));
                ui.label(egui::RichText::new(text).size(11.5).color(paper::INK));
                if plain_button(ui, "Forget").clicked() {
                    forget = Some(binding.control);
                }
            });
        }
        if !self.midi.bindings.is_empty() && plain_button(ui, "Forget all").clicked() {
            all = true;
        }
        if let Some(control) = forget {
            self.midi.forget(control);
            self.store_midi();
        }
        if all {
            self.midi.forget_all();
            self.store_midi();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(bytes: &[u8]) -> MidiMessage {
        MidiMessage { port: "BCF2000".into(), bytes: bytes.to_vec() }
    }

    fn cc(number: u8) -> Source {
        Source { port: "BCF2000".into(), channel: 1, kind: MidiKind::Cc, number }
    }

    #[test]
    fn parses_control_changes_and_notes() {
        assert_eq!(parse(&message(&[0xB0, 7, 100])), Some((cc(7), 100)));
        let note = Source { channel: 10, kind: MidiKind::Note, number: 36, ..cc(0) };
        assert_eq!(parse(&message(&[0x99, 36, 127])), Some((note.clone(), 127)));
        assert_eq!(parse(&message(&[0x89, 36, 64])), Some((note.clone(), 0)), "note-off");
        assert_eq!(parse(&message(&[0x99, 36, 0])), Some((note, 0)), "note-on at 0 is a note-off");
        assert_eq!(parse(&message(&[0xF8])), None, "clock");
        assert_eq!(parse(&message(&[0xE0, 0, 64])), None, "pitch bend");
        assert_eq!(parse(&message(&[0xB0, 7])), None, "too short");
    }

    #[test]
    fn labels_and_descriptions() {
        assert_eq!(label(Control::Fader(2)), "Fader 3");
        assert_eq!(label(Control::Master), "STEREO MASTER");
        assert_eq!(describe(&cc(83)), "BCF2000, ch 1, CC 83");
        let note = Source { port: "MK-225C".into(), channel: 10, kind: MidiKind::Note, number: 36 };
        assert_eq!(describe(&note), "MK-225C, ch 10, note 36");
    }

    #[test]
    fn values_follow_the_fader_curve_and_pan_has_its_middle() {
        let fader = Control::Fader(0);
        assert_eq!(from_cc(fader, 127), fader_db(1.0));
        assert_eq!(from_cc(fader, 0), fader_db(0.0));
        let zero = to_cc(fader, 0.0);
        assert!((from_cc(fader, zero.round() as u8)).abs() < 1.0, "0 dB comes back within a step");
        let pan = Control::Pan(0);
        assert_eq!((from_cc(pan, 0), from_cc(pan, 64), from_cc(pan, 127)), (-1.0, 0.0, 1.0));
        assert!((to_cc(pan, 0.0) - 63.5).abs() < 1e-6);
        let pitch = Control::Pitch;
        assert_eq!((from_cc(pitch, 0), from_cc(pitch, 64), from_cc(pitch, 127)), (-PITCH_PERCENT, 0.0, PITCH_PERCENT));
    }

    #[test]
    fn pickup_waits_for_the_controller() {
        let mut pickup = Pickup::default();
        assert!(!pickup.admit(10, 100.0), "far away");
        assert!(!pickup.admit(40, 100.0));
        assert!(pickup.admit(110, 100.0), "passing the screen value picks it up");
        assert!(pickup.admit(20, 100.0), "then it follows");
        pickup.drop_it();
        assert!(!pickup.admit(20, 100.0));
        assert!(pickup.admit(99, 100.0), "within two steps");
    }

    #[test]
    fn keys_that_toggle_themselves_and_momentary_keys() {
        let mut midi = Midi::default();
        let (toggling, momentary) = (cc(16), cc(17));
        // A BCF2000 key: 127 on one press, 0 on the next.
        assert_eq!(midi.press(&toggling, 127, 1.0), Press::Down);
        assert_eq!(midi.press(&toggling, 0, 3.0), Press::Off, "a second press, later: off");
        // A momentary key: 127 when pressed, 0 when let go, soon after.
        assert_eq!(midi.press(&momentary, 127, 1.0), Press::Down);
        assert_eq!(midi.press(&momentary, 0, 1.1), Press::Ignore, "let go");
        assert_eq!(midi.press(&momentary, 127, 5.0), Press::Down);
        assert_eq!(midi.press(&momentary, 0, 6.0), Press::Ignore, "held longer: still a release, it is known now");
        let note = Source { kind: MidiKind::Note, ..cc(36) };
        assert_eq!((midi.press(&note, 100, 0.0), midi.press(&note, 0, 9.0)), (Press::Down, Press::Ignore));
    }

    #[test]
    fn learning_binds_moves_and_forgets() {
        let mut midi = Midi::default();
        assert_eq!(midi.learn(&cc(7), 50), None, "not learning");
        midi.learning = Some(Control::Fader(0));
        assert_eq!(midi.learn(&cc(7), 50), Some(Control::Fader(0)));
        assert_eq!(midi.control_of(&cc(7)), Some(Control::Fader(0)));
        assert!(midi.learning.is_none());
        midi.learning = Some(Control::Fader(1));
        midi.learn(&cc(7), 50);
        assert_eq!(midi.control_of(&cc(7)), Some(Control::Fader(1)), "one source, one control");
        assert!(midi.bound(Control::Fader(0)).is_none());
        midi.learning = Some(Control::Play);
        let note = Source { kind: MidiKind::Note, number: 36, ..cc(0) };
        assert_eq!(midi.learn(&note, 0), None, "a note-off does not learn");
        assert_eq!(midi.learn(&note, 100), Some(Control::Play));
        midi.forget(Control::Play);
        assert!(midi.bound(Control::Play).is_none());
        assert_eq!(midi.bindings.len(), 1);
    }
}
