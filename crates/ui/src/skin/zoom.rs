//! The whole UI scales with the window, so the console and the drawer always fit, like a photo of the machine.

use egui::Vec2;

pub const MIN: f32 = 0.5;
pub const MAX: f32 = 1.25;
/// The zoom moves in steps, so dragging the window edge does not make the text shimmer. Small
/// ones, so the console comes close to filling a screen.
const STEP: f32 = 0.02;

/// The zoom for a window of `window` points at zoom 1 that has to show `needed` points: the
/// largest step at which it all fits.
pub fn zoom_for(window: Vec2, needed: Vec2) -> f32 {
    let fit = (window.x / needed.x).min(window.y / needed.y);
    // A little slack, so an exact fit does not fall a step down through rounding.
    (((fit + 1e-3) / STEP).floor() * STEP).clamp(MIN, MAX)
}

/// How far the fit may stray from the current zoom before it changes: a step down only when the
/// window is short by more than this, a step up only with this to spare.
const HOLD: f32 = 0.01;

/// The zoom for the next frame: [`zoom_for`], but the current zoom holds while the fit stays
/// within [`HOLD`] of its step. What is needed is measured at the current zoom, and text rounds
/// to pixels differently at each zoom (a few points on the width of the console). Right at a
/// step the measurement then pointed to the other zoom in turn, and the window trembled.
pub fn next_zoom(current: f32, window: Vec2, needed: Vec2) -> f32 {
    let fit = (window.x / needed.x).min(window.y / needed.y);
    let holds = (MIN..=MAX).contains(&current) && fit >= current - HOLD && (fit < current + STEP + HOLD || current >= MAX);
    if holds { current } else { zoom_for(window, needed) }
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
        assert!(close(zoom_for(Vec2::new(1680.0, 950.0), console), 0.68), "a step down, so it fits");
        assert!(close(zoom_for(Vec2::new(2880.0, 1800.0), console), 1.25));
        assert!(close(zoom_for(Vec2::new(400.0, 300.0), console), 0.5), "scrolls below the minimum");
    }

    #[test]
    fn the_zoom_holds_at_a_step() {
        // Measured at 0.90 the fit falls just under it, measured at 0.88 just over 0.90.
        let window = Vec2::new(1500.0, 779.0);
        let (at_90, at_88) = (Vec2::new(1668.6, 779.0 / 0.899), Vec2::new(1666.1, 779.0 / 0.901));
        assert!(close(zoom_for(window, at_90), 0.88) && close(zoom_for(window, at_88), 0.9), "zoom_for alone flips");
        assert!(close(next_zoom(0.9, window, at_90), 0.9));
        assert!(close(next_zoom(0.88, window, at_88), 0.88));
        // A real change of the window still moves it.
        assert!(close(next_zoom(0.9, Vec2::new(1500.0, 779.0 * 0.8 / 0.9), at_90), 0.8));
        assert!(close(next_zoom(0.88, Vec2::new(1700.0, 900.0), at_88), 1.02));
    }

    #[test]
    fn opening_the_drawer_makes_room() {
        let window = Vec2::new(1800.0, 1360.0);
        let closed = zoom_for(window, Vec2::new(1800.0, 1360.0));
        let open = zoom_for(window, Vec2::new(1800.0, 1360.0 + 400.0));
        assert!(open < closed && close(open, 0.76), "{open}");
    }
}
