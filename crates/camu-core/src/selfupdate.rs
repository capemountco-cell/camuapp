//! Camu keeping itself up to date, from this repository's GitHub releases.
//!
//! A Developer ID signed Camu holds itself to the same rule as every other
//! app: a download replaces it only when both are signed by the same Apple
//! team. An ad-hoc signed Camu has no team, so instead it installs only a
//! download from this repository's own releases whose SHA-256 matches the
//! `.sha256` file published beside it. That is the same trust as downloading
//! the release by hand from GitHub.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::fetch::Fetch;
use crate::macos::{self, sh_quote};
use crate::sources::{github, Source};
use crate::InstalledApp;

pub const OWNER: &str = "capemountco-cell";
pub const REPO: &str = "camuapp";
/// Where people download Camu by hand: the website, which always serves the
/// newest build. Shown when Camu can't install an update itself.
pub const DOWNLOAD_PAGE: &str = "https://camu.cc/download";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfUpdate {
    pub current: String,
    pub version: String,
    pub notes: Option<String>,
    pub page_url: Option<String>,
    pub download_url: Option<String>,
    /// The download's SHA-256, from the `.sha256` file published with it.
    pub sha256: Option<String>,
    /// Camu can verify and install this itself.
    pub can_install: bool,
}

/// The running app's bundle, from the executable's path
/// (`Camu.app/Contents/MacOS/camu`).
pub fn own_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let bundle = exe.ancestors().nth(3)?;
    (bundle.extension()? == "app").then(|| bundle.to_path_buf())
}

/// Why Camu can't replace itself where it is, if it can't: running from a
/// disk image, from macOS's quarantine copy of a download, or from a folder
/// this user can't write to. Trying anyway would quit and reopen the old copy
/// in a loop.
pub fn replace_blocker(bundle: &Path) -> Option<String> {
    let path = bundle.to_string_lossy();
    if path.contains("/AppTranslocation/") || path.starts_with("/Volumes/") {
        return Some("Move Camu to your Applications folder to update it".into());
    }
    let parent = bundle.parent()?;
    let probe = parent.join(format!(".camu-write-test-{}", std::process::id()));
    match std::fs::write(&probe, b"") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            None
        }
        Err(_) => Some(format!(
            "Camu can't write to {}. Download the new version from camu.cc",
            parent.display()
        )),
    }
}

/// Ask GitHub for Camu's latest release. `Ok(None)` when up to date.
pub fn check(
    fetch: &dyn Fetch,
    current: &str,
    bundle: Option<&Path>,
) -> Result<Option<SelfUpdate>, String> {
    let me = InstalledApp {
        path: bundle.map(Path::to_path_buf).unwrap_or_default(),
        file_name: "Camu".into(),
        name: "Camu".into(),
        bundle_id: "co.capemount.camu".into(),
        version: current.to_string(),
        build: None,
        sparkle_feed: None,
        electron_feed: None,
        from_app_store: false,
        minimum_os: None,
        icon_file: None,
        ..Default::default()
    };
    let body = fetch.get(&github::latest_url(OWNER, REPO))?;
    let Some(found) = github::parse(&body, &me, Source::Github)? else {
        return Ok(None);
    };
    if !found.is_update {
        return Ok(None);
    }
    let ours = found
        .download_url
        .as_deref()
        .is_some_and(is_own_release_download);
    let sha256 = if ours {
        published_sha256(
            fetch,
            &body,
            found.download_url.as_deref().unwrap_or_default(),
        )
    } else {
        None
    };
    let signed = bundle.is_some_and(|b| macos::team_id(b).is_some());
    Ok(Some(SelfUpdate {
        current: current.to_string(),
        can_install: ours && (signed || sha256.is_some()),
        sha256,
        version: found.version,
        notes: found.release_notes,
        page_url: Some(DOWNLOAD_PAGE.to_string()),
        download_url: found.download_url,
    }))
}

/// True for a release download from Camu's GitHub account
/// (`https://github.com/<OWNER>/<any repo>/releases/download/...`). The repo
/// name isn't pinned: GitHub's API follows a renamed repository, and its
/// downloads then come back under the new name.
fn is_own_release_download(url: &str) -> bool {
    url.strip_prefix(&format!("https://github.com/{OWNER}/"))
        .and_then(|rest| rest.split_once('/'))
        .is_some_and(|(repo, path)| !repo.is_empty() && path.starts_with("releases/download/"))
}

/// Fetch the `<download>.sha256` asset of the release, if it has one.
fn published_sha256(fetch: &dyn Fetch, release_json: &str, download: &str) -> Option<String> {
    let release: serde_json::Value = serde_json::from_str(release_json).ok()?;
    let want = format!("{download}.sha256");
    let listed = release["assets"]
        .as_array()?
        .iter()
        .any(|a| a["browser_download_url"].as_str() == Some(want.as_str()));
    if !listed {
        return None;
    }
    crate::update::parse_sha256(&fetch.get(&want).ok()?)
}

/// A script that waits for Camu (`pid`) to quit, swaps in the new bundle,
/// puts the old one back if the copy fails, and reopens Camu.
pub fn swap_script(pid: u32, new: &Path, dest: &Path, aside: &Path) -> String {
    let (new, dest, aside) = (
        sh_quote(&new.to_string_lossy()),
        sh_quote(&dest.to_string_lossy()),
        sh_quote(&aside.to_string_lossy()),
    );
    format!(
        "#!/bin/sh\n\
         # Written by Camu to replace itself once it has quit.\n\
         while /bin/kill -0 {pid} 2>/dev/null; do /bin/sleep 0.2; done\n\
         /bin/rm -rf {aside}\n\
         if /bin/mv {dest} {aside}; then\n\
         \x20 if /usr/bin/ditto {new} {dest}; then /bin/rm -rf {aside}; else /bin/rm -rf {dest}; /bin/mv {aside} {dest}; fi\n\
         fi\n\
         /usr/bin/open {dest}\n"
    )
}

/// Start the swap in the background. The caller must quit Camu right after.
pub fn hand_over(new: &Path, dest: &Path) -> Result<(), String> {
    let dir = new
        .parent()
        .and_then(Path::parent)
        .ok_or("no staging folder")?;
    let script = dir.join("swap.sh");
    let aside = dir.join("previous.app");
    std::fs::write(&script, swap_script(std::process::id(), new, dest, &aside))
        .map_err(|e| e.to_string())?;
    Command::new("/bin/sh")
        .arg(&script)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::fake::FakeFetch;

    const DMG: &str =
        "https://github.com/capemountco-cell/camuapp/releases/download/v0.2.0/Camu-0.2.0-universal.dmg";
    const RELEASE: &str = r#"{"tag_name":"v0.2.0","body":"Menu bar mode","html_url":"https://github.com/capemountco-cell/camuapp/releases/tag/v0.2.0",
      "assets":[{"browser_download_url":"https://github.com/capemountco-cell/camuapp/releases/download/v0.2.0/Camu.dmg"},
                {"browser_download_url":"https://github.com/capemountco-cell/camuapp/releases/download/v0.2.0/Camu-0.2.0-universal.dmg"},
                {"browser_download_url":"https://github.com/capemountco-cell/camuapp/releases/download/v0.2.0/Camu-0.2.0-universal.dmg.sha256"}]}"#;

    #[test]
    fn unsigned_camu_self_installs_only_with_a_published_checksum() {
        let hash = "0f".repeat(32);
        let fetch = FakeFetch::default()
            .with(&github::latest_url(OWNER, REPO), RELEASE)
            .with(
                &format!("{DMG}.sha256"),
                &format!("{hash}  release/Camu-0.2.0-universal.dmg\n"),
            );
        let found = check(&fetch, "0.1.0", None).unwrap().unwrap();
        assert_eq!(found.version, "0.2.0");
        assert_eq!(found.download_url.as_deref(), Some(DMG));
        assert_eq!(found.page_url.as_deref(), Some(DOWNLOAD_PAGE));
        assert_eq!(found.sha256.as_deref(), Some(hash.as_str()));
        assert!(found.can_install);
        assert_eq!(check(&fetch, "0.2.0", None).unwrap(), None);

        // No checksum published: show the release instead.
        let bare = RELEASE.replace(
            &format!(",\n                {{\"browser_download_url\":\"{DMG}.sha256\"}}"),
            "",
        );
        assert!(!bare.contains(".sha256"));
        let fetch = FakeFetch::default().with(&github::latest_url(OWNER, REPO), &bare);
        let found = check(&fetch, "0.1.0", None).unwrap().unwrap();
        assert_eq!(found.sha256, None);
        assert!(!found.can_install);
    }

    #[test]
    fn trusts_downloads_from_its_own_account_even_after_a_rename() {
        assert!(is_own_release_download(DMG));
        assert!(is_own_release_download(
            "https://github.com/capemountco-cell/camu-renamed/releases/download/v1/Camu.dmg"
        ));
        assert!(!is_own_release_download(
            "https://github.com/someone-else/camuapp/releases/download/v1/Camu.dmg"
        ));
        assert!(!is_own_release_download(
            "https://github.com/capemountco-cell/camuapp/archive/main.zip"
        ));
        assert!(!is_own_release_download(
            "https://github.com.evil.example/capemountco-cell/camuapp/releases/download/v1/Camu.dmg"
        ));
    }

    #[test]
    fn wont_replace_itself_from_a_disk_image_or_quarantine() {
        assert!(replace_blocker(Path::new("/Volumes/Camu/Camu.app")).is_some());
        assert!(replace_blocker(Path::new(
            "/private/var/folders/x/AppTranslocation/ABC/d/Camu.app"
        ))
        .is_some());
        let dir = std::env::temp_dir().join(format!("camu-rb-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(replace_blocker(&dir.join("Camu.app")), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn swap_waits_for_camu_and_restores_on_failure() {
        let s = swap_script(
            42,
            Path::new("/c/new/Camu.app"),
            Path::new("/Applications/Camu.app"),
            Path::new("/c/old.app"),
        );
        assert!(s.contains("kill -0 42"));
        assert!(s.contains("/bin/mv '/c/old.app' '/Applications/Camu.app'"));
        assert!(s.ends_with("/usr/bin/open '/Applications/Camu.app'\n"));
    }
}
