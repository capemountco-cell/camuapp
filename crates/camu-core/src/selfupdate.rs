//! Camu keeping itself up to date, from this repository's GitHub releases.
//!
//! Camu holds itself to the same rule as every other app: a download replaces
//! the running copy only when both are signed by the same Apple developer
//! team. An ad-hoc signed build has no team, so it can't prove a download is
//! genuine; it shows the release and lets the user download it instead.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Serialize;

use crate::fetch::Fetch;
use crate::macos::{self, sh_quote};
use crate::sources::{github, Source};
use crate::InstalledApp;

pub const OWNER: &str = "capemountco-cell";
pub const REPO: &str = "camuapp";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SelfUpdate {
    pub current: String,
    pub version: String,
    pub notes: Option<String>,
    pub page_url: Option<String>,
    pub download_url: Option<String>,
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
    };
    let body = fetch.get(&github::latest_url(OWNER, REPO))?;
    let Some(found) = github::parse(&body, &me, Source::Github)? else {
        return Ok(None);
    };
    if !found.is_update {
        return Ok(None);
    }
    let signed = bundle.is_some_and(|b| macos::team_id(b).is_some());
    Ok(Some(SelfUpdate {
        current: current.to_string(),
        can_install: signed && found.download_url.is_some(),
        version: found.version,
        notes: found.release_notes,
        page_url: found.release_notes_url,
        download_url: found.download_url,
    }))
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

    const RELEASE: &str = r#"{"tag_name":"v0.2.0","body":"Menu bar mode","html_url":"https://github.com/capemountco-cell/camuapp/releases/tag/v0.2.0",
      "assets":[{"browser_download_url":"https://github.com/capemountco-cell/camuapp/releases/download/v0.2.0/Camu-0.2.0-universal.dmg"}]}"#;

    #[test]
    fn finds_a_newer_release_but_wont_self_install_unsigned() {
        let fetch = FakeFetch::default().with(&github::latest_url(OWNER, REPO), RELEASE);
        let found = check(&fetch, "0.1.0", None).unwrap().unwrap();
        assert_eq!(found.version, "0.2.0");
        assert!(found.download_url.unwrap().ends_with(".dmg"));
        assert!(!found.can_install, "no signed bundle, no in-place install");
        assert_eq!(check(&fetch, "0.2.0", None).unwrap(), None);
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
