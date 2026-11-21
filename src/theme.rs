//! Black & white theme + one accent colour. Colours are Win32 COLORREF values (0x00BBGGRR).
pub const fn rgb(r: u32, g: u32, b: u32) -> u32 {
    r | (g << 8) | (b << 16)
}

pub const BG: u32 = rgb(0x0A, 0x0A, 0x0A);
pub const CARD: u32 = rgb(0x14, 0x14, 0x14);
pub const CARD2: u32 = rgb(0x1F, 0x1F, 0x1F);
pub const BORDER: u32 = rgb(0x2E, 0x2E, 0x2E);
pub const FG: u32 = rgb(0xFF, 0xFF, 0xFF);
pub const MUTED: u32 = rgb(0x9A, 0x9A, 0x9A);
pub const ACCENT: u32 = rgb(0xFF, 0x6A, 0x00);
pub const ACCENT_HOVER: u32 = rgb(0xFF, 0x85, 0x33);
pub const REC: u32 = rgb(0xFF, 0x3B, 0x30);
pub const KNOB_HOVER: u32 = rgb(0xDD, 0xDD, 0xDD);

pub fn mix(a: u32, b: u32, t: f32) -> u32 {
    let ch = |v: u32, s: u32| ((v >> s) & 0xFF) as f32;
    let m = |s: u32| (ch(a, s) + (ch(b, s) - ch(a, s)) * t) as u32;
    m(0) | (m(8) << 8) | (m(16) << 16)
}
