mod config;
mod main_screen;

use crate::spotify::MetadataSource;
use eframe::egui;
use std::env;
use std::sync::mpsc::Receiver;

/// Ok((track_urls, status_message))
type LoadOutcome = Result<(Vec<String>, String), String>;

pub struct DownloaderApp {
    pub(crate) input_link: String,
    pub(crate) show_config: bool,
    pub(crate) spotify_client_id: String,
    pub(crate) spotify_client_secret: String,
    pub(crate) redirect_uri: String,
    pub(crate) download_dir: String,
    pub(crate) metadata_source: MetadataSource,
    pub(crate) connection_status: String,
    pub(crate) conn_rx: Option<Receiver<String>>,
    pub(crate) loaded_links: Vec<String>,
    pub(crate) load_rx: Option<Receiver<LoadOutcome>>,
    pub(crate) load_status: String,
    pub(crate) downloaded_songs: Vec<String>,
    pub(crate) download_rx: Option<Receiver<String>>,
    pub(crate) download_status: String,
    pub(crate) yt_dlp_status: String,
    pub(crate) ytdlp_rx: Option<Receiver<String>>,
    pub(crate) is_loading: bool,
    pub(crate) is_downloading: bool,
    pub(crate) rt: tokio::runtime::Runtime,
}

impl Default for DownloaderApp {
    fn default() -> Self {
        dotenvy::dotenv().ok();
        let id = env::var("RSPOTIFY_CLIENT_ID").unwrap_or_default();
        let secret = env::var("RSPOTIFY_CLIENT_SECRET").unwrap_or_default();
        let dir = env::var("DOWNLOAD_DIR").unwrap_or_else(|_| "downloads".to_string());
        let redirect_uri = env::var("RSPOTIFY_REDIRECT_URI")
            .unwrap_or_else(|_| crate::library::DEFAULT_REDIRECT_URI.to_string())
            .trim()
            .trim_end_matches('/')
            .trim_end_matches("/callback")
            .to_string();
        let metadata_source = MetadataSource::from_env();

        Self {
            input_link: String::new(),
            show_config: false,
            spotify_client_id: id,
            spotify_client_secret: secret,
            redirect_uri,
            download_dir: dir,
            metadata_source,
            connection_status: String::from("Ready to check."),
            conn_rx: None,
            loaded_links: Vec::new(),
            load_rx: None,
            load_status: String::new(),
            downloaded_songs: Vec::new(),
            download_rx: None,
            download_status: String::new(),
            yt_dlp_status: String::from("Click Check yt-dlp to verify the downloader."),
            ytdlp_rx: None,
            is_loading: false,
            is_downloading: false,
            rt: tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime"),
        }
    }
}

impl eframe::App for DownloaderApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if let Some(rx) = &self.conn_rx {
            if let Ok(msg) = rx.try_recv() {
                self.connection_status = msg;
                self.conn_rx = None;
            }
        }

        if let Some(rx) = &self.ytdlp_rx {
            if let Ok(msg) = rx.try_recv() {
                self.yt_dlp_status = msg;
                self.ytdlp_rx = None;
            }
        }

        if let Some(rx) = &self.load_rx {
            if let Ok(result) = rx.try_recv() {
                self.is_loading = false;
                match result {
                    Ok((tracks, status)) => {
                        let before = self.loaded_links.len();
                        for track in tracks {
                            if !self.loaded_links.contains(&track) {
                                self.loaded_links.push(track);
                            }
                        }
                        let added = self.loaded_links.len().saturating_sub(before);
                        self.load_status = format!(
                            "{} Added {} track(s). Queue now has {}.",
                            status,
                            added,
                            self.loaded_links.len()
                        );
                    }
                    Err(e) => self.load_status = format!("❌ Error: {}", e),
                }
                self.load_rx = None;
            }
        }

        if let Some(rx) = &self.download_rx {
            let mut finished = false;
            while let Ok(msg) = rx.try_recv() {
                if let Some(status) = msg.strip_prefix("STATUS:") {
                    self.download_status = status.to_string();
                    if status.contains("finished") {
                        finished = true;
                    }
                } else {
                    self.downloaded_songs.push(msg);
                }
            }
            if finished {
                self.is_downloading = false;
            }
        }

        if self.is_loading
            || self.is_downloading
            || self.conn_rx.is_some()
            || self.ytdlp_rx.is_some()
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.show_config {
                self.show_config_screen(ui);
            } else {
                self.show_main_screen(ui);
            }
        });
    }
}
