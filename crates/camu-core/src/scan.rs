//! Finding the apps installed on this Mac.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::app::InstalledApp;

/// The folders Camu looks in: /Applications and ~/Applications.
pub fn default_roots() -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from("/Applications")];
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(Path::new(&home).join("Applications"));
    }
    roots
}

/// How deep to look inside folders that are not apps, e.g.
/// /Applications/Utilities or /Applications/Adobe Photoshop 2025.
const MAX_DEPTH: usize = 3;

/// Every app bundle under `roots`, sorted by name.
///
/// Apple's own system apps are left out: macOS updates those itself. Apple
/// apps bought from the App Store (Pages, Final Cut Pro, ...) are kept.
pub fn scan(roots: &[PathBuf]) -> Vec<InstalledApp> {
    let mut found = BTreeMap::new();
    for root in roots {
        walk(root, 0, &mut found);
    }
    let mut apps: Vec<_> = found.into_values().filter(is_relevant).collect();
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

fn walk(dir: &Path, depth: usize, found: &mut BTreeMap<PathBuf, InstalledApp>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Follow symlinks to apps, but not symlinked folders: they loop.
        let is_dir = path.is_dir();
        if !is_dir {
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "app") {
            let real = fs::canonicalize(&path).unwrap_or(path.clone());
            if let Some(app) = InstalledApp::read(&real) {
                found.entry(real).or_insert(app);
            }
        } else if depth + 1 < MAX_DEPTH
            && !entry.file_type().is_ok_and(|t| t.is_symlink())
            && !entry.file_name().to_string_lossy().starts_with('.')
        {
            walk(&path, depth + 1, found);
        }
    }
}

fn is_relevant(app: &InstalledApp) -> bool {
    !(app.bundle_id.starts_with("com.apple.") && !app.from_app_store)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_app(dir: &Path, file: &str, id: &str, version: &str) {
        let contents = dir.join(file).join("Contents");
        fs::create_dir_all(&contents).unwrap();
        fs::write(
            contents.join("Info.plist"),
            format!(
                r#"<?xml version="1.0" encoding="UTF-8"?><plist version="1.0"><dict>
<key>CFBundleIdentifier</key><string>{id}</string>
<key>CFBundleShortVersionString</key><string>{version}</string>
</dict></plist>"#
            ),
        )
        .unwrap();
    }

    #[test]
    fn finds_apps_in_subfolders_and_skips_system_apps() {
        let root = std::env::temp_dir().join(format!("camu-scan-{}", std::process::id()));
        make_app(&root, "Zebra.app", "com.z.zebra", "1.0");
        make_app(&root, "Utilities/Alpha.app", "com.a.alpha", "2.0");
        make_app(&root, "Safari.app", "com.apple.Safari", "18.0");
        // Apps inside an app are its helpers, not separate apps.
        make_app(
            &root,
            "Zebra.app/Contents/Helper.app",
            "com.z.helper",
            "1.0",
        );
        fs::create_dir_all(root.join("NotAnApp.app")).unwrap();

        let apps = scan(std::slice::from_ref(&root));
        let names: Vec<_> = apps.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["Alpha", "Zebra"]);

        fs::remove_dir_all(&root).unwrap();
    }
}
