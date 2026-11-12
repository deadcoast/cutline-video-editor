use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};

pub const CANVAS: Color32 = Color32::from_rgb(17, 19, 22);
pub const SURFACE: Color32 = Color32::from_rgb(29, 32, 37);
pub const EDGE: Color32 = Color32::from_rgb(69, 75, 83);
pub const INK: Color32 = Color32::from_rgb(231, 234, 236);
pub const MUTED: Color32 = Color32::from_rgb(166, 175, 185);
pub const TRIM: Color32 = Color32::from_rgb(237, 197, 139);
pub const CROP: Color32 = Color32::from_rgb(159, 197, 206);
pub const DANGER: Color32 = Color32::from_rgb(246, 154, 148);

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = CANVAS;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.override_text_color = Some(INK);
    style.visuals.selection.bg_fill = TRIM.gamma_multiply(0.2);
    style.visuals.selection.stroke = Stroke::new(1.5_f32, TRIM);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(50, 56, 63);
    style.visuals.widgets.inactive.weak_bg_fill = SURFACE;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, EDGE);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(53, 59, 66);
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(53, 59, 66);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, CROP);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(66, 72, 78);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, TRIM);
    for widget in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.noninteractive,
    ] {
        widget.corner_radius = egui::CornerRadius::same(3);
        widget.fg_stroke.color = INK;
    }
    style.visuals.window_corner_radius = egui::CornerRadius::same(6);
    style.visuals.window_stroke = Stroke::new(1.0_f32, EDGE);
    style.spacing.item_spacing = egui::vec2(8.0, 10.0);
    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.interact_size = egui::vec2(32.0, 32.0);
    style.spacing.slider_width = 120.0;
    style.animation_time = 0.0;
    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(14.0, FontFamily::Proportional));
    style.text_styles.insert(
        TextStyle::Button,
        FontId::new(14.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Monospace,
        FontId::new(13.0, FontFamily::Monospace),
    );
    style.text_styles.insert(
        TextStyle::Small,
        FontId::new(12.0, FontFamily::Proportional),
    );
    style.text_styles.insert(
        TextStyle::Heading,
        FontId::new(20.0, FontFamily::Proportional),
    );
    ctx.set_style(style);
}

pub fn button(ui: &mut egui::Ui, label: &str, tip: &str) -> egui::Response {
    ui.add(egui::Button::new(label).frame(false))
        .on_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    ui.add(egui::Button::new(egui::RichText::new(label).color(CANVAS)).fill(TRIM))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
