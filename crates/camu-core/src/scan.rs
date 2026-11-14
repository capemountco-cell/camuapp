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
    let apps: Vec<_> = found.into_values().filter(is_relevant).collect();
    let mut apps = merge_copies(apps, roots);
    apps.sort_by_key(|app| app.name.to_lowercase());
    apps
}

/// One entry per app: copies with the same bundle id (say /Applications/Rocket.app
/// and ~/Applications/Rocket.app) become one, listing the others in
/// `other_copies`. The copy kept is the newest version; on a tie, the one in
/// the first root (/Applications), then the one nested least deeply.
fn merge_copies(apps: Vec<InstalledApp>, roots: &[PathBuf]) -> Vec<InstalledApp> {
    // App paths are canonical (symlinks resolved), so compare with canonical roots.
    let roots: Vec<PathBuf> = roots
        .iter()
        .map(|r| fs::canonicalize(r).unwrap_or_else(|_| r.clone()))
        .collect();
    let rank = |app: &InstalledApp| {
        let root = roots
            .iter()
            .position(|r| app.path.starts_with(r))
            .unwrap_or(roots.len());
        (root, app.path.components().count())
    };
    let mut by_id: BTreeMap<String, Vec<InstalledApp>> = BTreeMap::new();
    let mut out = Vec::new();
    for app in apps {
        if app.bundle_id.is_empty() {
            out.push(app);
        } else {
            by_id.entry(app.bundle_id.clone()).or_default().push(app);
        }
    }
    for (_, mut copies) in by_id {
        copies.sort_by(|a, b| {
            crate::version::compare(&b.version, &a.version).then_with(|| rank(a).cmp(&rank(b)))
        });
        let mut keep = copies.remove(0);
        keep.other_copies = copies.into_iter().map(|c| c.path).collect();
        out.push(keep);
    }
    out
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

    #[test]
    fn copies_of_one_app_are_listed_once() {
        let base = std::env::temp_dir().join(format!("camu-copies-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let system = base.join("Applications");
        let user = base.join("home/Applications");
        make_app(&system, "ChatGPT.app", "com.openai.chat", "1.2025.1");
        make_app(&user, "ChatGPT.app", "com.openai.chat", "1.2025.1");
        make_app(&user.join("Old"), "Rocket.app", "com.acme.rocket", "2.0");
        make_app(&system, "Rocket.app", "com.acme.rocket", "1.0");

        let apps = scan(&[system.clone(), user.clone()]);
        assert_eq!(apps.len(), 2, "{apps:?}");
        let chat = apps
            .iter()
            .find(|a| a.bundle_id == "com.openai.chat")
            .unwrap();
        // Same version: the /Applications copy wins.
        assert!(chat.path.starts_with(&system));
        assert_eq!(chat.other_copies.len(), 1);
        assert!(chat.other_copies[0].starts_with(&user));
        // Newest version wins, wherever it is.
        let rocket = apps
            .iter()
            .find(|a| a.bundle_id == "com.acme.rocket")
            .unwrap();
        assert_eq!(rocket.version, "2.0");
        assert!(rocket.other_copies[0].starts_with(&system));
        let _ = fs::remove_dir_all(&base);
    }
}
