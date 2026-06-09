use std::path::Path;
use tracing::info;

pub async fn download_from_spotify(spotify_url: &str, output_folder: &str) -> Result<(), String> {
    
    // 1. Get metadata from Spotify
    let metadata = crate::spotify::get_track_metadata(spotify_url).await?;
    
    // 2. Sanitize strings so Windows/Linux don't freak out over weird characters in song names
    let safe_artist = metadata.artist.replace("/", "_").replace("\\", "_");
    let safe_album = metadata.album.replace("/", "_").replace("\\", "_");
    let safe_title = metadata.title.replace("/", "_").replace("\\", "_");
    
    // 3. Build the final, bulletproof file path using the user's custom directory!
    let final_path = Path::new(output_folder)
        .join(&safe_artist)
        .join(&safe_album)
        .join(format!("{} - {}", safe_artist, safe_title));

    // 4. Construct the targeted yt-dlp search query
    // Adding "audio" helps ensure yt-dlp favors static audio streams over music videos
    let search_query = format!("ytsearch1:{} {} audio", metadata.title, metadata.artist);

    info!("Forwarding search query to YouTube downloader engine...");

    // 5. Download and convert using our yt-dlp runner
    crate::youtube::download_to_mp3(&search_query, &final_path).await?;

    println!("\n🎉 Success! Track saved to: {:?}", final_path.with_extension("mp3"));
    Ok(())
}