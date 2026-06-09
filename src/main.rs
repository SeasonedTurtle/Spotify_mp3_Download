// ** Use the commented code for a pure terminal based setup **

// use dotenvy::dotenv;
// use std::fs;
// use std::sync::Arc;
// use tokio::sync::Semaphore;
// use tracing_subscriber;

// mod spotify;
// mod youtube;
// mod downloader;

// #[tokio::main]
// async fn main() {
//     tracing_subscriber::fmt::init();
//     dotenv().ok(); 

//     println!("Initializing Concurrent Spotify-to-MP3 Downloader...");

//     let file_contents = fs::read_to_string("urls.txt")
//         .expect("❌ Failed to read urls.txt!");

//     let raw_urls: Vec<&str> = file_contents.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();

//     if raw_urls.is_empty() {
//         println!("urls.txt is empty!");
//         return;
//     }

//     println!("Found {} links. Expanding albums/playlists...", raw_urls.len());
//     let mut master_track_queue = Vec::new();

//     for raw_url in raw_urls {
//         match spotify::expand_url_to_tracks(raw_url).await {
//             Ok(tracks) => {
//                 println!("✅ Expanded into {} track(s).", tracks.len());
//                 master_track_queue.extend(tracks);
//             }
//             Err(e) => eprintln!("❌ Skipping {}: {}", raw_url, e),
//         }
//     }

//     println!("\n=== Final Queue: {} tracks to download ===\n", master_track_queue.len());

//     // 🚨 THE CONCURRENCY UPGRADE 🚨
//     // This semaphore limits us to 5 parallel downloads at a time
//     let bouncer = Arc::new(Semaphore::new(5));
//     let mut background_tasks = Vec::new();

//     for track_url in master_track_queue {
//         // Clone the variables so they can safely live inside a background thread
//         let permit = bouncer.clone().acquire_owned().await.unwrap();
//         let safe_url = track_url.clone();
        
//         // Spawn a background task for each song
//         let task = tokio::spawn(async move {
//             println!("🚀 Starting: {}", safe_url);
            
//             if let Err(e) = downloader::download_from_spotify(&safe_url).await {
//                 eprintln!("❌ Failed: {} - {}", safe_url, e);
//             } else {
//                 println!("✅ Finished: {}", safe_url);
//             }
            
//             // Drop the permit to let the next song in line start
//             drop(permit); 
//         });

//         background_tasks.push(task);
//     }

//     // Tell the main thread to wait until every single background task finishes
//     for task in background_tasks {
//         let _ = task.await;
//     }

//     println!("\n🎉 Massive batch download complete!");
// }

#![windows_subsystem = "windows"]

use dotenvy::dotenv;
use eframe::egui;

mod spotify;
mod youtube;
mod downloader;
mod ui; // Register our new UI file!

fn main() -> Result<(), eframe::Error> {
    // Load env variables
    dotenv().ok(); 

    // Setup window options
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    // Boot up the GUI
    eframe::run_native(
        "Rusty Spotify Downloader",
        options,
        Box::new(|_cc| Ok(Box::new(ui::DownloaderApp::default()))),
    )
}