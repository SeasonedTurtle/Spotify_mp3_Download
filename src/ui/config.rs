use crate::library;
use crate::spotify::{self, MetadataSource};
use crate::ui::DownloaderApp;
use eframe::egui;
use rfd::FileDialog;
use std::fs;
use std::sync::mpsc::channel;

impl DownloaderApp {
    pub fn apply_env_from_fields(&self) {
        // Keep the running process in sync with Config fields (writing .env alone is not enough).
        unsafe {
            std::env::set_var("RSPOTIFY_CLIENT_ID", &self.spotify_client_id);
            std::env::set_var("RSPOTIFY_CLIENT_SECRET", &self.spotify_client_secret);
            std::env::set_var("RSPOTIFY_REDIRECT_URI", &self.redirect_uri);
            std::env::set_var("DOWNLOAD_DIR", &self.download_dir);
            std::env::set_var("METADATA_SOURCE", self.metadata_source.as_env_value());
        }
    }

    pub fn show_config_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Configuration");
        ui.add_space(10.0);

        ui.label("Metadata source");
        ui.horizontal(|ui| {
            ui.selectable_value(
                &mut self.metadata_source,
                MetadataSource::Scrape,
                MetadataSource::Scrape.label(),
            );
            ui.selectable_value(
                &mut self.metadata_source,
                MetadataSource::Api,
                MetadataSource::Api.label(),
            );
        });
        ui.label(
            egui::RichText::new(match self.metadata_source {
                MetadataSource::Scrape => {
                    "Uses Spotify's public embed/oEmbed JSON — no keys for normal links. Large playlists may be truncated (~100 tracks)."
                }
                MetadataSource::Api => {
                    "Uses the official Spotify Web API for links. Requires Client ID/Secret."
                }
            })
            .small()
            .weak(),
        );

        ui.add_space(12.0);
        ui.label("Spotify app credentials");
        ui.label(
            egui::RichText::new(
                "Required for API mode and for Import Saved Albums (browser login).",
            )
            .small()
            .weak(),
        );
        ui.horizontal(|ui| {
            ui.label("Client ID:");
            ui.text_edit_singleline(&mut self.spotify_client_id);
        });
        ui.horizontal(|ui| {
            ui.label("Client Secret:");
            ui.text_edit_singleline(&mut self.spotify_client_secret);
        });
        ui.horizontal(|ui| {
            ui.label("Redirect URI:");
            ui.text_edit_singleline(&mut self.redirect_uri);
        });
        ui.label(
            egui::RichText::new(format!(
                "Add this exact Redirect URI in the Spotify Developer Dashboard. Default: {}",
                library::DEFAULT_REDIRECT_URI
            ))
            .small()
            .weak(),
        );

        ui.add_space(10.0);

        ui.horizontal(|ui| {
            ui.label("Download Folder: ");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Browse...").clicked() {
                    if let Some(path) = FileDialog::new().pick_folder() {
                        self.download_dir = path.display().to_string();
                    }
                }
                ui.add(
                    egui::TextEdit::singleline(&mut self.download_dir).desired_width(f32::INFINITY),
                );
            });
        });
        ui.label(
            egui::RichText::new(
                "Files save as: DOWNLOAD_DIR/<Album Name>/<Artist> - <Title>.mp3",
            )
            .small()
            .weak(),
        );

        ui.add_space(20.0);

        ui.horizontal(|ui| {
            if ui.button("Save Config").clicked() {
                self.apply_env_from_fields();
                self.rt.spawn(async {
                    spotify::reset_api_client().await;
                });

                let env_content = format!(
                    "RSPOTIFY_CLIENT_ID={}\nRSPOTIFY_CLIENT_SECRET={}\nRSPOTIFY_REDIRECT_URI={}\nDOWNLOAD_DIR={}\nMETADATA_SOURCE={}\n",
                    self.spotify_client_id,
                    self.spotify_client_secret,
                    self.redirect_uri,
                    self.download_dir,
                    self.metadata_source.as_env_value(),
                );
                match fs::write(".env", env_content) {
                    Ok(()) => {
                        self.connection_status =
                            "Configuration saved (and applied to this session).".to_string()
                    }
                    Err(e) => {
                        self.connection_status =
                            format!("Saved to process env, but .env write failed: {}", e)
                    }
                }
            }

            if ui.button("Check Connection").clicked() {
                self.apply_env_from_fields();
                self.connection_status = "Checking connection...".to_string();
                let (tx, rx) = channel();
                self.conn_rx = Some(rx);
                let ctx = ui.ctx().clone();
                let source = self.metadata_source;

                self.rt.spawn(async move {
                    spotify::reset_api_client().await;
                    let result = match source {
                        MetadataSource::Scrape => crate::scraper::check_scrape_connection().await,
                        MetadataSource::Api => spotify::check_api_connection().await,
                    };
                    let msg = match result {
                        Ok(ok) => ok,
                        Err(e) => format!("❌ {}", e),
                    };
                    let _ = tx.send(msg);
                    ctx.request_repaint();
                });
            }
        });

        ui.add_space(5.0);
        ui.label(egui::RichText::new(&self.connection_status).color(egui::Color32::LIGHT_BLUE));

        ui.add_space(16.0);
        ui.separator();
        ui.add_space(8.0);
        ui.heading("yt-dlp (audio engine)");
        ui.label(
            egui::RichText::new(
                "Looks for yt-dlp on PATH or next to this app. Keep it updated so YouTube downloads keep working. ffmpeg is required for MP3 conversion.",
            )
            .small()
            .weak(),
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Check yt-dlp").clicked() {
                self.yt_dlp_status = "Checking yt-dlp...".to_string();
                let (tx, rx) = channel();
                self.ytdlp_rx = Some(rx);
                let ctx = ui.ctx().clone();
                self.rt.spawn(async move {
                    let msg = match crate::youtube::check_yt_dlp().await {
                        Ok(status) => {
                            let ffmpeg = if status.ffmpeg_ok {
                                "ffmpeg OK"
                            } else {
                                "⚠️ ffmpeg MISSING"
                            };
                            format!(
                                "✅ yt-dlp {} at {}\n{}",
                                status.version,
                                status.path.display(),
                                ffmpeg
                            )
                        }
                        Err(e) => format!("❌ {}", e),
                    };
                    let _ = tx.send(msg);
                    ctx.request_repaint();
                });
            }

            if ui.button("Update yt-dlp").clicked() {
                self.yt_dlp_status = "Updating yt-dlp (yt-dlp -U)...".to_string();
                let (tx, rx) = channel();
                self.ytdlp_rx = Some(rx);
                let ctx = ui.ctx().clone();
                self.rt.spawn(async move {
                    let msg = match crate::youtube::update_yt_dlp().await {
                        Ok(ok) => ok,
                        Err(e) => format!("❌ {}", e),
                    };
                    let _ = tx.send(msg);
                    ctx.request_repaint();
                });
            }
        });
        ui.add_space(5.0);
        ui.label(egui::RichText::new(&self.yt_dlp_status).color(egui::Color32::LIGHT_BLUE));

        ui.add_space(20.0);
        if ui.button("Back to Downloader").clicked() {
            self.show_config = false;
        }
    }
}
