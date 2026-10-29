//! A VU meter: the needle's ballistics, the mapping from energy to needle position and the drawing.

use crate::palette;
use egui::{Align2, Color32, FontId, Pos2, Stroke, Ui, Vec2, pos2, vec2};

/// 0 VU in dBFS RMS, the usual calibration for 24-bit recording.
pub const REFERENCE_DBFS: f32 = -18.0;
/// The scale, in VU.
pub const SCALE_MIN: f32 = -20.0;
pub const SCALE_MAX: f32 = 3.0;
/// The size of a TRK meter and of a big meter, at zoom 1.
pub const SMALL: Vec2 = Vec2::new(112.0, 60.0);
pub const LARGE: Vec2 = Vec2::new(150.0, 80.0);
/// The scale marks, in VU.
const MARKS: [f32; 11] = [-20.0, -10.0, -7.0, -5.0, -3.0, -2.0, -1.0, 0.0, 1.0, 2.0, 3.0];
/// The scale's sweep either side of straight up, in radians.
const SWEEP: f32 = 0.85;
/// Damping ratio: a little under critical, for the VU's slight overshoot (about 1%).
const DAMPING: f32 = 0.81;
/// Natural frequency in rad/s: within 1% of a step after 300 ms.
const OMEGA: f32 = 22.0;
/// Integration step in seconds, small enough to stay stable at any frame rate.
const SUBSTEP: f32 = 0.001;

/// Needle position (0 at -20 VU, 1 at +3 VU) for a level in VU. A real VU's deflection follows
/// the voltage, so the scale is linear in amplitude, not in dB.
pub fn position(vu: f32) -> f32 {
    let amplitude = |vu: f32| 10f32.powf(vu / 20.0);
    let (low, high) = (amplitude(SCALE_MIN), amplitude(SCALE_MAX));
    (amplitude(vu.clamp(SCALE_MIN, SCALE_MAX)) - low) / (high - low)
}

/// Needle position for a mean square from the engine; silence rests at -20.
pub fn target(mean_square: f32) -> f32 {
    if mean_square <= 0.0 {
        return 0.0;
    }
    position(10.0 * mean_square.log10() - REFERENCE_DBFS)
}

/// A VU needle: a damped spring towards its target, against the stops at both ends.
#[derive(Debug, Default, Clone, Copy)]
pub struct Needle {
    position: f32,
    velocity: f32,
}

impl Needle {
    /// Move the needle `dt` seconds towards `target` (a position from [`target`]).
    pub fn step(&mut self, dt: f32, target: f32) {
        let steps = (dt / SUBSTEP).ceil().max(1.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            let accel = OMEGA * OMEGA * (target - self.position) - 2.0 * DAMPING * OMEGA * self.velocity;
            self.velocity += accel * h;
            self.position += self.velocity * h;
            if self.position < 0.0 {
                self.position = 0.0;
                self.velocity = self.velocity.max(0.0);
            } else if self.position > 1.0 {
                self.position = 1.0;
                self.velocity = self.velocity.min(0.0);
            }
        }
    }

    /// Where the needle stands, 0 to 1.
    pub fn position(&self) -> f32 {
        self.position
    }
}

/// Draw a VU meter of `size` with the needle at `needle` (0 to 1) and the OL lamp.
pub fn draw(ui: &mut Ui, size: Vec2, needle: f32, ol: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, 3.0, palette::EDGE);
    let face = rect.shrink(3.0);
    painter.rect_filled(face, 2.0, palette::VU_FACE);
    // The pivot sits under the face, so only the top of the arc shows, as on a real VU.
    let pivot = pos2(face.center().x, face.bottom() + face.height() * 0.35);
    let radius = face.height() * 1.05;
    let at = |fraction: f32, r: f32| -> Pos2 {
        let angle = -SWEEP + 2.0 * SWEEP * fraction;
        pivot + vec2(angle.sin(), -angle.cos()) * r
    };
    let arc = |from: f32, to: f32, stroke: Stroke| {
        let points: Vec<Pos2> = (0..=24).map(|i| at(from + (to - from) * i as f32 / 24.0, radius)).collect();
        painter.add(egui::Shape::line(points, stroke));
    };
    let zero = position(0.0);
    arc(0.0, zero, Stroke::new(1.2, palette::VU_INK));
    arc(zero, 1.0, Stroke::new(3.0, palette::VU_RED));
    let tick = face.height() * 0.12;
    for mark in MARKS {
        let fraction = position(mark);
        let colour = if mark > 0.0 { palette::VU_RED } else { palette::VU_INK };
        painter.line_segment([at(fraction, radius), at(fraction, radius + tick)], Stroke::new(1.0, colour));
    }
    let small = FontId::proportional((face.height() * 0.16).clamp(7.0, 12.0));
    // Inside the arc, so the ends of the scale stay on the face.
    let numbers = radius - face.height() * 0.15;
    painter.text(at(0.0, numbers), Align2::CENTER_CENTER, "-20", small.clone(), palette::VU_INK);
    painter.text(at(zero, numbers), Align2::CENTER_CENTER, "0", small.clone(), palette::VU_RED);
    painter.text(at(1.0, numbers), Align2::CENTER_CENTER, "+3", small, palette::VU_RED);
    let label = FontId::proportional((face.height() * 0.2).clamp(8.0, 15.0));
    painter.text(at(0.5, radius * 0.72), Align2::CENTER_CENTER, "VU", label, palette::VU_INK);
    painter.line_segment([pivot, at(needle, radius + tick)], Stroke::new(1.4, Color32::BLACK));
    if ol {
        painter.circle_filled(pos2(face.right() - 7.0, face.top() + 7.0), 3.5, palette::LED);
    }
    painter.rect_stroke(face, 2.0, Stroke::new(1.0, palette::EDGE), egui::StrokeKind::Inside);
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run a needle at 60 frames per second; the highest position it reached.
    fn run(needle: &mut Needle, seconds: f32, target: f32) -> f32 {
        let mut highest = needle.position();
        for _ in 0..(seconds * 60.0).round() as usize {
            needle.step(1.0 / 60.0, target);
            highest = highest.max(needle.position());
        }
        highest
    }

    #[test]
    fn minus_18_dbfs_rms_is_0_vu() {
        // A sine with a peak of -15 dBFS has an RMS of -18 dBFS.
        let mean_square = 10f32.powf(-18.0 / 10.0);
        assert!((target(mean_square) - position(0.0)).abs() < 1e-4);
        assert!(position(0.0) > 0.6 && position(0.0) < 0.75, "0 VU at about two thirds");
    }

    #[test]
    fn reaches_a_step_in_300_ms_with_a_little_overshoot() {
        let goal = position(0.0);
        let mut needle = Needle::default();
        let highest = run(&mut needle, 0.3, goal);
        assert!((needle.position() - goal).abs() < 0.01 * goal, "{} after 300 ms", needle.position());
        let highest = highest.max(run(&mut needle, 1.0, goal));
        assert!(highest > goal, "a VU overshoots a little");
        assert!(highest <= goal * 1.015, "overshoot {highest}");
    }

    #[test]
    fn falls_back_to_minus_20_in_silence() {
        let mut needle = Needle::default();
        run(&mut needle, 1.0, position(0.0));
        run(&mut needle, 1.0, target(0.0));
        assert_eq!(needle.position(), 0.0);
    }

    #[test]
    fn stays_on_the_stop_above_plus_3() {
        let mut needle = Needle::default();
        let highest = run(&mut needle, 2.0, target(1.0));
        assert_eq!(needle.position(), 1.0);
        assert!(highest <= 1.0);
    }
}
