//! Facts about the Mac Camu runs on. On other systems these return neutral
//! answers so the rest of the crate still builds and tests.

use std::process::Command;

use crate::check::Environment;
use crate::sources::homebrew;

fn run(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
}

/// The running macOS version, e.g. "15.1".
pub fn os_version() -> Option<String> {
    if cfg!(target_os = "macos") {
        run("/usr/bin/sw_vers", &["-productVersion"])
    } else {
        None
    }
}

/// App Store storefronts to try: the user's region, then the US store.
pub fn storefronts() -> Vec<String> {
    let locale = if cfg!(target_os = "macos") {
        run("/usr/bin/defaults", &["read", "-g", "AppleLocale"])
    } else {
        None
    };
    storefronts_for(locale.as_deref())
}

/// "en_GB" or "de_DE@currency=EUR" → ["gb", "us"].
pub fn storefronts_for(locale: Option<&str>) -> Vec<String> {
    let region = locale
        .and_then(|l| l.split('@').next())
        .and_then(|l| l.split(['_', '-']).nth(1))
        .filter(|r| r.len() == 2 && r.chars().all(|c| c.is_ascii_alphabetic()))
        .map(|r| r.to_ascii_lowercase());
    let mut out: Vec<String> = region.into_iter().collect();
    if !out.iter().any(|r| r == "us") {
        out.push("us".to_string());
    }
    out
}

/// Everything a [`crate::Checker`] needs to know about this Mac.
pub fn environment() -> Environment {
    Environment {
        os_version: os_version(),
        storefronts: storefronts(),
        installed_casks: homebrew::installed_casks(),
    }
}

#[cfg(test)]
mod tests {
    use super::storefronts_for;

    #[test]
    fn region_from_locale() {
        assert_eq!(storefronts_for(Some("en_GB")), ["gb", "us"]);
        assert_eq!(storefronts_for(Some("de_DE@currency=EUR")), ["de", "us"]);
        assert_eq!(storefronts_for(Some("en_US")), ["us"]);
        assert_eq!(storefronts_for(Some("en")), ["us"]);
        assert_eq!(storefronts_for(None), ["us"]);
    }
}
