# Android

Android builds use the shared Svelte GUI inside the Tauri 2 shell and start an embedded local Auralux daemon on `127.0.0.1:4147`. This gives the APK the same library, playlist, upload/import, and job APIs as the desktop/web control surface.

## Media Access

- The generated APK declares `READ_MEDIA_AUDIO` for Android 13+ and `READ_EXTERNAL_STORAGE` with `maxSdkVersion=32` for older Android releases.
- `MainActivity` requests the matching audio-library permission at startup.
- The GUI provides an `Add music files` action that opens Android's system file picker through the WebView file chooser.
- Selected `content://` files are read through the user-granted picker permission and uploaded to the embedded daemon's app-local import directory before being indexed and added to the active playlist.
- The app also allows cleartext traffic only to `127.0.0.1`/`localhost` so the packaged GUI can talk to the embedded local API without exposing remote HTTP access.

This path intentionally avoids `MANAGE_EXTERNAL_STORAGE`; broad all-files access is heavier than needed for user-selected music imports and is harder to justify on modern Android.

## Build

Use the cached build script:

```bash
npm run build:android:apk
```

The script initializes or reuses the generated Tauri Android project, patches the manifest/runtime permission prompt, preserves download caches under `.auralux-build/`, signs APK artifacts, and stages them under `release/auralux-android`.

## Still Planned

- Storage Access Framework directory import with persisted tree grants for scanning whole music folders.
- Android-native playback backend with libmpv, MediaSession, audio focus, headset controls, notification transport buttons, and background playback service.
- FFmpeg codec pack split by ABI in Android App Bundle builds.
- Conversion cancellation and progress surfaced through Android notifications.
