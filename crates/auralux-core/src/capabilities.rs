use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolInfo {
    pub name: String,
    pub path: Option<PathBuf>,
    pub version: Option<String>,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodecCapabilities {
    pub ffmpeg: ToolInfo,
    pub ffprobe: ToolInfo,
    pub mpv: ToolInfo,
    pub decoders: Vec<String>,
    pub encoders: Vec<String>,
    pub muxers: Vec<String>,
    pub demuxers: Vec<String>,
    pub android_codec_pack: String,
}

impl CodecCapabilities {
    pub fn empty() -> Self {
        Self {
            ffmpeg: missing("ffmpeg"),
            ffprobe: missing("ffprobe"),
            mpv: missing("mpv"),
            decoders: Vec::new(),
            encoders: Vec::new(),
            muxers: Vec::new(),
            demuxers: Vec::new(),
            android_codec_pack: "not_applicable".into(),
        }
    }
}

pub async fn detect_capabilities(
    ffmpeg_override: Option<PathBuf>,
    ffprobe_override: Option<PathBuf>,
    mpv_override: Option<PathBuf>,
) -> CodecCapabilities {
    let ffmpeg = detect_tool("ffmpeg", ffmpeg_override).await;
    let ffprobe = detect_tool("ffprobe", ffprobe_override).await;
    let mpv = detect_tool("mpv", mpv_override).await;
    let mut capabilities = CodecCapabilities {
        ffmpeg: ffmpeg.clone(),
        ffprobe,
        mpv,
        decoders: Vec::new(),
        encoders: Vec::new(),
        muxers: Vec::new(),
        demuxers: Vec::new(),
        android_codec_pack: if cfg!(target_os = "android") {
            "bundled_required".into()
        } else {
            "not_applicable".into()
        },
    };

    if let Some(path) = ffmpeg.path.as_ref() {
        capabilities.decoders = ffmpeg_list(path, "-decoders").await;
        capabilities.encoders = ffmpeg_list(path, "-encoders").await;
        capabilities.muxers = ffmpeg_list(path, "-muxers").await;
        capabilities.demuxers = ffmpeg_list(path, "-demuxers").await;
    }

    capabilities
}

async fn detect_tool(name: &str, override_path: Option<PathBuf>) -> ToolInfo {
    let path = override_path.or_else(|| which::which(name).ok());
    let Some(path) = path else {
        return missing(name);
    };

    let output = Command::new(&path).arg("-version").output().await;
    let version = output.ok().and_then(|output| {
        let text = String::from_utf8_lossy(&output.stdout);
        text.lines().next().map(|line| line.trim().to_string())
    });

    ToolInfo {
        name: name.into(),
        path: Some(path),
        version,
        available: true,
    }
}

async fn ffmpeg_list(path: &PathBuf, arg: &str) -> Vec<String> {
    let output = Command::new(path).arg(arg).output().await;
    let Ok(output) = output else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout);
    text.lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if trimmed.len() < 8 || trimmed.starts_with('-') {
                return None;
            }
            let mut parts = trimmed.split_whitespace();
            let flags = parts.next()?;
            let name = parts.next()?;
            if flags.chars().any(|c| c.is_ascii_alphabetic()) {
                Some(name.to_string())
            } else {
                None
            }
        })
        .collect()
}

fn missing(name: &str) -> ToolInfo {
    ToolInfo {
        name: name.into(),
        path: None,
        version: None,
        available: false,
    }
}
