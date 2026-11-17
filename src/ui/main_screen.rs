use crate::ui::DownloaderApp;
use eframe::egui;
use rfd::FileDialog;
use std::path::PathBuf;
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
            ui.label(egui::RichText::new("(change in Config)").small().weak());
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
                        let result = crate::spotify::expand_url_to_tracks(&url_to_load, source)
                            .await
                            .map(|tracks| {
                                (
                                    tracks,
                                    "✅ Link expanded.".to_string(),
                                )
                            });
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

        ui.add_space(8.0);
        ui.horizontal(|ui| {
            let can_import = !self.is_loading
                && !self.spotify_client_id.trim().is_empty()
                && !self.spotify_client_secret.trim().is_empty();
            if ui
                .add_enabled(can_import, egui::Button::new("Import Saved Albums"))
                .on_hover_text(
                    "Login once to list saved albums, then expand tracks via embed scrape (avoids API 429 on track lists).",
                )
                .clicked()
            {
                self.apply_env_from_fields();
                self.is_loading = true;
                self.load_status =
                    "Login (if needed), list saved albums, then scrape each album's tracks..."
                        .to_string();

                let (tx, rx) = channel();
                self.load_rx = Some(rx);
                let ctx = ui.ctx().clone();

                self.rt.spawn(async move {
                    let result = match crate::library::import_saved_albums().await {
                        Ok(imported) => {
                            let preview = if imported.album_names.len() <= 5 {
                                imported.album_names.join(", ")
                            } else {
                                format!(
                                    "{}, … (+{} more)",
                                    imported.album_names[..5].join(", "),
                                    imported.album_names.len() - 5
                                )
                            };
                            Ok((
                                imported.track_urls,
                                format!(
                                    "✅ Imported {} saved album(s) via scrape [{}].",
                                    imported.album_count, preview
                                ),
                            ))
                        }
                        Err(e) => Err(e),
                    };
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
            }

            if self.spotify_client_id.trim().is_empty()
                || self.spotify_client_secret.trim().is_empty()
            {
                ui.label(
                    egui::RichText::new("API login optional — prefer Paste albums (scrape) below.")
                        .small()
                        .weak(),
                );
            }
        });

        ui.add_space(12.0);
        ui.separator();
        ui.heading("Paste albums / browser scraper");
        ui.label(
            egui::RichText::new(
                "No API rate limits. Use this instead of Import Saved Albums for big libraries.",
            )
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        ui.label(
            "1) Open https://open.spotify.com/collection/albums (logged in)
\
             2) Copy console scraper → paste in console (F12) → Enter
\
             3) Leave it alone while it scrolls (no API / no 429)
\
             4) When TOTAL prints, run: copy(window.__spotifyAlbumUrls)
\
             5) Paste here → Import pasted albums → Download All
\
             Ignore CORS errors. If count looks low, scroll manually and run again.",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Copy console scraper").clicked() {
                ui.ctx()
                    .copy_text(crate::library::BROWSER_ALBUM_SCRAPER_JS.to_string());
                self.load_status =
                    "📋 Console scraper copied. Paste it on open.spotify.com/collection/albums console."
                        .to_string();
            }
            if ui.button("Open collection/albums").clicked() {
                let _ = webbrowser::open("https://open.spotify.com/collection/albums");
            }
        });
        ui.add_space(6.0);
        ui.add(
            egui::TextEdit::multiline(&mut self.paste_albums)
                .desired_width(f32::INFINITY)
                .desired_rows(5)
                .hint_text(
                    "Paste album URLs here, one per line:\nhttps://open.spotify.com/album/...",
                ),
        );
        ui.add_space(6.0);
        let can_paste = !self.is_loading && !self.paste_albums.trim().is_empty();
        if ui
            .add_enabled(
                can_paste,
                egui::Button::new("Import pasted albums (scrape)"),
            )
            .on_hover_text(
                "Expands every pasted album URL using public embed JSON — no Spotify API track calls.",
            )
            .clicked()
        {
            self.is_loading = true;
            self.load_status =
                "Scraping pasted albums via embed JSON (no API track listing)...".to_string();
            let pasted = self.paste_albums.clone();
            let (tx, rx) = channel();
            self.load_rx = Some(rx);
            let ctx = ui.ctx().clone();
            self.rt.spawn(async move {
                let result = match crate::library::import_albums_from_pasted_text(&pasted).await {
                    Ok(imported) => {
                        let n_tracks = imported.track_urls.len();
                        Ok((
                            imported.track_urls,
                            format!(
                                "✅ Scraped {} album(s) from paste ({} tracks).",
                                imported.album_count, n_tracks
                            ),
                        ))
                    }
                    Err(e) => Err(e),
                };
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        }

        ui.add_space(14.0);
        ui.separator();
        ui.heading("Screenshot OCR import");
        ui.label(
            egui::RichText::new(
                "No Spotify API and no page JavaScript. Needs tesseract installed on your PC.",
            )
            .small()
            .weak(),
        );
        ui.add_space(4.0);
        ui.label(
            "1) Open Your Library → Albums (web or desktop)\n\
             2) Scroll slowly and take overlapping screenshots (Win+Shift+S / macOS Cmd+Shift+4)\n\
             3) Put every PNG/JPG into one folder\n\
             4) Choose that folder → Import from screenshots\n\
             5) App OCRs titles, finds Spotify album links, scrapes tracks → Download All",
        );
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui.button("Choose screenshot folder").clicked() {
                if let Some(path) = FileDialog::new().pick_folder() {
                    self.screenshot_dir = path.display().to_string();
                }
            }
            if ui.button("Check OCR (tesseract)").clicked() {
                self.load_status = "Checking tesseract + OCR resolve pipeline...".to_string();
                let (tx, rx) = channel();
                self.load_rx = Some(rx);
                let ctx = ui.ctx().clone();
                self.is_loading = true;
                self.rt.spawn(async move {
                    let result = match crate::ocr_import::check_ocr_pipeline().await {
                        Ok(msg) => Ok((Vec::new(), msg)),
                        Err(e) => Err(e),
                    };
                    let _ = tx.send(result);
                    ctx.request_repaint();
                });
            }
        });
        ui.add_space(4.0);
        ui.add(
            egui::TextEdit::singleline(&mut self.screenshot_dir)
                .desired_width(f32::INFINITY)
                .hint_text("Path to folder of album screenshots…"),
        );
        ui.add_space(6.0);
        let can_ocr = !self.is_loading && !self.screenshot_dir.trim().is_empty();
        if ui
            .add_enabled(
                can_ocr,
                egui::Button::new("Import from screenshots (OCR)"),
            )
            .on_hover_text(
                "Runs tesseract on every image, resolves album names to Spotify links via web search, then scrapes track lists.",
            )
            .clicked()
        {
            self.is_loading = true;
            self.load_status =
                "OCR'ing screenshots → resolving album links → scraping tracks...".to_string();
            let folder = PathBuf::from(self.screenshot_dir.trim());
            let (tx, rx) = channel();
            self.load_rx = Some(rx);
            let ctx = ui.ctx().clone();
            self.rt.spawn(async move {
                let result = match crate::ocr_import::import_from_screenshots(&folder).await {
                    Ok(report) => {
                        // Surface unresolved OCR lines so the user can retry / paste.
                        if !report.unresolved_queries.is_empty() {
                            tracing::info!(
                                "OCR unresolved ({}): {}",
                                report.unresolved_queries.len(),
                                report.unresolved_queries.iter().take(20).cloned().collect::<Vec<_>>().join(" | ")
                            );
                        }
                        let n_tracks = report.imported.track_urls.len();
                        Ok((
                            report.imported.track_urls,
                            format!(
                                "✅ OCR: {} image(s) → {} queries → {} album link(s) → {} album(s) / {} tracks ({} unresolved text lines).",
                                report.image_count,
                                report.candidate_queries.len(),
                                report.resolved_album_urls.len(),
                                report.imported.album_count,
                                n_tracks,
                                report.unresolved_queries.len()
                            ),
                        ))
                    }
                    Err(e) => Err(e),
                };
                let _ = tx.send(result);
                ctx.request_repaint();
            });
        }

        if !self.load_status.is_empty() {
            ui.add_space(5.0);
            ui.label(egui::RichText::new(&self.load_status).color(egui::Color32::LIGHT_BLUE));
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

                        let can_download = !self.loaded_links.is_empty() && !self.is_downloading;
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

                                        let done =
                                            completed_clone.fetch_add(1, Ordering::Relaxed) + 1;
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
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(
                                egui::RichText::new(&self.download_status)
                                    .color(egui::Color32::LIGHT_GREEN),
                            );
                        });
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
