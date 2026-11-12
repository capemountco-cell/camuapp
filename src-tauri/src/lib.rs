//! The Tauri shell.
//!
//! No logic lives here. A command deserialises its arguments, calls one
//! function in the core crate, and returns the result. A decision made in a
//! command handler is a decision that cannot be tested on Linux.
//!
//! Commands that touch the disk or the network run on a blocking thread, so
//! the window never stalls.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use camu_core::fetch::HttpFetch;
use camu_core::update::{self, Backup, Dirs, Job, Updater};
use camu_core::{icon, launch, platform, scan, Checker, InstalledApp, Report};
use tauri::{AppHandle, Emitter, State};

/// Shared across commands.
struct Camu {
    /// Caches the Homebrew catalogue between checks.
    checker: Arc<Checker>,
    updater: Option<Arc<Updater>>,
    dirs: Option<Dirs>,
    /// The latest report per app, so updates act on what the user saw.
    reports: Arc<Mutex<HashMap<String, Report>>>,
    /// One update run at a time.
    updating: Arc<AtomicBool>,
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
    let reports = state.reports.clone();
    blocking(move || {
        checker.refresh(platform::environment());
        let apps = scan::scan(&scan::default_roots());
        let beta: HashSet<String> = include_beta.into_iter().collect();
        reports.lock().unwrap().clear();
        checker.check_all(&apps, &beta, 8, &|report| {
            reports
                .lock()
                .unwrap()
                .insert(report.id.clone(), report.clone());
            let _ = app.emit("camu://report", report);
        });
        apps.len()
    })
    .await
}

/// Update the given apps (by id). Progress arrives as `camu://update`
/// events; the command returns when the run is over.
#[tauri::command]
async fn update_apps(
    app: AppHandle,
    state: State<'_, Camu>,
    ids: Vec<String>,
) -> Result<(), String> {
    let updater = state
        .updater
        .clone()
        .ok_or("Camu can't find your home folder")?;
    if state.updating.swap(true, Ordering::SeqCst) {
        return Err("An update is already running".into());
    }
    let jobs: Vec<Job> = {
        let reports = state.reports.lock().unwrap();
        ids.iter()
            .filter_map(|id| reports.get(id))
            .filter_map(Job::from_report)
            .collect()
    };
    let updating = state.updating.clone();
    let result = blocking(move || {
        updater.run(jobs, &|event| {
            let _ = app.emit("camu://update", event);
        })
    })
    .await;
    updating.store(false, Ordering::SeqCst);
    result
}

/// Saved previous versions, newest first per app.
#[tauri::command]
async fn list_backups(state: State<'_, Camu>) -> Result<Vec<Backup>, String> {
    let dirs = state
        .dirs
        .clone()
        .ok_or("Camu can't find your home folder")?;
    blocking(move || update::list_backups(&dirs.backups)).await
}

/// Put an app's newest backup back. Returns the restored version.
#[tauri::command]
async fn roll_back(state: State<'_, Camu>, path: PathBuf) -> Result<String, String> {
    let updater = state
        .updater
        .clone()
        .ok_or("Camu can't find your home folder")?;
    blocking(move || {
        let app = InstalledApp::read(&path).ok_or("that app is no longer installed")?;
        updater.roll_back(&app)
    })
    .await?
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
    let dirs = Dirs::default_for_user();
    let updater = dirs
        .clone()
        .map(|d| Arc::new(Updater::new(HttpFetch::new(), d)));
    tauri::Builder::default()
        .manage(Camu {
            checker: Arc::new(checker),
            updater,
            dirs,
            reports: Default::default(),
            updating: Default::default(),
        })
        .invoke_handler(tauri::generate_handler![
            list_apps,
            check_updates,
            update_apps,
            list_backups,
            roll_back,
            app_icon,
            open_url,
            open_app,
            reveal_app
        ])
        .run(tauri::generate_context!())
        .expect("failed to start the application");
}
