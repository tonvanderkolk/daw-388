//! The windows on paper, after the mockup "Tape-instellingen en export": a cream sheet with a
//! dark title bar, small-caps section titles, choice cards, and checks, radios and sliders in
//! rust brown.

use super::fonts::{bold, bold_font};
use crate::palette;
use egui::{
    Align2, Color32, CornerRadius, FontId, Frame, Id, Margin, Rect, Response, RichText, Sense, Stroke, TextStyle, Ui, Vec2, pos2, vec2,
};

/// The paper, its lighter fields, the ink and the rust of the selections.
pub const PAPER: Color32 = Color32::from_rgb(0xe6, 0xdf, 0xcc);
pub const PAPER_LIGHT: Color32 = Color32::from_rgb(0xf3, 0xee, 0xdf);
pub const INK: Color32 = Color32::from_rgb(0x1e, 0x1a, 0x17);
pub const INK_SOFT: Color32 = Color32::from_rgb(0x5a, 0x52, 0x48);
pub const RUST: Color32 = Color32::from_rgb(0xb0, 0x50, 0x1e);
/// The text on a chosen (black) card.
pub const AMBER: Color32 = Color32::from_rgb(0xf0, 0xa4, 0x3c);
/// Height of the title bar.
const BAR: f32 = 34.0;

/// Light visuals for the inside of a sheet: ink on paper, rust for what is chosen.
fn paper_style(ui: &mut Ui) {
    let style = ui.style_mut();
    style.visuals = egui::Visuals::light();
    let v = &mut style.visuals;
    v.override_text_color = Some(INK);
    v.panel_fill = PAPER;
    v.window_fill = PAPER;
    v.extreme_bg_color = PAPER_LIGHT;
    v.selection.bg_fill = RUST;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.slider_trailing_fill = true;
    for (widget, edge) in
        [(&mut v.widgets.inactive, INK_SOFT), (&mut v.widgets.hovered, INK), (&mut v.widgets.active, INK), (&mut v.widgets.open, INK)]
    {
        widget.bg_fill = PAPER_LIGHT;
        widget.weak_bg_fill = PAPER_LIGHT;
        widget.bg_stroke = Stroke::new(1.0, edge);
        widget.fg_stroke = Stroke::new(1.5, INK);
        widget.corner_radius = CornerRadius::same(3);
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, INK);
    style.text_styles.insert(TextStyle::Body, FontId::proportional(12.5));
    style.text_styles.insert(TextStyle::Button, FontId::proportional(12.5));
    style.spacing.item_spacing = vec2(8.0, 6.0);
}

/// A sheet without the window chrome: the dark bar with `title`, `note` on the right and a
/// close cross, then the content on paper. `open` goes false when the cross is clicked.
fn sheet(ui: &mut Ui, title: &str, note: &str, open: Option<&mut bool>, width: f32, add: impl FnOnce(&mut Ui)) {
    ui.set_width(width);
    ui.spacing_mut().item_spacing.y = 0.0;
    let (bar, _) = ui.allocate_exact_size(vec2(width, BAR), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(bar, CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 }, palette::PANEL_DARK);
    painter.text(bar.left_center() + vec2(18.0, 0.0), Align2::LEFT_CENTER, title, bold_font(15.0), palette::CREAM);
    let mut right = bar.right() - 14.0;
    if let Some(open) = open {
        let cross = Rect::from_center_size(pos2(right - 6.0, bar.center().y), Vec2::splat(16.0));
        let response = ui.interact(cross, ui.scope_id().with("close"), Sense::click()).on_hover_text("Close");
        let colour = if response.hovered() { palette::CREAM } else { palette::TEXT_DIM };
        let painter = ui.painter();
        let r = cross.shrink(4.0);
        painter.line_segment([r.left_top(), r.right_bottom()], Stroke::new(1.5, colour));
        painter.line_segment([r.right_top(), r.left_bottom()], Stroke::new(1.5, colour));
        if response.clicked() {
            *open = false;
        }
        right -= 26.0;
    }
    if !note.is_empty() {
        ui.painter().text(pos2(right, bar.center().y), Align2::RIGHT_CENTER, note, FontId::proportional(11.0), palette::TEXT_DIM);
    }
    Frame::new().inner_margin(Margin::symmetric(20, 16)).show(ui, |ui| {
        ui.set_width(width - 40.0);
        paper_style(ui);
        add(ui);
    });
}

/// The frame of a sheet: paper with a dark edge and no padding (the title bar fills the top).
fn sheet_frame() -> Frame {
    Frame::new().fill(PAPER).stroke(Stroke::new(1.0, palette::EDGE)).corner_radius(4).inner_margin(0).shadow(egui::Shadow {
        offset: [0, 8],
        blur: 24,
        spread: 0,
        color: Color32::from_black_alpha(110),
    })
}

/// A window on paper: movable, no egui title bar, closed with the cross.
pub fn paper_window(ctx: &egui::Context, id: &str, title: &str, note: &str, open: &mut bool, width: f32, add: impl FnOnce(&mut Ui)) {
    if !*open {
        return;
    }
    let mut keep = true;
    egui::Window::new(title)
        .id(Id::unique(id))
        .title_bar(false)
        .resizable(false)
        .collapsible(false)
        .frame(sheet_frame())
        .show(ctx, |ui| sheet(ui, title, note, Some(&mut keep), width, add));
    *open = keep;
}

/// A modal question on paper; `should_close` of the result means a click outside or Escape.
pub fn paper_modal(ctx: &egui::Context, id: &str, title: &str, width: f32, add: impl FnOnce(&mut Ui)) -> egui::ModalResponse<()> {
    egui::Modal::new(Id::unique(id)).frame(sheet_frame()).show(ctx, |ui| sheet(ui, title, "", None, width, add))
}

/// A section title in small capitals, as "MACHINE" and "TAPE SPEED" in the mockup.
pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(bold(text.to_uppercase()).size(11.0).extra_letter_spacing(1.2).color(INK_SOFT));
    ui.add_space(2.0);
}

/// One card: a frame with an optional radio ring, the label and an optional second line. A
/// chosen card is black with amber text.
fn card(ui: &mut Ui, rect: Rect, label: &str, sub: Option<&str>, chosen: bool, radio: bool) -> Response {
    let response = ui.interact(rect, ui.scope_id().with(("card", label, rect.min.x as i32, rect.min.y as i32)), Sense::click());
    let painter = ui.painter();
    let (fill, text) = if chosen { (INK, AMBER) } else { (PAPER_LIGHT, INK) };
    let fill = if response.hovered() && !chosen { Color32::WHITE } else { fill };
    painter.rect_filled(rect, 3.0, fill);
    painter.rect_stroke(rect, 3.0, Stroke::new(if chosen { 2.0 } else { 1.0 }, INK), egui::StrokeKind::Inside);
    let mut x = rect.left() + 14.0;
    let line_y = if sub.is_some() { rect.top() + 15.0 } else { rect.center().y };
    if radio {
        let centre = pos2(x + 7.0, line_y);
        painter.circle_stroke(centre, 7.0, Stroke::new(1.3, if chosen { AMBER } else { INK_SOFT }));
        if chosen {
            painter.circle_filled(centre, 3.5, AMBER);
        }
        x += 22.0;
    }
    let anchor = if radio || sub.is_some() { Align2::LEFT_CENTER } else { Align2::CENTER_CENTER };
    let pos = if radio || sub.is_some() { pos2(x, line_y) } else { rect.center() };
    painter.text(pos, anchor, label, bold_font(13.0), text);
    if let Some(sub) = sub {
        let colour = if chosen { AMBER.gamma_multiply(0.75) } else { INK_SOFT };
        painter.text(pos2(rect.left() + 14.0, rect.bottom() - 12.0), Align2::LEFT_CENTER, sub, FontId::proportional(11.0), colour);
    }
    response
}

/// Cards side by side for one choice; returns whether it changed.
pub fn choice_cards<T: PartialEq + Copy>(ui: &mut Ui, options: &[(T, &str, Option<&str>)], value: &mut T) -> bool {
    let height = if options.iter().any(|(_, _, sub)| sub.is_some()) { 52.0 } else { 40.0 };
    let gap = 8.0;
    let width = (ui.available_width() - gap * (options.len() as f32 - 1.0)) / options.len() as f32;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let mut changed = false;
    for (i, (option, label, sub)) in options.iter().enumerate() {
        let rect = Rect::from_min_size(pos2(row.left() + i as f32 * (width + gap), row.top()), vec2(width, height));
        if card(ui, rect, label, *sub, *value == *option, true).clicked() && *value != *option {
            *value = *option;
            changed = true;
        }
    }
    changed
}

/// Cards side by side that are each on or off (Export: stereo bounce and separate tracks).
pub fn toggle_cards(ui: &mut Ui, options: &mut [(&str, &mut bool)]) {
    let gap = 8.0;
    let width = (ui.available_width() - gap * (options.len() as f32 - 1.0)) / options.len() as f32;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::hover());
    for (i, (label, on)) in options.iter_mut().enumerate() {
        let rect = Rect::from_min_size(pos2(row.left() + i as f32 * (width + gap), row.top()), vec2(width, 40.0));
        if card(ui, rect, label, None, **on, true).clicked() {
            **on = !**on;
        }
    }
}

/// Two plain cards for a choice of two (Clean / 388), `width` wide together.
pub fn switch_pair<T: PartialEq + Copy>(ui: &mut Ui, options: [(T, &str); 2], value: &mut T, width: f32) -> bool {
    let (row, _) = ui.allocate_exact_size(vec2(width, 34.0), Sense::hover());
    let mut changed = false;
    for (i, (option, label)) in options.into_iter().enumerate() {
        let rect = Rect::from_min_size(pos2(row.left() + i as f32 * width / 2.0, row.top()), vec2(width / 2.0, 34.0));
        if card(ui, rect, label, None, *value == option, false).clicked() && *value != option {
            *value = option;
            changed = true;
        }
    }
    changed
}

/// A check box: a square that fills rust with a white tick when on.
pub fn check(ui: &mut Ui, on: &mut bool, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.5), INK);
    let (rect, mut response) = ui.allocate_exact_size(vec2(22.0 + galley.size().x, 20.0_f32.max(galley.size().y)), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let enabled = ui.is_enabled();
    let painter = ui.painter();
    let square = Rect::from_center_size(pos2(rect.left() + 8.0, rect.center().y), Vec2::splat(15.0));
    if *on {
        painter.rect_filled(square, 2.0, if enabled { RUST } else { RUST.gamma_multiply(0.4) });
        let tick = [square.left_center() + vec2(3.5, 0.0), square.center_bottom() + vec2(-1.0, -4.0), square.right_top() + vec2(-3.0, 4.0)];
        painter.line_segment([tick[0], tick[1]], Stroke::new(2.0, Color32::WHITE));
        painter.line_segment([tick[1], tick[2]], Stroke::new(2.0, Color32::WHITE));
    } else {
        painter.rect_filled(square, 2.0, PAPER_LIGHT);
        painter.rect_stroke(
            square,
            2.0,
            Stroke::new(1.0, if enabled { INK_SOFT } else { INK_SOFT.gamma_multiply(0.4) }),
            egui::StrokeKind::Inside,
        );
    }
    let colour = if enabled { INK } else { INK.gamma_multiply(0.4) };
    painter.galley(pos2(rect.left() + 22.0, rect.center().y - galley.size().y / 2.0), galley, colour);
    response
}

/// A radio button: a ring with a rust dot when chosen. Returns a response that is clicked when chosen now.
pub fn radio(ui: &mut Ui, chosen: bool, label: &str) -> Response {
    let galley = ui.painter().layout_no_wrap(label.to_string(), FontId::proportional(12.5), INK);
    let (rect, response) = ui.allocate_exact_size(vec2(24.0 + galley.size().x, 20.0_f32.max(galley.size().y)), Sense::click());
    let enabled = ui.is_enabled();
    let painter = ui.painter();
    let centre = pos2(rect.left() + 8.0, rect.center().y);
    painter.circle_filled(centre, 8.0, PAPER_LIGHT);
    painter.circle_stroke(centre, 8.0, Stroke::new(1.2, if chosen { RUST } else { INK_SOFT }));
    if chosen {
        painter.circle_filled(centre, 4.0, if enabled { RUST } else { RUST.gamma_multiply(0.4) });
    }
    let colour = if enabled { INK } else { INK.gamma_multiply(0.4) };
    painter.galley(pos2(rect.left() + 24.0, rect.center().y - galley.size().y / 2.0), galley, colour);
    response
}

/// A slider row: the label on the left, the slider, and the value in mono on the right.
/// Double-click sets `default`.
pub fn slider_row(
    ui: &mut Ui,
    label: &str,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    default: f32,
    shown: String,
    tip: &str,
) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.add_sized(vec2(130.0, 20.0), egui::Label::new(RichText::new(label).size(12.5)));
        ui.spacing_mut().slider_width = ui.available_width() - 60.0;
        let response = ui.add(egui::Slider::new(value, range).show_value(false).trailing_fill(true)).on_hover_text(tip);
        if response.double_clicked() {
            *value = default;
            changed = true;
        }
        changed |= response.changed();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.label(RichText::new(shown).monospace().size(12.0).color(INK));
        });
    });
    changed
}

/// The main button of a sheet (Export): rust with white text.
pub fn primary_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(egui::Button::new(bold(text).size(13.0).color(Color32::WHITE)).fill(RUST).stroke(Stroke::NONE).min_size(vec2(96.0, 32.0)))
}

/// Another button of a sheet (Cancel): paper with an edge.
pub fn plain_button(ui: &mut Ui, text: &str) -> Response {
    ui.add(
        egui::Button::new(RichText::new(text).size(13.0).color(INK))
            .fill(PAPER_LIGHT)
            .stroke(Stroke::new(1.0, INK_SOFT))
            .min_size(vec2(88.0, 32.0)),
    )
}

/// A small note under a choice, in soft ink.
pub fn note(ui: &mut Ui, text: &str) {
    ui.label(RichText::new(text).size(11.5).color(INK_SOFT));
}
