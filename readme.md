# Spotify Downloader

A cross-platform, asynchronous desktop application built in Rust using `eframe`/`egui` and `Tokio`. This tool expands Spotify links into track listings and concurrently downloads them with a concurrency queue, leveraging `yt-dlp` for audio extraction.

---

## Metadata modes

| Mode | Needs API keys? | Notes |
|------|-----------------|-------|
| **Web Scrape (JSON)** (default) | No | Reads Spotify's public embed `__NEXT_DATA__` / oEmbed JSON. Avoids Developer API rate limits. Large playlists may be truncated by the embed (~100 tracks). |
| **Spotify API** | Yes (`Client ID` / `Client Secret`) | Official Web API with full playlist/album pagination. Spotify Premium + Developer Dashboard required. |

Switch modes in **Config**. Save Config applies credentials to the running session immediately (not only to `.env`).

---

## Prerequisites

- **yt-dlp** must be installed and available on your `PATH`
- **No VPN** while authenticating against Spotify services (API mode especially)
- API mode only: Spotify Premium + [Developer Dashboard](https://developer.spotify.com/dashboard) credentials

---

## Screenshots

![Screenshot 1](examples/Screenshot%20From%202026-06-08%2022-08-56.png)

![Screenshot 2](examples/Screenshot%20From%202026-06-08%2022-11-14.png)

![Screenshot 3](examples/Screenshot%20From%202026-06-08%2022-09-34.png)

![Screenshot 4](examples/Screenshot%20From%202026-06-08%2022-10-43.png)

---

## Downloading & Running the App

Pre-compiled portable packages are available in Releases / `portable/`.

### Windows
1. Download `spotify_downloader_windows.zip`
2. Extract anywhere
3. Run `spotify_download.exe`

### Linux
```bash
chmod +x spotify_download
./spotify_download
```

---

## Building From Source

### 1. Clone
```bash
git clone git@github.com:SeasonedTurtle/Spotify_mp3_Download.git
cd Spotify_mp3_Download
```

### 2. System dependencies (Ubuntu/Debian)
```bash
sudo apt update
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libssl-dev pkg-config
```

Also install [yt-dlp](https://github.com/yt-dlp/yt-dlp).

### 3. Dev run
```bash
cargo run
```

### 4. Release build
```bash
cargo build --release
```
Binary: `target/release/spotify_download`

### Cross-compile Windows from Linux
```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --target x86_64-pc-windows-gnu --release
```

---

## Config (`.env`)

```env
METADATA_SOURCE=scrape
DOWNLOAD_DIR=downloads
RSPOTIFY_CLIENT_ID=
RSPOTIFY_CLIENT_SECRET=
```

`METADATA_SOURCE` may be `scrape` (default) or `api`.

---

## Reliability notes

- Spotify API clients are cached (one token) instead of re-authing per track
- Album track lists are paginated in API mode
- Existing MP3s are skipped; failed yt-dlp downloads retry up to 3 times
- Filenames are sanitized for Windows/Linux illegal characters
- Duplicate track URLs are ignored when loading

---

## Contributing
1. Fork the project
2. Create your feature branch (`git checkout -b feature/AmazingFeature`)
3. Commit your changes
4. Push to the branch
5. Open a Pull Request
