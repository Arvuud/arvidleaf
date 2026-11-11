#![allow(deprecated)] // egui::Id::new, as in arvidlab
//! arvidleaf desktop: project panel, LaTeX editor, PDF preview, templates
//! and history. Look and feel shared with arvidlab (`theme.rs`).

mod app;
mod editor;
mod history_ui;
pub mod offscreen;
mod pdfview;
mod templates_ui;
mod theme;

/// Starts the desktop application (blocks until the window closes).
pub fn run(open: Vec<std::path::PathBuf>) -> Result<(), String> {
    let mut viewport = eframe::egui::ViewportBuilder::default().with_title("arvidleaf").with_app_id("arvidleaf");
    // X11 takes the icon from the window; Wayland looks it up via the app id
    // in the installed .desktop file.
    if let Ok(icon) = eframe::icon_data::from_png_bytes(include_bytes!("../../../icons/arvidleaf-256.png")) {
        viewport = viewport.with_icon(icon);
    }
    let mut options = eframe::NativeOptions {
        viewport: viewport.with_inner_size([1440.0, 900.0]).with_min_inner_size([760.0, 480.0]).with_drag_and_drop(true),
        ..Default::default()
    };
    options.glow_options.vsync = true;
    eframe::run_native("arvidleaf", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, open))))).map_err(|e| e.to_string())
}
