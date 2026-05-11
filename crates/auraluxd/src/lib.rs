use anyhow::Result;
use auralux_core::capabilities::detect_capabilities;
use auralux_core::conversion::sanitize_file_name;
use auralux_core::conversion::{ConversionJobRequest, JobManager};
use auralux_core::events::{AuraluxEvent, EventBus};
use auralux_core::metadata::MetadataReader;
use auralux_core::playback::{LoadRequest, PlaybackBackend, PlaybackCommand};
use auralux_core::scanner::{LibraryScanner, ScanRequest};
use auralux_core::settings::AuraluxPaths;
use auralux_core::types::{Health, Playlist, PlaylistDetail, TrackQuery};
use auralux_core::LibraryDatabase;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::DefaultBodyLimit;
use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use std::net::SocketAddr;
use std::path::{Path as FsPath, PathBuf};
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::Mutex;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

#[derive(Debug, Clone)]
pub struct DaemonConfig {
    pub bind: SocketAddr,
    pub data_dir: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub mpv: Option<PathBuf>,
    pub gui_dist: PathBuf,
}

#[derive(Clone)]
struct AppState {
    database: LibraryDatabase,
    events: EventBus,
    scanner: Arc<LibraryScanner>,
    jobs: Option<JobManager>,
    playback: Option<Arc<Mutex<PlaybackBackend>>>,
    health: Health,
    import_dir: PathBuf,
}

pub async fn run(config: DaemonConfig) -> Result<()> {
    let paths = AuraluxPaths::resolve(config.data_dir)?;
    let database = LibraryDatabase::open(&paths.database_path)?;
    let events = EventBus::new(256);
    let capabilities = detect_capabilities(
        config.ffmpeg.clone(),
        config.ffprobe.clone(),
        config.mpv.clone(),
    )
    .await;
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
    let import_dir = paths.data_dir.join("imports");
    tokio::fs::create_dir_all(&import_dir).await?;
    let state = AppState {
        database,
        events,
        scanner,
        jobs,
        playback,
        health: health_snapshot,
        import_dir,
    };

    let api = Router::new()
        .route("/health", get(health))
        .route("/events", get(events_ws))
        .route("/library/scan", post(scan_library))
        .route("/library/tracks", get(list_tracks))
        .route("/playlists", get(list_playlists).post(create_playlist))
        .route("/playlists/:id", get(get_playlist))
        .route("/playlists/:id/tracks", post(add_playlist_track))
        .route(
            "/playlists/:id/import",
            post(import_playlist_tracks).layer(DefaultBodyLimit::max(1024 * 1024 * 1024)),
        )
        .route("/playlists/:id/import-paths", post(import_playlist_paths))
        .route("/playback/state", get(playback_state))
        .route("/playback/load", post(playback_load))
        .route("/playback/command", post(playback_command))
        .route("/conversions", post(create_conversion))
        .route("/jobs", get(list_jobs))
        .route("/jobs/:id", get(get_job));

    let spa = ServeDir::new(&config.gui_dist)
        .not_found_service(ServeFile::new(config.gui_dist.join("index.html")));

    let app = Router::new()
        .nest("/api", api)
        .fallback_service(spa)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = TcpListener::bind(config.bind).await?;
    tracing::info!("Auralux daemon listening on http://{}", config.bind);
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

#[derive(Debug, Deserialize)]
struct CreatePlaylistRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
struct AddPlaylistTrackRequest {
    track_id: i64,
}

#[derive(Debug, Deserialize)]
struct ImportPlaylistPathsRequest {
    paths: Vec<PathBuf>,
}

async fn list_playlists(State(state): State<AppState>) -> Result<Json<Vec<Playlist>>, ApiError> {
    Ok(Json(state.database.list_playlists()?))
}

async fn create_playlist(
    State(state): State<AppState>,
    Json(request): Json<CreatePlaylistRequest>,
) -> Result<Json<Playlist>, ApiError> {
    let playlist = state.database.ensure_playlist(&request.name)?;
    state.events.emit(AuraluxEvent::PlaylistUpdated {
        playlist_id: playlist.id,
    });
    Ok(Json(playlist))
}

async fn get_playlist(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<Json<PlaylistDetail>, ApiError> {
    let detail = state
        .database
        .get_playlist_detail(id)?
        .ok_or(ApiError::NotFound("playlist not found"))?;
    Ok(Json(detail))
}

async fn add_playlist_track(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<AddPlaylistTrackRequest>,
) -> Result<Json<PlaylistDetail>, ApiError> {
    let detail = state.database.add_track_to_playlist(id, request.track_id)?;
    state
        .events
        .emit(AuraluxEvent::PlaylistUpdated { playlist_id: id });
    Ok(Json(detail))
}

async fn import_playlist_tracks(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    mut multipart: Multipart,
) -> Result<Json<PlaylistDetail>, ApiError> {
    state
        .database
        .get_playlist_detail(id)?
        .ok_or(ApiError::NotFound("playlist not found"))?;

    let mut imported = 0usize;
    while let Some(field) = multipart.next_field().await? {
        if field.name() != Some("files") {
            continue;
        }
        let file_name = field
            .file_name()
            .map(str::to_string)
            .unwrap_or_else(|| "track".into());
        let bytes = field.bytes().await?;
        let import_path = write_import_file(&state.import_dir, &file_name, bytes).await?;
        let track_id = state.scanner.import_file(&import_path).await?;
        state.database.add_track_to_playlist(id, track_id)?;
        imported += 1;
    }

    if imported == 0 {
        return Err(anyhow::anyhow!("no audio files were imported").into());
    }
    let detail = state
        .database
        .get_playlist_detail(id)?
        .ok_or(ApiError::NotFound("playlist not found"))?;
    state
        .events
        .emit(AuraluxEvent::PlaylistUpdated { playlist_id: id });
    Ok(Json(detail))
}

async fn import_playlist_paths(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(request): Json<ImportPlaylistPathsRequest>,
) -> Result<Json<PlaylistDetail>, ApiError> {
    state
        .database
        .get_playlist_detail(id)?
        .ok_or(ApiError::NotFound("playlist not found"))?;

    let mut imported = 0usize;
    for path in request.paths {
        let track_id = state.scanner.import_file(&path).await?;
        state.database.add_track_to_playlist(id, track_id)?;
        imported += 1;
    }

    if imported == 0 {
        return Err(anyhow::anyhow!("no audio files were imported").into());
    }

    let detail = state
        .database
        .get_playlist_detail(id)?
        .ok_or(ApiError::NotFound("playlist not found"))?;
    state
        .events
        .emit(AuraluxEvent::PlaylistUpdated { playlist_id: id });
    Ok(Json(detail))
}

async fn write_import_file(
    import_dir: &FsPath,
    file_name: &str,
    bytes: Bytes,
) -> anyhow::Result<PathBuf> {
    let sanitized = sanitize_upload_file_name(file_name);
    tokio::fs::create_dir_all(import_dir).await?;
    let mut candidate = import_dir.join(&sanitized);
    if candidate.exists() {
        let stem = FsPath::new(&sanitized)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or("track");
        let extension = FsPath::new(&sanitized)
            .extension()
            .and_then(|value| value.to_str())
            .map(|value| format!(".{value}"))
            .unwrap_or_default();
        let mut suffix = 1usize;
        while candidate.exists() {
            candidate = import_dir.join(format!("{stem}-{suffix}{extension}"));
            suffix += 1;
        }
    }
    tokio::fs::write(&candidate, bytes).await?;
    Ok(candidate)
}

fn sanitize_upload_file_name(file_name: &str) -> String {
    let safe = sanitize_file_name(
        FsPath::new(file_name)
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("track"),
    );
    if safe == "untitled" {
        "track".into()
    } else {
        safe
    }
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

impl From<axum::extract::multipart::MultipartError> for ApiError {
    fn from(error: axum::extract::multipart::MultipartError) -> Self {
        Self::Anyhow(error.into())
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
