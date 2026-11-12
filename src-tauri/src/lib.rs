//! The Tauri shell.
//!
//! No logic lives here. A command deserialises its arguments, calls one
//! function in the core crate, and returns the result. A decision made in a
//! command handler is a decision that cannot be tested on Linux.
//!
//! Commands that touch the disk or the network run on a blocking thread, so
//! the window never stalls.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

use camu_core::fetch::HttpFetch;
use camu_core::{icon, launch, platform, scan, Checker, InstalledApp};
use tauri::{AppHandle, Emitter, State};

/// Shared across commands. The checker caches the Homebrew catalogue.
struct Camu {
    checker: Arc<Checker>,
}

/// Run blocking work off the main thread and flatten the join error.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
}

/// Every app in /Applications and ~/Applications. Fast: reads disk only.
#[tauri::command]
async fn list_apps() -> Result<Vec<InstalledApp>, String> {
    blocking(|| scan::scan(&scan::default_roots())).await
}

/// Check every app for updates. Each result is sent as a `camu://report`
/// event as soon as it is ready; the command returns when all are done.
#[tauri::command]
async fn check_updates(
    app: AppHandle,
    state: State<'_, Camu>,
    include_beta: Vec<String>,
) -> Result<usize, String> {
    let checker = state.checker.clone();
    blocking(move || {
        checker.refresh(platform::environment());
        let apps = scan::scan(&scan::default_roots());
        let beta: HashSet<String> = include_beta.into_iter().collect();
        checker.check_all(&apps, &beta, 8, &|report| {
            let _ = app.emit("camu://report", report);
        });
        apps.len()
    })
    .await
}

#[tauri::command]
async fn app_icon(path: PathBuf, icon_file: String) -> Result<Option<String>, String> {
    blocking(move || icon::data_url(&path, &icon_file)).await
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    launch::open_url(&url)
}

#[tauri::command]
fn open_app(path: PathBuf) -> Result<(), String> {
    launch::open_app(&path)
}

#[tauri::command]
fn reveal_app(path: PathBuf) -> Result<(), String> {
    launch::reveal(&path)
}

/// Build and run the application.
pub fn run() {
    let checker = Checker::new(Box::new(HttpFetch::new()), Default::default());
    tauri::Builder::default()
        .manage(Camu {
            checker: Arc::new(checker),
        })
        .invoke_handler(tauri::generate_handler![
            list_apps,
            check_updates,
            app_icon,
            open_url,
            open_app,
            reveal_app
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
