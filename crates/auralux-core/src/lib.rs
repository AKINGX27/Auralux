pub mod capabilities;
pub mod conversion;
pub mod db;
pub mod events;
pub mod metadata;
pub mod playback;
pub mod scanner;
pub mod settings;
pub mod types;

pub use capabilities::{detect_capabilities, CodecCapabilities, ToolInfo};
pub use conversion::{ConversionFormat, ConversionJobRequest, ConversionPreset, JobManager};
pub use db::LibraryDatabase;
pub use events::{AuraluxEvent, EventBus};
pub use playback::{PlaybackBackend, PlaybackCommand, PlaybackState};
pub use scanner::{LibraryScanner, ScanRequest, ScanSummary};
