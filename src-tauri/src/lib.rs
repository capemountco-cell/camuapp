//! The Tauri shell: window, menu bar icon, background schedule, IPC.
//!
//! No decisions live here. Commands and the scheduler call the core crate and
//! pass results on as events, so the window, the menu bar and the background
//! checks all see the same state. A decision made here could not be tested
//! on Linux.
//!
//! Events sent to the window:
//! - `camu://check-started` (apps), `camu://report` (one per app),
//!   `camu://check-finished` (seconds since 1970)
//! - `camu://update-started` (ids), `camu://update` (progress),
//!   `camu://update-finished`
//! - `camu://self-update` (a newer Camu, or null)

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use camu_core::fetch::HttpFetch;
use camu_core::selfupdate::{self, SelfUpdate};
use camu_core::settings::{self, Settings};
use camu_core::update::{self, Backup, Dirs, Job, Phase, Updater};
use camu_core::{icon, launch, platform, scan, Checker, Install, InstalledApp, Report, Status};
use serde::Serialize;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, State, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_notification::NotificationExt;

/// Everything shared between commands, the menu bar and the scheduler.
struct Inner {
    checker: Checker,
    updater: Option<Updater>,
    dirs: Option<Dirs>,
    support: Option<PathBuf>,
    reports: Mutex<HashMap<String, Report>>,
    settings: Mutex<Settings>,
    state: Mutex<settings::State>,
    self_update: Mutex<Option<SelfUpdate>>,
    tray: Mutex<Option<TrayIcon>>,
    checking: AtomicBool,
    updating: AtomicBool,
}

type Shared = Arc<Inner>;

const NO_HOME: &str = "Camu can't find your home folder";

/// Run blocking work off the main thread and flatten the join error.
async fn blocking<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|e| e.to_string())
}

impl Inner {
    fn settings(&self) -> Settings {
        self.settings.lock().unwrap().clone()
    }

    fn save_state(&self) {
        if let Some(dir) = &self.support {
            let _ = settings::save_state(dir, &self.state.lock().unwrap());
        }
    }

    /// Updates waiting, ignored apps left out.
    fn pending(&self) -> Vec<Report> {
        let ignored = self.settings().ignored;
        let mut out: Vec<Report> = self
            .reports
            .lock()
            .unwrap()
            .values()
            .filter(|r| r.status == Status::UpdateAvailable && !ignored.contains(&r.app.bundle_id))
            .cloned()
            .collect();
        out.sort_by_key(|r| r.app.name.to_lowercase());
        out
    }
}

/// Check every app. Shared by the window, the menu bar and the schedule.
fn run_check(app: &AppHandle, inner: &Inner) -> Result<usize, String> {
    if inner.checking.swap(true, Ordering::SeqCst) {
        return Err("A check is already running".into());
    }
    let settings = inner.settings();
    inner.checker.refresh(platform::environment());
    let apps = scan::scan(&scan::default_roots());
    let _ = app.emit("camu://check-started", &apps);
    inner.reports.lock().unwrap().clear();

    let beta: HashSet<String> = settings.beta.iter().cloned().collect();
    inner.checker.check_all(&apps, &beta, 8, &|report| {
        inner
            .reports
            .lock()
            .unwrap()
            .insert(report.id.clone(), report.clone());
        let _ = app.emit("camu://report", report);
    });

    let now = settings::now_secs();
    let reports: Vec<Report> = inner.reports.lock().unwrap().values().cloned().collect();
    let fresh_text = {
        let mut state = inner.state.lock().unwrap();
        state.last_check = Some(now);
        let fresh = settings::fresh_updates(&reports, &settings, &mut state);
        settings::notification_text(&fresh)
    };
    inner.save_state();
    if let (true, Some((title, body))) = (settings.notify, fresh_text) {
        let _ = app.notification().builder().title(title).body(body).show();
    }
    refresh_tray(app, inner);
    let _ = app.emit("camu://check-finished", now);
    inner.checking.store(false, Ordering::SeqCst);
    Ok(apps.len())
}

/// Update apps by id, reporting progress as events.
fn run_updates(
    app: &AppHandle,
    inner: &Inner,
    ids: &[String],
) -> Result<Vec<(String, Phase)>, String> {
    let updater = inner.updater.as_ref().ok_or(NO_HOME)?;
    if inner.updating.swap(true, Ordering::SeqCst) {
        return Err("An update is already running".into());
    }
    let jobs: Vec<Job> = {
        let reports = inner.reports.lock().unwrap();
        ids.iter()
            .filter_map(|id| reports.get(id))
            .filter_map(Job::from_report)
            .collect()
    };
    let ids: Vec<&String> = jobs.iter().map(|j| &j.id).collect();
    let _ = app.emit("camu://update-started", &ids);

    let results = Mutex::new(Vec::new());
    updater.run(jobs.clone(), &|event| {
        if matches!(
            event.phase,
            Phase::Done { .. } | Phase::Failed { .. } | Phase::Handoff { .. }
        ) {
            results
                .lock()
                .unwrap()
                .push((event.id.clone(), event.phase.clone()));
        }
        let _ = app.emit("camu://update", event);
    });
    let results = results.into_inner().unwrap();

    // Finished apps are no longer pending.
    {
        let mut reports = inner.reports.lock().unwrap();
        for (id, phase) in &results {
            if matches!(phase, Phase::Done { .. }) {
                if let Some(r) = reports.get_mut(id) {
                    r.status = Status::UpToDate;
                }
            }
        }
    }
    inner.updating.store(false, Ordering::SeqCst);
    refresh_tray(app, inner);
    let _ = app.emit("camu://update-finished", ());
    Ok(results)
}

/// The small panel that opens under the menu bar icon.
const POPOVER: &str = "popover";
const POPOVER_SIZE: (f64, f64) = (380.0, 480.0);

fn build_popover(app: &AppHandle) -> tauri::Result<()> {
    WebviewWindowBuilder::new(app, POPOVER, WebviewUrl::App("index.html".into()))
        .title("Camu")
        .inner_size(POPOVER_SIZE.0, POPOVER_SIZE.1)
        .resizable(false)
        .decorations(false)
        .transparent(true)
        .shadow(true)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible(false)
        .build()?;
    Ok(())
}

/// Open the panel centred under the menu bar icon, or close it if open.
fn toggle_popover(app: &AppHandle, icon: tauri::Rect) {
    let Some(panel) = app.get_webview_window(POPOVER) else {
        return;
    };
    if panel.is_visible().unwrap_or(false) {
        let _ = panel.hide();
        return;
    }
    let scale = panel.scale_factor().unwrap_or(2.0);
    let at = icon.position.to_physical::<f64>(scale);
    let size = icon.size.to_physical::<f64>(scale);
    let width = POPOVER_SIZE.0 * scale;
    let x = (at.x + size.width / 2.0 - width / 2.0).max(8.0 * scale);
    let y = at.y + size.height + 6.0 * scale;
    let _ = panel.set_position(PhysicalPosition::new(x, y));
    let _ = panel.show();
    let _ = panel.set_focus();
    let _ = app.emit_to(POPOVER, "camu://popover-shown", ());
}

fn show_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Redraw the menu bar icon's count and menu from current state.
fn refresh_tray(app: &AppHandle, inner: &Inner) {
    let pending = inner.pending();
    let settings = inner.settings();
    let busy = inner.updating.load(Ordering::SeqCst);
    let checking = inner.checking.load(Ordering::SeqCst);
    // Clone the handle and drop the lock before touching the tray: tray calls
    // run on the main thread, which may itself be waiting for this lock.
    let Some(tray) = inner.tray.lock().unwrap().clone() else {
        return;
    };

    let _ = tray.set_visible(settings.menu_bar);
    let _ = tray.set_title(if pending.is_empty() {
        None
    } else {
        Some(pending.len().to_string())
    });
    let summary = match pending.len() {
        0 if checking => "Checking for updates…".to_string(),
        0 => "Everything is up to date".to_string(),
        1 => "1 update available".to_string(),
        n => format!("{n} updates available"),
    };
    let _ = tray.set_tooltip(Some(format!("Camu — {summary}")));
    if let Ok(menu) = build_menu(app, &summary, &pending, busy, checking) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_menu(
    app: &AppHandle,
    summary: &str,
    pending: &[Report],
    busy: bool,
    checking: bool,
) -> tauri::Result<Menu<tauri::Wry>> {
    let menu = Menu::new(app)?;
    menu.append(&MenuItem::with_id(
        app,
        "summary",
        summary,
        false,
        None::<&str>,
    )?)?;
    for report in pending.iter().take(8) {
        let version = report
            .best
            .as_ref()
            .map(|b| b.version.as_str())
            .unwrap_or("");
        let label = format!(
            "    {}  {} → {}",
            report.app.name, report.app.version, version
        );
        menu.append(&MenuItem::with_id(
            app,
            format!("app:{}", report.id),
            label,
            true,
            None::<&str>,
        )?)?;
    }
    if pending.len() > 8 {
        let more = format!("    and {} more…", pending.len() - 8);
        menu.append(&MenuItem::with_id(app, "open", more, true, None::<&str>)?)?;
    }
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    let update_label = if busy {
        "Updating…".to_string()
    } else {
        format!("Update All ({})", pending.len())
    };
    menu.append(&MenuItem::with_id(
        app,
        "update_all",
        update_label,
        !busy && !pending.is_empty(),
        Some("CmdOrCtrl+U"),
    )?)?;
    let check_label = if checking { "Checking…" } else { "Check Now" };
    menu.append(&MenuItem::with_id(
        app,
        "check",
        check_label,
        !checking && !busy,
        Some("CmdOrCtrl+R"),
    )?)?;
    menu.append(&PredefinedMenuItem::separator(app)?)?;
    menu.append(&MenuItem::with_id(
        app,
        "open",
        "Open Camu",
        true,
        None::<&str>,
    )?)?;
    menu.append(&MenuItem::with_id(
        app,
        "quit",
        "Quit Camu",
        true,
        Some("CmdOrCtrl+Q"),
    )?)?;
    Ok(menu)
}

fn on_menu(app: &AppHandle, id: &str) {
    let inner = app.state::<Shared>().inner().clone();
    let app = app.clone();
    match id {
        "update_all" => {
            std::thread::spawn(move || {
                let ids: Vec<String> = inner.pending().into_iter().map(|r| r.id).collect();
                let _ = run_updates(&app, &inner, &ids);
            });
        }
        "check" => {
            std::thread::spawn(move || {
                let _ = run_check(&app, &inner);
            });
        }
        "quit" => app.exit(0),
        id if id.starts_with("app:") => {
            show_window(&app);
            let _ = app.emit("camu://select", &id[4..]);
        }
        _ => show_window(&app),
    }
}

/// Show or hide the Dock icon.
fn apply_dock(app: &AppHandle, settings: &Settings) {
    #[cfg(target_os = "macos")]
    {
        let policy = if settings.show_in_dock {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        let _ = app.set_activation_policy(policy);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, settings);
}

/// Every minute: run a scheduled check when one is due, install one-click
/// updates if the user asked for that, and look for a new Camu once a day.
fn scheduler(app: AppHandle, inner: Shared) {
    let mut last_self_check = 0u64;
    // Give the window time to start its own launch check first.
    std::thread::sleep(Duration::from_secs(30));
    loop {
        let settings = inner.settings();
        let now = settings::now_secs();
        let idle = !inner.checking.load(Ordering::SeqCst) && !inner.updating.load(Ordering::SeqCst);

        // Copy the state out first: holding its lock into run_check, which
        // locks it again, would deadlock.
        let state = inner.state.lock().unwrap().clone();
        let due = idle && settings::check_due(&settings, &state, now);

        if due && run_check(&app, &inner).is_ok() && settings.auto_update {
            let ids: Vec<String> = inner
                .pending()
                .into_iter()
                .filter(|r| {
                    matches!(
                        r.install,
                        Install::Homebrew { .. } | Install::Download { .. }
                    )
                })
                .map(|r| r.id)
                .collect();
            if !ids.is_empty() {
                if let Ok(results) = run_updates(&app, &inner, &ids) {
                    let done = results
                        .iter()
                        .filter(|(_, p)| matches!(p, Phase::Done { .. }))
                        .count();
                    let failed = results
                        .iter()
                        .filter(|(_, p)| matches!(p, Phase::Failed { .. }))
                        .count();
                    if settings.notify && done + failed > 0 {
                        let body = match failed {
                            0 => format!("Updated {done} app{}.", if done == 1 { "" } else { "s" }),
                            _ => format!("Updated {done}; {failed} need attention."),
                        };
                        let _ = app.notification().builder().title("Camu").body(body).show();
                    }
                }
            }
        }

        if now.saturating_sub(last_self_check) >= 24 * 3600 {
            last_self_check = now;
            check_self(&app, &inner);
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

fn check_self(app: &AppHandle, inner: &Inner) {
    let current = app.package_info().version.to_string();
    let bundle = selfupdate::own_bundle();
    if let Ok(found) = selfupdate::check(&HttpFetch::new(), &current, bundle.as_deref()) {
        *inner.self_update.lock().unwrap() = found.clone();
        let _ = app.emit("camu://self-update", found);
    }
}

// Commands.

#[tauri::command]
async fn list_apps() -> Result<Vec<InstalledApp>, String> {
    blocking(|| scan::scan(&scan::default_roots())).await
}

/// Check every app; results arrive as events. Returns how many were checked.
#[tauri::command]
async fn check_updates(app: AppHandle, state: State<'_, Shared>) -> Result<usize, String> {
    let inner = state.inner().clone();
    blocking(move || run_check(&app, &inner)).await?
}

#[tauri::command]
async fn update_apps(
    app: AppHandle,
    state: State<'_, Shared>,
    ids: Vec<String>,
) -> Result<(), String> {
    let inner = state.inner().clone();
    blocking(move || run_updates(&app, &inner, &ids).map(|_| ())).await?
}

/// Everything the menu bar panel shows, in one call.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Snapshot {
    reports: Vec<Report>,
    last_check: Option<u64>,
    checking: bool,
    updating: bool,
}

#[tauri::command]
fn snapshot(state: State<'_, Shared>) -> Snapshot {
    let mut reports: Vec<Report> = state.reports.lock().unwrap().values().cloned().collect();
    reports.sort_by_key(|r| r.app.name.to_lowercase());
    Snapshot {
        reports,
        last_check: state.state.lock().unwrap().last_check,
        checking: state.checking.load(Ordering::SeqCst),
        updating: state.updating.load(Ordering::SeqCst),
    }
}

/// Bring the main window forward, optionally on a given view, and close the
/// menu bar panel.
#[tauri::command]
fn show_main(app: AppHandle, view: Option<String>) {
    if let Some(panel) = app.get_webview_window(POPOVER) {
        let _ = panel.hide();
    }
    show_window(&app);
    if let Some(view) = view {
        let _ = app.emit_to("main", "camu://navigate", view);
    }
}

#[tauri::command]
fn get_settings(state: State<'_, Shared>) -> Settings {
    state.settings()
}

/// Save settings and apply them. Returns them as stored (repaired if needed).
#[tauri::command]
fn set_settings(
    app: AppHandle,
    state: State<'_, Shared>,
    settings: Settings,
) -> Result<Settings, String> {
    let settings = settings.normalised();
    if let Some(dir) = &state.support {
        settings::save_settings(dir, &settings)?;
    }
    *state.settings.lock().unwrap() = settings.clone();
    apply_dock(&app, &settings);
    refresh_tray(&app, &state);
    Ok(settings)
}

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
fn self_update_status(state: State<'_, Shared>) -> Option<SelfUpdate> {
    state.self_update.lock().unwrap().clone()
}

#[tauri::command]
async fn check_self_update(
    app: AppHandle,
    state: State<'_, Shared>,
) -> Result<Option<SelfUpdate>, String> {
    let inner = state.inner().clone();
    blocking(move || {
        check_self(&app, &inner);
        inner.self_update.lock().unwrap().clone()
    })
    .await
}

/// Download, verify and install a new Camu, then relaunch. Only for signed
/// builds; unsigned ones get the release page instead.
#[tauri::command]
async fn install_self_update(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let inner = state.inner().clone();
    let found = inner
        .self_update
        .lock()
        .unwrap()
        .clone()
        .ok_or("Camu is up to date")?;
    if !found.can_install {
        return Err(
            "This copy of Camu isn't signed, so it can't verify the update. Download it instead."
                .into(),
        );
    }
    let url = found
        .download_url
        .clone()
        .ok_or("the release has no download")?;
    let bundle = selfupdate::own_bundle().ok_or("Camu isn't running from an app bundle")?;
    blocking(move || {
        let updater = inner.updater.as_ref().ok_or(NO_HOME)?;
        let me = InstalledApp::read(&bundle).ok_or("Camu can't read its own bundle")?;
        let new = updater.prepare(&me, &url, &found.version)?;
        selfupdate::hand_over(&new, &bundle)
    })
    .await??;
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn list_backups(state: State<'_, Shared>) -> Result<Vec<Backup>, String> {
    let dirs = state.dirs.clone().ok_or(NO_HOME)?;
    blocking(move || update::list_backups(&dirs.backups)).await
}

#[tauri::command]
async fn roll_back(state: State<'_, Shared>, path: PathBuf) -> Result<String, String> {
    let inner = state.inner().clone();
    blocking(move || {
        let app = InstalledApp::read(&path).ok_or("that app is no longer installed")?;
        inner.updater.as_ref().ok_or(NO_HOME)?.roll_back(&app)
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
    let dirs = Dirs::default_for_user();
    let support = settings::support_dir();
    let inner: Shared = Arc::new(Inner {
        checker: Checker::new(Box::new(HttpFetch::new()), Default::default()),
        updater: dirs.clone().map(|d| Updater::new(HttpFetch::new(), d)),
        dirs,
        settings: Mutex::new(
            support
                .as_deref()
                .map(settings::load_settings)
                .unwrap_or_default(),
        ),
        state: Mutex::new(
            support
                .as_deref()
                .map(settings::load_state)
                .unwrap_or_default(),
        ),
        support,
        reports: Default::default(),
        self_update: Default::default(),
        tray: Default::default(),
        checking: Default::default(),
        updating: Default::default(),
    });

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .manage(inner.clone())
        .setup(move |app| {
            let handle = app.handle().clone();
            let tray_icon = tauri::image::Image::from_bytes(include_bytes!("../icons/tray.png"))?;
            let tray = TrayIconBuilder::with_id("camu")
                .icon(tray_icon)
                .icon_as_template(true)
                .tooltip("Camu")
                // Left click opens the panel; right click shows the menu.
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        rect,
                        ..
                    } = event
                    {
                        toggle_popover(tray.app_handle(), rect);
                    }
                })
                .build(app)?;
            build_popover(&handle)?;
            *inner.tray.lock().unwrap() = Some(tray);
            apply_dock(&handle, &inner.settings());
            refresh_tray(&handle, &inner);

            let (h, i) = (handle.clone(), inner.clone());
            std::thread::spawn(move || scheduler(h, i));
            Ok(())
        })
        .on_window_event(|window, event| {
            // The panel closes when it loses focus, like a menu.
            if window.label() == POPOVER {
                match event {
                    WindowEvent::Focused(false) => {
                        let _ = window.hide();
                    }
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                    _ => {}
                }
                return;
            }
            // With the menu bar icon on, closing the window keeps Camu
            // running there; Quit is in the menu.
            if let WindowEvent::CloseRequested { api, .. } = event {
                let inner = window.state::<Shared>();
                if inner.settings().menu_bar {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            list_apps,
            snapshot,
            show_main,
            check_updates,
            update_apps,
            get_settings,
            set_settings,
            app_version,
            self_update_status,
            check_self_update,
            install_self_update,
            list_backups,
            roll_back,
            app_icon,
            open_url,
            open_app,
            reveal_app
        ])
        .build(tauri::generate_context!())
        .expect("failed to start the application");

    app.run(|app, event| {
        // Clicking the Dock icon brings the hidden window back.
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_window(app);
        }
        let _ = (app, &event);
    });
}
