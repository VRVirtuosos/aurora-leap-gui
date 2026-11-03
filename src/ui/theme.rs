//! UI Theme and Visual Styling Constants.
#![allow(dead_code)]

use egui::{Color32, Frame, Margin, Rounding, Stroke};

pub const COLOR_BG_DARK: Color32 = Color32::from_rgb(18, 20, 24);
pub const COLOR_CARD_BG: Color32 = Color32::from_rgb(26, 30, 38);
pub const COLOR_CARD_BORDER: Color32 = Color32::from_rgb(42, 48, 60);

pub const COLOR_ACCENT: Color32 = Color32::from_rgb(76, 145, 255);
pub const COLOR_ACCENT_HOVER: Color32 = Color32::from_rgb(105, 165, 255);

pub const COLOR_SUCCESS: Color32 = Color32::from_rgb(52, 199, 89);
pub const COLOR_SUCCESS_BG: Color32 = Color32::from_rgb(20, 48, 30);

pub const COLOR_DANGER: Color32 = Color32::from_rgb(255, 69, 58);
pub const COLOR_DANGER_BG: Color32 = Color32::from_rgb(56, 24, 24);

pub const COLOR_WARNING: Color32 = Color32::from_rgb(255, 179, 64);
pub const COLOR_WARNING_BG: Color32 = Color32::from_rgb(52, 40, 16);

pub const COLOR_TEXT_MUTED: Color32 = Color32::from_rgb(140, 148, 160);

pub fn card_frame() -> Frame {
    Frame::none()
        .fill(COLOR_CARD_BG)
        .stroke(Stroke::new(1.0_f32, COLOR_CARD_BORDER))
        .rounding(Rounding::same(8.0))
        .inner_margin(Margin::same(12.0))
}

pub fn inner_box_frame() -> Frame {
    Frame::none()
        .fill(Color32::from_rgb(20, 23, 29))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(34, 39, 48)))
        .rounding(Rounding::same(6.0))
        .inner_margin(Margin::same(8.0))
}
