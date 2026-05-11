# Auralux Architecture

## Principle

Auralux keeps compute local. The browser UI can be served from the daemon or from Cloudflare Workers, but scanning, playback, conversion, and media access stay on the user's machine.

## Runtime Topology

- Local daemon: binds to `127.0.0.1`, owns SQLite, FFmpeg/mpv processes, job queue, and event bus.
- Shared GUI: uses REST for queries and WebSocket events for progress/state.
- Tauri shell: desktop and Android wrapper around the same GUI. Android native playback/codec work is represented as a platform seam for the first full native implementation.
- Worker relay: relays encrypted control-plane messages only; it never receives audio files.

## Format Policy

"All formats" means every audio format supported by the active FFmpeg/mpv toolchain. Capability detection is visible in settings so users can see what their current install supports.

