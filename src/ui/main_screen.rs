use crate::ui::DownloaderApp;
use eframe::egui;
use std::sync::mpsc::channel;
use std::sync::Arc;
use tokio::sync::Semaphore;

impl DownloaderApp {
    pub fn show_main_screen(&mut self, ui: &mut egui::Ui) {
        ui.add_space(20.0);
        
        // 🚨 RESPONSIVENESS FIX: Right-to-Left layout makes the text box dynamically fill all space!
        ui.horizontal(|ui| {
            ui.label("Enter Spotify Link:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Load").clicked() && !self.input_link.is_empty() {
                    self.load_status = "Contacting Spotify API...".to_string();
                    let url_to_load = self.input_link.clone();
                    self.input_link.clear(); 
                    
                    let (tx, rx) = channel();
                    self.load_rx = Some(rx);
                    let ctx = ui.ctx().clone();
                    
                    self.rt.spawn(async move {
                        let result = crate::spotify::expand_url_to_tracks(&url_to_load).await;
                        let _ = tx.send(result);
                        ctx.request_repaint(); 
                    });
                }
                
                // This tells the text box to perfectly stretch to fill the gap
                ui.add(egui::TextEdit::singleline(&mut self.input_link).desired_width(f32::INFINITY));
            });
        });
        
        if !self.load_status.is_empty() {
            ui.add_space(5.0);
            ui.label(egui::RichText::new(&self.load_status).color(egui::Color32::LIGHT_BLUE));
        }

        ui.add_space(20.0);
        ui.separator();
        ui.add_space(10.0);

        ui.columns(2, |columns| {
            columns[0].group(|ui| {
                ui.set_min_height(300.0);
                ui.horizontal(|ui| {
                    ui.heading("Loaded Links");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear").clicked() {
                            self.loaded_links.clear();
                            self.load_status = String::new();
                            self.downloaded_songs.clear();
                            self.download_status = String::new();
                        }
                        
                        if ui.button("Download All").clicked() && !self.loaded_links.is_empty() {
                            let total_tracks = self.loaded_links.len();
                            self.download_status = format!("Starting download of {} tracks...", total_tracks);
                            
                            let (tx, rx) = channel();
                            self.download_rx = Some(rx);
                            let ctx = ui.ctx().clone();
                            let queue = self.loaded_links.clone();
                            let target_dir = self.download_dir.clone();
                            
                            self.rt.spawn(async move {
                                let bouncer = Arc::new(Semaphore::new(5));
                                let mut tasks = Vec::new();
                                
                                for (index, track_url) in queue.into_iter().enumerate() {
                                    let permit = bouncer.clone().acquire_owned().await.unwrap();
                                    let tx_clone = tx.clone();
                                    let ctx_clone = ctx.clone();
                                    let dir_clone = target_dir.clone(); 
                                    
                                    let task = tokio::spawn(async move {
                                        let _ = tx_clone.send(format!("STATUS:Downloading {}/{}...", index + 1, total_tracks));
                                        ctx_clone.request_repaint();
                                        
                                        // 🚨 THE FIX: We now pass dir_clone directly into the downloader!
                                        match crate::downloader::download_from_spotify(&track_url, &dir_clone).await {
                                            Ok(_) => { let _ = tx_clone.send(format!("✅ Track {}", index + 1)); }
                                            Err(e) => { let _ = tx_clone.send(format!("❌ Failed Track {}: {}", index + 1, e)); }
                                        }
                                        
                                        ctx_clone.request_repaint();
                                        drop(permit); 
                                    });
                                    tasks.push(task);
                                }
                                
                                for task in tasks { let _ = task.await; }
                                let _ = tx.send(format!("STATUS:🎉 All {} tracks complete!", total_tracks));
                                ctx.request_repaint();
                            });
                        }
                    });
                });
                ui.separator();
                
                egui::ScrollArea::vertical().id_source("loaded_links").show(ui, |ui| {
                    if self.loaded_links.is_empty() {
                        ui.label(egui::RichText::new("No links loaded yet.").italics().weak());
                    } else {
                        for (index, link) in self.loaded_links.iter().enumerate() {
                            ui.label(format!("{}. {}", index + 1, link));
                        }
                    }
                });
            });
            
            columns[1].group(|ui| {
                ui.set_min_height(300.0);
                ui.horizontal(|ui| {
                    ui.heading("Downloaded Songs");
                    if !self.download_status.is_empty() {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(egui::RichText::new(&self.download_status).color(egui::Color32::LIGHT_GREEN));
                        });
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical().id_source("downloaded_links").show(ui, |ui| {
                    if self.downloaded_songs.is_empty() {
                        ui.label(egui::RichText::new("No songs downloaded yet.").italics().weak());
                    } else {
                        for song in self.downloaded_songs.iter() {
                            ui.label(song);
                        }
                    }
                });
            });
        });
    }
}