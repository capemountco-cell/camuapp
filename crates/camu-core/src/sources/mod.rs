//! The places a new version can be found. Each module reads one kind of feed
//! and turns it into a [`Finding`].

pub mod appstore;
pub mod electron;
pub mod github;
pub mod homebrew;
pub mod sparkle;

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// The app's own Sparkle appcast.
    Sparkle,
    /// The app's own Electron update feed.
    Electron,
    AppStore,
    Github,
    Homebrew,
}

impl Source {
    /// Which source to believe when two report the same version. The app's own
    /// feed first, Homebrew last: its API can lag the real release.
    pub fn trust(self) -> u8 {
        match self {
            Source::Sparkle | Source::Electron => 0,
            Source::AppStore => 1,
            Source::Github => 2,
            Source::Homebrew => 3,
        }
    }
}

/// What one source says about one app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub source: Source,
    /// The version to show, e.g. "2.5.0".
    pub version: String,
    /// The build number, when the source has one.
    pub build: Option<String>,
    /// Whether this source considers its version newer than the installed one.
    pub is_update: bool,
    /// Plain text, never HTML.
    pub release_notes: Option<String>,
    pub release_notes_url: Option<String>,
    /// A direct download of the new version (zip, dmg or pkg).
    pub download_url: Option<String>,
    /// A web or App Store page for the app.
    pub page_url: Option<String>,
    pub minimum_os: Option<String>,
    /// Release date as the source wrote it.
    pub date: Option<String>,
    /// Set for Homebrew findings: the cask's token, e.g. "visual-studio-code".
    pub cask: Option<String>,
    /// Sparkle's `edSignature` for the download, checked against the app's
    /// SUPublicEDKey before installing.
    pub signature: Option<String>,
}

impl Finding {
    pub fn new(source: Source, version: impl Into<String>) -> Self {
        Self {
            source,
            version: version.into(),
            build: None,
            is_update: false,
            release_notes: None,
            release_notes_url: None,
            download_url: None,
            page_url: None,
            minimum_os: None,
            date: None,
            cask: None,
            signature: None,
        }
    }
}

/// Percent-encode one URL query value.
pub(crate) fn encode(value: &str) -> String {
    value
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}
