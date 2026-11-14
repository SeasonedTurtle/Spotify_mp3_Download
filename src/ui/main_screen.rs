use crate::ui::DownloaderApp;
use eframe::egui;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::channel;
use std::sync::Arc;
use tokio::sync::Semaphore;

impl DownloaderApp {
    pub fn show_main_screen(&mut self, ui: &mut egui::Ui) {
        ui.add_space(12.0);

        ui.horizontal(|ui| {
            ui.label("Metadata:");
            ui.strong(self.metadata_source.label());
            ui.label(
                egui::RichText::new("(change in Config)")
                    .small()
                    .weak(),
            );
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Enter Spotify Link:");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let load_enabled = !self.input_link.trim().is_empty() && !self.is_loading;
                if ui
                    .add_enabled(load_enabled, egui::Button::new("Load"))
                    .clicked()
                {
                    self.is_loading = true;
                    self.load_status = match self.metadata_source {
                        crate::spotify::MetadataSource::Scrape => {
                            "Scraping embed JSON for tracks...".to_string()
                        }
                        crate::spotify::MetadataSource::Api => {
                            "Contacting Spotify API...".to_string()
                        }
                    };

                    let url_to_load = self.input_link.trim().to_string();
                    self.input_link.clear();

                    let (tx, rx) = channel();
                    self.load_rx = Some(rx);
                    let ctx = ui.ctx().clone();
                    let source = self.metadata_source;

                    self.rt.spawn(async move {
                        let result =
                            crate::spotify::expand_url_to_tracks(&url_to_load, source).await;
                        let _ = tx.send(result);
                        ctx.request_repaint();
                    });
                }

                ui.add(
                    egui::TextEdit::singleline(&mut self.input_link)
                        .desired_width(f32::INFINITY)
                        .hint_text("https://open.spotify.com/track/..."),
                );
            });
        });

        if !self.load_status.is_empty() {
            ui.add_space(5.0);
            ui.label(
                egui::RichText::new(&self.load_status).color(egui::Color32::LIGHT_BLUE),
            );
        }

        ui.add_space(16.0);
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
                            self.is_downloading = false;
                        }

                        let can_download =
                            !self.loaded_links.is_empty() && !self.is_downloading;
                        if ui
                            .add_enabled(can_download, egui::Button::new("Download All"))
                            .clicked()
                        {
                            let total_tracks = self.loaded_links.len();
                            self.is_downloading = true;
                            self.download_status =
                                format!("Starting download of {} tracks...", total_tracks);

                            let (tx, rx) = channel();
                            self.download_rx = Some(rx);
                            let ctx = ui.ctx().clone();
                            let queue = self.loaded_links.clone();
                            let target_dir = self.download_dir.clone();
                            let source = self.metadata_source;

                            self.rt.spawn(async move {
                                let bouncer = Arc::new(Semaphore::new(5));
                                let completed = Arc::new(AtomicUsize::new(0));
                                let mut tasks = Vec::new();

                                for (index, track_url) in queue.into_iter().enumerate() {
                                    let permit = match bouncer.clone().acquire_owned().await {
                                        Ok(p) => p,
                                        Err(_) => break,
                                    };
                                    let tx_clone = tx.clone();
                                    let ctx_clone = ctx.clone();
                                    let dir_clone = target_dir.clone();
                                    let completed_clone = completed.clone();

                                    let task = tokio::spawn(async move {
                                        let _ = tx_clone.send(format!(
                                            "STATUS:Queued/downloading track {}/{}...",
                                            index + 1,
                                            total_tracks
                                        ));
                                        ctx_clone.request_repaint();

                                        let result = crate::downloader::download_from_spotify(
                                            &track_url,
                                            &dir_clone,
                                            source,
                                        )
                                        .await;

                                        let done = completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
                                        match result {
                                            Ok(msg) => {
                                                let _ = tx_clone.send(format!("✅ {}", msg));
                                            }
                                            Err(e) => {
                                                let _ = tx_clone.send(format!(
                                                    "❌ Track {}: {}",
                                                    index + 1,
                                                    e
                                                ));
                                            }
                                        }
                                        let _ = tx_clone.send(format!(
                                            "STATUS:Progress {}/{} finished...",
                                            done, total_tracks
                                        ));
                                        ctx_clone.request_repaint();
                                        drop(permit);
                                    });
                                    tasks.push(task);
                                }

                                for task in tasks {
                                    let _ = task.await;
                                }
                                let _ = tx.send(format!(
                                    "STATUS:🎉 All {} tracks finished!",
                                    total_tracks
                                ));
                                ctx.request_repaint();
                            });
                        }
                    });
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("loaded_links")
                    .show(ui, |ui| {
                        if self.loaded_links.is_empty() {
                            ui.label(
                                egui::RichText::new("No links loaded yet.")
                                    .italics()
                                    .weak(),
                            );
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
                        ui.with_layout(
                            egui::Layout::right_to_left(egui::Align::Center),
                            |ui| {
                                ui.label(
                                    egui::RichText::new(&self.download_status)
                                        .color(egui::Color32::LIGHT_GREEN),
                                );
                            },
                        );
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("downloaded_links")
                    .show(ui, |ui| {
                        if self.downloaded_songs.is_empty() {
                            ui.label(
                                egui::RichText::new("No songs downloaded yet.")
                                    .italics()
                                    .weak(),
                            );
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
