//! Keeping Camu's backups in check: how much space they use, and removing
//! old ones, ones of apps since deleted, and the oldest when over a cap.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::update::{list_backups, Backup};

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Bytes used by every backup (an upper bound: APFS clones share space
    /// with the installed app until it changes).
    pub total_bytes: u64,
    /// Bytes used by backups of apps that are no longer installed.
    pub deleted_apps_bytes: u64,
    pub count: usize,
    pub folder: PathBuf,
}

/// The size of everything under `path`, not following links.
pub fn dir_size(path: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(path) else {
        return 0;
    };
    if !meta.is_dir() {
        return meta.len();
    }
    fs::read_dir(path)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| dir_size(&e.path()))
        .sum()
}

/// The folder holding one backup (`<root>/<bundle id>/<version>`).
fn folder(b: &Backup) -> Option<&Path> {
    b.path.parent()
}

pub fn summary(root: &Path, installed: &HashSet<String>) -> Summary {
    let mut out = Summary {
        folder: root.to_path_buf(),
        ..Default::default()
    };
    for b in list_backups(root) {
        let size = folder(&b).map(dir_size).unwrap_or(0);
        out.total_bytes += size;
        out.count += 1;
        if !installed.contains(&b.bundle_id) {
            out.deleted_apps_bytes += size;
        }
    }
    out
}

fn remove(b: &Backup) {
    if let Some(dir) = folder(b) {
        let _ = fs::remove_dir_all(dir);
        // Drop the app's folder when it's now empty.
        if let Some(app_dir) = dir.parent() {
            let _ = fs::remove_dir(app_dir);
        }
    }
}

/// What a prune would remove: backups older than `days`, backups of apps
/// not in `installed` (when given), then the oldest until the rest fit in
/// `max_bytes`. `days` 0 or `max_bytes` 0 turn that rule off.
pub fn to_prune(
    backups: &[(Backup, u64)],
    days: u32,
    max_bytes: u64,
    installed: Option<&HashSet<String>>,
    now: u64,
) -> Vec<usize> {
    let mut gone = Vec::new();
    for (i, (b, _)) in backups.iter().enumerate() {
        let old = days > 0 && now.saturating_sub(b.made) > u64::from(days) * 86_400;
        let orphan = installed.is_some_and(|ids| !ids.contains(&b.bundle_id));
        if old || orphan {
            gone.push(i);
        }
    }
    if max_bytes > 0 {
        let mut rest: Vec<usize> = (0..backups.len()).filter(|i| !gone.contains(i)).collect();
        rest.sort_by_key(|&i| backups[i].0.made);
        let mut used: u64 = rest.iter().map(|&i| backups[i].1).sum();
        for i in rest {
            if used <= max_bytes {
                break;
            }
            used -= backups[i].1;
            gone.push(i);
        }
    }
    gone.sort_unstable();
    gone
}

/// Apply the retention rules. Returns how many backups were removed.
pub fn prune(
    root: &Path,
    days: u32,
    max_gb: u32,
    installed: Option<&HashSet<String>>,
    now: u64,
) -> usize {
    let backups: Vec<(Backup, u64)> = list_backups(root)
        .into_iter()
        .map(|b| {
            let size = folder(&b).map(dir_size).unwrap_or(0);
            (b, size)
        })
        .collect();
    let gone = to_prune(
        &backups,
        days,
        u64::from(max_gb) * 1_000_000_000,
        installed,
        now,
    );
    for &i in &gone {
        remove(&backups[i].0);
    }
    gone.len()
}

/// Remove every backup, or only those of apps no longer installed.
pub fn delete(root: &Path, only_deleted_apps: Option<&HashSet<String>>) -> usize {
    let mut n = 0;
    for b in list_backups(root) {
        if only_deleted_apps.is_none_or(|ids| !ids.contains(&b.bundle_id)) {
            remove(&b);
            n += 1;
        }
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(id: &str, made: u64) -> Backup {
        Backup {
            bundle_id: id.into(),
            version: "1.0".into(),
            path: PathBuf::from(format!("/b/{id}/1.0/X.app")),
            newest: true,
            made,
        }
    }

    #[test]
    fn prunes_old_orphaned_and_over_the_cap() {
        let day = 86_400;
        let now = 100 * day;
        let list = vec![
            (b("old", now - 40 * day), 10),
            (b("gone", now - day), 10),
            (b("a", now - 3 * day), 600),
            (b("b", now - 2 * day), 600),
        ];
        let installed: HashSet<String> = ["old", "a", "b"].map(String::from).into();
        // 30 days, 1000 bytes max: "old" is too old, "gone" was deleted,
        // and of a+b (1200) the older "a" goes to fit.
        assert_eq!(
            to_prune(&list, 30, 1000, Some(&installed), now),
            vec![0, 1, 2]
        );
        // No rules: nothing goes.
        assert!(to_prune(&list, 0, 0, None, now).is_empty());
    }

    #[test]
    fn sizes_and_deletes_on_disk() {
        let root = std::env::temp_dir().join(format!("camu-bk-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for id in ["com.a", "com.gone"] {
            let app = root.join(id).join("1.0").join("X.app/Contents");
            fs::create_dir_all(&app).unwrap();
            fs::write(app.join("data"), vec![0u8; 100]).unwrap();
        }
        let installed: HashSet<String> = ["com.a".to_string()].into();
        let s = summary(&root, &installed);
        assert_eq!(
            (s.count, s.total_bytes, s.deleted_apps_bytes),
            (2, 200, 100)
        );
        assert_eq!(delete(&root, Some(&installed)), 1);
        assert_eq!(summary(&root, &installed).count, 1);
        assert_eq!(delete(&root, None), 1);
        assert_eq!(summary(&root, &installed).count, 0);
        let _ = fs::remove_dir_all(&root);
    }
}
