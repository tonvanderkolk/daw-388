//! The vintage skin after design A2: colours, fonts, the meter bridge and the tape deck.

pub(crate) mod bridge;
pub(crate) mod fonts;
pub(crate) mod reels;
pub(crate) mod vu;
pub(crate) mod zoom;

/// Colours from mockup A2.
pub(crate) mod palette {
    use egui::Color32;
    pub const BACKGROUND: Color32 = Color32::from_rgb(0x1a, 0x17, 0x16);
    pub const WOOD: Color32 = Color32::from_rgb(0x3a, 0x1d, 0x1a);
    pub const PANEL: Color32 = Color32::from_rgb(0x2f, 0x2f, 0x31);
    pub const PANEL_DARK: Color32 = Color32::from_rgb(0x1f, 0x1f, 0x21);
    pub const EDGE: Color32 = Color32::from_rgb(0x1b, 0x1b, 0x1c);
    pub const TEXT: Color32 = Color32::from_rgb(0xd9, 0xd3, 0xc6);
    pub const TEXT_DIM: Color32 = Color32::from_rgb(0xb9, 0xb2, 0xa4);
    pub const CREAM: Color32 = Color32::from_rgb(0xef, 0xe6, 0xcc);
    pub const LED: Color32 = Color32::from_rgb(0xff, 0x5a, 0x3c);
    pub const RED: Color32 = Color32::from_rgb(0xc8, 0x40, 0x2a);
    pub const ORANGE: Color32 = Color32::from_rgb(0xe5, 0x83, 0x2c);
    pub const GREEN: Color32 = Color32::from_rgb(0x9f, 0xd3, 0x9a);
    pub const BLUE: Color32 = Color32::from_rgb(0x7f, 0xa8, 0xcc);
    pub const KEY: Color32 = Color32::from_rgb(0x9a, 0x9a, 0x9c);
    pub const LEATHER: Color32 = Color32::from_rgb(0x4b, 0x23, 0x1d);
    pub const LEATHER_EDGE: Color32 = Color32::from_rgb(0x2a, 0x12, 0x0f);
    pub const LANE: Color32 = Color32::from_rgb(0x2b, 0x2b, 0x2d);
    pub const LANE_ALT: Color32 = Color32::from_rgb(0x29, 0x29, 0x2b);
    pub const REGION: Color32 = Color32::from_rgb(0xe9, 0xdf, 0xc8);
    pub const REGION_PICKED: Color32 = Color32::from_rgb(0xf6, 0xee, 0xdb);
    pub const REGION_INK: Color32 = Color32::from_rgb(0x1b, 0x17, 0x16);
    pub const RANGE: Color32 = Color32::from_rgba_premultiplied(0x38, 0x14, 0x0d, 0x38);
    /// The VU's face, scale and needle.
    pub const VU_FACE: Color32 = Color32::from_rgb(0xf1, 0xe8, 0xd0);
    pub const VU_INK: Color32 = Color32::from_rgb(0x2a, 0x24, 0x1e);
    pub const VU_RED: Color32 = Color32::from_rgb(0xc8, 0x3a, 0x26);
    /// The smoked-glass window of the tape deck, its reels and the tape.
    pub const GLASS: Color32 = Color32::from_rgb(0x1d, 0x17, 0x15);
    pub const REEL: Color32 = Color32::from_rgb(0x4a, 0x33, 0x2b);
    pub const REEL_FLANGE: Color32 = Color32::from_rgb(0x5e, 0x42, 0x38);
    pub const TAPE_PACK: Color32 = Color32::from_rgb(0x12, 0x0e, 0x0c);
    pub const TAPE_EDGE: Color32 = Color32::from_rgb(0x6a, 0x4c, 0x40);
}
