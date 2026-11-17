//! Import the current user's saved Spotify albums via Authorization Code login.

use rspotify::{prelude::*, scopes, AuthCodeSpotify, Config, Credentials, OAuth};
use tracing::info;

use crate::metadata_cache;
use crate::spotify::TrackMetadata;

pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8888/callback";

#[derive(Debug, Clone)]
pub struct SavedAlbumsImport {
    pub album_count: usize,
    pub track_urls: Vec<String>,
    pub album_names: Vec<String>,
}

fn redirect_uri() -> String {
    std::env::var("RSPOTIFY_REDIRECT_URI").unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string())
}

fn user_client() -> Result<AuthCodeSpotify, String> {
    let credentials = Credentials::from_env().ok_or_else(|| {
        "Missing RSPOTIFY_CLIENT_ID / RSPOTIFY_CLIENT_SECRET. Add them in Config — saved albums require Spotify login.".to_string()
    })?;

    let oauth = OAuth {
        redirect_uri: redirect_uri(),
        scopes: scopes!("user-library-read"),
        ..Default::default()
    };

    let config = Config {
        token_cached: true,
        token_refreshing: true,
        ..Default::default()
    };

    Ok(AuthCodeSpotify::with_config(credentials, oauth, config))
}

async fn ensure_user_token(spotify: &AuthCodeSpotify) -> Result<(), String> {
    let url = spotify
        .get_authorize_url(false)
        .map_err(|e| format!("Failed to build Spotify login URL: {}", e))?;

    info!("Requesting Spotify user authorization for saved albums...");
    spotify
        .prompt_for_token(&url)
        .await
        .map_err(|e| {
            format!(
                "Spotify login failed: {}. Make sure redirect URI `{}` is added in your Spotify Developer Dashboard.",
                e,
                redirect_uri()
            )
        })
}

/// Open a browser login (first time), then list every saved album and expand to track URLs.
pub async fn import_saved_albums() -> Result<SavedAlbumsImport, String> {
    let spotify = user_client()?;
    ensure_user_token(&spotify).await?;

    let mut album_urls = Vec::new();
    let mut album_names = Vec::new();
    let mut offset = 0u32;
    let limit = 50u32;

    loop {
        let page = spotify
            .current_user_saved_albums_manual(None, Some(limit), Some(offset))
            .await
            .map_err(|e| format!("Failed to fetch saved albums: {}", e))?;

        for saved in page.items {
            let album = saved.album;
            album_names.push(album.name.clone());
            album_urls.push((
                album.id.clone(),
                album.name.clone(),
                album
                    .artists
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| "Unknown".to_string()),
            ));
        }

        if page.next.is_none() {
            break;
        }
        offset += limit;
    }

    if album_urls.is_empty() {
        return Err("No saved albums found on this Spotify account.".to_string());
    }

    info!(
        "Found {} saved albums — expanding tracks...",
        album_urls.len()
    );

    let spot_base = "https://open.spotify.com";
    let mut track_urls = Vec::new();

    for (album_id, album_name, album_artist) in &album_urls {
        let mut track_offset = 0u32;
        let track_limit = 50u32;

        loop {
            let page = spotify
                .album_track_manual(album_id.clone(), None, Some(track_limit), Some(track_offset))
                .await
                .map_err(|e| format!("Failed to list tracks for '{}': {}", album_name, e))?;

            for track in page.items {
                let Some(id) = track.id else {
                    continue;
                };
                let track_url = format!("{}/track/{}", spot_base, id.id());
                let artist = track
                    .artists
                    .first()
                    .map(|a| a.name.clone())
                    .unwrap_or_else(|| album_artist.clone());

                metadata_cache::put(
                    &track_url,
                    TrackMetadata {
                        title: track.name,
                        artist,
                        album: album_name.clone(),
                    },
                );
                track_urls.push(track_url);
            }

            if page.next.is_none() {
                break;
            }
            track_offset += track_limit;
        }
    }

    if track_urls.is_empty() {
        return Err("Saved albums were found, but none contained playable tracks.".to_string());
    }

    Ok(SavedAlbumsImport {
        album_count: album_urls.len(),
        track_urls,
        album_names,
    })
}
