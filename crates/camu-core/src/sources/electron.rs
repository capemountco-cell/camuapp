//! Electron apps built with electron-builder carry an `app-update.yml` that
//! says where their updater looks: a GitHub repository or a plain web folder
//! holding `latest-mac.yml`.

use super::{github, Finding, Source};
use crate::app::{flat_yaml, ElectronFeed, InstalledApp};
use crate::fetch::Fetch;
use crate::version;

pub fn manifest_url(base: &str) -> String {
    format!("{}/latest-mac.yml", base.trim_end_matches('/'))
}

/// Read `latest-mac.yml`.
pub fn parse(yml: &str, base: &str, app: &InstalledApp) -> Option<Finding> {
    let pairs = flat_yaml(yml);
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    let remote = get("version")?;
    let mut f = Finding::new(Source::Electron, version::clean(&remote));
    f.is_update = version::is_newer(&remote, &app.version);
    f.date = get("releaseDate");
    f.release_notes = get("releaseNotes");
    f.download_url = get("path").map(|path| {
        if path.starts_with("https://") {
            path
        } else {
            format!("{}/{}", base.trim_end_matches('/'), path)
        }
    });
    Some(f)
}

pub fn check(
    app: &InstalledApp,
    feed: &ElectronFeed,
    fetch: &dyn Fetch,
) -> Result<Option<Finding>, String> {
    match feed {
        ElectronFeed::Generic { url } => {
            let body = fetch.get(&manifest_url(url))?;
            Ok(parse(&body, url, app))
        }
        ElectronFeed::Github { owner, repo } => {
            let body = fetch.get(&github::latest_url(owner, repo))?;
            github::parse(&body, app, Source::Electron)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_latest_mac_yml() {
        let app = InstalledApp {
            path: "/Applications/Rocket.app".into(),
            file_name: "Rocket".into(),
            name: "Rocket".into(),
            bundle_id: "com.acme.rocket".into(),
            version: "1.4.0".into(),
            build: None,
            sparkle_feed: None,
            electron_feed: None,
            from_app_store: false,
            minimum_os: None,
            icon_file: None,
        };
        let yml = "version: 1.5.0\nfiles:\n  - url: Rocket-1.5.0-mac.zip\n    sha512: abc\npath: Rocket-1.5.0-mac.zip\nsha512: abc\nreleaseDate: '2026-11-01T10:00:00.000Z'\n";
        let f = parse(yml, "https://dl.acme.com/mac/", &app).unwrap();
        assert!(f.is_update);
        assert_eq!(f.version, "1.5.0");
        assert_eq!(
            f.download_url.as_deref(),
            Some("https://dl.acme.com/mac/Rocket-1.5.0-mac.zip")
        );
        assert_eq!(f.date.as_deref(), Some("2026-11-01T10:00:00.000Z"));
    }
}
