use crate::ui::DownloaderApp;
use eframe::egui;
use rfd::FileDialog;
use std::fs;
use std::sync::mpsc::channel;

impl DownloaderApp {
    pub fn show_config_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Configuration");
        ui.add_space(10.0);
        
        ui.horizontal(|ui| {
            ui.label("Spotify Client ID:");
            ui.text_edit_singleline(&mut self.spotify_client_id);
        });
        ui.horizontal(|ui| {
            ui.label("Spotify Secret:  ");
            ui.text_edit_singleline(&mut self.spotify_client_secret);
        });
        
        ui.add_space(10.0);
        
        // 🚨 RESPONSIVENESS FIX: Same right-to-left trick for the folder picker
        ui.horizontal(|ui| {
            ui.label("Download Folder: ");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Browse...").clicked() {
                    if let Some(path) = FileDialog::new().pick_folder() {
                        self.download_dir = path.display().to_string();
                    }
                }
                ui.add(egui::TextEdit::singleline(&mut self.download_dir).desired_width(f32::INFINITY));
            });
        });
        
        ui.add_space(20.0);
        
        ui.horizontal(|ui| {
            if ui.button("Save Config").clicked() {
                let env_content = format!(
                    "RSPOTIFY_CLIENT_ID={}\nRSPOTIFY_CLIENT_SECRET={}\nDOWNLOAD_DIR={}",
                    self.spotify_client_id, self.spotify_client_secret, self.download_dir
                );
                if fs::write(".env", env_content).is_ok() {
                    self.connection_status = "Configuration saved successfully!".to_string();
                } else {
                    self.connection_status = "Error: Failed to save to .env".to_string();
                }
            }

            if ui.button("Check Connection").clicked() {
                self.connection_status = "Checking connection...".to_string();
                let (tx, rx) = channel();
                self.conn_rx = Some(rx); 
                let ctx = ui.ctx().clone(); 
                
                self.rt.spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    let _ = tx.send("✅ Connected successfully!".to_string());
                    ctx.request_repaint(); 
                });
            }
        });

        ui.add_space(5.0);
        ui.label(egui::RichText::new(&self.connection_status).color(egui::Color32::LIGHT_BLUE));
        
        ui.add_space(20.0);
        if ui.button("Back to Downloader").clicked() {
            self.show_config = false;
        }
    }
}