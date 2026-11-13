//! A log of what Camu changed: updates, failures, hand-offs and rollbacks,
//! kept for 30 days in `history.json` beside the settings.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// How long entries are kept, and how many at most.
pub const KEEP_SECS: u64 = 30 * 24 * 3600;
pub const KEEP_MAX: usize = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum How {
    /// A scheduled automatic install.
    Automatic,
    /// Update or Update All.
    Manual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Outcome {
    Updated,
    Failed,
    /// Camu opened the App Store, the app, or its website.
    HandedOff,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    /// Seconds since 1970.
    pub at: u64,
    pub id: String,
    pub bundle_id: String,
    pub name: String,
    /// The version before.
    pub from: String,
    /// The version after (or aimed for, when it failed).
    pub to: Option<String>,
    /// Where the new version came from, e.g. "homebrew".
    pub source: Option<String>,
    pub how: How,
    pub outcome: Outcome,
    /// Why it failed, or what to do next.
    pub message: Option<String>,
}

fn file(dir: &Path) -> std::path::PathBuf {
    dir.join("history.json")
}

/// Every kept entry, newest first.
pub fn load(dir: &Path) -> Vec<Entry> {
    fs::read_to_string(file(dir))
        .ok()
        .and_then(|text| serde_json::from_str::<Vec<Entry>>(&text).ok())
        .unwrap_or_default()
}

/// Entries newer than 30 days, newest first, at most [`KEEP_MAX`].
pub fn trimmed(mut entries: Vec<Entry>, now: u64) -> Vec<Entry> {
    entries.retain(|e| now.saturating_sub(e.at) <= KEEP_SECS);
    entries.sort_by_key(|e| std::cmp::Reverse(e.at));
    entries.truncate(KEEP_MAX);
    entries
}

/// Add entries and save.
pub fn append(dir: &Path, new: Vec<Entry>, now: u64) -> Result<(), String> {
    if new.is_empty() {
        return Ok(());
    }
    let mut all = load(dir);
    all.extend(new);
    let all = trimmed(all, now);
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let tmp = file(dir).with_extension("json.tmp");
    let json = serde_json::to_string(&all).map_err(|e| e.to_string())?;
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, file(dir)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(at: u64) -> Entry {
        Entry {
            at,
            id: "/Applications/Rocket.app".into(),
            bundle_id: "com.acme.rocket".into(),
            name: "Rocket".into(),
            from: "1.0".into(),
            to: Some("1.1".into()),
            source: Some("sparkle".into()),
            how: How::Manual,
            outcome: Outcome::Updated,
            message: None,
        }
    }

    #[test]
    fn keeps_thirty_days_newest_first() {
        let now = 100 * 24 * 3600;
        let kept = trimmed(
            vec![entry(now - KEEP_SECS - 1), entry(now - 10), entry(now)],
            now,
        );
        assert_eq!(
            kept.iter().map(|e| e.at).collect::<Vec<_>>(),
            vec![now, now - 10]
        );
    }

    #[test]
    fn appends_to_disk() {
        let dir = std::env::temp_dir().join(format!("camu-history-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        append(&dir, vec![entry(5)], 10).unwrap();
        append(&dir, vec![entry(7)], 10).unwrap();
        assert_eq!(load(&dir).len(), 2);
        assert_eq!(load(&dir)[0].at, 7);
        let _ = fs::remove_dir_all(&dir);
    }
}
