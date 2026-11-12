use eframe::egui::{self, Color32, FontFamily, FontId, Stroke, TextStyle};

#[path = "icons.rs"]
mod icons;
pub use icons::Icon;

pub const CANVAS: Color32 = Color32::from_rgb(17, 19, 22);
pub const SURFACE: Color32 = Color32::from_rgb(29, 32, 37);
pub const EDGE: Color32 = Color32::from_rgb(69, 75, 83);
pub const INK: Color32 = Color32::from_rgb(231, 234, 236);
pub const MUTED: Color32 = Color32::from_rgb(166, 175, 185);
pub const TRIM: Color32 = Color32::from_rgb(237, 197, 139);
pub const CROP: Color32 = Color32::from_rgb(159, 197, 206);
pub const DANGER: Color32 = Color32::from_rgb(246, 154, 148);
pub const CONTROL: Color32 = Color32::from_rgb(38, 42, 47);
pub const HOVER: Color32 = Color32::from_rgb(51, 57, 63);
pub const PRESSED: Color32 = Color32::from_rgb(62, 69, 76);
pub const CONTROL_HEIGHT: f32 = 32.0;
pub const ICON_BUTTON_WIDTH: f32 = 36.0;

pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::dark();
    style.visuals.panel_fill = CANVAS;
    style.visuals.window_fill = SURFACE;
    style.visuals.extreme_bg_color = CANVAS;
    style.visuals.override_text_color = Some(INK);
    style.visuals.selection.bg_fill = TRIM.gamma_multiply(0.2);
    style.visuals.selection.stroke = Stroke::new(1.5_f32, TRIM);
    style.visuals.widgets.inactive.bg_fill = CONTROL;
    style.visuals.widgets.inactive.weak_bg_fill = CONTROL;
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, EDGE);
    style.visuals.widgets.hovered.bg_fill = HOVER;
    style.visuals.widgets.hovered.weak_bg_fill = HOVER;
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, MUTED);
    style.visuals.widgets.active.bg_fill = PRESSED;
    style.visuals.widgets.active.weak_bg_fill = PRESSED;
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.5_f32, CROP);
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
    style.spacing.item_spacing = egui::vec2(8.0, 8.0);
    style.spacing.button_padding = egui::vec2(12.0, 6.0);
    style.spacing.interact_size = egui::vec2(32.0, CONTROL_HEIGHT);
    style.spacing.slider_width = 120.0;
    style.interaction.tooltip_delay = 0.35;
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
    let response = ui.add(egui::Button::new(label).min_size(egui::vec2(0.0, CONTROL_HEIGHT)));
    finish_button(ui, response, tip)
}

pub fn primary(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let response = ui
        .scope(|ui| {
            primary_style(ui);
            ui.add(egui::Button::new(label).min_size(egui::vec2(0.0, CONTROL_HEIGHT)))
        })
        .inner;
    focus_ring(ui, &response);
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A native button owns input and keyboard behavior; the icon is only its paint.
pub fn icon_button(ui: &mut egui::Ui, icon: Icon, label: &str, tip: &str) -> egui::Response {
    let response = ui
        .scope(|ui| {
            if icon == Icon::Close {
                let visuals = &mut ui.style_mut().visuals;
                visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(108, 47, 47);
                visuals.widgets.active.weak_bg_fill = Color32::from_rgb(132, 49, 49);
                visuals.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, DANGER);
            }
            ui.add_sized(
                egui::vec2(ICON_BUTTON_WIDTH, CONTROL_HEIGHT),
                egui::Button::new(""),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(response.rect) {
        icons::paint(
            ui.painter(),
            icon,
            egui::Rect::from_center_size(response.rect.center(), egui::vec2(16.0, 16.0)),
            INK,
        );
    }
    finish_button(ui, response, tip)
}

/// The empty state's main action has an explicit content width and vertical rhythm.
pub fn primary_icon(ui: &mut egui::Ui, label: &str, icon: Icon, tip: &str) -> egui::Response {
    labelled_icon(ui, label, icon, tip, true)
}

pub fn secondary_icon(ui: &mut egui::Ui, label: &str, icon: Icon, tip: &str) -> egui::Response {
    labelled_icon(ui, label, icon, tip, false)
}

fn labelled_icon(
    ui: &mut egui::Ui,
    label: &str,
    icon: Icon,
    tip: &str,
    primary: bool,
) -> egui::Response {
    let color = if primary { CANVAS } else { INK };
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        TextStyle::Button.resolve(ui.style()),
        color,
    );
    let content_width = 16.0 + 10.0 + galley.size().x;
    let size = if primary {
        egui::vec2((content_width + 40.0).max(168.0), 44.0)
    } else {
        egui::vec2(content_width + 24.0, CONTROL_HEIGHT)
    };
    let response = ui
        .scope(|ui| {
            if primary {
                primary_style(ui);
            }
            ui.add_sized(size, egui::Button::new(""))
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), label)
    });
    if ui.is_rect_visible(response.rect) {
        let left = response.rect.center().x - content_width * 0.5;
        icons::paint(
            ui.painter(),
            icon,
            egui::Rect::from_min_size(
                egui::pos2(left, response.rect.center().y - 8.0),
                egui::vec2(16.0, 16.0),
            ),
            color,
        );
        ui.painter().galley(
            egui::pos2(
                left + 26.0,
                response.rect.center().y - galley.size().y * 0.5,
            ),
            galley,
            color,
        );
    }
    finish_button(ui, response, tip)
}

fn primary_style(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.override_text_color = Some(CANVAS);
    for (widget, fill) in [
        (&mut visuals.widgets.inactive, TRIM),
        (
            &mut visuals.widgets.hovered,
            Color32::from_rgb(246, 211, 161),
        ),
        (
            &mut visuals.widgets.active,
            Color32::from_rgb(215, 173, 113),
        ),
    ] {
        widget.bg_fill = fill;
        widget.weak_bg_fill = fill;
        widget.bg_stroke = Stroke::new(1.0_f32, fill);
        widget.fg_stroke.color = CANVAS;
    }
}

fn focus_ring(ui: &egui::Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            5,
            Stroke::new(1.5_f32, CROP),
            egui::StrokeKind::Outside,
        );
    }
}

fn finish_button(ui: &egui::Ui, response: egui::Response, tip: &str) -> egui::Response {
    focus_ring(ui, &response);
    let response = if response.has_focus() {
        response.show_tooltip_text(tip);
        response
    } else {
        response.on_hover_text(tip)
    };
    response
        .on_disabled_hover_text(tip)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}
