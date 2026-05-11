use std::{
    collections::VecDeque,
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Mutex,
};

#[cfg(target_os = "android")]
use std::{
    net::{IpAddr, Ipv4Addr, SocketAddr},
    sync::OnceLock,
};

#[cfg(target_os = "android")]
use auraluxd::DaemonConfig;
use tauri::{AppHandle, Emitter, Manager, Runtime};
use url::Url;

#[derive(Default)]
struct PendingOpenFiles(Mutex<VecDeque<PathBuf>>);

#[tauri::command]
fn platform() -> &'static str {
    if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "linux") {
        "linux"
    } else {
        "unknown"
    }
}

#[tauri::command]
fn take_open_files(app: AppHandle) -> Vec<String> {
    drain_pending_open_files(&app)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    with_single_instance(tauri::Builder::default().plugin(tauri_plugin_opener::init()))
        .invoke_handler(tauri::generate_handler![platform, take_open_files])
        .setup(|app| {
            app.manage(PendingOpenFiles::default());
            start_embedded_daemon(app.handle().clone());
            queue_open_files(
                app.handle(),
                paths_from_cli_args(app.env().args_os.into_iter(), None),
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Auralux Tauri application");
}

fn start_embedded_daemon<R: Runtime>(app: AppHandle<R>) {
    #[cfg(target_os = "android")]
    {
        static STARTED: OnceLock<()> = OnceLock::new();
        if STARTED.set(()).is_err() {
            return;
        }

        let data_dir = app
            .path()
            .app_data_dir()
            .or_else(|_| std::env::current_dir().map(|dir| dir.join(".auralux-tauri")))
            .ok();

        tauri::async_runtime::spawn(async move {
            let config = DaemonConfig {
                bind: SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 4147),
                data_dir,
                ffmpeg: None,
                ffprobe: None,
                mpv: None,
                gui_dist: PathBuf::from("apps/gui/dist"),
            };
            if let Err(error) = auraluxd::run(config).await {
                eprintln!("failed to start embedded Auralux daemon: {error}");
            }
        });
    }

    #[cfg(not(target_os = "android"))]
    let _ = app;
}

#[cfg(target_os = "windows")]
fn with_single_instance<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder.plugin(tauri_plugin_single_instance::init::<R, _>(
        |app, args, cwd| {
            queue_open_files(
                app,
                paths_from_cli_args(args.into_iter().map(OsString::from), Some(cwd)),
            );
        },
    ))
}

#[cfg(not(target_os = "windows"))]
fn with_single_instance<R: tauri::Runtime>(builder: tauri::Builder<R>) -> tauri::Builder<R> {
    builder
}

fn queue_open_files<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    let paths = unique_supported_paths(paths);
    if paths.is_empty() {
        return;
    }

    if let Some(pending) = app.try_state::<PendingOpenFiles>() {
        if let Ok(mut queue) = pending.0.lock() {
            queue.extend(paths);
        }
    }

    let _ = app.emit("auralux:open-files", ());
}

fn drain_pending_open_files<R: Runtime>(app: &AppHandle<R>) -> Vec<String> {
    app.try_state::<PendingOpenFiles>()
        .and_then(|pending| {
            pending
                .0
                .lock()
                .ok()
                .map(|mut queue| queue.drain(..).collect::<Vec<_>>())
        })
        .unwrap_or_default()
        .into_iter()
        .map(|path: PathBuf| path.to_string_lossy().into_owned())
        .collect()
}

fn paths_from_cli_args<I>(args: I, cwd: Option<String>) -> Vec<PathBuf>
where
    I: IntoIterator<Item = OsString>,
{
    let cwd = cwd.and_then(|value| {
        if value.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(value))
        }
    });

    args.into_iter()
        .skip(1)
        .filter_map(|arg| path_from_arg(arg, cwd.as_deref()))
        .collect()
}

fn path_from_arg(arg: OsString, cwd: Option<&Path>) -> Option<PathBuf> {
    let arg_path = PathBuf::from(&arg);
    if arg_path.is_absolute() {
        return Some(arg_path);
    }

    if let Some(path) = arg.to_str().and_then(path_from_url_arg) {
        return Some(path);
    }

    cwd.map(|base| base.join(arg_path))
}

fn path_from_url_arg(arg: &str) -> Option<PathBuf> {
    Url::parse(arg).ok().and_then(|url| {
        if url.scheme() == "file" {
            url.to_file_path().ok()
        } else {
            None
        }
    })
}

fn unique_supported_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut unique = Vec::new();
    for path in paths {
        if is_supported_audio(&path) && !unique.iter().any(|seen| seen == &path) {
            unique.push(path);
        }
    }
    unique
}

fn is_supported_audio(path: &Path) -> bool {
    let ext = match path.extension().and_then(|value| value.to_str()) {
        Some(ext) => ext.to_ascii_lowercase(),
        None => return false,
    };
    matches!(
        ext.as_str(),
        "aac"
            | "aif"
            | "aiff"
            | "alac"
            | "ape"
            | "caf"
            | "dff"
            | "dsf"
            | "flac"
            | "m4a"
            | "mka"
            | "mp2"
            | "mp3"
            | "mp4"
            | "mpc"
            | "oga"
            | "ogg"
            | "opus"
            | "tak"
            | "tta"
            | "wav"
            | "weba"
            | "wma"
            | "wv"
    )
}
