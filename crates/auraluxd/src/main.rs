use anyhow::Result;
use auralux_core::capabilities::detect_capabilities;
use auralux_core::conversion::{ConversionJobRequest, JobManager};
use auralux_core::events::EventBus;
use auralux_core::metadata::MetadataReader;
use auralux_core::playback::{LoadRequest, PlaybackBackend, PlaybackCommand};
use auralux_core::scanner::{LibraryScanner, ScanRequest};
use auralux_core::settings::AuraluxPaths;
use auralux_core::types::{Health, TrackQuery};
use auralux_core::LibraryDatabase;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use clap::Parser;
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};
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

#[derive(Clone)]
struct AppState {
    database: LibraryDatabase,
    events: EventBus,
    scanner: Arc<LibraryScanner>,
    jobs: Option<JobManager>,
    playback: Option<Arc<Mutex<PlaybackBackend>>>,
    health: Health,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let args = Args::parse();
    let paths = AuraluxPaths::resolve(args.data_dir)?;
    let database = LibraryDatabase::open(&paths.database_path)?;
    let events = EventBus::new(256);
    let capabilities =
        detect_capabilities(args.ffmpeg.clone(), args.ffprobe.clone(), args.mpv.clone()).await;
    let metadata = MetadataReader::new(capabilities.ffprobe.path.clone());
    let scanner = Arc::new(LibraryScanner::new(
        database.clone(),
        metadata,
        events.clone(),
    ));
    let jobs = capabilities
        .ffmpeg
        .path
        .clone()
        .map(|path| JobManager::new(database.clone(), events.clone(), path));
    let playback = capabilities
        .mpv
        .path
        .clone()
        .map(|path| Arc::new(Mutex::new(PlaybackBackend::new(path))));
    let health_snapshot = Health {
        app: "Auralux".into(),
        version: env!("CARGO_PKG_VERSION").into(),
        database_path: paths.database_path,
        capabilities,
    };
    let state = AppState {
        database,
        events,
        scanner,
        jobs,
        playback,
        health: health_snapshot,
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/events", get(events_ws))
        .route("/library/scan", post(scan_library))
        .route("/library/tracks", get(list_tracks))
        .route("/playback/state", get(playback_state))
        .route("/playback/load", post(playback_load))
        .route("/playback/command", post(playback_command))
        .route("/conversions", post(create_conversion))
        .route("/jobs", get(list_jobs))
        .route("/jobs/:id", get(get_job));

    let spa = ServeDir::new(&args.gui_dist)
        .not_found_service(ServeFile::new(args.gui_dist.join("index.html")));

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(spa)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = TcpListener::bind(args.bind).await?;
    tracing::info!("Auralux daemon listening on http://{}", args.bind);
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health(State(state): State<AppState>) -> Json<Health> {
    Json(state.health)
}

async fn events_ws(State(state): State<AppState>, ws: WebSocketUpgrade) -> impl IntoResponse {
    ws.on_upgrade(move |socket| stream_events(socket, state.events))
}

async fn stream_events(mut socket: WebSocket, events: EventBus) {
    let mut rx = events.subscribe();
    while let Ok(event) = rx.recv().await {
        if let Ok(text) = serde_json::to_string(&event) {
            if socket.send(Message::Text(text)).await.is_err() {
                break;
            }
        }
    }
}

async fn scan_library(
    State(state): State<AppState>,
    Json(request): Json<ScanRequest>,
) -> Result<Json<auralux_core::ScanSummary>, ApiError> {
    let scanner = state.scanner.clone();
    let summary = scanner.scan(request).await?;
    Ok(Json(summary))
}

#[derive(Debug, Deserialize)]
struct TracksParams {
    search: Option<String>,
    limit: Option<usize>,
    offset: Option<usize>,
}

async fn list_tracks(
    State(state): State<AppState>,
    Query(params): Query<TracksParams>,
) -> Result<Json<Vec<auralux_core::types::Track>>, ApiError> {
    let tracks = state.database.list_tracks(TrackQuery {
        search: params.search,
        limit: params.limit.unwrap_or(100),
        offset: params.offset.unwrap_or(0),
    })?;
    Ok(Json(tracks))
}

async fn playback_state(
    State(state): State<AppState>,
) -> Result<Json<auralux_core::PlaybackState>, ApiError> {
    let playback = state
        .playback
        .as_ref()
        .ok_or(ApiError::Unavailable("mpv not found"))?;
    Ok(Json(playback.lock().await.state()))
}

async fn playback_load(
    State(state): State<AppState>,
    Json(request): Json<LoadRequest>,
) -> Result<Json<auralux_core::PlaybackState>, ApiError> {
    let playback = state
        .playback
        .as_ref()
        .ok_or(ApiError::Unavailable("mpv not found"))?;
    let mut backend = playback.lock().await;
    let state = backend.load(request).await?;
    Ok(Json(state))
}

async fn playback_command(
    State(state): State<AppState>,
    Json(command): Json<PlaybackCommand>,
) -> Result<Json<auralux_core::PlaybackState>, ApiError> {
    let playback = state
        .playback
        .as_ref()
        .ok_or(ApiError::Unavailable("mpv not found"))?;
    let mut backend = playback.lock().await;
    let state = backend.command(command).await?;
    Ok(Json(state))
}

async fn create_conversion(
    State(state): State<AppState>,
    Json(request): Json<ConversionJobRequest>,
) -> Result<Json<auralux_core::types::JobRecord>, ApiError> {
    let jobs = state
        .jobs
        .as_ref()
        .ok_or(ApiError::Unavailable("ffmpeg not found"))?;
    let job = jobs.enqueue_conversion(request).await?;
    Ok(Json(job))
}

async fn list_jobs(
    State(state): State<AppState>,
) -> Result<Json<Vec<auralux_core::types::JobRecord>>, ApiError> {
    Ok(Json(state.database.list_jobs(100)?))
}

async fn get_job(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<auralux_core::types::JobRecord>, ApiError> {
    let job = state
        .database
        .get_job(&id)?
        .ok_or(ApiError::NotFound("job not found"))?;
    Ok(Json(job))
}

#[derive(Debug)]
enum ApiError {
    Anyhow(anyhow::Error),
    NotFound(&'static str),
    Unavailable(&'static str),
}

impl From<anyhow::Error> for ApiError {
    fn from(error: anyhow::Error) -> Self {
        Self::Anyhow(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            Self::Anyhow(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
            Self::NotFound(message) => (StatusCode::NOT_FOUND, message.into()),
            Self::Unavailable(message) => (StatusCode::SERVICE_UNAVAILABLE, message.into()),
        };
        (status, Json(serde_json::json!({ "error": message }))).into_response()
    }
}
