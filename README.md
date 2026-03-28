# CrossPost-Rust

Cross-platform desktop application for uploading short videos to YouTube Shorts, Instagram Reels, TikTok, and VK Clips simultaneously.

## Architecture

Cargo workspace with hexagonal architecture:

```
├── core/     Shared library (domain, ports, adapters, services, validation)
├── cli/      Command-line interface (clap + indicatif)
├── gui/      Desktop GUI (egui + eframe + rfd)
```

### Core Library

- `domain/model.rs` — Domain models: `VideoMetadata`, `Platform`, `ThemePreference`, `UploadResult`, `PlatformConstraints`
- `domain/port.rs` — Port interface: `AsyncUploader` trait + `ProgressCallback`
- `adapter/mock_uploader.rs` — Mock uploader for all 4 platforms (simulated upload)
- `adapter/keyring_store.rs` — Secure OS keychain token storage via `keyring`
- `service/upload_orchestrator.rs` — Parallel upload coordination via `tokio::JoinSet`
- `validation/` — File size, format, extension validation per platform
- `config/` — `ConfigManager` with OS-specific paths via `directories`
- `error.rs` — `CoreError` enum via `thiserror`

## Prerequisites

- [Rust](https://rustup.rs/) 1.75+
- Platform-specific dependencies:

### macOS

```bash
xcode-select --install
```

### Linux (Debian/Ubuntu)

```bash
sudo apt install -y \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev \
  libxkbcommon-dev libssl-dev pkg-config \
  libdbus-1-dev libsecret-1-dev  # for keyring
```

### Linux (Fedora)

```bash
sudo dnf install -y \
  libxcb-devel libxkbcommon-devel openssl-devel pkg-config \
  dbus-devel libsecret-devel
```

### Windows

MSVC toolchain (Visual Studio Build Tools).

## Build

```bash
# Build all crates
cargo build --release

# Binaries output to:
#   target/release/crosspost-cli
#   target/release/crosspost-gui
```

## CLI Usage

```bash
# Show config
crosspost-cli config show

# Set theme
crosspost-cli config set-theme dark

# Validate a video against all platforms
crosspost-cli validate -v video.mp4

# Upload to specific platforms
crosspost-cli upload -v video.mp4 -t "My Short" -d "Description" --tags "tag1,tag2" -p youtube,tiktok

# Upload to all platforms (default)
crosspost-cli upload -v video.mp4 -t "My Short"
```

## GUI

```bash
cargo run -p crosspost-gui
```

Features:
- Native file picker for video and thumbnail
- Metadata form (title, description, tags)
- Per-platform toggle checkboxes
- Real-time progress bars during upload
- Dark/Light/System theme switching (persisted to config)

## Config Location

| OS      | Path                                                                           |
|---------|--------------------------------------------------------------------------------|
| Linux   | `~/.config/crosspost-rust/config.toml`                                         |
| macOS   | `~/Library/Application Support/com.CrossPost.CrossPost-Rust/config.toml`       |
| Windows | `C:\Users\<User>\AppData\Roaming\CrossPost\CrossPost-Rust\config\config.toml`  |

## Token Storage

API tokens are stored securely in the OS keychain:
- **macOS**: Keychain
- **Linux**: Secret Service (GNOME Keyring / KWallet)
- **Windows**: Credential Manager

## Roadmap

- [x] Phase 1: Workspace, config, domain models, uploader trait
- [x] Phase 2: Validation module, mock uploaders, parallel upload orchestrator
- [x] Phase 3: Full CLI with progress bars, validation, platform selection
- [x] Phase 4: Full GUI with file picker, upload dashboard, theme switching
- [x] Phase 5: Keyring integration, build instructions
- [ ] Phase 6: Real API integrations (YouTube OAuth2, TikTok, Instagram Graph, VK)
