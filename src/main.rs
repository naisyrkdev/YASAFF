#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[macro_use]
mod i18n;
mod addons;
mod app;
mod appstore;
mod autostart;
mod box3d;
mod case3d;
mod crt;
mod disc3d;
mod fonts;
mod fsutil;
mod initial;
mod global_mouse;
mod apps;
mod autoplay;
mod input;
mod launcher;
mod library;
mod logging;
mod metadata;
mod mock_game;
mod object3d;
mod physical;
mod portable;
mod profiles;
mod saves;
mod device_saves;
mod scan;
mod settings;
mod sound;
mod stores;
mod taskbar;
mod theme;
mod theme_code;
mod tools;
mod video;
mod watch;

fn main() -> eframe::Result {
    // Before logging::init(), which opens its log file under profiles::root(): otherwise, on a
    // brand-new data folder, that open file makes profiles::init() fail to "migrate" it below.
    let mut warnings = profiles::init();
    logging::init();
    let (settings, settings_warning) = settings::Settings::load();
    i18n::set(settings.language);
    warnings.extend(settings_warning.map(|e| tr!("Could not read settings: {error}", error = e)));
    let args: Vec<String> = std::env::args().collect();
    if let Some(dir) = args.iter().position(|a| a == mock_game::FLAG).and_then(|i| args.get(i + 1)) {
        return mock_game::run(std::path::Path::new(dir));
    }
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
    let result = eframe::run_native("YASAFF", options, Box::new(move |cc| Ok(Box::new(app::App::new(cc, settings, warnings, fullscreen, disc)))));
    profiles::restart_if_requested();
    result
}
