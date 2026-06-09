mod config;
mod main_screen;

use eframe::egui;
use std::env;
use std::sync::mpsc::Receiver;

pub struct DownloaderApp {
    // We use pub(crate) so our sub-files can access these variables!
    pub(crate) input_link: String,
    pub(crate) show_config: bool,
    pub(crate) spotify_client_id: String,
    pub(crate) spotify_client_secret: String,
    pub(crate) download_dir: String, 
    pub(crate) connection_status: String,
    pub(crate) conn_rx: Option<Receiver<String>>, 
    pub(crate) loaded_links: Vec<String>,
    pub(crate) load_rx: Option<Receiver<Result<Vec<String>, String>>>,
    pub(crate) load_status: String,
    pub(crate) downloaded_songs: Vec<String>,
    pub(crate) download_rx: Option<Receiver<String>>,
    pub(crate) download_status: String,
    pub(crate) rt: tokio::runtime::Runtime,
}

impl Default for DownloaderApp {
    fn default() -> Self {
        dotenvy::dotenv().ok();
        let id = env::var("RSPOTIFY_CLIENT_ID").unwrap_or_default();
        let secret = env::var("RSPOTIFY_CLIENT_SECRET").unwrap_or_default();
        let dir = env::var("DOWNLOAD_DIR").unwrap_or_else(|_| "downloads".to_string());

        Self {
            input_link: String::new(),
            show_config: false,
            spotify_client_id: id,
            spotify_client_secret: secret,
            download_dir: dir,
            connection_status: String::from("Ready to check."),
            conn_rx: None,
            loaded_links: Vec::new(),
            load_rx: None,
            load_status: String::new(),
            downloaded_songs: Vec::new(),
            download_rx: None,
            download_status: String::new(),
            rt: tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime"),
        }
    }
}

impl eframe::App for DownloaderApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        
        // --- CHECK BACKGROUND THREADS ---
        if let Some(rx) = &self.conn_rx {
            if let Ok(msg) = rx.try_recv() {
                self.connection_status = msg; 
                self.conn_rx = None;          
            }
        }

        if let Some(rx) = &self.load_rx {
            if let Ok(result) = rx.try_recv() {
                match result {
                    Ok(tracks) => {
                        self.loaded_links.extend(tracks);
                        self.load_status = format!("✅ Successfully loaded {} tracks!", self.loaded_links.len());
                    }
                    Err(e) => self.load_status = format!("❌ Error: {}", e),
                }
                self.load_rx = None; 
            }
        }

        if let Some(rx) = &self.download_rx {
            while let Ok(msg) = rx.try_recv() {
                if msg.starts_with("STATUS:") {
                    self.download_status = msg.replace("STATUS:", "");
                } else {
                    self.downloaded_songs.push(msg);
                }
            }
        }

        // --- TOP HEADER ---
        egui::Panel::top("header_panel").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("Spotify Downloader");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Exit").clicked() {
                        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    if ui.button("Config").clicked() {
                        self.show_config = !self.show_config;
                    }
                    egui::widgets::global_theme_preference_buttons(ui);
                });
            });
        });

        // --- CENTRAL PANEL ROUTING ---
        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.show_config {
                self.show_config_screen(ui); // Calls the code in config.rs
            } else {
                self.show_main_screen(ui);   // Calls the code in main_screen.rs
            }
        });
    }
}