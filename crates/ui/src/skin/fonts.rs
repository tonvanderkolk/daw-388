//! IBM Plex Sans Condensed and IBM Plex Mono (SIL Open Font License, see `assets/fonts/OFL.txt`).

use egui::{FontData, FontDefinitions, FontFamily, RichText};
use std::sync::Arc;

const SANS: &str = "plex-sans";
const BOLD: &str = "plex-bold";
const MONO: &str = "plex-mono";

/// egui's fonts with Plex in front: Proportional is Plex Sans Condensed, Monospace Plex Mono
/// (the LED counter) and the family [`BOLD`] Plex Sans Condensed SemiBold. egui's own fonts stay
/// behind them for characters Plex lacks.
pub fn definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();
    for (name, bytes) in [
        (SANS, &include_bytes!("../../assets/fonts/IBMPlexSansCondensed-Regular.ttf")[..]),
        (BOLD, &include_bytes!("../../assets/fonts/IBMPlexSansCondensed-SemiBold.ttf")[..]),
        (MONO, &include_bytes!("../../assets/fonts/IBMPlexMono-Medium.ttf")[..]),
    ] {
        fonts.font_data.insert(name.into(), Arc::new(FontData::from_static(bytes)));
    }
    let fallback = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let mono_fallback = fonts.families.get(&FontFamily::Monospace).cloned().unwrap_or_default();
    let family = |front: &str, base: Vec<String>| [vec![front.to_string()], base].concat();
    fonts.families.insert(FontFamily::Proportional, family(SANS, fallback.clone()));
    fonts.families.insert(FontFamily::Name(BOLD.into()), family(BOLD, fallback));
    fonts.families.insert(FontFamily::Monospace, family(MONO, mono_fallback));
    fonts
}

/// Install the fonts.
pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(definitions());
}

/// Text in Plex SemiBold: egui has no weight per text, so bold is its own family.
pub fn bold(text: impl Into<String>) -> RichText {
    RichText::new(text).family(FontFamily::Name(BOLD.into()))
}

/// Plex SemiBold at `size`, for text the painter draws.
pub fn bold_font(size: f32) -> egui::FontId {
    egui::FontId::new(size, FontFamily::Name(BOLD.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plex_comes_first_with_egui_behind_it() {
        let fonts = definitions();
        let proportional = &fonts.families[&FontFamily::Proportional];
        assert_eq!(proportional[0], SANS);
        assert!(proportional.len() > 1, "egui's fonts as fallback");
        assert_eq!(fonts.families[&FontFamily::Monospace][0], MONO);
        assert_eq!(fonts.families[&FontFamily::Name(BOLD.into())][0], BOLD);
        assert!(fonts.font_data[SANS].bytes().len() > 10_000);
    }
}
