//! The two things Camu ever sends to camu.cc, both anonymous:
//!
//! - a wrong-version report, only when the user asks for one;
//! - "installed", once, the first time Camu opens, with only Camu's version.
//!
//! Neither carries anything that identifies the Mac or the person.

use serde::{Deserialize, Serialize};

use crate::fetch::Post;

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

/// Say "installed" once. Returns true when camu.cc got it, so it isn't sent again.
pub fn send_install(post: &dyn Post, camu: &str) -> bool {
    let body = serde_json::json!({ "event": "install", "camu": camu }).to_string();
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
        assert!(send_install(&sent, "0.3.1"));
        let (url, body) = sent.0.lock().unwrap()[0].clone();
        assert_eq!(url, PING_URL);
        assert_eq!(body, r#"{"camu":"0.3.1","event":"install"}"#);
    }
}
