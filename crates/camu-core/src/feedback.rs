//! The two things Camu ever sends to camu.cc, both anonymous:
//!
//! - a wrong-version report, only when the user asks for one;
//! - "installed", once, the first time Camu opens, with only Camu's version;
//! - "updated", once per update, when Camu opens on a newer version.
//!
//! Neither carries anything that identifies the Mac or the person.

use serde::{Deserialize, Serialize};

use crate::fetch::Post;
use crate::settings::State;
use crate::version;

pub const REPORT_URL: &str = "https://camu.cc/api/report";
pub const PING_URL: &str = "https://camu.cc/api/ping";

/// What the user saw when a version looked wrong.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WrongVersion {
    pub app: String,
    pub bundle_id: String,
    /// The installed version, with its build when it differs.
    pub installed: String,
    /// What Camu found, e.g. "26.1.1-57288 (Homebrew)".
    pub found: String,
    /// Where it came from, e.g. "homebrew".
    pub source: String,
    /// The feed or page it came from, when there is one.
    pub url: String,
    /// Everything else Camu saw, one source per line.
    pub note: String,
    /// Filled in by [`send_report`].
    pub camu: String,
    pub macos: String,
}

/// Send a report. `camu` and `macos` are filled in here.
pub fn send_report(
    post: &dyn Post,
    mut report: WrongVersion,
    camu: &str,
    macos: Option<String>,
) -> Result<(), String> {
    if report.app.trim().is_empty() {
        return Err("The report has no app".into());
    }
    report.camu = camu.to_string();
    report.macos = macos.unwrap_or_default();
    let body = serde_json::to_string(&report).map_err(|e| e.to_string())?;
    post.post_json(REPORT_URL, &body)
}

/// What to tell camu.cc when Camu opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaunchEvent {
    /// A new copy, opening for the first time.
    Install,
    /// Camu opened on a newer version than last time: an update finished.
    Update,
}

impl LaunchEvent {
    fn name(self) -> &'static str {
        match self {
            LaunchEvent::Install => "install",
            LaunchEvent::Update => "update",
        }
    }
}

/// Decide what to tell camu.cc as `current` opens, and do the bookkeeping
/// that doesn't depend on sending it. Call [`mark_sent`] once it was sent, so
/// an unsent event is tried again next time.
pub fn launch_event(state: &mut State, current: &str) -> Option<LaunchEvent> {
    // Copies in use before install counting existed aren't new installs.
    if !state.install_counted && state.last_check.is_some() {
        state.install_counted = true;
    }
    if !state.install_counted {
        // Remember the version now, so a failed install ping never turns into
        // an "update" later.
        state.last_version = Some(current.to_string());
        return Some(LaunchEvent::Install);
    }
    match state.last_version.as_deref() {
        Some(last) if last == current => None,
        // Opened on an older version (a rollback): nothing finished.
        Some(last) if !version::is_newer(current, last) => {
            state.last_version = Some(current.to_string());
            None
        }
        // Newer than last time, or a copy from before versions were kept.
        _ => Some(LaunchEvent::Update),
    }
}

/// Record that `event` reached camu.cc.
pub fn mark_sent(state: &mut State, event: LaunchEvent, current: &str) {
    if event == LaunchEvent::Install {
        state.install_counted = true;
    }
    state.last_version = Some(current.to_string());
}

/// Tell camu.cc about a launch event. Returns true when it got it.
pub fn send_event(post: &dyn Post, event: LaunchEvent, camu: &str) -> bool {
    let body = serde_json::json!({ "event": event.name(), "camu": camu }).to_string();
    post.post_json(PING_URL, &body).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Sent(Mutex<Vec<(String, String)>>);
    impl Post for Sent {
        fn post_json(&self, url: &str, body: &str) -> Result<(), String> {
            self.0.lock().unwrap().push((url.into(), body.into()));
            Ok(())
        }
    }

    #[test]
    fn report_carries_versions_and_nothing_else() {
        let sent = Sent::default();
        let report = WrongVersion {
            app: "Parallels Desktop".into(),
            bundle_id: "com.parallels.desktop.console".into(),
            installed: "26.1.1".into(),
            found: "26.1.1-57288".into(),
            source: "homebrew".into(),
            ..Default::default()
        };
        send_report(&sent, report, "0.3.1", Some("15.5".into())).unwrap();
        let (url, body) = sent.0.lock().unwrap()[0].clone();
        assert_eq!(url, REPORT_URL);
        let v: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["bundleId"], "com.parallels.desktop.console");
        assert_eq!(v["camu"], "0.3.1");
        assert_eq!(v["macos"], "15.5");
        let keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
        assert_eq!(
            keys.len(),
            9,
            "only the report's own fields are sent: {keys:?}"
        );
    }

    #[test]
    fn empty_report_is_refused() {
        let sent = Sent::default();
        assert!(send_report(&sent, WrongVersion::default(), "0.3.1", None).is_err());
        assert!(sent.0.lock().unwrap().is_empty());
    }

    #[test]
    fn install_ping_is_only_the_version() {
        let sent = Sent::default();
        assert!(send_event(&sent, LaunchEvent::Install, "0.3.1"));
        let (url, body) = sent.0.lock().unwrap()[0].clone();
        assert_eq!(url, PING_URL);
        assert_eq!(body, r#"{"camu":"0.3.1","event":"install"}"#);
    }

    #[test]
    fn launch_events() {
        // A brand-new copy: an install, and never an update afterwards even
        // if the install ping didn't get through.
        let mut fresh = State::default();
        assert_eq!(
            launch_event(&mut fresh, "0.3.2"),
            Some(LaunchEvent::Install)
        );
        fresh.last_check = Some(1);
        assert_eq!(launch_event(&mut fresh, "0.3.2"), None);

        // Sent: next launch on the same version says nothing.
        let mut state = State::default();
        let event = launch_event(&mut state, "0.3.2").unwrap();
        mark_sent(&mut state, event, "0.3.2");
        state.last_check = Some(1);
        assert_eq!(launch_event(&mut state, "0.3.2"), None);

        // An update finished; repeated until it's sent.
        assert_eq!(launch_event(&mut state, "0.3.3"), Some(LaunchEvent::Update));
        assert_eq!(launch_event(&mut state, "0.3.3"), Some(LaunchEvent::Update));
        mark_sent(&mut state, LaunchEvent::Update, "0.3.3");
        assert_eq!(launch_event(&mut state, "0.3.3"), None);

        // A rollback to an older version isn't an update.
        assert_eq!(launch_event(&mut state, "0.3.2"), None);
        assert_eq!(state.last_version.as_deref(), Some("0.3.2"));

        // A copy that was in use before versions were kept, opening on a new
        // version: an update, not an install.
        let mut old = State {
            last_check: Some(1),
            install_counted: true,
            ..Default::default()
        };
        assert_eq!(launch_event(&mut old, "0.3.2"), Some(LaunchEvent::Update));
        let mut older = State {
            last_check: Some(1),
            ..Default::default()
        };
        assert_eq!(launch_event(&mut older, "0.3.2"), Some(LaunchEvent::Update));
        assert!(older.install_counted);
    }
}
