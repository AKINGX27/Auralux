use crate::db::LibraryDatabase;
use crate::events::{AuraluxEvent, EventBus};
use crate::types::JobRecord;
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::Semaphore;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionFormat {
    Flac,
    Opus,
    Mp3,
    Aac,
    Alac,
    Wav,
}

impl ConversionFormat {
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Flac => "flac",
            Self::Opus => "opus",
            Self::Mp3 => "mp3",
            Self::Aac => "m4a",
            Self::Alac => "m4a",
            Self::Wav => "wav",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConversionPreset {
    pub format: ConversionFormat,
    pub quality: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ConversionJobRequest {
    pub source_path: PathBuf,
    pub output_dir: PathBuf,
    pub preset: ConversionPreset,
    pub overwrite: bool,
}

#[derive(Clone)]
pub struct JobManager {
    database: LibraryDatabase,
    events: EventBus,
    ffmpeg_path: PathBuf,
    semaphore: Arc<Semaphore>,
}

impl JobManager {
    pub fn new(database: LibraryDatabase, events: EventBus, ffmpeg_path: PathBuf) -> Self {
        Self {
            database,
            events,
            ffmpeg_path,
            semaphore: Arc::new(Semaphore::new(1)),
        }
    }

    pub async fn enqueue_conversion(&self, request: ConversionJobRequest) -> Result<JobRecord> {
        let (job, output_path) = self.prepare_conversion(&request)?;
        let manager = self.clone();
        let mut running_job = job.clone();
        tokio::spawn(async move {
            let Ok(_permit) = manager.semaphore.clone().acquire_owned().await else {
                return;
            };
            if let Err(error) = manager
                .run_prepared_conversion(request, output_path, &mut running_job)
                .await
            {
                manager.events.emit(AuraluxEvent::Error {
                    message: error.to_string(),
                });
            }
        });
        Ok(job)
    }

    pub async fn run_conversion_now(&self, request: ConversionJobRequest) -> Result<JobRecord> {
        let (mut job, output_path) = self.prepare_conversion(&request)?;
        let _permit = self.semaphore.clone().acquire_owned().await?;
        self.run_prepared_conversion(request, output_path, &mut job)
            .await?;
        Ok(job)
    }

    fn prepare_conversion(&self, request: &ConversionJobRequest) -> Result<(JobRecord, PathBuf)> {
        validate_safe_path(&request.source_path)?;
        std::fs::create_dir_all(&request.output_dir)?;
        let output_path = build_output_path(
            &request.source_path,
            &request.output_dir,
            request.preset.format.extension(),
        );
        if output_path.exists() && !request.overwrite {
            return Err(anyhow!("output already exists: {}", output_path.display()));
        }

        let job = JobRecord {
            id: Uuid::new_v4().to_string(),
            kind: "conversion".into(),
            state: "queued".into(),
            source_path: Some(request.source_path.clone()),
            output_path: Some(output_path.clone()),
            progress: 0.0,
            message: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.database.upsert_job(&job)?;
        self.events
            .emit(AuraluxEvent::JobProgress { job: job.clone() });
        Ok((job, output_path))
    }

    async fn run_prepared_conversion(
        &self,
        request: ConversionJobRequest,
        output_path: PathBuf,
        job: &mut JobRecord,
    ) -> Result<()> {
        job.state = "running".into();
        job.updated_at = Utc::now();
        self.database.upsert_job(job)?;
        self.events
            .emit(AuraluxEvent::JobProgress { job: job.clone() });

        let duration_ms = self
            .database
            .get_track_by_path(&request.source_path)
            .ok()
            .flatten()
            .and_then(|track| track.duration_ms);
        let result = self
            .run_conversion(&request, &output_path, job, duration_ms)
            .await;
        let final_result = match result {
            Ok(()) => {
                job.state = "finished".into();
                job.progress = 1.0;
                job.message = Some("finished".into());
                Ok(())
            }
            Err(error) => {
                let message = error.to_string();
                job.state = "failed".into();
                job.message = Some(message.clone());
                Err(anyhow!(message))
            }
        };
        job.updated_at = Utc::now();
        self.database.upsert_job(job)?;
        self.events
            .emit(AuraluxEvent::JobProgress { job: job.clone() });
        final_result
    }

    async fn run_conversion(
        &self,
        request: &ConversionJobRequest,
        output_path: &Path,
        job: &mut JobRecord,
        duration_ms: Option<i64>,
    ) -> Result<()> {
        let args = ffmpeg_args(request, output_path);
        let mut child = Command::new(&self.ffmpeg_path)
            .args(args)
            .stderr(Stdio::piped())
            .stdout(Stdio::null())
            .spawn()
            .with_context(|| format!("spawning ffmpeg at {}", self.ffmpeg_path.display()))?;

        if let Some(stderr) = child.stderr.take() {
            let mut lines = BufReader::new(stderr).lines();
            while let Some(line) = lines.next_line().await? {
                if let Some(progress) = parse_ffmpeg_progress(&line, duration_ms) {
                    job.progress = progress;
                    job.updated_at = Utc::now();
                    self.database.upsert_job(job)?;
                    self.events
                        .emit(AuraluxEvent::JobProgress { job: job.clone() });
                }
            }
        }

        let status = child.wait().await?;
        if status.success() {
            Ok(())
        } else {
            Err(anyhow!("ffmpeg exited with {status}"))
        }
    }
}

pub fn ffmpeg_args(request: &ConversionJobRequest, output_path: &Path) -> Vec<String> {
    let mut args = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        request.source_path.display().to_string(),
        "-vn".into(),
    ];
    match request.preset.format {
        ConversionFormat::Flac => args.extend(["-c:a".into(), "flac".into()]),
        ConversionFormat::Opus => args.extend([
            "-c:a".into(),
            "libopus".into(),
            "-b:a".into(),
            request
                .preset
                .quality
                .clone()
                .unwrap_or_else(|| "160k".into()),
        ]),
        ConversionFormat::Mp3 => args.extend([
            "-c:a".into(),
            "libmp3lame".into(),
            "-q:a".into(),
            request.preset.quality.clone().unwrap_or_else(|| "2".into()),
        ]),
        ConversionFormat::Aac => args.extend([
            "-c:a".into(),
            "aac".into(),
            "-b:a".into(),
            request
                .preset
                .quality
                .clone()
                .unwrap_or_else(|| "256k".into()),
        ]),
        ConversionFormat::Alac => args.extend(["-c:a".into(), "alac".into()]),
        ConversionFormat::Wav => args.extend(["-c:a".into(), "pcm_s16le".into()]),
    }
    args.extend([
        "-progress".into(),
        "pipe:2".into(),
        output_path.display().to_string(),
    ]);
    args
}

pub fn parse_ffmpeg_progress(line: &str, duration_ms: Option<i64>) -> Option<f64> {
    if let Some(value) = line.strip_prefix("progress=") {
        return (value == "end").then_some(1.0);
    }
    if let Some(value) = line.strip_prefix("out_time_ms=") {
        let micros = value.parse::<f64>().ok()?;
        let duration = duration_ms? as f64;
        if duration <= 0.0 {
            return None;
        }
        let elapsed_ms = micros / 1000.0;
        return Some((elapsed_ms / duration).clamp(0.0, 0.99));
    }
    None
}

pub fn sanitize_file_name(input: &str) -> String {
    let re = Regex::new(r#"[<>:"/\\|?*\x00-\x1f]+"#).expect("valid filename regex");
    let sanitized = re.replace_all(input.trim(), "_");
    let collapsed = sanitized
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_matches('.')
        .to_string();
    if collapsed.is_empty() {
        "untitled".into()
    } else {
        collapsed
    }
}

fn build_output_path(source: &Path, output_dir: &Path, extension: &str) -> PathBuf {
    let stem = source
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("track");
    output_dir.join(format!("{}.{}", sanitize_file_name(stem), extension))
}

fn validate_safe_path(path: &Path) -> Result<()> {
    if path.as_os_str().is_empty() {
        return Err(anyhow!("empty path"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_output_names() {
        assert_eq!(sanitize_file_name("a/b:c*"), "a_b_c_");
        assert_eq!(sanitize_file_name("..."), "untitled");
    }

    #[test]
    fn builds_mp3_args() {
        let request = ConversionJobRequest {
            source_path: "in.flac".into(),
            output_dir: ".".into(),
            preset: ConversionPreset {
                format: ConversionFormat::Mp3,
                quality: None,
            },
            overwrite: true,
        };
        let args = ffmpeg_args(&request, Path::new("out.mp3"));
        assert!(args
            .windows(2)
            .any(|w| w[0] == "-c:a" && w[1] == "libmp3lame"));
    }

    #[test]
    fn parses_progress_with_duration() {
        assert_eq!(
            parse_ffmpeg_progress("progress=end", Some(10_000)),
            Some(1.0)
        );
        assert_eq!(
            parse_ffmpeg_progress("out_time_ms=5000000", Some(10_000)),
            Some(0.5)
        );
        assert_eq!(parse_ffmpeg_progress("out_time_ms=5000000", None), None);
    }
}
