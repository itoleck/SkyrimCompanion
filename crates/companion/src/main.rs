// Hide the console window in release builds on Windows.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod client;
mod config;
mod history;
mod icons;
mod quests;
mod runlog;
mod settings;
mod widgets;

use config::Config;

fn main() -> eframe::Result {
    let config_path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| config::DEFAULT_PATH.to_string());
    let config = Config::load_or_create(&config::resolve(&config_path));
    runlog::line(&format!(
        "Skyrim Companion {} started",
        env!("COMPANION_VERSION")
    ));

    let mut viewport = egui::ViewportBuilder::default()
        .with_title(format!("Skyrim Companion {}", env!("COMPANION_VERSION")))
        .with_inner_size([config.window.width, config.window.height])
        .with_min_inner_size([640.0, 400.0])
        .with_always_on_top_if(config.window.always_on_top);
    // Place the window on the second monitor by giving coordinates inside it,
    // e.g. x = 1920 for a monitor to the right of a 1080p primary.
    if let (Some(x), Some(y)) = (config.window.x, config.window.y) {
        viewport = viewport.with_position([x, y]);
    }
    // Full screen is switched on once the window exists (see CompanionApp::new): set here,
    // Windows keeps the configured size when the monitor's scaling differs from the primary's.

    let options = eframe::NativeOptions {
        viewport,
        // companion.toml owns the window position; a restored maximized geometry from
        // another monitor's DPI can leave the window larger than the screen.
        persist_window: false,
        ..Default::default()
    };
    eframe::run_native(
        "Skyrim Companion",
        options,
        Box::new(move |cc| Ok(Box::new(app::CompanionApp::new(cc, config)))),
    )
}

trait ViewportBuilderExt {
    fn with_always_on_top_if(self, on: bool) -> Self;
}

impl ViewportBuilderExt for egui::ViewportBuilder {
    fn with_always_on_top_if(self, on: bool) -> Self {
        if on {
            self.with_always_on_top()
        } else {
            self
        }
    }
}
