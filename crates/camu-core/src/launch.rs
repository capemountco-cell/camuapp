//! Handing things to macOS: opening web pages, App Store pages and apps.

use std::path::Path;
use std::process::Command;

/// Only these kinds of address are ever opened. Anything else could come from
/// a hostile feed and launch something unexpected.
pub fn is_openable_url(url: &str) -> bool {
    ["https://", "http://", "macappstore://"]
        .iter()
        .any(|scheme| url.starts_with(scheme))
        && !url.chars().any(char::is_whitespace)
}

fn open(args: &[&std::ffi::OsStr]) -> Result<(), String> {
    if !cfg!(target_os = "macos") {
        return Err("opening things only works on macOS".into());
    }
    let status = Command::new("/usr/bin/open")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("open exited with {status}"))
    }
}

/// Open a web or App Store page.
pub fn open_url(url: &str) -> Result<(), String> {
    if !is_openable_url(url) {
        return Err(format!("refusing to open {url}"));
    }
    open(&[url.as_ref()])
}

/// Launch an app, so its own updater can run.
pub fn open_app(path: &Path) -> Result<(), String> {
    if path.extension().is_none_or(|e| e != "app") || !path.is_dir() {
        return Err(format!("{} is not an app", path.display()));
    }
    open(&[path.as_os_str()])
}

/// Show an app in the Finder.
pub fn reveal(path: &Path) -> Result<(), String> {
    open(&["-R".as_ref(), path.as_os_str()])
}

#[cfg(test)]
mod tests {
    use super::is_openable_url;

    #[test]
    fn only_web_and_store_addresses() {
        assert!(is_openable_url("https://acme.com/notes"));
        assert!(is_openable_url("macappstore://apps.apple.com/app/id1"));
        assert!(!is_openable_url("file:///Applications/Calculator.app"));
        assert!(!is_openable_url("javascript:alert(1)"));
        assert!(!is_openable_url("https://acme.com/a b"));
    }
}
