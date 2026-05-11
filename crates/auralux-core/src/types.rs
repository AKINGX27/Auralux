use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: i64,
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub duration_ms: Option<i64>,
    pub format: Option<String>,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub size_bytes: i64,
    pub mtime: i64,
    pub artwork_hash: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewTrack {
    pub path: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: Option<String>,
    pub genre: Option<String>,
    pub track_number: Option<i64>,
    pub disc_number: Option<i64>,
    pub duration_ms: Option<i64>,
    pub format: Option<String>,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub channels: Option<i64>,
    pub size_bytes: i64,
    pub mtime: i64,
    pub artwork_hash: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrackQuery {
    pub search: Option<String>,
    pub limit: usize,
    pub offset: usize,
}

impl Default for TrackQuery {
    fn default() -> Self {
        Self {
            search: None,
            limit: 100,
            offset: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub track_count: i64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlaylistDetail {
    pub playlist: Playlist,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRecord {
    pub id: String,
    pub kind: String,
    pub state: String,
    pub source_path: Option<PathBuf>,
    pub output_path: Option<PathBuf>,
    pub progress: f64,
    pub message: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Health {
    pub app: String,
    pub version: String,
    pub database_path: PathBuf,
    pub capabilities: crate::capabilities::CodecCapabilities,
}
