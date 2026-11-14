//! Scrape Spotify track/playlist/album metadata from public embed `__NEXT_DATA__` JSON.
//! No Client ID/Secret and no official Web API rate limits.

use regex::Regex;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tracing::info;

use crate::spotify::TrackMetadata;

const USER_AGENT: &str =
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:124.0) Gecko/20100101 Firefox/124.0";
const SPOTIFY_BASE: &str = "https://open.spotify.com";

#[derive(Debug, Deserialize)]
struct EmbedArtist {
    name: String,
}

#[derive(Debug, Deserialize)]
struct EmbedTrackEntity {
    name: Option<String>,
    title: Option<String>,
    artists: Option<Vec<EmbedArtist>>,
    #[serde(rename = "subtitle")]
    subtitle: Option<String>,
}

#[derive(Debug, Deserialize)]
struct EmbedListItem {
    uri: String,
    title: Option<String>,
    subtitle: Option<String>,
    #[serde(rename = "entityType")]
    entity_type: Option<String>,
}

fn http_client() -> &'static reqwest::Client {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to build HTTP client")
    })
}

fn next_data_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<script id="__NEXT_DATA__"[^>]*>(.*?)</script>"#)
            .expect("invalid __NEXT_DATA__ regex")
    })
}

/// Cache filled when expanding albums/playlists so downloads skip a second embed fetch.
fn metadata_cache() -> &'static Mutex<HashMap<String, TrackMetadata>> {
    static CACHE: OnceLock<Mutex<HashMap<String, TrackMetadata>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_put(track_url: &str, meta: TrackMetadata) {
    if let Ok(mut guard) = metadata_cache().lock() {
        guard.insert(track_url.to_string(), meta);
    }
}

fn cache_get(track_url: &str) -> Option<TrackMetadata> {
    metadata_cache()
        .lock()
        .ok()
        .and_then(|guard| guard.get(track_url).cloned())
}

fn parse_spotify_id(url: &str, kind: &str) -> Result<String, String> {
    let marker = format!("/{}/", kind);
    url.split(&marker)
        .nth(1)
        .and_then(|s| s.split(|c| c == '?' || c == '/').next())
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("Could not parse Spotify {} id from URL", kind))
}

fn uri_to_track_url(uri: &str) -> Option<String> {
    let id = uri.strip_prefix("spotify:track:")?;
    Some(format!("{}/track/{}", SPOTIFY_BASE, id))
}

fn clean_subtitle(raw: &str) -> String {
    raw.replace('\u{00a0}', " ").trim().to_string()
}

async fn fetch_embed_html(kind: &str, id: &str) -> Result<String, String> {
    let url = format!("{}/embed/{}/{}", SPOTIFY_BASE, kind, id);
    let response = http_client()
        .get(&url)
        .header(
            "Accept",
            "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
        )
        .header("Accept-Language", "en-US,en;q=0.5")
        .send()
        .await
        .map_err(|e| format!("Embed request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!(
            "Embed request returned HTTP {} for {}",
            response.status(),
            url
        ));
    }

    response
        .text()
        .await
        .map_err(|e| format!("Failed reading embed body: {}", e))
}

fn extract_entity(html: &str) -> Result<Value, String> {
    let caps = next_data_re()
        .captures(html)
        .ok_or_else(|| "Spotify embed page missing __NEXT_DATA__ JSON".to_string())?;
    let raw = caps
        .get(1)
        .ok_or_else(|| "Malformed __NEXT_DATA__ script".to_string())?
        .as_str();

    let data: Value =
        serde_json::from_str(raw).map_err(|e| format!("Failed to parse embed JSON: {}", e))?;

    data.pointer("/props/pageProps/state/data/entity")
        .cloned()
        .ok_or_else(|| "Embed JSON missing entity metadata".to_string())
}

fn artists_from_entity(entity: &EmbedTrackEntity) -> String {
    if let Some(artists) = &entity.artists {
        let joined = artists
            .iter()
            .map(|a| a.name.trim())
            .filter(|n| !n.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
        if !joined.is_empty() {
            return joined;
        }
    }
    entity
        .subtitle
        .as_deref()
        .map(clean_subtitle)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Unknown".to_string())
}

/// Fetch track title/artist from the public embed JSON (no API keys).
pub async fn get_track_metadata(spotify_url: &str) -> Result<TrackMetadata, String> {
    let clean_url = spotify_url.split('?').next().unwrap_or(spotify_url);
    if let Some(cached) = cache_get(clean_url) {
        return Ok(cached);
    }

    let track_id = parse_spotify_id(clean_url, "track")?;
    info!("Scraping track metadata via embed JSON for {}", track_id);

    let html = fetch_embed_html("track", &track_id).await?;
    let entity_value = extract_entity(&html)?;
    let entity: EmbedTrackEntity = serde_json::from_value(entity_value)
        .map_err(|e| format!("Unexpected track embed shape: {}", e))?;

    let title = entity
        .title
        .clone()
        .or(entity.name.clone())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| "Track embed missing title".to_string())?;

    let meta = TrackMetadata {
        title,
        artist: artists_from_entity(&entity),
        // Standalone track embeds do not include album name.
        album: "Unknown Album".to_string(),
    };
    cache_put(clean_url, meta.clone());
    Ok(meta)
}

/// Expand track/album/playlist URLs using embed `__NEXT_DATA__` JSON.
pub async fn expand_url_to_tracks(url: &str) -> Result<Vec<String>, String> {
    if url.contains("/track/") {
        let clean = url.split('?').next().unwrap_or(url).to_string();
        let _ = parse_spotify_id(&clean, "track")?;
        return Ok(vec![clean]);
    }

    let (kind, id) = if url.contains("/playlist/") {
        ("playlist", parse_spotify_id(url, "playlist")?)
    } else if url.contains("/album/") {
        ("album", parse_spotify_id(url, "album")?)
    } else {
        return Err(
            "URL is not a recognizable Spotify Track, Album, or Playlist".to_string(),
        );
    };

    info!("Scraping {} {} via embed JSON...", kind, id);
    let html = fetch_embed_html(kind, &id).await?;
    let entity = extract_entity(&html)?;

    let collection_name = entity
        .get("name")
        .or_else(|| entity.get("title"))
        .and_then(|v| v.as_str())
        .unwrap_or("Unknown Album")
        .to_string();

    let collection_artist = entity
        .get("subtitle")
        .and_then(|v| v.as_str())
        .map(clean_subtitle)
        .filter(|s| !s.is_empty());

    let track_list = entity
        .get("trackList")
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("No trackList found in {} embed JSON", kind))?;

    let mut track_urls = Vec::new();
    for item in track_list {
        let parsed: EmbedListItem = serde_json::from_value(item.clone())
            .map_err(|e| format!("Bad trackList item: {}", e))?;

        if let Some(entity_type) = &parsed.entity_type {
            if entity_type != "track" {
                continue;
            }
        }

        let Some(track_url) = uri_to_track_url(&parsed.uri) else {
            continue;
        };

        let title = parsed
            .title
            .as_deref()
            .unwrap_or("Unknown")
            .trim()
            .to_string();
        let artist = parsed
            .subtitle
            .as_deref()
            .map(clean_subtitle)
            .filter(|s| !s.is_empty())
            .or_else(|| collection_artist.clone())
            .unwrap_or_else(|| "Unknown".to_string());

        let album = if kind == "album" {
            collection_name.clone()
        } else {
            "Unknown Album".to_string()
        };

        cache_put(
            &track_url,
            TrackMetadata {
                title,
                artist,
                album,
            },
        );
        track_urls.push(track_url);
    }

    if track_urls.is_empty() {
        return Err(format!(
            "Embed scrape found 0 playable tracks for this {}. Try API mode or a different link.",
            kind
        ));
    }

    // Large playlists may be truncated by the public embed (often ~100 tracks).
    Ok(track_urls)
}

/// Lightweight connectivity check that hits Spotify oEmbed (public JSON, no auth).
pub async fn check_scrape_connection() -> Result<String, String> {
    let url = format!(
        "{}/oembed?url={}/track/11dFghVXANMlKmJXsNCbNl",
        SPOTIFY_BASE, SPOTIFY_BASE
    );
    let response = http_client()
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("oEmbed request failed: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("oEmbed returned HTTP {}", response.status()));
    }

    let body: Value = response
        .json()
        .await
        .map_err(|e| format!("Invalid oEmbed JSON: {}", e))?;

    let title = body
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("ok");

    Ok(format!(
        "✅ Scrape/JSON mode reachable (oEmbed sample: {})",
        title
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_track_id() {
        assert_eq!(
            parse_spotify_id(
                "https://open.spotify.com/track/11dFghVXANMlKmJXsNCbNl?si=abc",
                "track"
            )
            .unwrap(),
            "11dFghVXANMlKmJXsNCbNl"
        );
    }

    #[test]
    fn uri_conversion() {
        assert_eq!(
            uri_to_track_url("spotify:track:11dFghVXANMlKmJXsNCbNl").unwrap(),
            "https://open.spotify.com/track/11dFghVXANMlKmJXsNCbNl"
        );
    }

    #[tokio::test]
    async fn live_scrape_track_metadata() {
        let meta = get_track_metadata("https://open.spotify.com/track/11dFghVXANMlKmJXsNCbNl")
            .await
            .expect("scrape should work");
        assert!(meta.title.to_lowercase().contains("cut"));
        assert!(meta.artist.to_lowercase().contains("carly"));
    }

    #[tokio::test]
    async fn live_scrape_album_expand_caches_album_name() {
        let tracks = expand_url_to_tracks("https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy")
            .await
            .expect("album scrape");
        assert!(
            tracks.len() >= 10,
            "expected several album tracks, got {}",
            tracks.len()
        );
        assert!(tracks[0].contains("/track/"));

        let meta = get_track_metadata(&tracks[0])
            .await
            .expect("cached metadata");
        assert_eq!(meta.album, "Global Warming");
        assert!(!meta.title.is_empty());
        assert!(!meta.artist.is_empty());
    }

    #[tokio::test]
    async fn live_oembed_connection() {
        let msg = check_scrape_connection().await.expect("oembed");
        assert!(msg.contains("reachable"));
    }
}
