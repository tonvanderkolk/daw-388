//! The keys of the 388: a raised key in the colour of its function, its label small on the
//! panel above it, a LED for its state and symbols on the transport. And the slide switch for
//! MIC / LINE / RMX.

use crate::palette;
use egui::{Align2, Color32, FontId, Pos2, Rect, Response, Sense, Stroke, Ui, Vec2, pos2, vec2};

/// The large transport keys, the small function keys and the narrow keys in the strips.
pub const LARGE: Vec2 = Vec2::new(44.0, 34.0);
pub const SMALL: Vec2 = Vec2::new(26.0, 20.0);
pub const NARROW: Vec2 = Vec2::new(22.0, 14.0);
/// Height of the label above a key, and the gap to the key.
const LABEL: f32 = 11.0;
const GAP: f32 = 2.0;
/// Room for the LED left of the label.
const LAMP: f32 = 8.0;

/// The colour of a key, after its function.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum KeyColour {
    Beige,
    Red,
    Green,
    Grey,
}

impl KeyColour {
    /// Face and shade.
    fn colours(self) -> (Color32, Color32) {
        match self {
            Self::Beige => (palette::KEY_BEIGE, palette::KEY_BEIGE_SHADE),
            Self::Red => (palette::KEY_RED, palette::KEY_RED_SHADE),
            Self::Green => (palette::KEY_GREEN, palette::KEY_GREEN_SHADE),
            Self::Grey => (palette::KEY_GREY, palette::KEY_GREY_SHADE),
        }
    }
}

/// What a transport key shows.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Symbol {
    Rewind,
    Forward,
    Stop,
    Play,
    Record,
}

/// A key cap: build it, then `show` it.
#[derive(Debug, Clone, Copy)]
pub struct KeyCap<'a> {
    label: &'a str,
    size: Vec2,
    colour: KeyColour,
    /// Lit or not, and its colour; `None`: the key has no LED.
    lamp: Option<(bool, Color32)>,
    symbol: Option<Symbol>,
    /// Only the key: the caller draws label and LED where its grid wants them.
    bare: bool,
}

impl<'a> KeyCap<'a> {
    /// A beige key without LED or symbol.
    pub fn new(label: &'a str, size: Vec2) -> Self {
        Self { label, size, colour: KeyColour::Beige, lamp: None, symbol: None, bare: false }
    }

    pub fn colour(mut self, colour: KeyColour) -> Self {
        self.colour = colour;
        self
    }

    /// Give the key a LED, lit or not.
    pub fn lamp(mut self, lit: bool, colour: Color32) -> Self {
        self.lamp = Some((lit, colour));
        self
    }

    pub fn symbol(mut self, symbol: Symbol) -> Self {
        self.symbol = Some(symbol);
        self
    }

    /// Only the key, without label or LED: for a grid that places those itself.
    pub fn bare(mut self) -> Self {
        self.bare = true;
        self
    }

    /// Draw the label, the LED and the key; the response is the key's.
    pub fn show(self, ui: &mut Ui) -> Response {
        let text = ui.painter().layout_no_wrap(self.label.to_string(), FontId::proportional(9.0), palette::TEXT_DIM);
        let lamp = if self.lamp.is_some() { LAMP } else { 0.0 };
        if self.bare {
            let (rect, response) = ui.allocate_exact_size(self.size, Sense::click());
            paint_key(ui, &response, rect, self.colour, self.symbol);
            return response;
        }
        let (width, height) = (key_width(self.size.x, text.size().x, lamp), LABEL + GAP + self.size.y);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height), Sense::click());
        let painter = ui.painter();
        let (label_left, label_top) = (rect.center().x - (text.size().x + lamp) / 2.0 + lamp, rect.top());
        if let Some((lit, colour)) = self.lamp {
            paint_lamp(painter, pos2(label_left - 5.0, label_top + text.size().y / 2.0), lit, colour);
        }
        painter.galley(pos2(label_left, label_top), text, palette::TEXT_DIM);

        let centre = pos2(rect.center().x, rect.top() + LABEL + GAP + self.size.y / 2.0);
        paint_key(ui, &response, Rect::from_center_size(centre, self.size), self.colour, self.symbol);
        response
    }
}

/// The key itself in `body`: face, the shade under it, pressed and hover states and the symbol.
fn paint_key(ui: &Ui, response: &Response, body: Rect, colour: KeyColour, symbol: Option<Symbol>) {
    let painter = ui.painter();
    let (mut face, shade) = colour.colours();
    if response.hovered() {
        face = face.lerp_to_gamma(Color32::WHITE, 0.12);
    }
    if !ui.is_enabled() {
        face = face.gamma_multiply(0.5);
    }
    let pressed = response.is_pointer_button_down_on();
    let face_rect = if pressed { body.translate(vec2(0.0, 1.0)) } else { body };
    if !pressed {
        painter.rect_filled(body.translate(vec2(0.0, 2.0)), 2.0, shade);
    }
    painter.rect_filled(face_rect, 2.0, face);
    painter.rect_stroke(face_rect, 2.0, Stroke::new(1.0, palette::EDGE), egui::StrokeKind::Outside);
    if let Some(symbol) = symbol {
        paint_symbol(painter, face_rect.center(), face_rect.height().min(face_rect.width()) * 0.3, symbol);
    }
}

/// A key's LED at `centre`: dark when off, with a soft glow when lit.
pub fn paint_lamp(painter: &egui::Painter, centre: Pos2, lit: bool, colour: Color32) {
    if lit {
        painter.circle_filled(centre, 4.5, colour.gamma_multiply(0.25));
    }
    painter.circle_filled(centre, 2.5, if lit { colour } else { Color32::from_rgb(0x2a, 0x26, 0x24) });
}

/// The look of a small LED display for the widgets inside `ui` (tempo, metre, input): a dark
/// field with a thin edge and red figures in Plex Mono, like the counter.
pub fn led_style(ui: &mut Ui) {
    let style = ui.style_mut();
    let field = Color32::from_rgb(0x12, 0x08, 0x07);
    for state in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        state.weak_bg_fill = field;
        state.bg_fill = field;
        state.bg_stroke = Stroke::new(1.0, palette::EDGE);
        state.fg_stroke = Stroke::new(1.0, palette::LED);
        state.corner_radius = egui::CornerRadius::same(2);
    }
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, palette::LED.gamma_multiply(0.5));
    style.visuals.extreme_bg_color = field;
    style.visuals.override_text_color = Some(palette::LED);
    style.override_font_id = Some(FontId::monospace(11.0));
}

/// How wide a key is drawn: its own width, or wider when its label (and LED) need it.
pub fn key_width(key: f32, label: f32, lamp: f32) -> f32 {
    key.max(label + lamp)
}

/// A transport symbol of radius `r` around `centre`.
fn paint_symbol(painter: &egui::Painter, centre: Pos2, r: f32, symbol: Symbol) {
    let ink = Stroke::new(1.3, palette::KEY_INK);
    // A triangle with its tip at x `tip` and its base at x `base`, both relative to the centre;
    // as tall as the STOP square.
    let half = 0.75 * r;
    let triangle = |tip: f32, base: f32| {
        let points = vec![pos2(centre.x + base, centre.y - half), pos2(centre.x + tip, centre.y), pos2(centre.x + base, centre.y + half)];
        egui::Shape::convex_polygon(points, Color32::TRANSPARENT, ink)
    };
    match symbol {
        Symbol::Rewind => {
            painter.add(triangle(-1.3 * half, 0.0));
            painter.add(triangle(0.0, 1.3 * half));
        }
        Symbol::Forward => {
            painter.add(triangle(0.0, -1.3 * half));
            painter.add(triangle(1.3 * half, 0.0));
        }
        Symbol::Stop => {
            painter.rect_stroke(Rect::from_center_size(centre, Vec2::splat(1.5 * r)), 0.0, ink, egui::StrokeKind::Middle);
        }
        Symbol::Play => {
            painter.add(triangle(0.85 * half, -0.85 * half));
        }
        Symbol::Record => {
            painter.circle_filled(centre, 0.6 * r, palette::KEY_RED);
        }
    }
}

/// Which of `count` positions lies under `x` on a slot from `left` that is `width` wide.
pub fn switch_index(x: f32, left: f32, width: f32, count: usize) -> usize {
    let fraction = ((x - left) / width).clamp(0.0, 0.999);
    ((fraction * count as f32) as usize).min(count.saturating_sub(1))
}

/// The slide switch at the top of a channel: the labels above, a black slot with a beige slider.
/// Click a position or drag the slider there; `selected` changes and the response says so.
pub fn slide_switch(ui: &mut Ui, labels: &[&str], selected: &mut usize, width: f32) -> Response {
    let (rect, mut response) = ui.allocate_exact_size(vec2(width, LABEL + GAP + 10.0), Sense::click_and_drag());
    let count = labels.len().max(1);
    let slot = Rect::from_min_size(pos2(rect.left(), rect.top() + LABEL + GAP), vec2(width, 10.0));
    if (response.clicked() || response.dragged())
        && let Some(pos) = response.interact_pointer_pos()
    {
        let index = switch_index(pos.x, slot.left(), slot.width(), count);
        if index != *selected {
            *selected = index;
            response.mark_changed();
        }
    }
    let painter = ui.painter();
    let step = width / count as f32;
    for (index, label) in labels.iter().enumerate() {
        let colour = if index == *selected { palette::CREAM } else { palette::TEXT_DIM.gamma_multiply(0.6) };
        let x = rect.left() + step * (index as f32 + 0.5);
        painter.text(pos2(x, rect.top()), Align2::CENTER_TOP, *label, FontId::proportional(9.0), colour);
    }
    painter.rect_filled(slot, 3.0, Color32::BLACK);
    let thumb = Rect::from_min_size(pos2(slot.left() + step * *selected as f32 + 1.0, slot.top() + 1.0), vec2(step - 2.0, 8.0));
    painter.rect_filled(thumb, 2.0, palette::KEY_BEIGE);
    let grip = [pos2(thumb.center().x, thumb.top() + 2.0), pos2(thumb.center().x, thumb.bottom() - 2.0)];
    painter.line_segment(grip, Stroke::new(1.0, palette::KEY_BEIGE_SHADE));
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_click_picks_the_position_under_it() {
        assert_eq!(switch_index(1.0, 0.0, 60.0, 3), 0);
        assert_eq!(switch_index(30.0, 0.0, 60.0, 3), 1);
        assert_eq!(switch_index(59.0, 0.0, 60.0, 3), 2);
        assert_eq!(switch_index(-10.0, 0.0, 60.0, 3), 0, "left of the slot");
        assert_eq!(switch_index(80.0, 0.0, 60.0, 3), 2, "right of the slot");
        assert_eq!(switch_index(45.0, 0.0, 60.0, 2), 1, "two positions in the simple mode");
    }

    #[test]
    fn a_key_grows_with_its_label() {
        assert_eq!(key_width(26.0, 14.0, 0.0), 26.0);
        assert_eq!(key_width(26.0, 40.0, 8.0), 48.0, "AUTO PUNCH with its LED");
    }
}
