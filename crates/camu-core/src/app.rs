//! What Camu knows about one installed app, read from its bundle on disk.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

/// Where an Electron app's built-in updater looks for new versions, read from
/// `Contents/Resources/app-update.yml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "provider", rename_all = "camelCase")]
pub enum ElectronFeed {
    Github { owner: String, repo: String },
    Generic { url: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstalledApp {
    /// Absolute path to the `.app` bundle. Unique, so it doubles as the id.
    pub path: PathBuf,
    /// The bundle's file name without `.app`, e.g. "Visual Studio Code".
    pub file_name: String,
    /// The name to show. Display name, then bundle name, then file name.
    pub name: String,
    pub bundle_id: String,
    /// CFBundleShortVersionString, the version people see, e.g. "2.4.1".
    pub version: String,
    /// CFBundleVersion, the build number. Sparkle compares against this.
    pub build: Option<String>,
    /// Sparkle's appcast address, from SUFeedURL.
    pub sparkle_feed: Option<String>,
    pub electron_feed: Option<ElectronFeed>,
    /// Installed from the Mac App Store (it carries a store receipt).
    pub from_app_store: bool,
    pub minimum_os: Option<String>,
    /// File name of the icon inside Contents/Resources, if it has one.
    pub icon_file: Option<String>,
}

impl InstalledApp {
    /// Read the bundle at `path`. `None` if it is not a usable app bundle.
    pub fn read(path: &Path) -> Option<Self> {
        let contents = path.join("Contents");
        let info = plist::Value::from_file(contents.join("Info.plist")).ok()?;
        let info = info.as_dictionary()?;
        let text = |key: &str| {
            info.get(key)
                .and_then(|v| v.as_string())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };

        let bundle_id = text("CFBundleIdentifier")?;
        let file_name = path.file_stem()?.to_string_lossy().into_owned();
        let build = text("CFBundleVersion");
        // Some apps only set the build number. It is better than nothing.
        let version = text("CFBundleShortVersionString").or_else(|| build.clone())?;
        let name = text("CFBundleDisplayName")
            .or_else(|| text("CFBundleName"))
            .unwrap_or_else(|| file_name.clone());

        let icon_file = text("CFBundleIconFile").map(|file| {
            if Path::new(&file).extension().is_some() {
                file
            } else {
                format!("{file}.icns")
            }
        });

        let electron_feed = fs::read_to_string(contents.join("Resources/app-update.yml"))
            .ok()
            .and_then(|yml| parse_electron_feed(&yml));

        Some(Self {
            path: path.to_path_buf(),
            file_name,
            name,
            bundle_id,
            version,
            build,
            sparkle_feed: text("SUFeedURL").filter(|u| u.starts_with("http")),
            electron_feed,
            from_app_store: contents.join("_MASReceipt/receipt").is_file(),
            minimum_os: text("LSMinimumSystemVersion"),
            icon_file,
        })
    }

    /// The id the UI uses for this app.
    pub fn id(&self) -> String {
        self.path.to_string_lossy().into_owned()
    }
}

/// Read the top-level `key: value` pairs of a simple YAML document.
///
/// Electron's update files are flat and machine-written, so a full YAML
/// parser is not needed. Nested blocks and list items are skipped.
pub fn flat_yaml(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter(|line| !line.starts_with([' ', '\t', '-', '#']))
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| {
            let value = value.trim().trim_matches(|c| c == '\'' || c == '"');
            (key.trim().to_string(), value.to_string())
        })
        .filter(|(_, value)| !value.is_empty())
        .collect()
}

fn parse_electron_feed(yml: &str) -> Option<ElectronFeed> {
    let pairs = flat_yaml(yml);
    let get = |key: &str| pairs.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone());
    match get("provider")?.as_str() {
        "github" => Some(ElectronFeed::Github {
            owner: get("owner")?,
            repo: get("repo")?,
        }),
        "generic" => Some(ElectronFeed::Generic { url: get("url")? }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn electron_github_feed() {
        let yml =
            "owner: acme\nrepo: rocket\nprovider: github\nupdaterCacheDirName: rocket-updater\n";
        assert_eq!(
            parse_electron_feed(yml),
            Some(ElectronFeed::Github {
                owner: "acme".into(),
                repo: "rocket".into()
            })
        );
    }

    #[test]
    fn electron_generic_feed() {
        let yml = "provider: generic\nurl: 'https://updates.example.com/mac'\n";
        assert_eq!(
            parse_electron_feed(yml),
            Some(ElectronFeed::Generic {
                url: "https://updates.example.com/mac".into()
            })
        );
    }

    #[test]
    fn unknown_electron_provider_is_ignored() {
        assert_eq!(parse_electron_feed("provider: s3\nbucket: x\n"), None);
    }

    #[test]
    fn reads_a_bundle_from_disk() {
        let dir = std::env::temp_dir().join(format!("camu-test-{}", std::process::id()));
        let app = dir.join("Rocket.app");
        let contents = app.join("Contents");
        fs::create_dir_all(contents.join("_MASReceipt")).unwrap();
        fs::write(contents.join("_MASReceipt/receipt"), b"x").unwrap();
        fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>com.acme.rocket</string>
<key>CFBundleName</key><string>Rocket</string>
<key>CFBundleShortVersionString</key><string>2.4.1</string>
<key>CFBundleVersion</key><string>512</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>SUFeedURL</key><string>https://acme.com/appcast.xml</string>
</dict></plist>"#,
        )
        .unwrap();

        let read = InstalledApp::read(&app).expect("bundle should read");
        assert_eq!(read.bundle_id, "com.acme.rocket");
        assert_eq!(read.name, "Rocket");
        assert_eq!(read.version, "2.4.1");
        assert_eq!(read.build.as_deref(), Some("512"));
        assert_eq!(read.icon_file.as_deref(), Some("AppIcon.icns"));
        assert_eq!(
            read.sparkle_feed.as_deref(),
            Some("https://acme.com/appcast.xml")
        );
        assert!(read.from_app_store);

        fs::remove_dir_all(&dir).unwrap();
    }
}
