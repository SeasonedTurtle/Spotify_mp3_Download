use std::fs;
use std::path::Path;
use tokio::process::Command;
use tracing::{error, instrument};

#[instrument(skip(output_path))]
pub async fn download_to_mp3(url: &str, output_path: &Path) -> Result<(), String> {
    if let Some(parent_folder) = output_path.parent() {
        if !parent_folder.as_os_str().is_empty() && !parent_folder.exists() {
            fs::create_dir_all(parent_folder)
                .map_err(|e| format!("Failed to create directories: {}", e))?;
        }
    }

    let output_template = output_path.with_extension("%(ext)s");
    let output_str = output_template
        .to_str()
        .ok_or_else(|| "Output path contains invalid UTF-8".to_string())?;

    // Prefer the `yt-dlp` on PATH; fall back to common Windows portable name.
    let mut command = Command::new("yt-dlp");
    command.args([
        "-x",
        "--audio-format",
        "mp3",
        "--audio-quality",
        "192K",
        "--no-playlist",
        "--no-overwrites",
        "--extractor-retries",
        "3",
        "--retries",
        "3",
        "-o",
        output_str,
        url,
    ]);

    let output = command.output().await.map_err(|e| {
        error!("Failed to execute yt-dlp. Is it installed? {}", e);
        format!(
            "yt-dlp execution failed (is it installed and on PATH?): {}",
            e
        )
    })?;

    if !output.status.success() {
        let err_msg = String::from_utf8_lossy(&output.stderr);
        let stdout_msg = String::from_utf8_lossy(&output.stdout);
        let combined = if err_msg.trim().is_empty() {
            stdout_msg.trim().to_string()
        } else {
            err_msg.trim().to_string()
        };
        error!("yt-dlp failed: {}", combined);
        return Err(format!("yt-dlp failed: {}", combined));
    }

    Ok(())
}
