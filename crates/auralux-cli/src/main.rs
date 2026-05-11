use anyhow::{anyhow, Result};
use auralux_core::capabilities::detect_capabilities;
use auralux_core::conversion::{
    ConversionFormat, ConversionJobRequest, ConversionPreset, JobManager,
};
use auralux_core::events::EventBus;
use auralux_core::metadata::MetadataReader;
use auralux_core::scanner::{LibraryScanner, ScanRequest};
use auralux_core::settings::AuraluxPaths;
use auralux_core::types::TrackQuery;
use auralux_core::LibraryDatabase;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;
use std::process::Command;

#[derive(Debug, Parser)]
#[command(
    name = "auralux",
    version,
    about = "Lightweight music library, playback, and conversion CLI."
)]
struct Cli {
    #[arg(long, env = "AURALUX_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "AURALUX_FFMPEG")]
    ffmpeg: Option<PathBuf>,
    #[arg(long, env = "AURALUX_FFPROBE")]
    ffprobe: Option<PathBuf>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    Serve {
        #[arg(long, default_value = "127.0.0.1:4147")]
        bind: String,
    },
    Scan {
        roots: Vec<PathBuf>,
        #[arg(long)]
        force: bool,
    },
    Search {
        query: String,
        #[arg(long, default_value_t = 25)]
        limit: usize,
    },
    Play {
        path: PathBuf,
    },
    Pause,
    Queue,
    Convert {
        source: PathBuf,
        output_dir: PathBuf,
        #[arg(long, value_enum)]
        format: CliFormat,
        #[arg(long)]
        quality: Option<String>,
        #[arg(long)]
        overwrite: bool,
    },
    Jobs,
    Config,
}

#[derive(Debug, Clone, ValueEnum)]
enum CliFormat {
    Flac,
    Opus,
    Mp3,
    Aac,
    Alac,
    Wav,
}

impl From<CliFormat> for ConversionFormat {
    fn from(value: CliFormat) -> Self {
        match value {
            CliFormat::Flac => Self::Flac,
            CliFormat::Opus => Self::Opus,
            CliFormat::Mp3 => Self::Mp3,
            CliFormat::Aac => Self::Aac,
            CliFormat::Alac => Self::Alac,
            CliFormat::Wav => Self::Wav,
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Serve { bind } => {
            let daemon = which::which("auraluxd")
                .or_else(|_| std::env::current_exe().map(|exe| exe.with_file_name("auraluxd")))?;
            let status = Command::new(daemon).arg("--bind").arg(bind).status();
            match status {
                Ok(status) if status.success() => Ok(()),
                Ok(status) => Err(anyhow!("auraluxd exited with {status}")),
                Err(error) => Err(anyhow!("failed to launch auraluxd: {error}")),
            }
        }
        command => {
            let paths = AuraluxPaths::resolve(cli.data_dir)?;
            let database = LibraryDatabase::open(&paths.database_path)?;
            let events = EventBus::new(64);
            let capabilities = detect_capabilities(cli.ffmpeg, cli.ffprobe, None).await;
            match command {
                Commands::Scan { roots, force } => {
                    if roots.is_empty() {
                        return Err(anyhow!("provide at least one scan root"));
                    }
                    let scanner = LibraryScanner::new(
                        database.clone(),
                        MetadataReader::new(capabilities.ffprobe.path),
                        events,
                    );
                    let summary = scanner.scan(ScanRequest { roots, force }).await?;
                    println!("{}", serde_json::to_string_pretty(&summary)?);
                }
                Commands::Search { query, limit } => {
                    let tracks = database.list_tracks(TrackQuery {
                        search: Some(query),
                        limit,
                        offset: 0,
                    })?;
                    println!("{}", serde_json::to_string_pretty(&tracks)?);
                }
                Commands::Play { path } => {
                    println!(
                        "Playback is handled by auraluxd/mpv. Start `auralux serve`, then load: {}",
                        path.display()
                    );
                }
                Commands::Pause => {
                    println!(
                        "Pause is available through the daemon API: POST /api/playback/command"
                    );
                }
                Commands::Queue => {
                    println!("Queue persistence is reserved for the next playback iteration.");
                }
                Commands::Convert {
                    source,
                    output_dir,
                    format,
                    quality,
                    overwrite,
                } => {
                    let ffmpeg = capabilities.ffmpeg.path.ok_or_else(|| {
                        anyhow!("ffmpeg not found; install ffmpeg or set AURALUX_FFMPEG")
                    })?;
                    let manager = JobManager::new(database, events, ffmpeg);
                    let job = manager
                        .run_conversion_now(ConversionJobRequest {
                            source_path: source,
                            output_dir,
                            preset: ConversionPreset {
                                format: format.into(),
                                quality,
                            },
                            overwrite,
                        })
                        .await?;
                    println!("{}", serde_json::to_string_pretty(&job)?);
                }
                Commands::Jobs => {
                    let jobs = database.list_jobs(50)?;
                    println!("{}", serde_json::to_string_pretty(&jobs)?);
                }
                Commands::Config => {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "database_path": paths.database_path,
                            "capabilities": capabilities
                        }))?
                    );
                }
                Commands::Serve { .. } => unreachable!(),
            }
            Ok(())
        }
    }
}
