//! The eight inks. DESIGN.md is the authority for what each one may be used for; nothing in
//! the interface is drawn in a colour that is not on this list, and none of them is blended.
use eframe::egui::{self, Color32};

/// Behind the video.
pub const VOID: Color32 = Color32::from_rgb(0, 0, 0);
/// Every bar and sheet.
pub const PANEL: Color32 = Color32::from_rgb(19, 19, 18);
/// A recess that holds a value: entry fields, the timeline rail.
pub const FIELD: Color32 = Color32::from_rgb(33, 32, 30);
/// Rules, empty meter segments, unavailable controls.
pub const RULE: Color32 = Color32::from_rgb(66, 64, 59);
/// Captions, key names, the ruler.
pub const DIM: Color32 = Color32::from_rgb(140, 136, 126);
/// Values and actions.
pub const INK: Color32 = Color32::from_rgb(228, 224, 212);
/// Time: the trim brackets, the selection, a latched key, the one committing action.
pub const TRIM: Color32 = Color32::from_rgb(245, 166, 35);
/// Space: the crop frame and everything that belongs to it.
pub const CROP: Color32 = Color32::from_rgb(92, 200, 214);
/// A problem that needs the person.
pub const DANGER: Color32 = Color32::from_rgb(240, 86, 63);
/// The single translucent surface: it dims the frame outside a crop and behind a sheet.
pub const VEIL: Color32 = Color32::from_black_alpha(168);

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = VOID;
    style.animation_time = 0.0;
    ctx.set_style(style);
}
