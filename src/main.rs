#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod app;
mod appstore;
mod box3d;
mod disc3d;
mod fonts;
mod apps;
mod input;
mod launcher;
mod library;
mod metadata;
mod object3d;
mod physical;
mod saves;
mod scan;
mod settings;
mod stores;
mod theme;
mod watch;

fn main() -> eframe::Result {
    let settings = settings::Settings::load();
    i18n::set(settings.language);
    let args: Vec<String> = std::env::args().collect();
    let fullscreen = settings.start_fullscreen || args.iter().any(|a| a == "--fullscreen");
    let disc = args.iter().position(|a| a == "--disc").and_then(|i| args.get(i + 1)).map(std::path::PathBuf::from);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("YASAFF")
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([800.0, 450.0])
            .with_fullscreen(fullscreen)
            .with_icon(eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon-256.png")).expect("valid icon")),
        ..Default::default()
    };
    eframe::run_native("YASAFF", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, settings, fullscreen, disc)))))
}
