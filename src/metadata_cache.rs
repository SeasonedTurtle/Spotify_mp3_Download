//! Shared in-memory track metadata cache (filled by scrape/API/library expand).

use crate::spotify::TrackMetadata;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

fn cache() -> &'static Mutex<HashMap<String, TrackMetadata>> {
    static CACHE: OnceLock<Mutex<HashMap<String, TrackMetadata>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn clean_url(url: &str) -> String {
    url.split('?').next().unwrap_or(url).to_string()
}

pub fn put(track_url: &str, meta: TrackMetadata) {
    if let Ok(mut guard) = cache().lock() {
        guard.insert(clean_url(track_url), meta);
    }
}

pub fn get(track_url: &str) -> Option<TrackMetadata> {
    cache()
        .lock()
        .ok()
        .and_then(|guard| guard.get(&clean_url(track_url)).cloned())
}
