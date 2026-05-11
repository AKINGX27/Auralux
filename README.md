# Auralux

[简体中文](README.zh-CN.md)

Auralux is a lightweight, local-first music player and audio conversion project for desktop, Android, web control surfaces, and CLI workflows.

It is designed around one Rust core, one shared GUI, and thin platform shells. Heavy work such as scanning, playback, decoding, and conversion runs on the user's device. A hosted web UI can exist, but it should only control local compute and never receive the user's audio files.

## Status

Auralux is currently an early implementation scaffold, not a finished player.

Implemented in this repository:

- Rust workspace with core library, local daemon, and CLI.
- SQLite library schema with FTS5 search.
- Local folder scanner with tag reading and optional `ffprobe` enrichment.
- FFmpeg/mpv capability detection.
- Single-concurrency FFmpeg conversion queue.
- Basic mpv-backed playback control abstraction.
- Axum daemon exposing REST and WebSocket APIs.
- Svelte/Vite GUI with a responsive glass-style interface.
- Tauri 2 desktop/Android shell scaffold.
- Android APK media-file import through the system picker, audio media permissions, and an embedded local daemon.
- Cloudflare Worker relay scaffold for encrypted remote control pairing.
- CI, docs, GPL license, and NOTICE files.

Still planned:

- Full Android native playback/conversion plugin with libmpv, FFmpeg codec packs, MediaSession, audio focus, notifications, and SAF folder grants.
- Durable playback queue and playlist editing in the GUI.
- End-to-end pairing crypto between hosted web UI and local daemon.
- Broader integration tests with real and fake FFmpeg/mpv binaries.

## Goals

- Be lightweight: no Electron, minimal runtime layers, local SQLite, and optional codec packs.
- Support as many audio formats as the active FFmpeg/mpv toolchain supports.
- Share one GUI across local web, desktop, and Android where practical.
- Keep audio files local, including when the web UI is hosted remotely.
- Provide a real CLI for library management, conversion, and daemon workflows.
- Stay GPL-3.0-or-later and avoid FFmpeg `nonfree` components in official builds.

## Architecture

```text
apps/gui              Svelte/Vite shared GUI
apps/tauri            Tauri 2 desktop and Android shell
crates/auralux-core   SQLite, scanner, metadata, conversion, playback, shared types
crates/auraluxd       Local REST/WebSocket daemon
crates/auralux-cli    CLI frontend over the same core
workers/web-relay     Cloudflare Worker static host and WebSocket relay scaffold
docs                  Architecture, commands, Android, and remote web notes
```

Runtime model:

- The daemon binds to `127.0.0.1:4147` by default.
- The GUI talks to `/api` with REST and listens to `/api/events` over WebSocket.
- Desktop playback uses the user's `mpv` binary when available.
- Conversion uses the user's `ffmpeg` binary when available.
- Format support is shown in the GUI settings page as a capability matrix.
- Cloudflare Workers are only for static hosting and encrypted signaling. They must not process media.

More detail lives in [docs/architecture.md](docs/architecture.md), [docs/remote-web.md](docs/remote-web.md), and [docs/android.md](docs/android.md).

## Requirements

Development:

- Rust stable with `cargo`, `rustfmt`, and `clippy`.
- Node.js 20+ and npm.
- Optional: Tauri platform prerequisites for desktop or Android builds.
- Optional: Wrangler for Cloudflare Worker development.

Runtime:

- `ffmpeg` and `ffprobe` for scanning enrichment and conversion.
- `mpv` for desktop playback.

Environment variables:

```bash
AURALUX_BIND=127.0.0.1:4147
AURALUX_DATA_DIR=/path/to/data
AURALUX_FFMPEG=/path/to/ffmpeg
AURALUX_FFPROBE=/path/to/ffprobe
AURALUX_MPV=/path/to/mpv
AURALUX_GUI_DIST=apps/gui/dist
```

See [.env.example](.env.example).

## Quick Start

Install dependencies:

```bash
npm install
```

Run Rust tests:

```bash
cargo test --workspace
```

Start the local daemon:

```bash
cargo run -p auraluxd -- --bind 127.0.0.1:4147
```

In another terminal, start the GUI dev server:

```bash
npm run dev
```

Open:

```text
http://127.0.0.1:5173
```

Scan music:

```bash
cargo run -p auralux-cli -- scan ~/Music
```

Convert a file:

```bash
cargo run -p auralux-cli -- convert ~/Music/in.flac ~/Music/Converted --format opus
```

## CLI

The CLI binary is named `auralux`.

```bash
auralux serve --bind 127.0.0.1:4147
auralux scan ~/Music
auralux search "artist or title"
auralux convert ~/Music/in.flac ~/Music/Converted --format opus
auralux jobs
auralux config
```

Current subcommands:

- `serve`: launch the local daemon.
- `scan`: scan one or more local folders.
- `search`: search the SQLite FTS index.
- `play`: placeholder entry for daemon playback loading.
- `pause`: placeholder entry for daemon playback command.
- `queue`: placeholder for durable queue work.
- `convert`: run a foreground FFmpeg conversion.
- `jobs`: print recent conversion jobs.
- `config`: print database path and codec capability detection.

See [docs/commands.md](docs/commands.md).

## API

The daemon mounts APIs under `/api`:

- `GET /api/health`
- `GET /api/events`
- `POST /api/library/scan`
- `GET /api/library/tracks`
- `GET /api/playback/state`
- `POST /api/playback/load`
- `POST /api/playback/command`
- `POST /api/conversions`
- `GET /api/jobs`
- `GET /api/jobs/:id`

Example scan request:

```bash
curl -X POST http://127.0.0.1:4147/api/library/scan \
  -H 'content-type: application/json' \
  -d '{"roots":["/home/me/Music"],"force":false}'
```

Example conversion request:

```bash
curl -X POST http://127.0.0.1:4147/api/conversions \
  -H 'content-type: application/json' \
  -d '{
    "source_path": "/home/me/Music/in.flac",
    "output_dir": "/home/me/Music/Converted",
    "preset": { "format": "opus", "quality": "160k" },
    "overwrite": false
  }'
```

## GUI

The GUI is in [apps/gui](apps/gui). It provides:

- Library search and track list.
- Folder scan entry.
- Playback bar.
- Conversion job panel.
- Codec capability matrix.
- Responsive desktop/mobile layout.

Development:

```bash
npm --workspace apps/gui run dev
npm --workspace apps/gui run check
npm --workspace apps/gui run build
```

## Tauri

The shell is in [apps/tauri](apps/tauri).

```bash
npm --workspace apps/tauri run dev
npm --workspace apps/tauri run build
```

Windows desktop builds can be produced with:

```bash
npm run build:desktop:windows
```

The portable Windows EXE is staged under `release/auralux-windows-x86_64`. It includes `windows-register-file-associations.ps1`, which registers common audio extensions for the current Windows user. File associations are also declared in the Tauri bundle config and are applied automatically by installer builds. To create an installer on a Windows host or an environment with NSIS available, run:

```bash
AURALUX_WINDOWS_BUNDLE=1 npm run build:desktop:windows
```

The Windows shell forwards associated audio files to the shared GUI. When the local daemon is running, opened files are imported into the active playlist by path.

Android commands are scaffolded:

```bash
npm --workspace apps/tauri run android:dev
npm --workspace apps/tauri run android:build
npm run build:android:apk
```

The Android APK requests audio media permission, starts an embedded local daemon, and imports selected music files through Android's system file picker into the active playlist. Full Android native background playback and codec-pack conversion are still planned. See [docs/android.md](docs/android.md).

Release CI currently publishes CLI/daemon binaries and web assets. Native Tauri installers will be added once desktop platform dependencies and signing are configured.

## Cloudflare Worker

The Worker scaffold is in [workers/web-relay](workers/web-relay). It serves the built GUI and exposes `/relay/:room` WebSocket rooms through Durable Objects.

```bash
npm --workspace workers/web-relay run dev
npm --workspace workers/web-relay run build
npm --workspace workers/web-relay run test
```

The Worker relay is intentionally control-plane only. Pairing messages should be encrypted by browser and daemon clients before entering the Worker.

## Testing

Recommended checks:

```bash
cargo generate-lockfile
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm run check
npm run build
npm test
```

This repository was scaffolded in an environment that did not have `cargo`, `node`, or `npm` installed, so those commands must be run after installing the toolchains.

Commit `Cargo.lock` for reproducible application releases after generating it with Cargo.

## Packaging Notes

- Official builds should remain GPL-3.0-or-later.
- Official FFmpeg builds must not enable `nonfree` components.
- Desktop base packages should prefer system `ffmpeg`, `ffprobe`, and `mpv`; optional codec packs can be distributed separately.
- Android should use ABI split/AAB packaging for native codec libraries.
- Optional codec packs must ship corresponding license notices and source/build information.

## License

Auralux is licensed under GPL-3.0-or-later. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
