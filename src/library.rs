//! Import saved albums without burning Spotify API track-list quota.
//!
//! - OAuth `/me/albums` is only used to discover album IDs (with 429 backoff).
//! - Each album's tracks are expanded via public embed JSON scrape (no API).
//! - Or paste album URLs / run the browser console scraper (zero API).

use regex::Regex;
use rspotify::{prelude::*, scopes, AuthCodeSpotify, Config, Credentials, OAuth};
use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::sync::OnceLock;
use std::time::Duration;
use tracing::info;
use url::Url;

use crate::metadata_cache;
use crate::scraper;

/// Must match the Spotify Developer Dashboard Redirect URI exactly.
pub const DEFAULT_REDIRECT_URI: &str = "http://127.0.0.1:8888";

/// Paste on https://open.spotify.com/collection/albums (logged in).
/// DOM-only: scrolls the page/list and collects album links. No Spotify API calls
/// (avoids 429). After it finishes, run: copy(window.__spotifyAlbumUrls)
pub const BROWSER_ALBUM_SCRAPER_JS: &str = r#"(async () => {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

  const albumRe = /open\.spotify\.com\/album\/([A-Za-z0-9]+)/;

  function collectFromDom() {
    const found = new Set();
    for (const a of document.querySelectorAll('a[href*="/album/"]')) {
      const href = (a.href || a.getAttribute('href') || '').split('?')[0];
      const m = href.match(albumRe);
      if (m) found.add('https://open.spotify.com/album/' + m[1]);
    }
    // Also pick up URIs hidden in attributes / row data.
    for (const el of document.querySelectorAll('[href*="spotify:album:"], [uri*="spotify:album:"], [data-uri*="spotify:album:"]')) {
      const raw = el.getAttribute('href') || el.getAttribute('uri') || el.getAttribute('data-uri') || '';
      const m = raw.match(/spotify:album:([A-Za-z0-9]+)/);
      if (m) found.add('https://open.spotify.com/album/' + m[1]);
    }
    return [...found];
  }

  function scrollCandidates() {
    const nodes = new Set();
    for (const sel of [
      '[data-overlayscrollbars-viewport]',
      '[data-testid="section-entity-list"]',
      '[data-testid="grid"]',
      'main',
      'section',
      '[role="grid"]',
      '[role="presentation"]',
    ]) {
      document.querySelectorAll(sel).forEach((n) => nodes.add(n));
    }
    nodes.add(document.scrollingElement || document.documentElement);
    nodes.add(document.body);
    return [...nodes];
  }

  function scrollAll() {
    for (const n of scrollCandidates()) {
      try {
        const max = (n.scrollHeight || 0) - (n.clientHeight || 0);
        if (max > 0) n.scrollTop = Math.min((n.scrollTop || 0) + Math.max(n.clientHeight * 0.9, 400), n.scrollHeight);
        else n.scrollTop = n.scrollHeight;
      } catch (_) {}
    }
    window.scrollBy(0, Math.max(window.innerHeight * 0.9, 500));
  }

  console.log('DOM album scanner starting — no API calls. Ignore CORS noise.');
  console.log('Keep this tab focused on Your Library → Albums.');

  // Jump to albums collection if needed.
  if (!location.pathname.includes('/collection/albums')) {
    location.href = 'https://open.spotify.com/collection/albums';
    await sleep(2500);
  }

  let unique = new Set();
  let stable = 0;
  const maxPasses = 200;

  for (let i = 1; i <= maxPasses && stable < 12; i++) {
    scrollAll();
    await sleep(450);

    const before = unique.size;
    for (const u of collectFromDom()) unique.add(u);
    const after = unique.size;

    if (after === before) stable++;
    else stable = 0;

    if (i % 5 === 0 || after !== before) {
      console.log('Pass ' + i + ': ' + after + ' albums (stable ' + stable + '/12)');
    }

    // Every so often jump back to top then bottom to force virtualized rows to remount.
    if (i % 25 === 0) {
      for (const n of scrollCandidates()) {
        try { n.scrollTop = 0; } catch (_) {}
      }
      window.scrollTo(0, 0);
      await sleep(600);
      for (const n of scrollCandidates()) {
        try { n.scrollTop = n.scrollHeight; } catch (_) {}
      }
      window.scrollTo(0, document.body.scrollHeight);
      await sleep(600);
    }
  }

  const list = [...unique];
  const text = list.join('\\n');
  window.__spotifyAlbumUrls = text;
  console.log('TOTAL album URLs: ' + list.length);
  console.log(text);
  alert(
    'Collected ' + list.length + ' album URLs from the page (no API).\\n\\n' +
    'In the console run:\\n\\ncopy(window.__spotifyAlbumUrls)\\n\\n' +
    'Then paste into Spotify Downloader.\\n\\n' +
    'If the count is low, scroll the albums list manually a bit and run the scraper again.'
  );
  return list;
})();"#;

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
        "Missing RSPOTIFY_CLIENT_ID / RSPOTIFY_CLIENT_SECRET. Add them in Config — or use Paste album links instead (no API).".to_string()
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
                message = "Login OK — close this tab and return to Spotify Downloader.".to_string();
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
    if let Ok(Some(token)) = spotify.read_token_cache(true).await {
        let expired = token.is_expired();
        *spotify
            .get_token()
            .lock()
            .await
            .map_err(|e| format!("Token lock error: {e:?}"))? = Some(token);
        if expired {
            if let Ok(Some(refreshed)) = spotify.refetch_token().await {
                *spotify
                    .get_token()
                    .lock()
                    .await
                    .map_err(|e| format!("Token lock error: {e:?}"))? = Some(refreshed);
                return Ok(());
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

fn is_rate_limited(err: &str) -> bool {
    let lower = err.to_lowercase();
    lower.contains("429") || lower.contains("too many requests") || lower.contains("rate limit")
}

async fn sleep_backoff(attempt: u32) {
    let secs = 2u64.saturating_pow(attempt.min(5)).max(2);
    tokio::time::sleep(Duration::from_secs(secs)).await;
}

/// Expand many album URLs via embed scrape (no Web API track calls).
pub async fn expand_albums_via_scrape(album_urls: &[String]) -> Result<SavedAlbumsImport, String> {
    let mut track_urls = Vec::new();
    let mut album_names = Vec::new();
    let mut seen_albums = HashSet::new();
    let mut failures = Vec::new();

    for (i, album_url) in album_urls.iter().enumerate() {
        let clean = album_url.split('?').next().unwrap_or(album_url).to_string();
        if !clean.contains("/album/") || !seen_albums.insert(clean.clone()) {
            continue;
        }

        info!(
            "Scraping album {}/{}: {}",
            i + 1,
            album_urls.len(),
            clean
        );

        let mut last_err = None;
        let mut tracks = None;
        for attempt in 0..3u32 {
            match scraper::expand_url_to_tracks(&clean).await {
                Ok(t) => {
                    tracks = Some(t);
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    sleep_backoff(attempt).await;
                }
            }
        }

        match tracks {
            Some(t) => {
                // Album name is stored in metadata cache for the first track when scrape succeeds.
                let name = t
                    .first()
                    .and_then(|u| metadata_cache::get(u))
                    .map(|m| m.album)
                    .filter(|a| a != "Unknown Album")
                    .unwrap_or_else(|| clean.clone());
                album_names.push(name);
                track_urls.extend(t);
            }
            None => {
                failures.push(format!(
                    "{} ({})",
                    clean,
                    last_err.unwrap_or_else(|| "unknown error".into())
                ));
            }
        }

        // Be gentle with public embed endpoints.
        tokio::time::sleep(Duration::from_millis(350)).await;
    }

    if track_urls.is_empty() {
        return Err(format!(
            "No tracks scraped from {} album link(s). Failures: {}",
            album_urls.len(),
            if failures.is_empty() {
                "none".into()
            } else {
                failures.join("; ")
            }
        ));
    }

    let status_note = if failures.is_empty() {
        String::new()
    } else {
        format!(" ({} album(s) failed to scrape)", failures.len())
    };
    let _ = status_note; // used by callers via album_names/track counts

    Ok(SavedAlbumsImport {
        album_count: album_names.len(),
        track_urls,
        album_names,
    })
}

fn album_url_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"https?://open\.spotify\.com/album/[A-Za-z0-9]+"#)
            .expect("album url regex")
    })
}

/// Parse pasted text: newline URLs, JSON string arrays, or mixed HTML/console dumps.
pub fn parse_album_urls_from_text(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();

    // JSON array of strings
    if let Ok(serde_json::Value::Array(items)) =
        serde_json::from_str::<serde_json::Value>(text.trim())
    {
        for item in items {
            if let Some(s) = item.as_str() {
                for m in album_url_re().find_iter(s) {
                    let u = m.as_str().to_string();
                    if seen.insert(u.clone()) {
                        out.push(u);
                    }
                }
            }
        }
    }

    for m in album_url_re().find_iter(text) {
        let u = m.as_str().to_string();
        if seen.insert(u.clone()) {
            out.push(u);
        }
    }

    out
}

/// Fully scrape-based import from pasted album links / console scraper output.
pub async fn import_albums_from_pasted_text(text: &str) -> Result<SavedAlbumsImport, String> {
    let albums = parse_album_urls_from_text(text);
    if albums.is_empty() {
        return Err(
            "No Spotify album URLs found in the paste. Open collection/albums, run the console scraper, then paste here."
                .to_string(),
        );
    }
    expand_albums_via_scrape(&albums).await
}

/// Login once to list saved albums, then expand every album via embed scrape (avoids API 429s).
pub async fn import_saved_albums() -> Result<SavedAlbumsImport, String> {
    let spotify = user_client()?;
    ensure_user_token(&spotify).await?;

    let mut album_page_urls = Vec::new();
    let mut offset = 0u32;
    let limit = 50u32;

    loop {
        let mut page = None;
        let mut last_err = None;
        for attempt in 0..6u32 {
            match spotify
                .current_user_saved_albums_manual(None, Some(limit), Some(offset))
                .await
            {
                Ok(p) => {
                    page = Some(p);
                    break;
                }
                Err(e) => {
                    let msg = e.to_string();
                    if is_rate_limited(&msg) {
                        info!("Saved-albums list rate-limited; backing off (attempt {})", attempt + 1);
                        sleep_backoff(attempt + 1).await;
                        last_err = Some(msg);
                        continue;
                    }
                    return Err(format!("Failed to fetch saved albums: {msg}"));
                }
            }
        }
        let page = page.ok_or_else(|| {
            format!(
                "Failed to fetch saved albums after retries: {}",
                last_err.unwrap_or_else(|| "unknown".into())
            )
        })?;

        for saved in page.items {
            let id = saved.album.id.id().to_string();
            album_page_urls.push(format!("https://open.spotify.com/album/{id}"));
        }

        if page.next.is_none() {
            break;
        }
        offset += limit;
        // Small pause between list pages to reduce 429s.
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    if album_page_urls.is_empty() {
        return Err("No saved albums found on this Spotify account.".to_string());
    }

    info!(
        "Found {} saved albums — expanding tracks via embed scrape (not API)...",
        album_page_urls.len()
    );

    expand_albums_via_scrape(&album_page_urls).await
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
            normalize_redirect_uri("http://127.0.0.1:8888"),
            "http://127.0.0.1:8888"
        );
    }

    #[test]
    fn parses_pasted_album_urls() {
        let text = r#"
        https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy?si=abc
        junk
        https://open.spotify.com/album/0sNOF9WDwhWunNAHPD3Baj
        "#;
        let urls = parse_album_urls_from_text(text);
        assert_eq!(urls.len(), 2);
        assert!(urls[0].contains("4aawyAB9vmqN3uQ7FjRGTy"));
    }

    #[test]
    fn parses_json_array_of_urls() {
        let text = r#"["https://open.spotify.com/album/4aawyAB9vmqN3uQ7FjRGTy","https://open.spotify.com/album/0sNOF9WDwhWunNAHPD3Baj"]"#;
        assert_eq!(parse_album_urls_from_text(text).len(), 2);
    }
}
