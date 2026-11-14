#![allow(deprecated)]

use rspotify::model::{AlbumId, PlayableItem, PlaylistId, TrackId};
use rspotify::{prelude::*, ClientCredsSpotify, Credentials};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

use crate::scraper;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetadataSource {
    /// Official Spotify Web API (needs Client ID/Secret).
    Api,
    /// Public embed `__NEXT_DATA__` / oEmbed JSON (no API keys).
    Scrape,
}

impl MetadataSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Api => "Spotify API",
            Self::Scrape => "Web Scrape (JSON)",
        }
    }

    pub fn from_env() -> Self {
        match std::env::var("METADATA_SOURCE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "api" | "spotify" | "spotify_api" => Self::Api,
            _ => Self::Scrape,
        }
    }

    pub fn as_env_value(self) -> &'static str {
        match self {
            Self::Api => "api",
            Self::Scrape => "scrape",
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
}

fn parse_spotify_id(url: &str, kind: &str) -> Result<String, String> {
    let marker = format!("/{}/", kind);
    url.split(&marker)
        .nth(1)
        .and_then(|s| s.split(['?', '/']).next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Invalid Spotify {} URL format", kind))
}

static API_CLIENT_SLOT: Mutex<Option<Arc<ClientCredsSpotify>>> = Mutex::const_new(None);

/// Drop the cached API client so the next call picks up new credentials from the environment.
pub async fn reset_api_client() {
    API_CLIENT_SLOT.lock().await.take();
}

async fn api_client() -> Result<Arc<ClientCredsSpotify>, String> {
    let mut slot = API_CLIENT_SLOT.lock().await;
    if let Some(existing) = slot.as_ref() {
        return Ok(existing.clone());
    }

    let credentials = Credentials::from_env().ok_or_else(|| {
        "Missing RSPOTIFY_CLIENT_ID or RSPOTIFY_CLIENT_SECRET. Set them in Config, or switch to Web Scrape mode.".to_string()
    })?;
    let spotify = ClientCredsSpotify::new(credentials);
    spotify
        .request_token()
        .await
        .map_err(|e| format!("Spotify token error: {}", e))?;
    let arc = Arc::new(spotify);
    *slot = Some(arc.clone());
    Ok(arc)
}

pub async fn get_track_metadata(
    spotify_url: &str,
    source: MetadataSource,
) -> Result<TrackMetadata, String> {
    match source {
        MetadataSource::Scrape => scraper::get_track_metadata(spotify_url).await,
        MetadataSource::Api => get_track_metadata_api(spotify_url).await,
    }
}

async fn get_track_metadata_api(spotify_url: &str) -> Result<TrackMetadata, String> {
    let track_id_str = parse_spotify_id(spotify_url, "track")?;
    let track_id = TrackId::from_id(&track_id_str)
        .map_err(|e| format!("Failed to parse Track ID: {}", e))?;

    let spotify = api_client().await?;
    let track = spotify
        .track(track_id, None)
        .await
        .map_err(|e| format!("Failed to fetch track: {}", e))?;

    Ok(TrackMetadata {
        title: track.name,
        artist: track
            .artists
            .first()
            .map(|a| a.name.clone())
            .unwrap_or_else(|| "Unknown".into()),
        album: track.album.name,
    })
}

pub async fn expand_url_to_tracks(
    url: &str,
    source: MetadataSource,
) -> Result<Vec<String>, String> {
    match source {
        MetadataSource::Scrape => scraper::expand_url_to_tracks(url).await,
        MetadataSource::Api => expand_url_to_tracks_api(url).await,
    }
}

async fn expand_url_to_tracks_api(url: &str) -> Result<Vec<String>, String> {
    if url.contains("/track/") {
        let _ = parse_spotify_id(url, "track")?;
        return Ok(vec![url.split('?').next().unwrap_or(url).to_string()]);
    }

    info!("Detecting playlist or album URL (API mode)...");
    let spotify = api_client().await?;
    let mut track_urls = Vec::new();
    let spot_base = "https://open.spotify.com";

    if url.contains("/playlist/") {
        let id_str = parse_spotify_id(url, "playlist")?;
        let playlist_id =
            PlaylistId::from_id(&id_str).map_err(|_| "Invalid Playlist ID".to_string())?;

        let mut offset = 0;
        let limit = 100;

        loop {
            let playlist_page = spotify
                .playlist_items_manual(
                    playlist_id.clone(),
                    None,
                    None,
                    Some(limit),
                    Some(offset),
                )
                .await
                .map_err(|e| format!("Playlist error: {}", e))?;

            for item in playlist_page.items {
                if let Some(PlayableItem::Track(track)) = item.track {
                    if let Some(id) = track.id {
                        track_urls.push(format!("{}/track/{}", spot_base, id.id()));
                    }
                }
            }
            if playlist_page.next.is_none() {
                break;
            }
            offset += limit;
        }
    } else if url.contains("/album/") {
        let id_str = parse_spotify_id(url, "album")?;
        let album_id = AlbumId::from_id(&id_str).map_err(|_| "Invalid Album ID".to_string())?;

        // Paginate album tracks — large albums exceed a single page.
        let mut offset = 0;
        let limit = 50;
        loop {
            let page = spotify
                .album_track_manual(album_id.clone(), None, Some(limit), Some(offset))
                .await
                .map_err(|e| format!("Album error: {}", e))?;

            for track in page.items {
                if let Some(id) = track.id {
                    track_urls.push(format!("{}/track/{}", spot_base, id.id()));
                }
            }
            if page.next.is_none() {
                break;
            }
            offset += limit;
        }
    } else {
        return Err(
            "URL is not a recognizable Spotify Track, Album, or Playlist".to_string(),
        );
    }

    if track_urls.is_empty() {
        return Err("No playable tracks found for this URL".to_string());
    }

    Ok(track_urls)
}

pub async fn check_api_connection() -> Result<String, String> {
    let spotify = api_client().await?;
    let track_id = TrackId::from_id("11dFghVXANMlKmJXsNCbNl")
        .map_err(|e| format!("Internal track id error: {}", e))?;
    let track = spotify
        .track(track_id, None)
        .await
        .map_err(|e| format!("API check failed: {}", e))?;
    Ok(format!(
        "✅ Spotify API connected (sample: {})",
        track.name
    ))
}
