#![windows_subsystem = "windows"]

use dotenvy::dotenv;
use eframe::egui;

mod downloader;
mod library;
mod metadata_cache;
mod ocr_import;
mod scraper;
mod spotify;
mod ui;
mod youtube;

fn main() -> eframe::Result {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_target(false)
        .with_max_level(tracing::Level::INFO)
        .init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([960.0, 780.0])
            .with_min_inner_size([700.0, 520.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Rusty Spotify Downloader",
        options,
        Box::new(|_cc| Ok(Box::new(ui::DownloaderApp::default()))),
    )
}
