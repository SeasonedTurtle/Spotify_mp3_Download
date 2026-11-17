//! Import the current user's saved Spotify albums via Authorization Code login.

use rspotify::{prelude::*, scopes, AuthCodeSpotify, Config, Credentials, OAuth};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use tracing::info;
use url::Url;

use crate::metadata_cache;
use crate::spotify::TrackMetadata;

/// Must match the Spotify Developer Dashboard Redirect URI exactly.
/// Use host+port only (no `/callback` path) — rspotify's listener concatenates
/// the request path onto this string.
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8888";

#[derive(Debug, Clone)]
pub struct SavedAlbumsImport {
    pub album_count: usize,
    pub track_urls: Vec<String>,
    pub album_names: Vec<String>,
}

fn redirect_uri() -> String {
    let raw = std::env::var("RSPOTIFY_REDIRECT_URI")
        .unwrap_or_else(|_| DEFAULT_REDIRECT_URI.to_string());
    normalize_redirect_uri(&raw)
}

/// Avoid `.../callback/callback` by stripping a trailing `/callback` path that
/// conflicts with how the local OAuth listener rebuilds the URL.
fn normalize_redirect_uri(raw: &str) -> String {
    let trimmed = raw.trim().trim_end_matches('/');
    if let Some(stripped) = trimmed.strip_suffix("/callback") {
        stripped.to_string()
    } else {
        trimmed.to_string()
    }
}

fn user_client() -> Result<AuthCodeSpotify, String> {
    let credentials = Credentials::from_env().ok_or_else(|| {
        "Missing RSPOTIFY_CLIENT_ID / RSPOTIFY_CLIENT_SECRET. Add them in Config — saved albums require a Spotify Developer app, not just open.spotify.com.".to_string()
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

/// Localhost OAuth callback that correctly handles Spotify `error=` responses
/// and avoids rspotify's redirect_uri + path double-append bug.
fn wait_for_auth_code(spotify: &AuthCodeSpotify, authorize_url: &str) -> Result<String, String> {
    let redirect = redirect_uri();
    let parsed = Url::parse(&redirect)
        .map_err(|e| format!("Invalid redirect URI `{}`: {}", redirect, e))?;
    let host = parsed
        .host_str()
        .ok_or_else(|| "Redirect URI missing host".to_string())?;
    let port = parsed
        .port_or_known_default()
        .ok_or_else(|| "Redirect URI missing port".to_string())?;

    let listener = TcpListener::bind((host, port)).map_err(|e| {
        format!(
            "Could not listen on {}:{} ({}). Close anything using that port and try again.",
            host, port, e
        )
    })?;

    match webbrowser::open(authorize_url) {
        Ok(_) => info!("Opened Spotify login in browser"),
        Err(e) => {
            return Err(format!(
                "Could not open browser ({}). Open this URL manually:\n{}",
                e, authorize_url
            ));
        }
    }

    info!("Waiting for Spotify OAuth callback on {}...", redirect);

    let mut stream = listener
        .incoming()
        .flatten()
        .next()
        .ok_or_else(|| "OAuth listener closed before receiving a callback".to_string())?;

    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader
        .read_line(&mut request_line)
        .map_err(|e| format!("Failed reading OAuth callback: {}", e))?;

    // Drain the rest of the request so the browser gets a clean response.
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" || line == "\n" {
            break;
        }
    }

    let path_and_query = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("Malformed OAuth HTTP request: {request_line}"))?;

    let callback_url = if path_and_query.starts_with("http://") || path_and_query.starts_with("https://")
    {
        path_and_query.to_string()
    } else {
        format!(
            "{}{}",
            redirect.trim_end_matches('/'),
            if path_and_query.starts_with('/') {
                path_and_query.to_string()
            } else {
                format!("/{path_and_query}")
            }
        )
    };

    let params: HashMap<String, String> = Url::parse(&callback_url)
        .map_err(|e| format!("Failed to parse callback URL `{callback_url}`: {e}"))?
        .query_pairs()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();

    let message;
    let result = if let Some(err) = params.get("error") {
        let desc = params
            .get("error_description")
            .cloned()
            .unwrap_or_default();
        message = format!("Login failed ({err}). You can close this tab.");
        Err(format!(
            "Spotify returned error `{err}`{}. Check that:\n\
             1) Redirect URI in Dashboard is exactly `{redirect}`\n\
             2) Client ID/Secret in Config match that app\n\
             3) Your Spotify account is added as a user if the app is in Development mode",
            if desc.is_empty() {
                String::new()
            } else {
                format!(" ({desc})")
            }
        ))
    } else if let Some(code) = params.get("code") {
        let expected_state = &spotify.get_oauth().state;
        match params.get("state") {
            Some(state) if state == expected_state => {
                message = "Login OK — go back to the Spotify Downloader.".to_string();
                Ok(code.clone())
            }
            _ => {
                message = "Login failed (bad state). You can close this tab.".to_string();
                Err("OAuth state mismatch — try Import Saved Albums again.".to_string())
            }
        }
    } else {
        message = "Login failed (no code). You can close this tab.".to_string();
        Err(format!(
            "Callback had no authorization code. URL was: {callback_url}"
        ))
    };

    let response = format!(
        "HTTP/1.1 200 OK\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
        message.len(),
        message
    );
    let _ = stream.write_all(response.as_bytes());

    result
}

async fn ensure_user_token(spotify: &AuthCodeSpotify) -> Result<(), String> {
    // Reuse a cached token when possible.
    if let Ok(Some(token)) = spotify.read_token_cache(true).await {
        let expired = token.is_expired();
        *spotify
            .get_token()
            .lock()
            .await
            .map_err(|e| format!("Token lock error: {e:?}"))? = Some(token);
        if expired {
            match spotify.refetch_token().await {
                Ok(Some(refreshed)) => {
                    *spotify
                        .get_token()
                        .lock()
                        .await
                        .map_err(|e| format!("Token lock error: {e:?}"))? = Some(refreshed);
                    return Ok(());
                }
                Ok(None) | Err(_) => {
                    // Fall through to interactive login.
                }
            }
        } else {
            return Ok(());
        }
    }

    let url = spotify
        .get_authorize_url(false)
        .map_err(|e| format!("Failed to build Spotify login URL: {}", e))?;

    info!("Requesting Spotify user authorization for saved albums...");
    let code = wait_for_auth_code(spotify, &url)?;
    spotify
        .request_token(&code)
        .await
        .map_err(|e| format!("Token exchange failed: {e}"))?;
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_callback_suffix() {
        assert_eq!(
            normalize_redirect_uri("http://127.0.0.1:8888/callback"),
            "http://127.0.0.1:8888"
        );
        assert_eq!(
            normalize_redirect_uri("http://127.0.0.1:8888/callback/"),
            "http://127.0.0.1:8888"
        );
        assert_eq!(
            normalize_redirect_uri("http://127.0.0.1:8888"),
            "http://127.0.0.1:8888"
        );
    }
}
