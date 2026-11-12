//! Camu's settings and remembered state, as JSON files in
//! ~/Library/Application Support/co.capemount.camu.
//!
//! They live on the Rust side (not in the web view's storage) because the
//! background scheduler and the menu bar need them while no window is open.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::check::{Report, Status};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    /// Hours between automatic checks; 0 turns them off.
    pub check_every_hours: u32,
    /// Send a notification when new updates appear.
    pub notify: bool,
    /// Install one-click updates automatically after a scheduled check.
    pub auto_update: bool,
    /// Keep running in the menu bar when the window is closed.
    pub menu_bar: bool,
    /// Show the Dock icon. Only off when the menu bar icon is on.
    pub show_in_dock: bool,
    /// Bundle ids whose updates are hidden.
    pub ignored: Vec<String>,
    /// Bundle ids that follow beta channels.
    pub beta: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            check_every_hours: 6,
            notify: true,
            auto_update: false,
            menu_bar: true,
            show_in_dock: true,
            ignored: Vec::new(),
            beta: Vec::new(),
        }
    }
}

impl Settings {
    /// Repair combinations that make no sense rather than refusing them.
    pub fn normalised(mut self) -> Self {
        if !self.menu_bar {
            // Without the menu bar icon, hiding the Dock icon would leave no
            // way back to the app.
            self.show_in_dock = true;
        }
        self.check_every_hours = self.check_every_hours.min(24 * 7);
        self.ignored.sort();
        self.ignored.dedup();
        self.beta.sort();
        self.beta.dedup();
        self
    }
}

/// What Camu remembers between runs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct State {
    /// Seconds since 1970 of the last finished check.
    pub last_check: Option<u64>,
    /// The newest version already notified about, per bundle id.
    pub notified: BTreeMap<String, String>,
}

pub fn support_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join("Library/Application Support/co.capemount.camu"))
}

fn load<T: Default + for<'de> Deserialize<'de>>(path: &Path) -> T {
    fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write atomically, so a crash mid-write never leaves a broken file.
fn save<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let dir = path.parent().ok_or("no folder")?;
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    let json = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())
}

pub fn load_settings(dir: &Path) -> Settings {
    load::<Settings>(&dir.join("settings.json")).normalised()
}

pub fn save_settings(dir: &Path, settings: &Settings) -> Result<(), String> {
    save(&dir.join("settings.json"), settings)
}

pub fn load_state(dir: &Path) -> State {
    load(&dir.join("state.json"))
}

pub fn save_state(dir: &Path, state: &State) -> Result<(), String> {
    save(&dir.join("state.json"), state)
}

pub fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

/// Is a scheduled check due?
pub fn check_due(settings: &Settings, state: &State, now: u64) -> bool {
    if settings.check_every_hours == 0 {
        return false;
    }
    let every = u64::from(settings.check_every_hours) * 3600;
    state
        .last_check
        .is_none_or(|last| now.saturating_sub(last) >= every)
}

/// Updates not yet notified about, skipping ignored apps. Marks them as
/// notified in `state`, so each new version is announced once.
pub fn fresh_updates<'a>(
    reports: &'a [Report],
    settings: &Settings,
    state: &mut State,
) -> Vec<&'a Report> {
    let mut fresh = Vec::new();
    for report in reports {
        if report.status != Status::UpdateAvailable
            || settings.ignored.contains(&report.app.bundle_id)
        {
            continue;
        }
        let Some(best) = &report.best else { continue };
        if state.notified.get(&report.app.bundle_id) != Some(&best.version) {
            state
                .notified
                .insert(report.app.bundle_id.clone(), best.version.clone());
            fresh.push(report);
        }
    }
    fresh
}

/// The notification text for new updates, or `None` when there are none.
pub fn notification_text(fresh: &[&Report]) -> Option<(String, String)> {
    let names: Vec<&str> = fresh.iter().map(|r| r.app.name.as_str()).collect();
    let body = match names.as_slice() {
        [] => return None,
        [one] => format!("{one} {} is available.", fresh[0].best.as_ref()?.version),
        [a, b] => format!("{a} and {b} have updates."),
        [a, b, rest @ ..] => format!("{a}, {b} and {} more have updates.", rest.len()),
    };
    let title = if names.len() == 1 {
        "Update available".to_string()
    } else {
        format!("{} updates available", names.len())
    };
    Some((title, body))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check::{decide, Install};
    use crate::sources::{Finding, Source};
    use crate::InstalledApp;

    fn report(name: &str, version: &str, latest: &str) -> Report {
        let app = InstalledApp {
            path: format!("/Applications/{name}.app").into(),
            file_name: name.into(),
            name: name.into(),
            bundle_id: format!("com.test.{}", name.to_lowercase()),
            version: version.into(),
            build: None,
            sparkle_feed: None,
            electron_feed: None,
            from_app_store: false,
            minimum_os: None,
            icon_file: None,
        };
        let mut f = Finding::new(Source::Homebrew, latest);
        f.is_update = crate::version::is_newer(latest, version);
        let mut r = decide(&app, vec![f], vec![], None);
        r.install = Install::None;
        r
    }

    #[test]
    fn settings_survive_a_round_trip_and_repair_nonsense() {
        let dir = std::env::temp_dir().join(format!("camu-settings-{}", std::process::id()));
        assert_eq!(load_settings(&dir), Settings::default());
        let odd = Settings {
            menu_bar: false,
            show_in_dock: false,
            ignored: vec!["b".into(), "a".into(), "a".into()],
            ..Settings::default()
        };
        save_settings(&dir, &odd).unwrap();
        let back = load_settings(&dir);
        assert!(back.show_in_dock, "no menu bar means the Dock icon stays");
        assert_eq!(back.ignored, ["a", "b"]);
        fs::write(dir.join("settings.json"), "{ broken").unwrap();
        assert_eq!(load_settings(&dir), Settings::default());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_or_missing_fields_fall_back_to_defaults() {
        let s: Settings = serde_json::from_str(r#"{"notify": false, "future": 1}"#).unwrap();
        assert!(!s.notify);
        assert_eq!(s.check_every_hours, 6);
    }

    #[test]
    fn checks_are_due_on_schedule() {
        let s = Settings::default();
        let mut state = State::default();
        assert!(check_due(&s, &state, 1000), "never checked");
        state.last_check = Some(1000);
        assert!(!check_due(&s, &state, 1000 + 3600));
        assert!(check_due(&s, &state, 1000 + 6 * 3600));
        let off = Settings {
            check_every_hours: 0,
            ..s
        };
        assert!(!check_due(&off, &State::default(), 1000));
    }

    #[test]
    fn each_new_version_is_announced_once() {
        let settings = Settings {
            ignored: vec!["com.test.muted".into()],
            ..Settings::default()
        };
        let mut state = State::default();
        let first = [
            report("Arc", "1.0", "1.1"),
            report("Muted", "1.0", "2.0"),
            report("Same", "1.0", "1.0"),
        ];
        let fresh = fresh_updates(&first, &settings, &mut state);
        assert_eq!(
            fresh
                .iter()
                .map(|r| r.app.name.as_str())
                .collect::<Vec<_>>(),
            ["Arc"]
        );

        assert!(
            fresh_updates(&first, &settings, &mut state).is_empty(),
            "already announced"
        );
        let newer = [report("Arc", "1.0", "1.2")];
        assert_eq!(
            fresh_updates(&newer, &settings, &mut state).len(),
            1,
            "a newer version is news"
        );
    }

    #[test]
    fn notification_wording() {
        let a = report("Arc", "1.0", "1.1");
        let b = report("Zoom", "1.0", "1.1");
        let c = report("Slack", "1.0", "1.1");
        assert_eq!(
            notification_text(&[&a]),
            Some(("Update available".into(), "Arc 1.1 is available.".into()))
        );
        assert_eq!(
            notification_text(&[&a, &b]).unwrap().1,
            "Arc and Zoom have updates."
        );
        assert_eq!(
            notification_text(&[&a, &b, &c]).unwrap().0,
            "3 updates available"
        );
        assert_eq!(notification_text(&[]), None);
    }
}
