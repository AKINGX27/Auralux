use anyhow::Result;
use auraluxd::DaemonConfig;
use clap::Parser;
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
struct Args {
    #[arg(long, env = "AURALUX_BIND", default_value = "127.0.0.1:4147")]
    bind: SocketAddr,
    #[arg(long, env = "AURALUX_DATA_DIR")]
    data_dir: Option<PathBuf>,
    #[arg(long, env = "AURALUX_FFMPEG")]
    ffmpeg: Option<PathBuf>,
    #[arg(long, env = "AURALUX_FFPROBE")]
    ffprobe: Option<PathBuf>,
    #[arg(long, env = "AURALUX_MPV")]
    mpv: Option<PathBuf>,
    #[arg(long, env = "AURALUX_GUI_DIST", default_value = "apps/gui/dist")]
    gui_dist: PathBuf,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    auraluxd::run(DaemonConfig {
        bind: args.bind,
        data_dir: args.data_dir,
        ffmpeg: args.ffmpeg,
        ffprobe: args.ffprobe,
        mpv: args.mpv,
        gui_dist: args.gui_dist,
    })
    .await
}
