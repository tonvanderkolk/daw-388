//! The whole UI scales with the window, so the console and the drawer always fit, like a photo of the machine.

use egui::Vec2;

pub const MIN: f32 = 0.5;
pub const MAX: f32 = 1.25;
/// The zoom moves in steps, so dragging the window edge does not make the text shimmer.
const STEP: f32 = 0.05;

/// The zoom for a window of `window` points at zoom 1 that has to show `needed` points: the
/// largest step at which it all fits.
pub fn zoom_for(window: Vec2, needed: Vec2) -> f32 {
    let fit = (window.x / needed.x).min(window.y / needed.y);
    // A little slack, so an exact fit does not fall a step down through rounding.
    (((fit + 1e-3) / STEP).floor() * STEP).clamp(MIN, MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn fits_what_is_needed_in_the_window() {
        let console = Vec2::new(1800.0, 1360.0);
        assert!(close(zoom_for(Vec2::new(1800.0, 1360.0), console), 1.0));
        assert!(close(zoom_for(Vec2::new(1680.0, 950.0), console), 0.65), "a step down, so it fits");
        assert!(close(zoom_for(Vec2::new(2880.0, 1800.0), console), 1.25));
        assert!(close(zoom_for(Vec2::new(400.0, 300.0), console), 0.5), "scrolls below the minimum");
    }

    #[test]
    fn opening_the_drawer_makes_room() {
        let window = Vec2::new(1800.0, 1360.0);
        let closed = zoom_for(window, Vec2::new(1800.0, 1360.0));
        let open = zoom_for(window, Vec2::new(1800.0, 1360.0 + 400.0));
        assert!(open < closed && close(open, 0.75), "{open}");
    }
}
