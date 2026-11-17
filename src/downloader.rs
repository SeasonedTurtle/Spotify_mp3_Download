use std::path::{Path, PathBuf};
use tracing::info;

use crate::spotify::MetadataSource;

/// Characters that are illegal or awkward in Windows/Linux filenames.
pub fn sanitize_filename(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        match ch {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0' => out.push('_'),
            c if c.is_control() => out.push('_'),
            c => out.push(c),
        }
    }
    let trimmed = out.trim().trim_matches('.');
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        // Keep paths from getting absurdly long on Windows.
        trimmed.chars().take(120).collect()
    }
}

/// Layout: `DOWNLOAD_DIR/<Album Name>/<Artist> - <Title>.mp3`
fn final_mp3_path(output_folder: &str, artist: &str, album: &str, title: &str) -> PathBuf {
    Path::new(output_folder)
        .join(sanitize_filename(album))
        .join(format!(
            "{} - {}.mp3",
            sanitize_filename(artist),
            sanitize_filename(title)
        ))
}

pub async fn download_from_spotify(
    spotify_url: &str,
    output_folder: &str,
    source: MetadataSource,
) -> Result<String, String> {
    let metadata = crate::spotify::get_track_metadata(spotify_url, source).await?;

    let final_file = final_mp3_path(
        output_folder,
        &metadata.artist,
        &metadata.album,
        &metadata.title,
    );

    if final_file.exists() {
        let msg = format!("Skipped (already exists): {}", final_file.display());
        info!("{}", msg);
        return Ok(msg);
    }

    // yt-dlp output template without the fixed .mp3 suffix (it fills %(ext)s).
    let output_stem = final_file.with_extension("");
    let search_query = format!("ytsearch1:{} {} audio", metadata.title, metadata.artist);

    info!(
        "Downloading '{}' by '{}' into album folder '{}'...",
        metadata.title, metadata.artist, metadata.album
    );

    // A couple of retries helps with transient yt-dlp / network blips.
    let mut last_err = String::new();
    for attempt in 1..=3 {
        match crate::youtube::download_to_mp3(&search_query, &output_stem).await {
            Ok(()) => {
                let msg = format!(
                    "[{}] {} - {} → {}",
                    metadata.album,
                    metadata.artist,
                    metadata.title,
                    final_file.display()
                );
                return Ok(msg);
            }
            Err(e) => {
                last_err = e;
                if attempt < 3 {
                    tokio::time::sleep(std::time::Duration::from_secs(attempt)).await;
                }
            }
        }
    }

    Err(format!(
        "Failed after retries for '{} - {}': {}",
        metadata.artist, metadata.title, last_err
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_folder_layout() {
        let path = final_mp3_path("downloads", "Pitbull", "Global Warming", "Feel This Moment");
        assert_eq!(
            path,
            PathBuf::from("downloads/Global Warming/Pitbull - Feel This Moment.mp3")
        );
    }

    #[test]
    fn sanitizes_illegal_album_chars() {
        let path = final_mp3_path("out", "A", "Foo:Bar/Baz", "T");
        assert_eq!(path, PathBuf::from("out/Foo_Bar_Baz/A - T.mp3"));
    }
}
