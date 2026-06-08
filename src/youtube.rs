use std::path::Path;
use tokio::process::Command; // THE CRITICAL UPGRADE
use tracing::{error, instrument};
use std::fs;

#[instrument(skip(output_path))]
pub async fn download_to_mp3(url: &str, output_path: &Path) -> Result<(), String> {
    
    if let Some(parent_folder) = output_path.parent() {
        if !parent_folder.as_os_str().is_empty() && !parent_folder.exists() {
            fs::create_dir_all(parent_folder).map_err(|e| format!("Failed to create directories: {}", e))?;
        }
    }

    let output_template = output_path.with_extension("%(ext)s");
    let output_str = output_template.to_str().unwrap_or("output.%(ext)s");
    
    let status = Command::new("yt-dlp")
        .args([
            "-x",
            "--audio-format", "mp3",
            "--audio-quality", "192K",
            "--no-playlist",
            "-o", output_str,
            url
        ])
        .output()
        .await // WE NOW AWAIT THE BACKGROUND PROCESS
        .map_err(|e| {
            error!("Failed to execute yt-dlp. Is it installed? {}", e);
            format!("yt-dlp execution failed: {}", e)
        })?;

    if !status.status.success() {
        let err_msg = String::from_utf8_lossy(&status.stderr);
        error!("yt-dlp failed: {}", err_msg);
        return Err(format!("yt-dlp failed: {}", err_msg));
    }

    Ok(())
}