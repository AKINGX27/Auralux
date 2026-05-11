# Android Roadmap

The shared GUI and Tauri shell are present. Full Android native media support needs a platform plugin with:

- Storage Access Framework folder grants and persisted URI permissions.
- libmpv playback backend.
- FFmpeg codec pack split by ABI in Android App Bundle builds.
- MediaSession, audio focus, headset controls, notification transport buttons, and background playback service.
- Conversion cancellation and progress surfaced through the shared event bus.

Until that plugin lands, Android builds should be treated as the GUI/control surface with native media work in progress.

