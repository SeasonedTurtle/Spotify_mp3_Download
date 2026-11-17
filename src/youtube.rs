//! yt-dlp runner: locate binary (PATH or next to the app), update it, download audio.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::process::Command;
use tracing::{error, info, instrument};

#[derive(Debug, Clone)]
pub struct YtDlpStatus {
    pub path: PathBuf,
    pub version: String,
    pub ffmpeg_ok: bool,
}

fn candidate_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["yt-dlp.exe", "yt-dlp"]
    } else {
        &["yt-dlp"]
    }
}

fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.to_path_buf()))
}

/// Resolve yt-dlp: next to this app (portable), then PATH.
pub fn resolve_yt_dlp() -> Result<PathBuf, String> {
    if let Some(dir) = exe_dir() {
        for name in candidate_names() {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }

    // `which`-style search via spawning `yt-dlp --version` is slow; use PATH lookup.
    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in candidate_names() {
                let candidate = dir.join(name);
                if candidate.is_file() {
                    return Ok(candidate);
                }
            }
        }
    }

    Err(
        "yt-dlp not found. Install it (https://github.com/yt-dlp/yt-dlp) or place yt-dlp next to this app, then use Config → Update yt-dlp."
            .to_string(),
    )
}

fn ffmpeg_names() -> &'static [&'static str] {
    if cfg!(windows) {
        &["ffmpeg.exe", "ffmpeg"]
    } else {
        &["ffmpeg"]
    }
}

fn ffmpeg_available() -> bool {
    if let Some(dir) = exe_dir() {
        for name in ffmpeg_names() {
            if dir.join(name).is_file() {
                return true;
            }
        }
    }

    if let Ok(path_var) = std::env::var("PATH") {
        for dir in std::env::split_paths(&path_var) {
            for name in ffmpeg_names() {
                if dir.join(name).is_file() {
                    return true;
                }
            }
        }
    }
    false
}

async fn run_yt_dlp(bin: &Path, args: &[&str]) -> Result<(i32, String, String), String> {
    let output = Command::new(bin)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| format!("Failed to run {}: {}", bin.display(), e))?;

    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Ok((code, stdout, stderr))
}

pub async fn check_yt_dlp() -> Result<YtDlpStatus, String> {
    let path = resolve_yt_dlp()?;
    let (code, stdout, stderr) = run_yt_dlp(&path, &["--version"]).await?;
    if code != 0 {
        return Err(format!(
            "yt-dlp --version failed ({}): {}",
            code,
            if stderr.is_empty() { stdout } else { stderr }
        ));
    }
    let version = if stdout.is_empty() { stderr } else { stdout };
    Ok(YtDlpStatus {
        path,
        version: version.lines().next().unwrap_or("unknown").to_string(),
        ffmpeg_ok: ffmpeg_available(),
    })
}

/// Run `yt-dlp -U` (self-update). Works for official installs / portable binaries.
pub async fn update_yt_dlp() -> Result<String, String> {
    let path = resolve_yt_dlp()?;
    info!("Updating yt-dlp at {} ...", path.display());
    let (code, stdout, stderr) = run_yt_dlp(&path, &["-U"]).await?;
    let combined = [stdout.as_str(), stderr.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    if code != 0 {
        return Err(format!(
            "yt-dlp update failed: {}",
            if combined.is_empty() {
                format!("exit {code}")
            } else {
                combined
            }
        ));
    }

    // Re-check version after update.
    let status = check_yt_dlp().await?;
    Ok(format!(
        "✅ yt-dlp updated to {} ({})\n{}",
        status.version,
        status.path.display(),
        combined
    ))
}

#[instrument(skip(output_path))]
pub async fn download_to_mp3(url: &str, output_path: &Path) -> Result<(), String> {
    if let Some(parent_folder) = output_path.parent() {
        if !parent_folder.as_os_str().is_empty() && !parent_folder.exists() {
            fs::create_dir_all(parent_folder)
                .map_err(|e| format!("Failed to create directories: {}", e))?;
        }
    }

    if !ffmpeg_available() {
        return Err(
            "ffmpeg not found (required for MP3 conversion). Install ffmpeg and ensure it is on PATH, or place it next to this app."
                .to_string(),
        );
    }

    let bin = resolve_yt_dlp()?;
    let output_template = output_path.with_extension("%(ext)s");
    let output_str = output_template
        .to_str()
        .ok_or_else(|| "Output path contains invalid UTF-8".to_string())?;

    // Prefer best audio stream, then extract/convert to mp3 via ffmpeg.
    let args = [
        "-f",
        "bestaudio/best",
        "-x",
        "--audio-format",
        "mp3",
        "--audio-quality",
        "192K",
        "--embed-metadata",
        "--no-playlist",
        "--no-overwrites",
        "--no-warnings",
        "--extractor-retries",
        "3",
        "--retries",
        "3",
        "--fragment-retries",
        "3",
        "-o",
        output_str,
        url,
    ];

    let (code, stdout, stderr) = run_yt_dlp(&bin, &args).await?;
    if code != 0 {
        let combined = if stderr.is_empty() { stdout } else { stderr };
        error!("yt-dlp failed: {}", combined);
        return Err(format!("yt-dlp failed: {}", combined));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_names_nonempty() {
        assert!(!candidate_names().is_empty());
    }
}
