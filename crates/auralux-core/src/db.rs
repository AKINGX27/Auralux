use crate::types::{JobRecord, NewTrack, Track, TrackQuery};
use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct LibraryDatabase {
    path: PathBuf,
    connection: Arc<Mutex<Connection>>,
}

impl LibraryDatabase {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let connection = Connection::open(path.as_ref())
            .with_context(|| format!("opening database {}", path.as_ref().display()))?;
        connection.pragma_update(None, "journal_mode", "WAL")?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        let database = Self {
            path: path.as_ref().to_path_buf(),
            connection: Arc::new(Mutex::new(connection)),
        };
        database.migrate()?;
        Ok(database)
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS artists (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL UNIQUE
            );

            CREATE TABLE IF NOT EXISTS albums (
              id INTEGER PRIMARY KEY,
              title TEXT NOT NULL,
              artist TEXT,
              year INTEGER,
              artwork_hash TEXT,
              UNIQUE(title, artist)
            );

            CREATE TABLE IF NOT EXISTS artwork (
              hash TEXT PRIMARY KEY,
              mime TEXT NOT NULL,
              bytes BLOB NOT NULL,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tracks (
              id INTEGER PRIMARY KEY,
              path TEXT NOT NULL UNIQUE,
              title TEXT NOT NULL,
              artist TEXT NOT NULL,
              album TEXT NOT NULL,
              album_artist TEXT,
              genre TEXT,
              track_number INTEGER,
              disc_number INTEGER,
              duration_ms INTEGER,
              format TEXT,
              codec TEXT,
              bitrate INTEGER,
              sample_rate INTEGER,
              channels INTEGER,
              size_bytes INTEGER NOT NULL,
              mtime INTEGER NOT NULL,
              artwork_hash TEXT REFERENCES artwork(hash),
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS tracks_fts USING fts5(
              title,
              artist,
              album,
              path,
              content='tracks',
              content_rowid='id'
            );

            CREATE TRIGGER IF NOT EXISTS tracks_ai AFTER INSERT ON tracks BEGIN
              INSERT INTO tracks_fts(rowid, title, artist, album, path)
              VALUES (new.id, new.title, new.artist, new.album, new.path);
            END;

            CREATE TRIGGER IF NOT EXISTS tracks_ad AFTER DELETE ON tracks BEGIN
              INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, path)
              VALUES ('delete', old.id, old.title, old.artist, old.album, old.path);
            END;

            CREATE TRIGGER IF NOT EXISTS tracks_au AFTER UPDATE ON tracks BEGIN
              INSERT INTO tracks_fts(tracks_fts, rowid, title, artist, album, path)
              VALUES ('delete', old.id, old.title, old.artist, old.album, old.path);
              INSERT INTO tracks_fts(rowid, title, artist, album, path)
              VALUES (new.id, new.title, new.artist, new.album, new.path);
            END;

            CREATE TABLE IF NOT EXISTS playlists (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL UNIQUE,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS playlist_items (
              playlist_id INTEGER NOT NULL REFERENCES playlists(id) ON DELETE CASCADE,
              track_id INTEGER NOT NULL REFERENCES tracks(id) ON DELETE CASCADE,
              position INTEGER NOT NULL,
              PRIMARY KEY (playlist_id, track_id)
            );

            CREATE TABLE IF NOT EXISTS jobs (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              state TEXT NOT NULL,
              source_path TEXT,
              output_path TEXT,
              progress REAL NOT NULL DEFAULT 0,
              message TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS settings (
              key TEXT PRIMARY KEY,
              value TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    pub fn upsert_track(&self, track: &NewTrack) -> Result<i64> {
        let now = Utc::now().to_rfc3339();
        let path = track.path.to_string_lossy();
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.execute(
            r#"
            INSERT INTO tracks (
              path, title, artist, album, album_artist, genre, track_number, disc_number,
              duration_ms, format, codec, bitrate, sample_rate, channels, size_bytes, mtime,
              artwork_hash, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19)
            ON CONFLICT(path) DO UPDATE SET
              title=excluded.title,
              artist=excluded.artist,
              album=excluded.album,
              album_artist=excluded.album_artist,
              genre=excluded.genre,
              track_number=excluded.track_number,
              disc_number=excluded.disc_number,
              duration_ms=excluded.duration_ms,
              format=excluded.format,
              codec=excluded.codec,
              bitrate=excluded.bitrate,
              sample_rate=excluded.sample_rate,
              channels=excluded.channels,
              size_bytes=excluded.size_bytes,
              mtime=excluded.mtime,
              artwork_hash=excluded.artwork_hash,
              updated_at=excluded.updated_at
            "#,
            params![
                path.as_ref(),
                &track.title,
                &track.artist,
                &track.album,
                track.album_artist.as_deref(),
                track.genre.as_deref(),
                track.track_number,
                track.disc_number,
                track.duration_ms,
                track.format.as_deref(),
                track.codec.as_deref(),
                track.bitrate,
                track.sample_rate,
                track.channels,
                track.size_bytes,
                track.mtime,
                track.artwork_hash.as_deref(),
                &now,
                &now,
            ],
        )?;
        let id = conn.query_row(
            "SELECT id FROM tracks WHERE path = ?1",
            params![path],
            |row| row.get(0),
        )?;
        Ok(id)
    }

    pub fn track_signature(&self, path: &Path) -> Result<Option<(i64, i64)>> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.query_row(
            "SELECT size_bytes, mtime FROM tracks WHERE path = ?1",
            params![path.to_string_lossy()],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_tracks(&self, query: TrackQuery) -> Result<Vec<Track>> {
        let limit = query.limit.min(500);
        let offset = query.offset;
        let conn = self.connection.lock().expect("database lock poisoned");
        if let Some(search) = query.search.filter(|value| !value.trim().is_empty()) {
            let escaped = fts_query(&search);
            let mut statement = conn.prepare(
                r#"
                SELECT t.*
                FROM tracks_fts f
                JOIN tracks t ON t.id = f.rowid
                WHERE tracks_fts MATCH ?1
                ORDER BY bm25(tracks_fts), t.album, t.track_number, t.title
                LIMIT ?2 OFFSET ?3
                "#,
            )?;
            let rows =
                statement.query_map(params![escaped, limit as i64, offset as i64], row_to_track)?;
            collect_rows(rows)
        } else {
            let mut statement = conn.prepare(
                r#"
                SELECT *
                FROM tracks
                ORDER BY album COLLATE NOCASE, disc_number, track_number, title COLLATE NOCASE
                LIMIT ?1 OFFSET ?2
                "#,
            )?;
            let rows = statement.query_map(params![limit as i64, offset as i64], row_to_track)?;
            collect_rows(rows)
        }
    }

    pub fn get_track(&self, id: i64) -> Result<Option<Track>> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.query_row(
            "SELECT * FROM tracks WHERE id = ?1",
            params![id],
            row_to_track,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_track_by_path(&self, path: &Path) -> Result<Option<Track>> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.query_row(
            "SELECT * FROM tracks WHERE path = ?1",
            params![path.to_string_lossy()],
            row_to_track,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn upsert_job(&self, job: &JobRecord) -> Result<()> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.execute(
            r#"
            INSERT INTO jobs (id, kind, state, source_path, output_path, progress, message, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            ON CONFLICT(id) DO UPDATE SET
              state=excluded.state,
              output_path=excluded.output_path,
              progress=excluded.progress,
              message=excluded.message,
              updated_at=excluded.updated_at
            "#,
            params![
                &job.id,
                &job.kind,
                &job.state,
                job.source_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                job.output_path.as_ref().map(|p| p.to_string_lossy().to_string()),
                job.progress,
                job.message.as_deref(),
                job.created_at.to_rfc3339(),
                job.updated_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn get_job(&self, id: &str) -> Result<Option<JobRecord>> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.query_row("SELECT * FROM jobs WHERE id = ?1", params![id], row_to_job)
            .optional()
            .map_err(Into::into)
    }

    pub fn list_jobs(&self, limit: usize) -> Result<Vec<JobRecord>> {
        let conn = self.connection.lock().expect("database lock poisoned");
        let mut statement = conn.prepare("SELECT * FROM jobs ORDER BY created_at DESC LIMIT ?1")?;
        let rows = statement.query_map(params![limit.min(200) as i64], row_to_job)?;
        collect_rows(rows)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.execute(
            r#"
            INSERT INTO settings (key, value, updated_at)
            VALUES (?1, ?2, ?3)
            ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_at=excluded.updated_at
            "#,
            params![key, value, now],
        )?;
        Ok(())
    }

    pub fn get_setting(&self, key: &str) -> Result<Option<String>> {
        let conn = self.connection.lock().expect("database lock poisoned");
        conn.query_row(
            "SELECT value FROM settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }
}

fn collect_rows<T>(rows: impl Iterator<Item = rusqlite::Result<T>>) -> Result<Vec<T>> {
    rows.collect::<rusqlite::Result<Vec<_>>>()
        .map_err(Into::into)
}

fn row_to_track(row: &Row<'_>) -> rusqlite::Result<Track> {
    Ok(Track {
        id: row.get("id")?,
        path: PathBuf::from(row.get::<_, String>("path")?),
        title: row.get("title")?,
        artist: row.get("artist")?,
        album: row.get("album")?,
        album_artist: row.get("album_artist")?,
        genre: row.get("genre")?,
        track_number: row.get("track_number")?,
        disc_number: row.get("disc_number")?,
        duration_ms: row.get("duration_ms")?,
        format: row.get("format")?,
        codec: row.get("codec")?,
        bitrate: row.get("bitrate")?,
        sample_rate: row.get("sample_rate")?,
        channels: row.get("channels")?,
        size_bytes: row.get("size_bytes")?,
        mtime: row.get("mtime")?,
        artwork_hash: row.get("artwork_hash")?,
        created_at: parse_ts(row.get::<_, String>("created_at")?),
        updated_at: parse_ts(row.get::<_, String>("updated_at")?),
    })
}

fn row_to_job(row: &Row<'_>) -> rusqlite::Result<JobRecord> {
    Ok(JobRecord {
        id: row.get("id")?,
        kind: row.get("kind")?,
        state: row.get("state")?,
        source_path: row
            .get::<_, Option<String>>("source_path")?
            .map(PathBuf::from),
        output_path: row
            .get::<_, Option<String>>("output_path")?
            .map(PathBuf::from),
        progress: row.get("progress")?,
        message: row.get("message")?,
        created_at: parse_ts(row.get::<_, String>("created_at")?),
        updated_at: parse_ts(row.get::<_, String>("updated_at")?),
    })
}

fn parse_ts(value: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&value)
        .map(|ts| ts.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}

fn fts_query(input: &str) -> String {
    input
        .split_whitespace()
        .map(|term| format!("\"{}\"*", term.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn upserts_and_searches_tracks() {
        let dir = tempdir().unwrap();
        let db = LibraryDatabase::open(dir.path().join("library.db")).unwrap();
        db.upsert_track(&NewTrack {
            path: dir.path().join("song.flac"),
            title: "Glass Sea".into(),
            artist: "Auralux".into(),
            album: "Refractions".into(),
            album_artist: None,
            genre: Some("Ambient".into()),
            track_number: Some(1),
            disc_number: None,
            duration_ms: Some(123_000),
            format: Some("flac".into()),
            codec: Some("flac".into()),
            bitrate: None,
            sample_rate: Some(48_000),
            channels: Some(2),
            size_bytes: 42,
            mtime: 10,
            artwork_hash: None,
        })
        .unwrap();

        let tracks = db
            .list_tracks(TrackQuery {
                search: Some("Glass".into()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].artist, "Auralux");
    }
}
