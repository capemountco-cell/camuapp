//! The Mac App Store, through Apple's public lookup API.
//!
//! An app is only in some storefronts, and a lookup in the wrong one returns
//! nothing, which is why other tools "miss App Store updates". Camu asks the
//! user's own storefront first and falls back to others.

use serde_json::Value;

use super::{encode, Finding, Source};
use crate::app::InstalledApp;
use crate::fetch::Fetch;
use crate::{notes, version};

pub fn lookup_url(bundle_id: &str, country: &str) -> String {
    format!(
        "https://itunes.apple.com/lookup?bundleId={}&country={}&entity=desktopSoftware",
        encode(bundle_id),
        encode(country)
    )
}

/// Read a lookup response. `Ok(None)` means the store has no such app.
pub fn parse(json: &str, app: &InstalledApp) -> Result<Option<Finding>, String> {
    let body: Value =
        serde_json::from_str(json).map_err(|e| format!("App Store reply is not JSON: {e}"))?;
    let Some(result) = body["results"].as_array().and_then(|r| {
        r.iter()
            .find(|r| r["bundleId"].as_str() == Some(app.bundle_id.as_str()))
    }) else {
        return Ok(None);
    };
    let Some(remote) = result["version"].as_str() else {
        return Ok(None);
    };
    let text = |key: &str| result[key].as_str().map(str::to_string);

    let mut f = Finding::new(Source::AppStore, remote);
    f.is_update = version::is_update(remote, None, &app.version, app.build.as_deref());
    f.release_notes = text("releaseNotes")
        .map(|n| notes::to_text(&n))
        .filter(|n| !n.is_empty());
    // Open the store app directly rather than a web page.
    f.page_url = result["trackId"]
        .as_u64()
        .map(|id| format!("macappstore://apps.apple.com/app/id{id}"))
        .or_else(|| text("trackViewUrl"));
    f.minimum_os = text("minimumOsVersion");
    f.date = text("currentVersionReleaseDate");
    Ok(Some(f))
}

/// Ask each storefront in turn until one knows the app.
pub fn check(
    app: &InstalledApp,
    countries: &[String],
    fetch: &dyn Fetch,
) -> Result<Option<Finding>, String> {
    let mut last_error = None;
    for country in countries {
        match fetch.get(&lookup_url(&app.bundle_id, country)) {
            Ok(body) => match parse(&body, app) {
                Ok(Some(found)) => return Ok(Some(found)),
                Ok(None) => {}
                Err(e) => last_error = Some(e),
            },
            Err(e) => last_error = Some(e),
        }
    }
    match last_error {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::fake::FakeFetch;

    fn app() -> InstalledApp {
        InstalledApp {
            path: "/Applications/Pages.app".into(),
            file_name: "Pages".into(),
            name: "Pages".into(),
            bundle_id: "com.apple.iWork.Pages".into(),
            version: "14.1".into(),
            build: None,
            sparkle_feed: None,
            electron_feed: None,
            from_app_store: true,
            minimum_os: None,
            icon_file: None,
        }
    }

    const FOUND: &str = r#"{"resultCount":1,"results":[{"bundleId":"com.apple.iWork.Pages","version":"14.2",
        "trackId":409201541,"releaseNotes":"Bug fixes.","minimumOsVersion":"14.0",
        "currentVersionReleaseDate":"2026-10-01T07:00:00Z"}]}"#;

    #[test]
    fn falls_back_to_another_storefront() {
        let fetch = FakeFetch::default()
            .with(
                &lookup_url("com.apple.iWork.Pages", "de"),
                r#"{"resultCount":0,"results":[]}"#,
            )
            .with(&lookup_url("com.apple.iWork.Pages", "us"), FOUND);
        let found = check(&app(), &["de".into(), "us".into()], &fetch)
            .unwrap()
            .unwrap();
        assert!(found.is_update);
        assert_eq!(found.version, "14.2");
        assert_eq!(
            found.page_url.as_deref(),
            Some("macappstore://apps.apple.com/app/id409201541")
        );
        assert_eq!(fetch.asked.lock().unwrap().len(), 2);
    }

    #[test]
    fn ignores_results_for_other_apps() {
        let other = FOUND.replace("com.apple.iWork.Pages", "com.other.app");
        assert_eq!(parse(&other, &app()).unwrap(), None);
    }
}
