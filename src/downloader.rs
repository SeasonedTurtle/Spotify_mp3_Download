use std::path::PathBuf;
use tracing::{info, error};

pub async fn download_from_spotify(spotify_url: &str) -> Result<(), String> {
    info!("Starting pipeline for Spotify track...");

    // 1. Fetch metadata from Spotify
    let metadata = crate::spotify::get_track_metadata(spotify_url).await?;
    info!("Metadata retrieved successfully: {} - {}", metadata.title, metadata.artist);

    // 2. Sanitize and construct the output folder structure
    // This removes forward/backward slashes to avoid unintended subfolder breaks
    let base_folder = PathBuf::from("downloaded tracks");
    let safe_artist = metadata.artist.replace("/", "_").replace("\\", "_");
    let safe_album = metadata.album.replace("/", "_").replace("\\", "_");
    let folder_path = base_folder.join(&safe_artist).join(&safe_album);
    
    // Sanitize the file name
    let safe_title = metadata.title.replace("/", "_").replace("\\", "_");
    let file_path = folder_path.join(safe_title);

    // 3. Construct the targeted yt-dlp search query
    // Adding "audio" helps ensure yt-dlp favors static audio streams over music videos
    let search_query = format!("ytsearch1:{} {} audio", metadata.title, metadata.artist);

    info!("Forwarding search query to YouTube downloader engine...");

    // 4. Download and convert using our yt-dlp runner
    crate::youtube::download_to_mp3(&search_query, &file_path).await?;

    println!("\n🎉 Success! Track saved to: {:?}", file_path.with_extension("mp3"));
    Ok(())
}