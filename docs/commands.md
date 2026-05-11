# Commands

```bash
auraluxd --bind 127.0.0.1:4147
auralux scan ~/Music
auralux search "artist or title"
auralux convert ~/Music/in.flac ~/Music/Converted --format opus
auralux jobs
auralux config
```

The daemon API is mounted under `/api`:

- `GET /api/health`
- `POST /api/library/scan`
- `GET /api/library/tracks`
- `GET /api/playback/state`
- `POST /api/playback/load`
- `POST /api/playback/command`
- `POST /api/conversions`
- `GET /api/jobs/:id`
- `GET /api/events`

