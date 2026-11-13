//! GitHub releases, for apps whose updates are published there.

use serde_json::Value;

use super::{Finding, Source};
use crate::app::InstalledApp;
use crate::arch::{self, Arch, Build};
use crate::fetch::Fetch;
use crate::version;

/// Find `owner/repo` in a github.com URL, e.g. a release download link.
pub fn repo_from_url(url: &str) -> Option<(String, String)> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))?;
    let mut parts = rest.split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts
        .next()
        .filter(|s| !s.is_empty())?
        .trim_end_matches(".git");
    Some((owner.to_string(), repo.to_string()))
}

pub fn latest_url(owner: &str, repo: &str) -> String {
    format!("https://api.github.com/repos/{owner}/{repo}/releases/latest")
}

/// The version inside a tag: "v2.4.1" → "2.4.1", "rocket-2.4.1" → "2.4.1".
fn version_from_tag(tag: &str) -> Option<&str> {
    let start = tag.find(|c: char| c.is_ascii_digit())?;
    Some(&tag[start..])
}

/// Pick the asset most likely to be the Mac download for `arch`: a Mac
/// build over others, the right chip (or universal) over the wrong one, then
/// dmg over zip over pkg. Names are read by whole words, so "darwin" isn't
/// mistaken for "win".
fn mac_asset(release: &Value, arch: Arch) -> Option<String> {
    let assets = release["assets"].as_array()?;
    let urls: Vec<&str> = assets
        .iter()
        .filter_map(|a| a["browser_download_url"].as_str())
        .collect();
    let score = |url: &&str| {
        let lower = url.to_lowercase();
        let kind = if lower.ends_with(".dmg") {
            3
        } else if lower.ends_with(".zip") {
            2
        } else if lower.ends_with(".pkg") {
            1
        } else {
            return None;
        };
        let name = lower.rsplit('/').next().unwrap_or(&lower).to_string();
        let words: Vec<&str> = name.split(|c: char| !c.is_ascii_alphanumeric()).collect();
        let has = |w: &[&str]| words.iter().any(|x| w.contains(x));
        let mac = has(&["mac", "macos", "darwin", "osx", "apple", "universal"]) as u8;
        let other_os = has(&["win", "windows", "win32", "win64", "linux", "blockmap"])
            || lower.ends_with(".blockmap");
        let chip = match arch::build_of(url) {
            Build::Universal => 2,
            Build::Only(a) if a == arch => 2,
            Build::Unknown => 1,
            Build::Only(_) => 0,
        };
        (!other_os).then_some((mac, chip, kind))
    };
    urls.iter()
        .filter_map(|u| score(u).map(|s| (s, *u)))
        .max_by_key(|(s, _)| *s)
        .map(|(_, u)| u.to_string())
}

pub fn parse(json: &str, app: &InstalledApp, source: Source) -> Result<Option<Finding>, String> {
    parse_for(json, app, source, Arch::host())
}

/// `parse`, choosing the download for `arch`.
pub fn parse_for(
    json: &str,
    app: &InstalledApp,
    source: Source,
    arch: Arch,
) -> Result<Option<Finding>, String> {
    let release: Value =
        serde_json::from_str(json).map_err(|e| format!("GitHub reply is not JSON: {e}"))?;
    if release["draft"].as_bool() == Some(true) || release["prerelease"].as_bool() == Some(true) {
        return Ok(None);
    }
    let Some(remote) = release["tag_name"].as_str().and_then(version_from_tag) else {
        return Ok(None);
    };
    let mut f = Finding::new(source, version::split_release(remote, &app.version).0);
    f.is_update = version::is_update(remote, None, &app.version, app.build.as_deref());
    f.release_notes = release["body"]
        .as_str()
        .map(|b| b.trim().to_string())
        .filter(|b| !b.is_empty());
    f.release_notes_url = release["html_url"].as_str().map(str::to_string);
    f.page_url = f.release_notes_url.clone();
    f.download_url = mac_asset(&release, arch);
    f.date = release["published_at"].as_str().map(str::to_string);
    Ok(Some(f))
}

pub fn check(
    app: &InstalledApp,
    owner: &str,
    repo: &str,
    fetch: &dyn Fetch,
    arch: Arch,
) -> Result<Option<Finding>, String> {
    let body = fetch.get(&latest_url(owner, repo))?;
    parse_for(&body, app, Source::Github, arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_repos_in_urls() {
        assert_eq!(
            repo_from_url("https://github.com/acme/rocket/releases/download/v1.0/Rocket.dmg"),
            Some(("acme".into(), "rocket".into()))
        );
        assert_eq!(repo_from_url("https://acme.com/rocket.dmg"), None);
        assert_eq!(repo_from_url("https://github.com/acme"), None);
    }

    #[test]
    fn tags_and_assets() {
        assert_eq!(version_from_tag("v2.4.1"), Some("2.4.1"));
        assert_eq!(version_from_tag("rocket-2.4.1"), Some("2.4.1"));
        assert_eq!(version_from_tag("nightly"), None);

        let release: Value = serde_json::from_str(
            r#"{"assets":[
              {"browser_download_url":"https://x/Rocket-2.0-win.zip"},
              {"browser_download_url":"https://x/Rocket-2.0.dmg.blockmap"},
              {"browser_download_url":"https://x/Rocket-2.0-mac.zip"},
              {"browser_download_url":"https://x/Rocket-2.0-universal.dmg"}]}"#,
        )
        .unwrap();
        assert_eq!(
            mac_asset(&release, Arch::Arm64).as_deref(),
            Some("https://x/Rocket-2.0-universal.dmg")
        );
    }

    #[test]
    fn picks_the_build_for_this_chip_and_keeps_darwin() {
        let release: Value = serde_json::from_str(
            r#"{"assets":[
              {"browser_download_url":"https://x/Tabby-1.0-macos-arm64.dmg"},
              {"browser_download_url":"https://x/Tabby-1.0-macos-x64.dmg"},
              {"browser_download_url":"https://x/Tabby-1.0-windows-x64.exe"}]}"#,
        )
        .unwrap();
        assert_eq!(
            mac_asset(&release, Arch::X86_64).as_deref(),
            Some("https://x/Tabby-1.0-macos-x64.dmg")
        );
        assert_eq!(
            mac_asset(&release, Arch::Arm64).as_deref(),
            Some("https://x/Tabby-1.0-macos-arm64.dmg")
        );
        let darwin: Value = serde_json::from_str(
            r#"{"assets":[{"browser_download_url":"https://x/tool-1.0-darwin-arm64.zip"}]}"#,
        )
        .unwrap();
        assert_eq!(
            mac_asset(&darwin, Arch::Arm64).as_deref(),
            Some("https://x/tool-1.0-darwin-arm64.zip")
        );
    }
}
