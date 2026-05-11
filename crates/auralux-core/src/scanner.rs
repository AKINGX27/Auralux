use crate::db::LibraryDatabase;
use crate::events::{AuraluxEvent, EventBus};
use crate::metadata::MetadataReader;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ScanRequest {
    pub roots: Vec<PathBuf>,
    pub force: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ScanSummary {
    pub scanned: usize,
    pub imported: usize,
    pub skipped: usize,
    pub errors: usize,
}

pub struct LibraryScanner {
    database: LibraryDatabase,
    metadata: MetadataReader,
    events: EventBus,
    extensions: HashSet<String>,
}

impl LibraryScanner {
    pub fn new(database: LibraryDatabase, metadata: MetadataReader, events: EventBus) -> Self {
        Self {
            database,
            metadata,
            events,
            extensions: default_audio_extensions(),
        }
    }

    pub async fn scan(&self, request: ScanRequest) -> Result<ScanSummary> {
        let mut summary = ScanSummary::default();
        for root in request.roots {
            let canonical = root
                .canonicalize()
                .with_context(|| format!("scan root does not exist: {}", root.display()))?;
            for entry in WalkDir::new(&canonical).follow_links(false).into_iter() {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => {
                        summary.errors += 1;
                        self.events.emit(AuraluxEvent::Error {
                            message: error.to_string(),
                        });
                        continue;
                    }
                };
                if !entry.file_type().is_file() || !is_supported(entry.path(), &self.extensions) {
                    continue;
                }
                summary.scanned += 1;
                self.events.emit(AuraluxEvent::LibraryScanProgress {
                    scanned: summary.scanned,
                    imported: summary.imported,
                    skipped: summary.skipped,
                    current_path: Some(entry.path().display().to_string()),
                });

                if !request.force && self.is_unchanged(entry.path())? {
                    summary.skipped += 1;
                    continue;
                }

                match self.metadata.read(entry.path()).await {
                    Ok(track) => {
                        self.database.upsert_track(&track)?;
                        summary.imported += 1;
                    }
                    Err(error) => {
                        summary.errors += 1;
                        self.events.emit(AuraluxEvent::Error {
                            message: format!("{}: {error}", entry.path().display()),
                        });
                    }
                }
            }
        }
        self.events.emit(AuraluxEvent::LibraryUpdated);
        Ok(summary)
    }

    pub async fn import_file(&self, path: &Path) -> Result<i64> {
        let canonical = path
            .canonicalize()
            .with_context(|| format!("audio file does not exist: {}", path.display()))?;
        anyhow::ensure!(
            canonical.is_file(),
            "audio import path is not a file: {}",
            canonical.display()
        );
        anyhow::ensure!(
            is_supported(&canonical, &self.extensions),
            "unsupported audio file extension: {}",
            canonical.display()
        );
        let track = self.metadata.read(&canonical).await?;
        let track_id = self.database.upsert_track(&track)?;
        self.events.emit(AuraluxEvent::LibraryUpdated);
        Ok(track_id)
    }

    fn is_unchanged(&self, path: &Path) -> Result<bool> {
        let metadata = std::fs::metadata(path)?;
        let size = metadata.len() as i64;
        let mtime = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs() as i64)
            .unwrap_or_default();
        Ok(self.database.track_signature(path)? == Some((size, mtime)))
    }
}

fn is_supported(path: &Path, extensions: &HashSet<String>) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| extensions.contains(&ext.to_ascii_lowercase()))
        .unwrap_or(false)
}

fn default_audio_extensions() -> HashSet<String> {
    [
        "aac", "aif", "aiff", "alac", "ape", "caf", "cue", "dff", "dsf", "flac", "m4a", "mka",
        "mp2", "mp3", "mp4", "mpc", "oga", "ogg", "opus", "tak", "tta", "wav", "weba", "wma", "wv",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
