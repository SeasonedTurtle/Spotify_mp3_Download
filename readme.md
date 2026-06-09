# Spotify Downloader

A cross-platform, asynchronous desktop application built in Rust using 'eframe'/'egui' and 'Tokio'. This tool expands Spotify links into track listings and concurrently downloads them with an advanced concurrency queue system, leveraging 'yt-dlp' for a reliable and secure download process.

---

## 📌 Prerequisites & Critical Warnings

Before using or developing this application, please ensure you meet the following requirements:

> ⚠️ **Spotify Premium Required:** You **must** have a Spotify Premium account to access the developer dashboard and generate your API credentials ('Client ID' and 'Client Secret').
> 
> 🛑 **No VPNs Allowed:** The Spotify API and downloader engine will block or fail authentication if you are actively connected to a VPN. Please disable your VPN before launching the app.

---

## 📸 Screenshots

![Screenshot 1](examples/Screenshot%20From%202026-06-08%2022-08-56.png)

![Screenshot 2](examples/Screenshot%20From%202026-06-08%2022-11-14.png)

![Screenshot 3](examples/Screenshot%20From%202026-06-08%2022-09-34.png)

![Screenshot 4](examples/Screenshot%20From%202026-06-08%2022-10-43.png)
---

## 📥 Downloading & Running the App

Pre-compiled, completely self-contained portable packages are available in our Releases section. 

👉 **[Download the Latest Release Here](https://github.com/SeasonedTurtle/Spotify_mp3_Download/tree/main/portable)**

### How to Run:

#### 🪟 On Windows
1. Download 'spotify_downloader_windows.zip'.
2. Extract the folder completely anywhere on your PC.
3. Open the folder and double-click 'spotify_download.exe' to launch the app.

#### 🐧 On Linux
1. Download 'spotify_downloader_linux.zip'.
2. Extract the archive folder.
3. Open your terminal inside the extracted folder, mark the binary as executable, and run it:
```bash
chmod +x spotify_download
./spotify_download
```
---

## Forking & Building From Source

If you want to customize the layout, modify the downloader engine, or contribute to the project, follow these steps to build the application from scratch.

### 1. Clone the Repository
First, fork this repository to your own GitHub account, then clone it locally:
```bash
git clone git@github.com:SeasonedTurtle/Spotify_mp3_Download.git
cd Spotify_mp3_Download
```

### 2. Install System Dependencies (Linux Only)
If you are building on Ubuntu/Debian, you will need the standard GTK and development libraries for 'egui' to compile successfully:
```bash
sudo apt update
sudo apt install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev libssl-dev
```

### 3. Running in Development Mode
To compile and test the application with full debug assertions and logs active, run:
```bash
cargo run
```

### 4. Compiling the Production Release
To build the highly optimized, compressed executable for distribution:

```bash
cargo build --release
```
The compiled binary will be placed inside ''.

### Cross-Compiling for Windows from Linux
If you are developing on a Linux environment but want to generate the '.exe' for Windows users, install the MinGW toolchain:
```bash
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cargo build --target x86_64-pc-windows-gnu --release
```
Your Windows binary will be placed inside 'target/x86_64-pc-windows-gnu/release/spotify_download.exe'.

---

## Contributing
Contributions are what make the open-source community such an amazing place to learn, inspire, and create. 
1. **Fork** the Project
2. Create your Feature Branch ('git checkout -b feature/AmazingFeature')
3. **Commit** your Changes ('git commit -m 'Add some AmazingFeature'')
4. **Push** to the Branch ('git push origin feature/AmazingFeature')
5. Open a **Pull Request**