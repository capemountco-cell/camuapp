//! Checking apps for updates: ask every source that knows an app, then decide
//! what to believe.
//!
//! Sources are asked in order of trust, and the most trusted one that answers
//! decides:
//!
//! 1. The app's own channel: its Sparkle appcast, its Electron feed or, for
//!    App Store apps, the App Store. These are what the app's own updater
//!    sees.
//! 2. GitHub releases, when the app is published there.
//! 3. Homebrew's catalogue, which covers the most apps but can lag.
//!
//! Every answer is kept in the report, so the UI can show where a version
//! came from and where sources disagreed.

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;

use crate::app::InstalledApp;
use crate::arch::Arch;
use crate::fetch::{CachingFetch, Fetch};
use crate::sources::homebrew::{self, CaskIndex};
use crate::sources::{appstore, electron, github, sparkle, Finding, Source};
use crate::version;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    UpdateAvailable,
    UpToDate,
    /// No source knows this app.
    Unsupported,
    /// Every source that should know the app failed to answer.
    Failed,
}

/// How an update would be installed. Decided here, carried out elsewhere.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Install {
    /// `brew upgrade --cask <cask>`: the app was installed with Homebrew.
    Homebrew {
        cask: String,
    },
    /// Open the App Store page; the store installs it.
    AppStore {
        url: String,
    },
    /// Download, verify the signature, back up the old app, replace.
    Download {
        url: String,
    },
    /// Open the app so its own updater can take over.
    OpenApp,
    /// Only a web page is known.
    Website {
        url: String,
    },
    /// The developer's own updater handles it (Adobe Creative Cloud,
    /// Microsoft AutoUpdate): open that.
    Vendor {
        name: String,
        path: std::path::PathBuf,
    },
    None,
}

/// Something the user should know about an app before updating it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Notice {
    /// The new version needs a newer macOS than this Mac runs.
    NeedsMacOs { version: String },
    /// Updated through the developer's own updater.
    Vendor { name: String },
    /// Setapp keeps it up to date; Camu leaves it alone.
    Setapp,
}

/// Why an available update is being held back, by the user's choice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Hold {
    /// "Skip this version", or rolled back from it.
    Skipped { version: String },
    /// "Remind me later", until this time (seconds since 1970).
    Snoozed { until: u64 },
    /// "Stay on version N.x".
    Pinned { major: u64 },
}

/// Per-app choices that change what a check looks for.
#[derive(Debug, Clone, Default)]
pub struct Prefs {
    /// Accept pre-release channels.
    pub include_beta: bool,
    /// "Stay on version N.x".
    pub max_major: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceError {
    pub source: Source,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    pub id: String,
    pub app: InstalledApp,
    pub status: Status,
    /// The finding that decided the status.
    pub best: Option<Finding>,
    /// Every source's answer, most trusted first.
    pub findings: Vec<Finding>,
    pub errors: Vec<SourceError>,
    pub install: Install,
    /// The cask this app was installed from, if it came from Homebrew.
    pub homebrew_cask: Option<String>,
    /// The update moves to a new major version.
    pub major: bool,
    pub notice: Option<Notice>,
    /// Set by the user's choices (see `settings::apply_choices`).
    pub hold: Option<Hold>,
}

/// Facts about this Mac that checks depend on.
#[derive(Debug, Clone, Default)]
pub struct Environment {
    /// e.g. "15.1". `None` accepts updates for any macOS.
    pub os_version: Option<String>,
    /// App Store storefronts to try, in order, e.g. ["de", "us"].
    pub storefronts: Vec<String>,
    /// Casks installed with Homebrew.
    pub installed_casks: HashSet<String>,
    /// This Mac's chip: downloads are chosen for it.
    pub arch: Arch,
}

/// The Homebrew catalogue is ~15 MB. Download it at most this often.
const CASK_INDEX_TTL: Duration = Duration::from_secs(60 * 60);

struct CachedIndex {
    fetched: Instant,
    index: Result<Arc<CaskIndex>, String>,
}

pub struct Checker {
    fetch: Box<dyn Fetch>,
    env: Mutex<Arc<Environment>>,
    casks: Mutex<Option<CachedIndex>>,
}

impl Checker {
    pub fn new(fetch: Box<dyn Fetch>, env: Environment) -> Self {
        Self {
            // GitHub allows 60 unauthenticated requests an hour and the App
            // Store throttles bursts: their answers are kept for an hour.
            fetch: Box::new(CachingFetch::new(fetch, Duration::from_secs(60 * 60))),
            env: Mutex::new(Arc::new(env)),
            casks: Mutex::new(None),
        }
    }

    /// Start a fresh round of checks with new facts about the Mac (a cask
    /// may have been installed since). The catalogue stays cached for its
    /// hour: re-downloading 15 MB on every check gains nothing.
    pub fn refresh(&self, env: Environment) {
        *self.env.lock().unwrap() = Arc::new(env);
    }

    fn cask_index(&self) -> Result<Arc<CaskIndex>, String> {
        let mut cache = self.casks.lock().unwrap();
        if let Some(cached) = cache.as_ref() {
            if cached.fetched.elapsed() < CASK_INDEX_TTL {
                return cached.index.clone();
            }
        }
        let index = self
            .fetch
            .get(homebrew::INDEX_URL)
            .and_then(|body| CaskIndex::parse(&body))
            .map(Arc::new);
        *cache = Some(CachedIndex {
            fetched: Instant::now(),
            index: index.clone(),
        });
        index
    }

    /// Check one app. `include_beta` opts into pre-release channels.
    pub fn check(&self, app: &InstalledApp, include_beta: bool) -> Report {
        self.check_with(
            app,
            &Prefs {
                include_beta,
                ..Default::default()
            },
        )
    }

    /// Check one app with the user's choices for it.
    pub fn check_with(&self, app: &InstalledApp, prefs: &Prefs) -> Report {
        let env = self.env.lock().unwrap().clone();
        if app.setapp {
            let mut report = decide(app, vec![], vec![], None);
            report.notice = Some(Notice::Setapp);
            return report;
        }
        let include_beta = prefs.include_beta;
        let mut findings = Vec::new();
        let mut errors = Vec::new();
        fn record(
            findings: &mut Vec<Finding>,
            errors: &mut Vec<SourceError>,
            source: Source,
            result: Result<Option<Finding>, String>,
        ) {
            match result {
                Ok(Some(f)) => findings.push(f),
                Ok(None) => {}
                Err(message) => errors.push(SourceError { source, message }),
            }
        }
        macro_rules! note {
            ($source:expr, $result:expr $(,)?) => {
                record(&mut findings, &mut errors, $source, $result)
            };
        }

        let mut homebrew_cask = None;

        if app.from_app_store {
            // A store app can only be updated by the store, whatever other
            // sources say.
            note!(
                Source::AppStore,
                appstore::check(app, &env.storefronts, self.fetch.as_ref()),
            );
        } else {
            if let Some(feed) = &app.sparkle_feed {
                let filter = sparkle::Filter {
                    os_version: env.os_version.as_deref(),
                    include_beta,
                    max_major: prefs.max_major,
                };
                let result = self
                    .fetch
                    .get(feed)
                    .and_then(|xml| sparkle::parse(&xml))
                    .map(|items| {
                        sparkle::newest(&items, &filter).and_then(|i| sparkle::finding(app, i))
                    });
                note!(Source::Sparkle, result);
            }
            if let Some(feed) = &app.electron_feed {
                note!(
                    Source::Electron,
                    electron::check(app, feed, self.fetch.as_ref())
                );
            }

            let cask = match self.cask_index() {
                Ok(index) => index.find(app, &env.installed_casks).cloned(),
                Err(message) => {
                    errors.push(SourceError {
                        source: Source::Homebrew,
                        message,
                    });
                    None
                }
            };
            if let Some(cask) = &cask {
                if env.installed_casks.contains(&cask.token) {
                    homebrew_cask = Some(cask.token.clone());
                }
                note!(
                    Source::Homebrew,
                    Ok(homebrew::finding_for(app, cask, env.arch))
                );
            }

            // GitHub corrects a lagging Homebrew catalogue. Only ask it when
            // the app has no feed of its own: unauthenticated GitHub requests
            // are limited to 60 an hour.
            let has_own_feed = findings
                .iter()
                .any(|f| matches!(f.source, Source::Sparkle | Source::Electron));
            if !has_own_feed {
                let repo = cask
                    .as_ref()
                    .and_then(|c| c.url.as_deref())
                    .and_then(github::repo_from_url)
                    .or_else(|| app.sparkle_feed.as_deref().and_then(github::repo_from_url));
                if let Some((owner, repo)) = repo {
                    note!(
                        Source::Github,
                        github::check(app, &owner, &repo, self.fetch.as_ref(), env.arch)
                    );
                }
            }
        }

        let mut report = decide(app, findings, errors, homebrew_cask);
        if report.status == Status::UpdateAvailable {
            let needs = report.best.as_ref().and_then(|b| b.minimum_os.clone());
            if let (Some(needed), Some(running)) = (needs, env.os_version.as_deref()) {
                if version::compare(&needed, running) == std::cmp::Ordering::Greater {
                    report.notice = Some(Notice::NeedsMacOs { version: needed });
                    report.install = Install::None;
                }
            }
            if report.notice.is_none() && !app.from_app_store {
                if let Some((name, path)) = vendor_updater(app) {
                    report.notice = Some(Notice::Vendor { name: name.clone() });
                    report.install = Install::Vendor { name, path };
                }
            }
        }
        report
    }

    /// Check many apps at once, `workers` at a time, calling `on_report` as
    /// each finishes. Reports arrive in completion order.
    pub fn check_all(
        &self,
        apps: &[InstalledApp],
        prefs: &(dyn Fn(&InstalledApp) -> Prefs + Sync),
        workers: usize,
        on_report: &(dyn Fn(Report) + Sync),
    ) {
        // Load the catalogue once up front rather than racing to load it.
        let _ = self.cask_index();
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..workers.max(1) {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(app) = apps.get(i) else { break };
                    on_report(self.check_with(app, &prefs(app)));
                });
            }
        });
    }
}

/// Decide what to believe, and how the update would be installed.
pub fn decide(
    app: &InstalledApp,
    mut findings: Vec<Finding>,
    errors: Vec<SourceError>,
    homebrew_cask: Option<String>,
) -> Report {
    findings.sort_by_key(|f| f.source.trust());

    let tier = |s: Source| match s {
        Source::Sparkle | Source::Electron | Source::AppStore => 0,
        Source::Github => 1,
        Source::Homebrew => 2,
    };
    // The most trusted tier that answered decides; within it, the newest.
    let best = findings
        .iter()
        .map(|f| tier(f.source))
        .min()
        .and_then(|top| {
            findings
                .iter()
                .filter(|f| tier(f.source) == top)
                .max_by(|a, b| {
                    a.is_update
                        .cmp(&b.is_update)
                        .then_with(|| version::compare(&a.version, &b.version))
                        .then_with(|| b.source.trust().cmp(&a.source.trust()))
                })
        })
        .cloned();

    let status = match &best {
        Some(f) if f.is_update => Status::UpdateAvailable,
        Some(_) => Status::UpToDate,
        None if errors.is_empty() => Status::Unsupported,
        None => Status::Failed,
    };

    let install = match &best {
        _ if app.from_app_store => best
            .as_ref()
            .and_then(|f| f.page_url.clone())
            .map(|url| Install::AppStore { url })
            .unwrap_or(Install::None),
        None => Install::None,
        // `brew upgrade` only helps when Homebrew already has the version the
        // best source found; otherwise it does nothing, every time.
        Some(f)
            if homebrew_cask.is_some()
                && findings.iter().any(|h| {
                    h.source == Source::Homebrew
                        && version::compare(&h.version, &f.version) != std::cmp::Ordering::Less
                }) =>
        {
            Install::Homebrew {
                cask: homebrew_cask.clone().unwrap(),
            }
        }
        // A Sparkle feed over plain http can be tampered with on the way, so
        // its download is only trusted with the developer's signature.
        Some(f)
            if f.source == Source::Sparkle
                && app
                    .sparkle_feed
                    .as_deref()
                    .is_some_and(|u| u.starts_with("http://"))
                && (f.signature.is_none() || app.sparkle_key.is_none()) =>
        {
            Install::OpenApp
        }
        Some(f) => match (&f.download_url, f.source) {
            (Some(url), Source::Sparkle | Source::Electron | Source::Github) => {
                Install::Download { url: url.clone() }
            }
            _ if app.sparkle_feed.is_some() || app.electron_feed.is_some() => Install::OpenApp,
            (Some(url), Source::Homebrew) => Install::Download { url: url.clone() },
            _ => f
                .page_url
                .clone()
                .map(|url| Install::Website { url })
                .unwrap_or(Install::OpenApp),
        },
    };

    let major = status == Status::UpdateAvailable
        && best
            .as_ref()
            .is_some_and(|b| version::is_major_jump(&b.version, &app.version));
    Report {
        id: app.id(),
        app: app.clone(),
        status,
        best,
        findings,
        errors,
        major,
        install,
        homebrew_cask,
        notice: None,
        hold: None,
    }
}

/// The developer's own updater for an app, when it has one installed:
/// (its name, where it is).
pub fn vendor_updater(app: &InstalledApp) -> Option<(String, std::path::PathBuf)> {
    let id = app.bundle_id.as_str();
    let (name, paths): (&str, &[&str]) =
        if id.starts_with("com.adobe.") && !id.starts_with("com.adobe.acc") {
            (
                "Adobe Creative Cloud",
                &["/Applications/Utilities/Adobe Creative Cloud/ACC/Creative Cloud.app"],
            )
        } else if id.starts_with("com.microsoft.") && !app.from_app_store {
            (
                "Microsoft AutoUpdate",
                &["/Library/Application Support/Microsoft/MAU2.0/Microsoft AutoUpdate.app"],
            )
        } else {
            return None;
        };
    paths
        .iter()
        .map(std::path::PathBuf::from)
        .find(|p| p.exists())
        .map(|p| (name.to_string(), p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::fake::FakeFetch;

    fn app(file: &str, version: &str) -> InstalledApp {
        InstalledApp {
            path: format!("/Applications/{file}.app").into(),
            file_name: file.into(),
            name: file.into(),
            bundle_id: format!("com.test.{}", file.to_lowercase()),
            version: version.into(),
            build: None,
            sparkle_feed: None,
            electron_feed: None,
            from_app_store: false,
            minimum_os: None,
            icon_file: None,
            ..Default::default()
        }
    }

    const CASKS: &str = r#"[
      {"token":"rocket","version":"3.0.0","homepage":"https://acme.com","url":"https://acme.com/Rocket.dmg","artifacts":[{"app":["Rocket.app"]}]},
      {"token":"comet","version":"1.1.0","url":"https://github.com/acme/comet/releases/download/v1.1.0/Comet.dmg","artifacts":[{"app":["Comet.app"]}]},
      {"token":"plain","version":"5.0","homepage":"https://plain.app","url":"https://plain.app/Plain.zip","artifacts":[{"app":["Plain.app"]}]}
    ]"#;

    const APPCAST: &str = r#"<rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"><channel>
      <item><enclosure url="https://acme.com/r.zip" sparkle:version="2.4.1" sparkle:shortVersionString="2.4.1"/></item>
    </channel></rss>"#;

    fn checker(fetch: FakeFetch, installed: &[&str]) -> Checker {
        Checker::new(
            Box::new(fetch),
            Environment {
                os_version: Some("15.1".into()),
                storefronts: vec!["us".into()],
                installed_casks: installed.iter().map(|s| s.to_string()).collect(),
                arch: Arch::Arm64,
            },
        )
    }

    #[test]
    fn own_feed_outranks_homebrew() {
        // Homebrew claims 3.0.0, but the app's own appcast says 2.4.1 is the
        // newest: trust the app.
        let mut rocket = app("Rocket", "2.4.1");
        rocket.sparkle_feed = Some("https://acme.com/appcast.xml".into());
        let fetch = FakeFetch::default()
            .with(homebrew::INDEX_URL, CASKS)
            .with("https://acme.com/appcast.xml", APPCAST);
        let report = checker(fetch, &[]).check(&rocket, false);
        assert_eq!(report.status, Status::UpToDate);
        assert_eq!(report.best.unwrap().source, Source::Sparkle);
        assert_eq!(
            report.findings.len(),
            2,
            "the Homebrew answer is still shown"
        );
    }

    #[test]
    fn github_corrects_a_lagging_catalogue() {
        let comet = app("Comet", "1.0.0");
        let fetch = FakeFetch::default()
            .with(homebrew::INDEX_URL, CASKS)
            .with(
                &github::latest_url("acme", "comet"),
                r#"{"tag_name":"v1.2.0","body":"New stuff","html_url":"https://github.com/acme/comet/releases/tag/v1.2.0",
                    "assets":[{"browser_download_url":"https://github.com/acme/comet/releases/download/v1.2.0/Comet-1.2.0.dmg"}]}"#,
            );
        let report = checker(fetch, &["comet"]).check(&comet, false);
        assert_eq!(report.status, Status::UpdateAvailable);
        let best = report.best.unwrap();
        assert_eq!(
            (best.source, best.version.as_str()),
            (Source::Github, "1.2.0")
        );
        // Installed with brew, but brew doesn't have 1.2.0 yet: `brew
        // upgrade` would do nothing, so the release is downloaded instead.
        assert_eq!(
            report.install,
            Install::Download {
                url: "https://github.com/acme/comet/releases/download/v1.2.0/Comet-1.2.0.dmg"
                    .into()
            }
        );
    }

    #[test]
    fn homebrew_covers_apps_without_feeds() {
        let plain = app("Plain", "4.2");
        let fetch = FakeFetch::default().with(homebrew::INDEX_URL, CASKS);
        let report = checker(fetch, &[]).check(&plain, false);
        assert_eq!(report.status, Status::UpdateAvailable);
        assert_eq!(
            report.install,
            Install::Download {
                url: "https://plain.app/Plain.zip".into()
            }
        );
    }

    #[test]
    fn unknown_apps_are_unsupported_and_dead_feeds_fail() {
        let fetch = FakeFetch::default().with(homebrew::INDEX_URL, CASKS);
        let c = checker(fetch, &[]);
        assert_eq!(
            c.check(&app("Mystery", "1.0"), false).status,
            Status::Unsupported
        );

        let mut dead = app("Dead", "1.0");
        dead.sparkle_feed = Some("https://gone.example/appcast.xml".into());
        let report = c.check(&dead, false);
        assert_eq!(report.status, Status::Failed);
        assert_eq!(report.errors[0].source, Source::Sparkle);
    }

    #[test]
    fn store_apps_only_ask_the_store() {
        let mut pages = app("Pages", "14.1");
        pages.from_app_store = true;
        pages.bundle_id = "com.apple.iWork.Pages".into();
        let fetch = FakeFetch::default().with(
            &appstore::lookup_url("com.apple.iWork.Pages", "us"),
            r#"{"results":[{"bundleId":"com.apple.iWork.Pages","version":"14.2","trackId":1}]}"#,
        );
        let c = checker(fetch, &[]);
        let report = c.check(&pages, false);
        assert_eq!(report.status, Status::UpdateAvailable);
        assert_eq!(
            report.install,
            Install::AppStore {
                url: "macappstore://apps.apple.com/app/id1".into()
            }
        );
    }

    #[test]
    fn checks_many_apps_in_parallel() {
        let apps: Vec<_> = (0..20).map(|i| app(&format!("App{i}"), "1.0")).collect();
        let fetch = FakeFetch::default().with(homebrew::INDEX_URL, "[]");
        let c = checker(fetch, &[]);
        let seen = Mutex::new(Vec::new());
        c.check_all(&apps, &|_| Prefs::default(), 4, &|r| {
            seen.lock().unwrap().push(r.id)
        });
        assert_eq!(seen.lock().unwrap().len(), 20);
    }
}
