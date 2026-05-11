use crate::types::NewTrack;
use anyhow::{Context, Result};
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::read_from_path;
use lofty::tag::Accessor;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::UNIX_EPOCH;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct MetadataReader {
    ffprobe_path: Option<PathBuf>,
}

impl MetadataReader {
    pub fn new(ffprobe_path: Option<PathBuf>) -> Self {
        Self { ffprobe_path }
    }

    pub async fn read(&self, path: &Path) -> Result<NewTrack> {
        let fs = std::fs::metadata(path)
            .with_context(|| format!("reading metadata for {}", path.display()))?;
        let size_bytes = fs.len() as i64;
        let mtime = fs
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();

        let mut track = read_tags(path, size_bytes, mtime)?;
        if let Some(ffprobe) = self.ffprobe_path.as_ref() {
            if let Ok(probed) = read_ffprobe(ffprobe, path).await {
                if track.duration_ms.is_none() {
                    track.duration_ms = probed.duration_ms;
                }
                track.format = probed.format.or(track.format);
                track.codec = probed.codec.or(track.codec);
                track.bitrate = probed.bitrate.or(track.bitrate);
                track.sample_rate = probed.sample_rate.or(track.sample_rate);
                track.channels = probed.channels.or(track.channels);
            }
        }
        Ok(track)
    }
}

#[derive(Default)]
struct ProbeData {
    duration_ms: Option<i64>,
    format: Option<String>,
    codec: Option<String>,
    bitrate: Option<i64>,
    sample_rate: Option<i64>,
    channels: Option<i64>,
}

fn read_tags(path: &Path, size_bytes: i64, mtime: i64) -> Result<NewTrack> {
    let tagged = read_from_path(path).ok();
    let tag = tagged.as_ref().and_then(|file| file.primary_tag());
    let properties = tagged.as_ref().map(|file| file.properties());
    let fallback_title = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or("Unknown Title")
        .to_string();

    Ok(NewTrack {
        path: path.to_path_buf(),
        title: tag
            .and_then(|tag| tag.title())
            .map(|value| value.to_string())
            .unwrap_or(fallback_title),
        artist: tag
            .and_then(|tag| tag.artist())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "Unknown Artist".into()),
        album: tag
            .and_then(|tag| tag.album())
            .map(|value| value.to_string())
            .unwrap_or_else(|| "Unknown Album".into()),
        album_artist: None,
        genre: tag
            .and_then(|tag| tag.genre())
            .map(|value| value.to_string()),
        track_number: tag.and_then(|tag| tag.track()).map(i64::from),
        disc_number: tag.and_then(|tag| tag.disk()).map(i64::from),
        duration_ms: properties.map(|props| props.duration().as_millis() as i64),
        format: path
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase()),
        codec: None,
        bitrate: properties.and_then(|props| props.audio_bitrate().map(i64::from)),
        sample_rate: properties.and_then(|props| props.sample_rate().map(i64::from)),
        channels: properties.and_then(|props| props.channels().map(i64::from)),
        size_bytes,
        mtime,
        artwork_hash: None,
    })
}

async fn read_ffprobe(ffprobe: &Path, path: &Path) -> Result<ProbeData> {
    let output = Command::new(ffprobe)
        .args([
            "-v",
            "quiet",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
        ])
        .arg(path)
        .stdout(Stdio::piped())
        .output()
        .await?;
    let value: Value = serde_json::from_slice(&output.stdout)?;
    let mut data = ProbeData::default();

    if let Some(format) = value.get("format") {
        data.duration_ms = format
            .get("duration")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<f64>().ok())
            .map(|seconds| (seconds * 1000.0) as i64);
        data.format = format
            .get("format_name")
            .and_then(Value::as_str)
            .map(str::to_string);
        data.bitrate = format
            .get("bit_rate")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok());
    }

    if let Some(stream) = value
        .get("streams")
        .and_then(Value::as_array)
        .and_then(|streams| {
            streams
                .iter()
                .find(|stream| stream.get("codec_type").and_then(Value::as_str) == Some("audio"))
        })
    {
        data.codec = stream
            .get("codec_name")
            .and_then(Value::as_str)
            .map(str::to_string);
        data.sample_rate = stream
            .get("sample_rate")
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<i64>().ok());
        data.channels = stream.get("channels").and_then(Value::as_i64);
    }

    Ok(data)
}
