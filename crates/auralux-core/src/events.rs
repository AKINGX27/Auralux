use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuraluxEvent {
    LibraryScanProgress {
        scanned: usize,
        imported: usize,
        skipped: usize,
        current_path: Option<String>,
    },
    LibraryUpdated,
    PlaylistUpdated {
        playlist_id: i64,
    },
    PlaybackState {
        state: crate::playback::PlaybackState,
    },
    JobProgress {
        job: crate::types::JobRecord,
    },
    Error {
        message: String,
    },
}

#[derive(Clone)]
pub struct EventBus {
    sender: broadcast::Sender<AuraluxEvent>,
}

impl EventBus {
    pub fn new(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AuraluxEvent> {
        self.sender.subscribe()
    }

    pub fn emit(&self, event: AuraluxEvent) {
        let _ = self.sender.send(event);
    }
}
