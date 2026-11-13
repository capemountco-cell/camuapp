//! Homebrew casks: a catalogue of thousands of Mac apps with their latest
//! versions, published at formulae.brew.sh.
//!
//! The catalogue covers apps that have no update feed of their own, so it
//! fills most of the gaps the other sources leave. It also tells Camu which
//! apps were installed with Homebrew, so those can be upgraded with `brew`.
//!
//! The published API can lag the cask repository by a few hours. Camu treats
//! Homebrew as the least trusted source: when an app's own feed or GitHub
//! release is newer, that wins.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

use super::{Finding, Source};
use crate::app::InstalledApp;
use crate::version;

pub const INDEX_URL: &str = "https://formulae.brew.sh/api/cask.json";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cask {
    pub token: String,
    pub version: String,
    pub homepage: Option<String>,
    pub url: Option<String>,
    /// The cask says the app updates itself.
    pub auto_updates: bool,
    pub notes: Option<String>,
}

/// Casks, looked up by the bundle id they quit when uninstalling and by the
/// `.app` file name they install.
#[derive(Debug, Default)]
pub struct CaskIndex {
    by_app: HashMap<String, Vec<Cask>>,
    by_bundle_id: HashMap<String, Vec<Cask>>,
}

impl CaskIndex {
    pub fn parse(json: &str) -> Result<Self, String> {
        let casks: Vec<Value> =
            serde_json::from_str(json).map_err(|e| format!("Homebrew index is not JSON: {e}"))?;
        let mut by_app: HashMap<String, Vec<Cask>> = HashMap::new();
        let mut by_bundle_id: HashMap<String, Vec<Cask>> = HashMap::new();
        for cask in &casks {
            if cask["disabled"].as_bool() == Some(true) {
                continue;
            }
            let (Some(token), Some(version)) = (cask["token"].as_str(), cask["version"].as_str())
            else {
                continue;
            };
            let entry = Cask {
                token: token.to_string(),
                version: version.to_string(),
                homepage: cask["homepage"].as_str().map(str::to_string),
                url: cask["url"].as_str().map(str::to_string),
                auto_updates: cask["auto_updates"].as_bool().unwrap_or(false),
                notes: cask["desc"].as_str().map(str::to_string),
            };
            for app in app_names(cask) {
                by_app
                    .entry(app.to_lowercase())
                    .or_default()
                    .push(entry.clone());
            }
            for id in bundle_ids(cask) {
                by_bundle_id
                    .entry(id.to_lowercase())
                    .or_default()
                    .push(entry.clone());
            }
        }
        Ok(Self {
            by_app,
            by_bundle_id,
        })
    }

    pub fn len(&self) -> usize {
        self.by_app.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_app.is_empty()
    }

    /// The cask that installs `app`, matched by bundle id first (exact, and
    /// survives renamed apps and .pkg installs), then by file name. A cask
    /// already installed on this Mac wins; otherwise the main cask beats
    /// variants like `firefox@beta`.
    pub fn find(&self, app: &InstalledApp, installed: &HashSet<String>) -> Option<&Cask> {
        let file = format!("{}.app", app.file_name).to_lowercase();
        let candidates = self
            .by_bundle_id
            .get(&app.bundle_id.to_lowercase())
            .or_else(|| self.by_app.get(&file))?;
        candidates
            .iter()
            .find(|c| installed.contains(&c.token))
            .or_else(|| candidates.iter().find(|c| !c.token.contains('@')))
            .or_else(|| candidates.first())
    }
}

/// Bundle ids a cask names in its `uninstall` stanza (`quit:`): the ids of the
/// apps it installs.
fn bundle_ids(cask: &Value) -> Vec<String> {
    let mut ids = Vec::new();
    for artifact in cask["artifacts"].as_array().into_iter().flatten() {
        for step in artifact["uninstall"].as_array().into_iter().flatten() {
            match &step["quit"] {
                Value::String(id) => ids.push(id.clone()),
                Value::Array(list) => {
                    ids.extend(list.iter().filter_map(|v| v.as_str()).map(str::to_string))
                }
                _ => {}
            }
        }
    }
    ids
}

/// The `.app` names a cask installs, from its `app` artifacts.
fn app_names(cask: &Value) -> Vec<String> {
    let mut names = Vec::new();
    for artifact in cask["artifacts"].as_array().into_iter().flatten() {
        for item in artifact["app"].as_array().into_iter().flatten() {
            match item {
                Value::String(name) => names.push(file_name(name)),
                // `app "Foo.app", target: "Bar.app"`: the second element
                // renames the first; the target is what lands on disk.
                Value::Object(options) => {
                    if let Some(target) = options.get("target").and_then(|t| t.as_str()) {
                        names.pop();
                        names.push(file_name(target));
                    }
                }
                _ => {}
            }
        }
    }
    names
}

fn file_name(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Casks installed with `brew` on this Mac, from its Caskroom folders.
pub fn installed_casks() -> HashSet<String> {
    caskrooms()
        .iter()
        .filter_map(|room| fs::read_dir(room).ok())
        .flatten()
        .flatten()
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

fn caskrooms() -> Vec<PathBuf> {
    ["/opt/homebrew/Caskroom", "/usr/local/Caskroom"]
        .iter()
        .map(PathBuf::from)
        .collect()
}

/// The `brew` executable, if Homebrew is installed.
pub fn brew_path() -> Option<PathBuf> {
    ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]
        .iter()
        .map(Path::new)
        .find(|p| p.is_file())
        .map(Path::to_path_buf)
}

pub fn finding(app: &InstalledApp, cask: &Cask) -> Option<Finding> {
    // "latest" casks have no version to compare.
    let (remote, build) = version::split_release(&cask.version, &app.version);
    if !remote.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    let mut f = Finding::new(Source::Homebrew, remote);
    f.build = build.map(str::to_string);
    f.is_update = version::is_update(&cask.version, None, &app.version, app.build.as_deref());
    f.page_url = cask.homepage.clone();
    f.download_url = cask.url.clone();
    f.cask = Some(cask.token.clone());
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = r#"[
      {"token":"rocket","version":"2.5.0,530","homepage":"https://acme.com","url":"https://github.com/acme/rocket/releases/download/v2.5.0/Rocket.dmg",
       "auto_updates":true,"artifacts":[{"uninstall":[{"quit":"com.acme.rocket"}]},{"app":["Rocket.app"]}]},
      {"token":"rocket@beta","version":"2.6.0b1","artifacts":[{"app":["Rocket.app"]}]},
      {"token":"renamer","version":"1.0","artifacts":[{"app":["Renamer v1.app",{"target":"Renamer.app"}]}]},
      {"token":"nightly","version":"latest","artifacts":[{"app":["Nightly.app"]}]},
      {"token":"gone","version":"1.0","disabled":true,"artifacts":[{"app":["Gone.app"]}]}
    ]"#;

    fn app(file: &str, version: &str) -> InstalledApp {
        InstalledApp {
            path: format!("/Applications/{file}.app").into(),
            file_name: file.into(),
            name: file.into(),
            bundle_id: format!("com.test.{file}"),
            version: version.into(),
            build: None,
            sparkle_feed: None,
            electron_feed: None,
            from_app_store: false,
            minimum_os: None,
            icon_file: None,
        }
    }

    #[test]
    fn prefers_the_installed_cask_then_the_main_one() {
        let index = CaskIndex::parse(INDEX).unwrap();
        let rocket = app("Rocket", "2.4.1");
        assert_eq!(
            index.find(&rocket, &HashSet::new()).unwrap().token,
            "rocket"
        );
        let installed = HashSet::from(["rocket@beta".to_string()]);
        assert_eq!(
            index.find(&rocket, &installed).unwrap().token,
            "rocket@beta"
        );
    }

    #[test]
    fn matches_by_bundle_id_even_when_renamed() {
        let index = CaskIndex::parse(INDEX).unwrap();
        let mut renamed = app("My Rocket", "2.4.1");
        renamed.bundle_id = "com.acme.rocket".into();
        assert_eq!(
            index.find(&renamed, &HashSet::new()).unwrap().token,
            "rocket"
        );
    }

    #[test]
    fn honours_renamed_targets_and_skips_disabled() {
        let index = CaskIndex::parse(INDEX).unwrap();
        assert_eq!(
            index
                .find(&app("Renamer", "1.0"), &HashSet::new())
                .unwrap()
                .token,
            "renamer"
        );
        assert!(index.find(&app("Gone", "1.0"), &HashSet::new()).is_none());
    }

    #[test]
    fn findings_strip_build_suffixes_and_skip_latest() {
        let index = CaskIndex::parse(INDEX).unwrap();
        let rocket = app("Rocket", "2.4.1");
        let f = finding(&rocket, index.find(&rocket, &HashSet::new()).unwrap()).unwrap();
        assert_eq!(f.version, "2.5.0");
        assert!(f.is_update);
        assert_eq!(f.cask.as_deref(), Some("rocket"));

        let nightly = app("Nightly", "1.0");
        assert!(finding(&nightly, index.find(&nightly, &HashSet::new()).unwrap()).is_none());
    }

    fn cask(token: &str, version: &str) -> Cask {
        Cask {
            token: token.into(),
            version: version.into(),
            ..Default::default()
        }
    }

    fn with_build(mut a: InstalledApp, build: &str) -> InstalledApp {
        a.build = Some(build.into());
        a
    }

    /// Versions as Homebrew and the apps' own Info.plist really report them.
    #[test]
    fn homebrew_build_suffixes_are_not_false_updates() {
        // Parallels Desktop: 27.1.0 (58756) against "27.1.0-58756".
        let parallels = with_build(app("Parallels Desktop", "27.1.0"), "58756");
        let f = finding(&parallels, &cask("parallels", "27.1.0-58756")).unwrap();
        assert_eq!(f.version, "27.1.0");
        assert!(!f.is_update);
        let older = with_build(app("Parallels Desktop", "27.1.0"), "58700");
        assert!(
            finding(&older, &cask("parallels", "27.1.0-58756"))
                .unwrap()
                .is_update
        );
        let old = with_build(app("Parallels Desktop", "26.2.1"), "57300");
        assert!(
            finding(&old, &cask("parallels", "27.1.0-58756"))
                .unwrap()
                .is_update
        );

        // MEGAsync: 6.7.0 with no build number against "6.7.0.3".
        let mega = app("MEGAsync", "6.7.0");
        let f = finding(&mega, &cask("megasync", "6.7.0.3")).unwrap();
        assert_eq!(f.version, "6.7.0");
        assert!(!f.is_update);
        assert!(
            finding(&app("MEGAsync", "6.6.2"), &cask("megasync", "6.7.0.3"))
                .unwrap()
                .is_update
        );

        // GitHub Desktop: 3.6.11 (3.6.11) against "3.6.11-0102f49a".
        let gh = with_build(app("GitHub Desktop", "3.6.11"), "3.6.11");
        let f = finding(&gh, &cask("github", "3.6.11-0102f49a")).unwrap();
        assert_eq!(f.version, "3.6.11");
        assert!(!f.is_update);
        let gh_old = with_build(app("GitHub Desktop", "3.6.10"), "3.6.10");
        assert!(
            finding(&gh_old, &cask("github", "3.6.11-0102f49a"))
                .unwrap()
                .is_update
        );
    }

    #[test]
    fn real_point_releases_still_count() {
        // A two-part version gaining a patch number is a real update.
        assert!(
            finding(&app("Rocket", "1.2"), &cask("rocket", "1.2.1"))
                .unwrap()
                .is_update
        );
        // A comma build newer than the app's build is an update.
        let rocket = with_build(app("Rocket", "2.5.0"), "520");
        assert!(
            finding(&rocket, &cask("rocket", "2.5.0,530"))
                .unwrap()
                .is_update
        );
        let same = with_build(app("Rocket", "2.5.0"), "530");
        assert!(
            !finding(&same, &cask("rocket", "2.5.0,530"))
                .unwrap()
                .is_update
        );
        // A dash that is part of the version itself stays.
        assert!(
            finding(&app("Tool", "1.0.0-beta.1"), &cask("tool", "1.0.0"))
                .unwrap()
                .is_update
        );
    }
}
