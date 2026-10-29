//! The tape deck: the two reels and the line about the tape.

use crate::palette;
use daw388_project::{KindEntry, SpeedEntry, TapeMode, TapeSettings};
use egui::{Align2, FontId, Pos2, Stroke, Ui, Vec2, pos2, vec2};
use std::f32::consts::TAU;

/// The size of the tape deck's window at zoom 1.
pub const WINDOW: Vec2 = Vec2::new(262.0, 190.0);

/// The tape runs past at 7½ ips, as on the 388.
const IPS: f32 = 7.5;
/// Hub and full-pack radius of a 7-inch reel, in inches.
pub const CORE: f32 = 1.0;
pub const FULL: f32 = 3.4;
/// Fastest drawn rotation, in turns per second, so winding does not strobe.
const MAX_TURNS: f32 = 3.0;
/// A jump in tape position larger than this (RTZ, a click in the drawer) is not winding.
const JUMP: f64 = 2.0;
/// The shortest tape the reels show, in seconds.
pub const MIN_LENGTH: f64 = 180.0;

/// The reels' angles (radians, clockwise on screen) and how far the tape has run.
#[derive(Debug, Default, Clone)]
pub struct Reels {
    pub left_angle: f32,
    pub right_angle: f32,
    /// Fraction of the tape on the right reel, 0 to 1.
    pub wound: f32,
    last_position: Option<f64>,
}

impl Reels {
    /// Follow the tape to `position` (seconds from the start) on a tape of `length` seconds,
    /// `dt` seconds after the previous call.
    pub fn advance(&mut self, position: f64, length: f64, dt: f32) {
        let length = length.max(MIN_LENGTH);
        self.wound = (position / length).clamp(0.0, 1.0) as f32;
        let moved = self.last_position.map_or(0.0, |last| position - last);
        self.last_position = Some(position);
        if moved.abs() > JUMP {
            return;
        }
        let inches = moved as f32 * IPS;
        let limit = MAX_TURNS * TAU * dt;
        let (left, right) = self.radii();
        // Seen from the front the tape runs off the bottom of the left reel onto the bottom of the
        // right one: playing turns both reels anticlockwise.
        self.left_angle = (self.left_angle - (inches / left).clamp(-limit, limit)) % TAU;
        self.right_angle = (self.right_angle - (inches / right).clamp(-limit, limit)) % TAU;
    }

    /// Radius of the tape pack on the left and the right reel, in inches. The tape's area is
    /// what moves from one reel to the other.
    pub fn radii(&self) -> (f32, f32) {
        let area = FULL * FULL - CORE * CORE;
        let radius = |share: f32| (CORE * CORE + share * area).sqrt();
        (radius(1.0 - self.wound), radius(self.wound))
    }
}

/// The line above the reels: "456 style · 7½ IPS · dbx on", or "Clean".
pub fn tape_line(tape: &TapeSettings) -> String {
    if tape.mode == TapeMode::Clean {
        return "Clean".into();
    }
    let kind = match tape.kind {
        KindEntry::T456 => "456",
        KindEntry::T250 => "250",
        KindEntry::T911 => "911",
    };
    let speed = match tape.speed {
        SpeedEntry::Ips3_75 => "3¾",
        SpeedEntry::Ips7_5 => "7½",
        SpeedEntry::Ips15 => "15",
    };
    let dbx = match tape.dbx {
        [true, true] => "dbx on",
        [true, false] => "dbx 1-4",
        [false, true] => "dbx 5-8",
        [false, false] => "dbx off",
    };
    format!("{kind} style · {speed} IPS · {dbx}")
}

/// Draw the smoked-glass window with both reels and the line about the tape.
pub fn draw(ui: &mut Ui, reels: &Reels, line: &str) {
    let (rect, _) = ui.allocate_exact_size(WINDOW, egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 4.0, palette::EDGE);
    let glass = rect.shrink(4.0);
    painter.rect_filled(glass, 3.0, palette::GLASS);
    painter.text(glass.left_top() + vec2(10.0, 8.0), Align2::LEFT_TOP, line, FontId::proportional(11.0), palette::TEXT_DIM);
    // Inches to points: a full reel takes a little under half the window, with room between them.
    let scale = glass.width() * 0.2 / FULL;
    let y = glass.center().y + 10.0;
    let centres = [pos2(glass.left() + glass.width() * 0.27, y), pos2(glass.left() + glass.width() * 0.73, y)];
    let (left, right) = reels.radii();
    // The tape between the packs, along the bottom.
    let bottom = |centre: Pos2, r: f32| centre + vec2(0.0, r * scale);
    painter.line_segment([bottom(centres[0], left), bottom(centres[1], right)], Stroke::new(1.5, palette::TAPE_EDGE));
    for (centre, pack, angle) in [(centres[0], left, reels.left_angle), (centres[1], right, reels.right_angle)] {
        // The flange, faint behind the smoked glass.
        painter.circle_filled(centre, FULL * scale, palette::REEL_FLANGE.gamma_multiply(0.35));
        painter.circle_stroke(centre, FULL * scale, Stroke::new(1.0, palette::REEL_FLANGE.gamma_multiply(0.7)));
        // The tape pack, with a lighter edge so its size reads.
        painter.circle_filled(centre, pack * scale, palette::TAPE_PACK);
        painter.circle_stroke(centre, pack * scale, Stroke::new(1.0, palette::TAPE_EDGE));
        // Three arms of the flange in front of the pack: they show the reel turning.
        for spoke in 0..3 {
            let a = angle + spoke as f32 * TAU / 3.0;
            let tip = centre + vec2(a.sin(), -a.cos()) * (FULL * scale);
            painter.line_segment([centre, tip], Stroke::new(CORE * scale * 0.45, palette::REEL_FLANGE.gamma_multiply(0.8)));
        }
        painter.circle_filled(centre, CORE * scale, palette::REEL);
        painter.circle_filled(centre, CORE * scale * 0.35, palette::REEL_FLANGE);
        painter.circle_filled(centre, CORE * scale * 0.12, palette::GLASS);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pack_moves_from_left_to_right() {
        let mut reels = Reels::default();
        reels.advance(0.0, 300.0, 0.016);
        assert_eq!(reels.radii(), (FULL, CORE));
        reels.advance(300.0, 300.0, 0.016);
        assert_eq!(reels.radii(), (CORE, FULL));
        reels.advance(10.0, 30.0, 0.016);
        assert!((reels.wound - 10.0 / 180.0).abs() < 1e-6, "at least three minutes of tape");
    }

    #[test]
    fn turns_with_the_tape_but_not_on_a_jump() {
        let mut reels = Reels::default();
        reels.advance(10.0, 300.0, 0.016);
        reels.advance(10.016, 300.0, 0.016);
        assert!(reels.left_angle < 0.0 && reels.right_angle < 0.0, "anticlockwise while playing");
        let before = (reels.left_angle, reels.right_angle);
        reels.advance(100.0, 300.0, 0.016);
        assert_eq!((reels.left_angle, reels.right_angle), before, "RTZ or a click is no winding");
    }

    #[test]
    fn winding_is_limited_to_three_turns_a_second() {
        let mut reels = Reels::default();
        reels.advance(100.0, 300.0, 0.016);
        reels.advance(101.5, 300.0, 0.016);
        let limit = 3.0 * TAU * 0.016;
        assert!((reels.right_angle + limit).abs() < 1e-5, "{}", reels.right_angle);
    }

    #[test]
    fn describes_the_tape() {
        let mut tape = TapeSettings::default();
        assert_eq!(tape_line(&tape), "Clean");
        tape.mode = TapeMode::Machine;
        assert_eq!(tape_line(&tape), "456 style · 7½ IPS · dbx on");
        tape.dbx = [false, true];
        assert_eq!(tape_line(&tape), "456 style · 7½ IPS · dbx 5-8");
        tape.dbx = [false, false];
        tape.speed = SpeedEntry::Ips15;
        tape.kind = KindEntry::T911;
        assert_eq!(tape_line(&tape), "911 style · 15 IPS · dbx off");
    }
}
