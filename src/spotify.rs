#![allow(deprecated)] 

use rspotify::{prelude::*, ClientCredsSpotify, Credentials};
use rspotify::model::{TrackId, PlaylistId, AlbumId, PlayableItem};
use tracing::info; 

#[derive(Debug)]
pub struct TrackMetadata {
    pub title: String,
    pub artist: String,
    pub album: String,
}

pub async fn get_track_metadata(spotify_url: &str) -> Result<TrackMetadata, String> {
    let track_id_str = spotify_url.split("/track/").nth(1).and_then(|s| s.split('?').next()).ok_or("Invalid Spotify URL format")?;
    let track_id = TrackId::from_id(track_id_str).map_err(|e| format!("Failed to parse Track ID: {}", e))?;
    let credentials = Credentials::from_env().ok_or("Missing RSPOTIFY_CLIENT_ID or RSPOTIFY_CLIENT_SECRET in .env")?;
    let spotify = ClientCredsSpotify::new(credentials);
    spotify.request_token().await.map_err(|e| format!("Token error: {}", e))?;
    let track = spotify.track(track_id, None).await.map_err(|e| format!("Failed to fetch track: {}", e))?;

    Ok(TrackMetadata {
        title: track.name,
        artist: track.artists.first().map(|a| a.name.clone()).unwrap_or_else(|| "Unknown".into()),
        album: track.album.name,
    })
}

pub async fn expand_url_to_tracks(url: &str) -> Result<Vec<String>, String> {
    if url.contains("/track/") { return Ok(vec![url.to_string()]); }

    info!("Detecting playlist or album URL...");
    let credentials = Credentials::from_env().ok_or("Missing API keys in .env")?;
    let spotify = ClientCredsSpotify::new(credentials);
    spotify.request_token().await.map_err(|e| format!("Token error: {}", e))?;

    let mut track_urls = Vec::new();
    let spot_base = "https://open.spotify.com";

    if url.contains("/playlist/") {
        let id_str = url.split("/playlist/").nth(1).and_then(|s| s.split('?').next()).unwrap_or("");
        let playlist_id = PlaylistId::from_id(id_str).map_err(|_| "Invalid Playlist ID")?;
        
        let mut offset = 0;
        let limit = 100; 

        loop {
            let playlist_page = spotify.playlist_items_manual(playlist_id.clone(), None, None, Some(limit), Some(offset)).await.map_err(|e| format!("Playlist error: {}", e))?;
            
            for item in playlist_page.items {
                if let Some(PlayableItem::Track(track)) = item.track {
                    if let Some(id) = track.id {
                        track_urls.push(format!("{}/track/{}", spot_base, id.id()));
                    }
                }
            }
            if playlist_page.next.is_none() { break; }
            offset += limit;
        }
    } else if url.contains("/album/") {
        let id_str = url.split("/album/").nth(1).and_then(|s| s.split('?').next()).unwrap_or("");
        let album_id = AlbumId::from_id(id_str).map_err(|_| "Invalid Album ID")?;
        let album = spotify.album(album_id, None).await.map_err(|e| format!("Album error: {}", e))?;
        
        for track in album.tracks.items {
            if let Some(id) = track.id {
                track_urls.push(format!("{}/track/{}", spot_base, id.id()));
            }
        }
    } else {
        return Err("URL is not a recognizable Spotify Track, Album, or Playlist".to_string());
    }

    Ok(track_urls)
}