use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, ChildStdin, Command};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlaybackState {
    pub loaded_path: Option<PathBuf>,
    pub playing: bool,
    pub position_seconds: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum PlaybackCommand {
    Play,
    Pause,
    Toggle,
    Stop,
    Seek { seconds: f64 },
    Volume { value: f64 },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct LoadRequest {
    pub path: PathBuf,
    pub play: bool,
}

pub struct PlaybackBackend {
    mpv_path: PathBuf,
    child: Option<Child>,
    stdin: Option<ChildStdin>,
    state: PlaybackState,
}

impl PlaybackBackend {
    pub fn new(mpv_path: PathBuf) -> Self {
        Self {
            mpv_path,
            child: None,
            stdin: None,
            state: PlaybackState {
                volume: 100.0,
                ..PlaybackState::default()
            },
        }
    }

    pub fn state(&self) -> PlaybackState {
        self.state.clone()
    }

    pub async fn load(&mut self, request: LoadRequest) -> Result<PlaybackState> {
        self.ensure_mpv().await?;
        self.send_line(&format!(
            "loadfile \"{}\" replace",
            escape_mpv_string(&request.path.display().to_string())
        ))
        .await?;
        if !request.play {
            self.send_line("set pause yes").await?;
        }
        self.state.loaded_path = Some(request.path);
        self.state.playing = request.play;
        Ok(self.state())
    }

    pub async fn command(&mut self, command: PlaybackCommand) -> Result<PlaybackState> {
        self.ensure_mpv().await?;
        match command {
            PlaybackCommand::Play => {
                self.send_line("set pause no").await?;
                self.state.playing = true;
            }
            PlaybackCommand::Pause => {
                self.send_line("set pause yes").await?;
                self.state.playing = false;
            }
            PlaybackCommand::Toggle => {
                self.send_line("cycle pause").await?;
                self.state.playing = !self.state.playing;
            }
            PlaybackCommand::Stop => {
                self.send_line("stop").await?;
                self.state.playing = false;
            }
            PlaybackCommand::Seek { seconds } => {
                self.send_line(&format!("seek {seconds} absolute")).await?;
                self.state.position_seconds = seconds;
            }
            PlaybackCommand::Volume { value } => {
                let clamped = value.clamp(0.0, 150.0);
                self.send_line(&format!("set volume {clamped}")).await?;
                self.state.volume = clamped;
            }
        }
        Ok(self.state())
    }

    async fn ensure_mpv(&mut self) -> Result<()> {
        if self.stdin.is_some() {
            return Ok(());
        }
        let mut child = Command::new(&self.mpv_path)
            .args([
                "--idle=yes",
                "--force-window=no",
                "--terminal=no",
                "--input-terminal=yes",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        self.stdin = child.stdin.take();
        self.child = Some(child);
        Ok(())
    }

    async fn send_line(&mut self, line: &str) -> Result<()> {
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| anyhow!("mpv stdin is unavailable"))?;
        stdin.write_all(line.as_bytes()).await?;
        stdin.write_all(b"\n").await?;
        stdin.flush().await?;
        Ok(())
    }
}

fn escape_mpv_string(input: &str) -> String {
    input.replace('\\', "\\\\").replace('"', "\\\"")
}
