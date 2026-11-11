//! A strip of masking tape under the channels, as engineers stuck on a 388 to write the parts
//! on in felt pen: one long strip, a little crooked, torn off at both ends.

use super::fonts::marker_font;
use egui::{Color32, Painter, Rect, Shape, Stroke, pos2, vec2};

/// Height of the strip.
pub const HEIGHT: f32 = 20.0;
const TAPE: Color32 = Color32::from_rgb(0xe4, 0xd8, 0xb2);
const TAPE_EDGE: Color32 = Color32::from_rgb(0xc4, 0xb5, 0x8a);
const TAPE_FLECK: Color32 = Color32::from_rgb(0xd3, 0xc5, 0x9b);
/// The felt pen.
pub const INK: Color32 = Color32::from_rgb(0x1f, 0x1c, 0x1a);
/// How crooked the strip is: points down per point to the right.
const SLOPE: f32 = 0.004;

/// A small deterministic hash, so the flecks and the torn edge stay put from frame to frame.
fn hash(x: f32, salt: u32) -> f32 {
    let mut h = (x.round() as i32 as u32).wrapping_mul(0x9e37_79b9) ^ salt.wrapping_mul(0x85eb_ca6b);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2c1b_3c6d);
    h ^= h >> 12;
    (h & 0xffff) as f32 / 65535.0
}

/// The strip's vertical shift at `x`, measured from `origin` (the left end).
pub fn tilt(x: f32, origin: f32) -> f32 {
    (x - origin) * SLOPE
}

/// Paint the tape over `left..right` on the row `row` (only its top and bottom count), tilted
/// from `origin`. `torn_left` and `torn_right` rip that end.
pub fn paint(painter: &Painter, left: f32, right: f32, row: Rect, origin: f32, torn_left: bool, torn_right: bool) {
    let (top, bottom) = (row.top(), row.bottom());
    let at = |x: f32, y: f32| pos2(x, y + tilt(x, origin));
    painter.add(Shape::convex_polygon(vec![at(left, top), at(right, top), at(right, bottom), at(left, bottom)], TAPE, Stroke::NONE));
    // Flecks in the paper and a faint crease now and then.
    let mut x = left + 2.0;
    while x < right - 2.0 {
        let y = top + 2.0 + hash(x, 11) * (bottom - top - 4.0);
        painter.circle_filled(at(x, y), 0.6, TAPE_FLECK);
        if hash(x, 23) > 0.96 {
            painter.line_segment([at(x, top + 1.0), at(x + 2.0, bottom - 1.0)], Stroke::new(0.6, TAPE_FLECK));
        }
        x += 3.0 + hash(x, 5) * 4.0;
    }
    let edge = Stroke::new(0.8, TAPE_EDGE);
    painter.line_segment([at(left, top), at(right, top)], edge);
    painter.line_segment([at(left, bottom), at(right, bottom)], edge);
    // The torn ends: the panel shows through between the teeth.
    for (torn, x, inward, salt) in [(torn_left, left, 1.0, 3), (torn_right, right, -1.0, 7)] {
        if !torn {
            continue;
        }
        let teeth = 6;
        for i in 0..teeth {
            let y0 = top + (bottom - top) * i as f32 / teeth as f32;
            let y1 = top + (bottom - top) * (i + 1) as f32 / teeth as f32;
            let depth = 1.5 + hash(y1 + x, salt) * 4.0;
            painter.add(Shape::convex_polygon(
                vec![at(x - inward, y0 - 0.5), at(x + inward * depth, (y0 + y1) / 2.0), at(x - inward, y1 + 0.5)],
                crate::palette::PANEL,
                Stroke::NONE,
            ));
        }
    }
}

/// Write `text` in felt pen, centred on `row` (following the tilt), as large as fits its width.
pub fn write(painter: &Painter, row: Rect, origin: f32, text: &str, colour: Color32) {
    if text.is_empty() {
        return;
    }
    let width = row.width() - 8.0;
    let mut size = 13.0;
    let mut galley = painter.layout_no_wrap(text.to_string(), marker_font(size), colour);
    while galley.size().x > width && size > 9.0 {
        size -= 0.5;
        galley = painter.layout_no_wrap(text.to_string(), marker_font(size), colour);
    }
    if galley.size().x > width {
        let mut chars: Vec<char> = text.chars().collect();
        while !chars.is_empty() && galley.size().x > width {
            chars.pop();
            let short: String = chars.iter().collect::<String>().trim_end().to_string() + "…";
            galley = painter.layout_no_wrap(short, marker_font(size), colour);
        }
    }
    let centre = row.center() + vec2(0.0, tilt(row.center().x, origin));
    painter.galley(centre - galley.size() / 2.0, galley, colour);
}
