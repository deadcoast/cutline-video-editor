#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
mod ui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Cutline")
            .with_icon(
                eframe::icon_data::from_png_bytes(include_bytes!("../assets/cutline.png"))
                    .expect("embedded app icon"),
            )
            .with_inner_size([1120.0, 720.0])
            .with_min_inner_size([720.0, 480.0])
            .with_decorations(false)
            .with_drag_and_drop(true),
        vsync: true,
        ..Default::default()
    };
    eframe::run_native(
        "Cutline",
        options,
        Box::new(|cc| Ok(Box::new(ui::App::new(cc)))),
    )
}
