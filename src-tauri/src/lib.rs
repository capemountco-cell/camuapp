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

use camu_core::backups;
use camu_core::check::Prefs;
use camu_core::feedback;
use camu_core::fetch::HttpFetch;
use camu_core::history::{self, Entry, How, Outcome};
use camu_core::selfupdate::{self, SelfUpdate};
use camu_core::settings::{self, Settings};
use camu_core::update::{self, Backup, Dirs, Job, Phase, Updater};
use camu_core::{icon, launch, platform, scan, Checker, InstalledApp, Report, Status};
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

/// Clears a busy flag when dropped, so a panic mid-run can't leave Camu
/// thinking a check or update is still running forever.
struct Busy<'a>(&'a AtomicBool);

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

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

    /// Updates waiting: ignored apps and held-back versions left out.
    fn pending(&self) -> Vec<Report> {
        let ignored = self.settings().ignored;
        let mut out: Vec<Report> = self
            .reports
            .lock()
            .unwrap()
            .values()
            .filter(|r| {
                r.status == Status::UpdateAvailable
                    && r.hold.is_none()
                    && !ignored.contains(&r.app.bundle_id)
            })
            .cloned()
            .collect();
        out.sort_by_key(|r| r.app.name.to_lowercase());
        out
    }

    /// What Update All installs: pending updates that don't need the user
    /// first (major versions when asked, ones that need a newer macOS or the
    /// developer's own updater).
    fn update_all_ids(&self) -> Vec<String> {
        let settings = self.settings();
        self.pending()
            .into_iter()
            .filter(|r| !(settings.ask_major && r.major))
            .filter(|r| r.notice.is_none())
            .map(|r| r.id)
            .collect()
    }

    fn installed_bundle_ids(&self) -> HashSet<String> {
        self.reports
            .lock()
            .unwrap()
            .values()
            .map(|r| r.app.bundle_id.clone())
            .collect()
    }

    /// Apply the backup retention settings.
    fn prune_backups(&self) {
        let (Some(dirs), settings) = (&self.dirs, self.settings()) else {
            return;
        };
        let installed = self.installed_bundle_ids();
        let installed = (!installed.is_empty()).then_some(&installed);
        backups::prune(
            &dirs.backups,
            settings.backup_days,
            settings.backup_max_gb,
            installed,
            settings::now_secs(),
        );
    }

    fn record(&self, entries: Vec<Entry>) {
        if let Some(dir) = &self.support {
            let _ = history::append(dir, entries, settings::now_secs());
        }
    }
}

/// Check every app. Shared by the window, the menu bar and the schedule.
fn run_check(app: &AppHandle, inner: &Inner) -> Result<usize, String> {
    if inner.checking.swap(true, Ordering::SeqCst) {
        return Err("A check is already running".into());
    }
    let _busy = Busy(&inner.checking);
    let settings = inner.settings();
    inner.checker.refresh(platform::environment());
    let apps = scan::scan(&scan::default_roots());
    let _ = app.emit("camu://check-started", &apps);
    inner.reports.lock().unwrap().clear();

    let beta: HashSet<String> = settings.beta.iter().cloned().collect();
    let prefs = |a: &InstalledApp| Prefs {
        include_beta: beta.contains(&a.bundle_id),
        max_major: settings.pinned.get(&a.bundle_id).copied(),
    };
    let now = settings::now_secs();
    inner.checker.check_all(&apps, &prefs, 8, &|mut report| {
        settings::apply_choices(&mut report, &settings, now);
        inner
            .reports
            .lock()
            .unwrap()
            .insert(report.id.clone(), report.clone());
        let _ = app.emit("camu://report", report);
    });

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
    drop(_busy);
    refresh_tray(app, inner);
    inner.prune_backups();
    let _ = app.emit("camu://check-finished", now);
    Ok(apps.len())
}

/// Update apps by id, reporting progress as events, and log the outcome.
fn run_updates(
    app: &AppHandle,
    inner: &Inner,
    ids: &[String],
    how: How,
) -> Result<Vec<(String, Phase)>, String> {
    let updater = inner.updater.as_ref().ok_or(NO_HOME)?;
    if inner.updating.swap(true, Ordering::SeqCst) {
        return Err("An update is already running".into());
    }
    let busy = Busy(&inner.updating);
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
    updater.run_with(
        jobs.clone(),
        &|event| {
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
        },
        how == How::Manual,
    );
    let results = results.into_inner().unwrap();

    // Log every outcome, and mark finished apps as no longer pending.
    let now = settings::now_secs();
    let mut entries = Vec::new();
    {
        let mut reports = inner.reports.lock().unwrap();
        for (id, phase) in &results {
            let Some(job) = jobs.iter().find(|j| &j.id == id) else {
                continue;
            };
            let (outcome, to, message) = match phase {
                Phase::Done { version } => (Outcome::Updated, Some(version.clone()), None),
                Phase::Failed { message } => (
                    Outcome::Failed,
                    Some(job.target_version.clone()),
                    Some(message.clone()),
                ),
                Phase::Handoff { message } => (
                    Outcome::HandedOff,
                    Some(job.target_version.clone()),
                    Some(message.clone()),
                ),
                _ => continue,
            };
            entries.push(Entry {
                at: now,
                id: id.clone(),
                bundle_id: job.app.bundle_id.clone(),
                name: job.app.name.clone(),
                from: job.app.version.clone(),
                to,
                source: job
                    .source
                    .and_then(|s| serde_json::to_value(s).ok())
                    .and_then(|v| v.as_str().map(str::to_string)),
                how,
                outcome,
                message,
            });
            if matches!(phase, Phase::Done { .. }) {
                if let Some(r) = reports.get_mut(id) {
                    r.status = Status::UpToDate;
                }
            }
        }
    }
    inner.record(entries);
    drop(busy);
    refresh_tray(app, inner);
    inner.prune_backups();
    let _ = app.emit("camu://update-finished", ());
    let _ = app.emit("camu://history-changed", ());
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
    let update_all = inner.update_all_ids().len();
    if let Ok(menu) = build_menu(app, &summary, &pending, update_all, busy, checking) {
        let _ = tray.set_menu(Some(menu));
    }
}

fn build_menu(
    app: &AppHandle,
    summary: &str,
    pending: &[Report],
    update_all: usize,
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
        format!("Update All ({})", update_all)
    };
    menu.append(&MenuItem::with_id(
        app,
        "update_all",
        update_label,
        !busy && update_all > 0,
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
                let ids = inner.update_all_ids();
                let _ = run_updates(&app, &inner, &ids, How::Manual);
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

/// Every minute: run a scheduled check when one is due, install updates
/// that are safe to install unattended if the user asked for that, and look
/// for a new Camu once a day.
fn scheduler(app: AppHandle, inner: Shared) {
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
            auto_install(&app, &inner, &settings);
        }

        let last_self = inner.state.lock().unwrap().last_self_check.unwrap_or(0);
        if now.saturating_sub(last_self) >= 24 * 3600 {
            inner.state.lock().unwrap().last_self_check = Some(now);
            inner.save_state();
            check_self(&app, &inner);
            auto_install_self(&app, &inner, &settings);
        }
        std::thread::sleep(Duration::from_secs(60));
    }
}

/// Install what's safe to install unattended: no apps in use, no password
/// prompts, no major versions (unless the user allowed them), only inside
/// quiet hours, and never something that already failed this way.
fn auto_install(app: &AppHandle, inner: &Inner, settings: &Settings) {
    if let (Some(window), Some(hour)) = (settings.quiet_hours, platform::local_hour()) {
        if !window.contains(hour) {
            return;
        }
    }
    let failed = inner.state.lock().unwrap().auto_failed.clone();
    let ids: Vec<String> = inner
        .pending()
        .into_iter()
        .filter(|r| {
            let facts = settings::AutoFacts {
                running: camu_core::macos::is_running(&r.app.path),
                needs_password: update::needs_password(r),
                failed_before: r
                    .best
                    .as_ref()
                    .is_some_and(|b| failed.get(&r.app.bundle_id) == Some(&b.version)),
            };
            settings::auto_installable(r, settings, facts)
        })
        .map(|r| r.id)
        .collect();
    if ids.is_empty() {
        return;
    }
    let Ok(results) = run_updates(app, inner, &ids, How::Automatic) else {
        return;
    };
    // Remember failures so they aren't retried, or announced, every run.
    {
        let reports = inner.reports.lock().unwrap();
        let mut state = inner.state.lock().unwrap();
        for (id, phase) in &results {
            if let (Phase::Failed { .. }, Some(r)) = (phase, reports.get(id)) {
                if let Some(best) = &r.best {
                    state
                        .auto_failed
                        .insert(r.app.bundle_id.clone(), best.version.clone());
                }
            }
        }
    }
    inner.save_state();
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
            _ => format!("Updated {done}; {failed} need you. See History in Camu."),
        };
        let _ = app.notification().builder().title("Camu").body(body).show();
    }
}

/// With automatic installs on, Camu updates itself too, between update
/// runs, and relaunches on the new version. A version that failed to
/// install itself isn't tried again by itself.
fn auto_install_self(app: &AppHandle, inner: &Inner, settings: &Settings) {
    let idle = !inner.checking.load(Ordering::SeqCst) && !inner.updating.load(Ordering::SeqCst);
    let ready = inner
        .self_update
        .lock()
        .unwrap()
        .clone()
        .filter(|f| f.can_install);
    let Some(found) = ready.filter(|_| idle && settings.auto_update) else {
        return;
    };
    if inner.state.lock().unwrap().self_install_failed.as_deref() == Some(&found.version) {
        return;
    }
    match install_self(inner) {
        Ok(()) => {
            if settings.notify {
                let _ = app
                    .notification()
                    .builder()
                    .title("Camu")
                    .body(format!("Updating Camu to {}.", found.version))
                    .show();
            }
            app.exit(0);
        }
        Err(_) => {
            inner.state.lock().unwrap().self_install_failed = Some(found.version.clone());
            inner.save_state();
        }
    }
}

/// Verify the waiting Camu update and hand over to the script that swaps it
/// in once Camu quits. The caller must quit Camu right after.
fn install_self(inner: &Inner) -> Result<(), String> {
    if inner.updating.load(Ordering::SeqCst) {
        return Err("Wait for the current updates to finish".into());
    }
    let found = inner
        .self_update
        .lock()
        .unwrap()
        .clone()
        .ok_or("Camu is up to date")?;
    if !found.can_install {
        return Err("Camu can't verify this update itself. Download it from camu.cc.".into());
    }
    let url = found
        .download_url
        .clone()
        .ok_or("the release has no download")?;
    let bundle = selfupdate::own_bundle().ok_or("Camu isn't running from an app bundle")?;
    if let Some(reason) = selfupdate::replace_blocker(&bundle) {
        return Err(reason);
    }
    let updater = inner.updater.as_ref().ok_or(NO_HOME)?;
    let me = InstalledApp::read(&bundle).ok_or("Camu can't read its own bundle")?;
    let new = updater.prepare(&me, &url, &found.version, found.sha256.as_deref())?;
    selfupdate::hand_over(&new, &bundle)
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
    blocking(move || run_updates(&app, &inner, &ids, How::Manual).map(|_| ())).await?
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
    reapply_choices(&app, &state);
    Ok(settings)
}

/// This Mac's chip, for the "Intel only" badge.
#[tauri::command]
fn host_arch() -> camu_core::arch::Arch {
    camu_core::arch::Arch::host()
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

/// Download, verify and install a new Camu, then relaunch.
#[tauri::command]
async fn install_self_update(app: AppHandle, state: State<'_, Shared>) -> Result<(), String> {
    let inner = state.inner().clone();
    blocking(move || install_self(&inner)).await??;
    app.exit(0);
    Ok(())
}

#[tauri::command]
async fn list_backups(state: State<'_, Shared>) -> Result<Vec<Backup>, String> {
    let dirs = state.dirs.clone().ok_or(NO_HOME)?;
    blocking(move || update::list_backups(&dirs.backups)).await
}

/// Put back the previous version of an app, skip the version it was on so
/// the next check doesn't reinstall it, log it, and re-check the app.
#[tauri::command]
async fn roll_back(
    app: AppHandle,
    state: State<'_, Shared>,
    path: PathBuf,
) -> Result<String, String> {
    let inner = state.inner().clone();
    blocking(move || {
        if inner.updating.load(Ordering::SeqCst) {
            return Err("Wait for the current updates to finish".to_string());
        }
        let current = InstalledApp::read(&path).ok_or("that app is no longer installed")?;
        let restored = inner.updater.as_ref().ok_or(NO_HOME)?.roll_back(&current)?;

        let mut settings = inner.settings();
        settings
            .skipped
            .insert(current.bundle_id.clone(), current.version.clone());
        save_settings(&inner, &settings);
        inner.record(vec![Entry {
            at: settings::now_secs(),
            id: current.id(),
            bundle_id: current.bundle_id.clone(),
            name: current.name.clone(),
            from: current.version.clone(),
            to: Some(restored.clone()),
            source: None,
            how: How::Manual,
            outcome: Outcome::RolledBack,
            message: Some(format!("{} will be skipped", current.version)),
        }]);
        recheck(&app, &inner, &path);
        let _ = app.emit("camu://history-changed", ());
        Ok(restored)
    })
    .await?
}

/// Check one app again and send its new report.
fn recheck(app: &AppHandle, inner: &Inner, path: &std::path::Path) {
    let Some(installed) = InstalledApp::read(path) else {
        return;
    };
    let settings = inner.settings();
    let prefs = Prefs {
        include_beta: settings.beta.contains(&installed.bundle_id),
        max_major: settings.pinned.get(&installed.bundle_id).copied(),
    };
    let mut report = inner.checker.check_with(&installed, &prefs);
    settings::apply_choices(&mut report, &settings, settings::now_secs());
    inner
        .reports
        .lock()
        .unwrap()
        .insert(report.id.clone(), report.clone());
    let _ = app.emit("camu://report", report);
    refresh_tray(app, inner);
}

fn save_settings(inner: &Inner, settings: &Settings) {
    let settings = settings.clone().normalised();
    if let Some(dir) = &inner.support {
        let _ = settings::save_settings(dir, &settings);
    }
    *inner.settings.lock().unwrap() = settings;
}

/// Re-apply the user's choices to every report and send the ones that
/// changed.
fn reapply_choices(app: &AppHandle, inner: &Inner) {
    let settings = inner.settings();
    let now = settings::now_secs();
    let changed: Vec<Report> = {
        let mut reports = inner.reports.lock().unwrap();
        reports
            .values_mut()
            .filter_map(|r| {
                let before = r.hold.clone();
                settings::apply_choices(r, &settings, now);
                (r.hold != before).then(|| r.clone())
            })
            .collect()
    };
    for report in changed {
        let _ = app.emit("camu://report", report);
    }
    refresh_tray(app, inner);
}

/// A per-app choice from the details panel: "skip" this version, "snooze"
/// for `days`, "pin" to the installed major version, or "clear" them all.
#[tauri::command]
fn set_choice(
    app: AppHandle,
    state: State<'_, Shared>,
    bundle_id: String,
    choice: String,
    version: Option<String>,
    days: Option<u64>,
) -> Result<Settings, String> {
    let inner = state.inner().clone();
    let mut settings = inner.settings();
    match choice.as_str() {
        "skip" => {
            let version = version.ok_or("which version to skip?")?;
            settings.skipped.insert(bundle_id, version);
        }
        "snooze" => {
            let until = settings::now_secs() + days.unwrap_or(30) * 86_400;
            settings.snoozed.insert(bundle_id, until);
        }
        "pin" => {
            let major = version
                .as_deref()
                .and_then(camu_core::version::major)
                .ok_or("which major version?")?;
            settings.pinned.insert(bundle_id, major);
        }
        "clear" => {
            settings.skipped.remove(&bundle_id);
            settings.snoozed.remove(&bundle_id);
            settings.pinned.remove(&bundle_id);
        }
        other => return Err(format!("unknown choice {other}")),
    }
    save_settings(&inner, &settings);
    reapply_choices(&app, &inner);
    Ok(inner.settings())
}

#[tauri::command]
fn list_history(state: State<'_, Shared>) -> Vec<Entry> {
    state
        .support
        .as_deref()
        .map(|dir| history::trimmed(history::load(dir), settings::now_secs()))
        .unwrap_or_default()
}

#[tauri::command]
async fn backup_summary(state: State<'_, Shared>) -> Result<backups::Summary, String> {
    let inner = state.inner().clone();
    let dirs = inner.dirs.clone().ok_or(NO_HOME)?;
    blocking(move || backups::summary(&dirs.backups, &inner.installed_bundle_ids())).await
}

/// Delete backups: `scope` "deleted" (of apps no longer installed) or "all".
#[tauri::command]
async fn delete_backups(state: State<'_, Shared>, scope: String) -> Result<usize, String> {
    let inner = state.inner().clone();
    let dirs = inner.dirs.clone().ok_or(NO_HOME)?;
    blocking(move || {
        if inner.updating.load(Ordering::SeqCst) {
            return Err("Wait for the current updates to finish".to_string());
        }
        let installed = inner.installed_bundle_ids();
        Ok(match scope.as_str() {
            "deleted" if !installed.is_empty() => backups::delete(&dirs.backups, Some(&installed)),
            "all" => backups::delete(&dirs.backups, None),
            _ => 0,
        })
    })
    .await?
}

#[tauri::command]
fn reveal_backups(state: State<'_, Shared>) -> Result<(), String> {
    let dirs = state.dirs.clone().ok_or(NO_HOME)?;
    let _ = std::fs::create_dir_all(&dirs.backups);
    launch::reveal(&dirs.backups)
}

#[tauri::command]
async fn app_icon(path: PathBuf, icon_file: String) -> Result<Option<String>, String> {
    blocking(move || icon::data_url(&path, &icon_file)).await
}

/// Tell camu.cc, once, that this copy was installed or that an update
/// finished. Only Camu's version is sent. Tried a few times, in case the Mac
/// is still joining a network; an unsent event is tried again next launch.
fn count_launch(app: &AppHandle, inner: &Inner, event: feedback::LaunchEvent) {
    let camu = app.package_info().version.to_string();
    let fetch = HttpFetch::new();
    for attempt in 0..3 {
        if attempt > 0 {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
        if feedback::send_event(&fetch, event, &camu) {
            feedback::mark_sent(&mut inner.state.lock().unwrap(), event, &camu);
            inner.save_state();
            return;
        }
    }
}

/// Send a wrong-version report the user asked to send.
#[tauri::command]
async fn send_report(app: AppHandle, report: feedback::WrongVersion) -> Result<(), String> {
    let camu = app.package_info().version.to_string();
    blocking(move || {
        feedback::send_report(&HttpFetch::new(), report, &camu, platform::os_version())
    })
    .await?
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

            // Decided before the first check, which sets `last_check`.
            let camu = handle.package_info().version.to_string();
            let launch = feedback::launch_event(&mut inner.state.lock().unwrap(), &camu);
            inner.save_state();
            let (h, i) = (handle.clone(), inner.clone());
            std::thread::spawn(move || scheduler(h, i));
            if let Some(event) = launch {
                let (h, i) = (handle.clone(), inner.clone());
                std::thread::spawn(move || count_launch(&h, &i, event));
            }
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
            host_arch,
            self_update_status,
            check_self_update,
            install_self_update,
            list_backups,
            roll_back,
            set_choice,
            list_history,
            backup_summary,
            delete_backups,
            reveal_backups,
            app_icon,
            open_url,
            send_report,
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
