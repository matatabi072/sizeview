#![windows_subsystem = "windows"]

mod app;
mod build_info;
mod cache;
mod config;
mod fonts;
mod fs_model;
mod fsutil;
mod size_worker;
mod theme;
mod ui;
#[cfg(windows)]
mod icons;
#[cfg(windows)]
mod winfs;

use eframe::egui;

fn main() -> eframe::Result<()> {
    let cfg = config::AppConfig::load();
    let init_size = [cfg.window_w, cfg.window_h];
    let cfg_for_app = cfg.clone();

    let vb = egui::ViewportBuilder::default()
        .with_title(build_info::WINDOW_TITLE)
        .with_inner_size(init_size)
        .with_min_inner_size([600.0, 400.0])
        .with_app_id(build_info::APP_ID);

    let opts = eframe::NativeOptions {
        viewport: vb,
        ..Default::default()
    };

    eframe::run_native(
        build_info::WINDOW_TITLE,
        opts,
        Box::new(move |cc| Ok(Box::new(app::App::new(cc, cfg_for_app)))),
    )
}
