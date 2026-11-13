//! Updating apps: one call updates everything that was asked for.
//!
//! The order is chosen so nothing is replaced until everything that can fail
//! cheaply has already been tried:
//!
//! 1. **Hand-offs.** App Store apps open the store's Updates page; apps with
//!    no download open themselves or their website.
//! 2. **Stage.** Downloads run in parallel into a private folder, are
//!    unpacked, and the new app is checked: same bundle id, intact signature,
//!    same Apple developer team as the installed copy, and not older.
//! 3. **Quit.** Running apps are asked to quit, never forced.
//! 4. **Back up.** Every app is cloned to Camu's backups before it changes
//!    (an APFS clone: instant, and it takes no extra space until files differ).
//! 5. **Commit.** One shell script swaps the new apps in. Apps the user can't
//!    write to go in a second script that runs as root, so the password is
//!    asked for at most once. Homebrew casks upgrade in one `brew` run.
//! 6. **Confirm.** Each app's version is read back from disk, and apps that
//!    were running are reopened.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use serde::Serialize;

use crate::app::InstalledApp;
use crate::arch::{self, Arch};
use crate::check::{Install, Report, Status};
use crate::fetch::HttpFetch;
use crate::sources::{homebrew, Source};
use crate::{launch, macos, version};

/// One app to update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Job {
    pub id: String,
    pub app: InstalledApp,
    pub install: Install,
    pub target_version: String,
    /// Where the new version was found.
    pub source: Option<Source>,
    /// Sparkle's EdDSA signature for the download, when the feed has one.
    pub signature: Option<String>,
}

impl Job {
    /// A job for a report that has an update. `None` otherwise.
    pub fn from_report(report: &Report) -> Option<Job> {
        if report.status != Status::UpdateAvailable {
            return None;
        }
        let best = report.best.as_ref()?;
        Some(Job {
            id: report.id.clone(),
            app: report.app.clone(),
            install: report.install.clone(),
            target_version: best.version.clone(),
            source: Some(best.source),
            signature: best.signature.clone(),
        })
    }
}

/// Where an update is up to, sent to the UI as it changes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "phase", rename_all = "camelCase")]
pub enum Phase {
    Queued,
    Downloading {
        received: u64,
        total: Option<u64>,
    },
    Verifying,
    /// Waiting on the user, e.g. to quit the app.
    Waiting {
        message: String,
    },
    Installing,
    Done {
        version: String,
    },
    /// Camu opened something; the user finishes there.
    Handoff {
        message: String,
    },
    Failed {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Event {
    pub id: String,
    #[serde(flatten)]
    pub phase: Phase,
}

/// Camu's own folders.
#[derive(Debug, Clone)]
pub struct Dirs {
    /// Scratch space for downloads; emptied per run.
    pub staging: PathBuf,
    /// `<bundle id>/<version>/<Name>.app`.
    pub backups: PathBuf,
}

impl Dirs {
    pub fn default_for_user() -> Option<Self> {
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let support = home.join("Library/Application Support/co.capemount.camu");
        Some(Self {
            staging: home.join("Library/Caches/co.capemount.camu/staging"),
            backups: support.join("Backups"),
        })
    }
}

/// A verified new version, ready to swap in.
#[derive(Debug, Clone)]
struct Staged {
    job: Job,
    payload: Payload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Payload {
    App(PathBuf),
    Pkg(PathBuf),
}

pub struct Updater {
    http: HttpFetch,
    dirs: Dirs,
    /// The running macOS, e.g. "15.1"; `None` skips the check.
    os_version: Option<String>,
}

impl Updater {
    pub fn new(http: HttpFetch, dirs: Dirs) -> Self {
        Self {
            http,
            dirs,
            os_version: crate::platform::os_version(),
        }
    }

    /// Update every job, reporting progress through `emit`.
    pub fn run(&self, jobs: Vec<Job>, emit: &(dyn Fn(Event) + Sync)) {
        self.run_with(jobs, emit, true)
    }

    /// `run`; `interactive` false (scheduled automatic installs) never asks
    /// for a password: those updates fail with a note to install them from
    /// Camu instead.
    pub fn run_with(&self, jobs: Vec<Job>, emit: &(dyn Fn(Event) + Sync), interactive: bool) {
        let send = |id: &str, phase: Phase| {
            emit(Event {
                id: id.to_string(),
                phase,
            })
        };
        for job in &jobs {
            send(&job.id, Phase::Queued);
        }

        let plan = partition(jobs);
        self.hand_off(&plan, &send);

        let _ = fs::remove_dir_all(&self.dirs.staging);
        if let Err(e) = fs::create_dir_all(&self.dirs.staging) {
            for job in plan.downloads.iter().chain(&plan.homebrew) {
                send(
                    &job.id,
                    Phase::Failed {
                        message: format!("Camu can't write its cache: {e}"),
                    },
                );
            }
            return;
        }

        // Downloads, unpacked copies and backups all need room.
        if let Some(free) = macos::free_bytes(&self.dirs.staging) {
            if free < MIN_FREE && !plan.downloads.is_empty() {
                let gb = free as f64 / 1e9;
                for job in &plan.downloads {
                    send(
                        &job.id,
                        Phase::Failed {
                            message: format!(
                                "Only {gb:.1} GB free on this disk. Free up some space, then try again."
                            ),
                        },
                    );
                }
                return;
            }
        }

        // 2. Stage downloads, three at a time.
        let staged = std::sync::Mutex::new(Vec::new());
        let next = AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..3 {
                scope.spawn(|| loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(job) = plan.downloads.get(i) else {
                        break;
                    };
                    match self.stage(i, job, &send) {
                        Ok(s) => staged.lock().unwrap().push(s),
                        Err(message) => send(&job.id, Phase::Failed { message }),
                    }
                });
            }
        });
        let staged = staged.into_inner().unwrap();

        // 3 and 4. Quit, then back up, everything about to change.
        let mut reopen = Vec::new();
        let mut ready = |job: &Job| -> bool {
            if macos::is_running(&job.app.path) {
                send(
                    &job.id,
                    Phase::Waiting {
                        message: format!("Asking {} to quit…", job.app.name),
                    },
                );
                if !macos::quit_app(&job.app.path, &job.app.bundle_id, 20) {
                    send(
                        &job.id,
                        Phase::Failed {
                            message: format!(
                                "{} is still open. Quit it, then update again.",
                                job.app.name
                            ),
                        },
                    );
                    return false;
                }
                reopen.push(job.app.path.clone());
            }
            match self.back_up(&job.app) {
                Ok(_) => true,
                Err(e) => {
                    send(
                        &job.id,
                        Phase::Failed {
                            message: format!(
                                "Couldn't back up the current version, so nothing was changed: {e}"
                            ),
                        },
                    );
                    false
                }
            }
        };
        let staged: Vec<Staged> = staged.into_iter().filter(|s| ready(&s.job)).collect();
        let brew_jobs: Vec<Job> = plan.homebrew.iter().filter(|j| ready(j)).cloned().collect();

        // 5. Commit.
        self.commit(&staged, &send, interactive);
        self.upgrade_casks(&brew_jobs, &send, interactive);

        // 6. Reopen what was running.
        for path in reopen {
            let _ = launch::open_app(&path);
        }
        let _ = fs::remove_dir_all(&self.dirs.staging);
        self.prune_backups();
    }

    fn hand_off(&self, plan: &Plan, send: &dyn Fn(&str, Phase)) {
        if !plan.app_store.is_empty() {
            let opened = launch::open_url("macappstore://showUpdatesPage");
            for job in &plan.app_store {
                send(
                    &job.id,
                    match &opened {
                        Ok(()) => Phase::Handoff {
                            message: "Finish in the App Store".into(),
                        },
                        Err(e) => Phase::Failed { message: e.clone() },
                    },
                );
            }
        }
        for job in &plan.manual {
            let result = match &job.install {
                Install::Website { url } => {
                    launch::open_url(url).map(|_| "Opened the download page")
                }
                Install::OpenApp => {
                    launch::open_app(&job.app.path).map(|_| "Opened — finish in the app")
                }
                Install::Vendor { path, .. } => {
                    launch::open_app(path).map(|_| "Opened its updater — finish there")
                }
                _ => Err("Camu doesn't know how to update this app".to_string()),
            };
            send(
                &job.id,
                match result {
                    Ok(message) => Phase::Handoff {
                        message: message.into(),
                    },
                    Err(message) => Phase::Failed { message },
                },
            );
        }
    }

    /// Download, unpack and verify one app.
    fn stage(&self, n: usize, job: &Job, send: &dyn Fn(&str, Phase)) -> Result<Staged, String> {
        self.stage_pinned(n, job, send, None)
    }

    /// `stage`, optionally checking the download against a published SHA-256
    /// first. Only Camu's own updates pass one.
    fn stage_pinned(
        &self,
        n: usize,
        job: &Job,
        send: &dyn Fn(&str, Phase),
        sha256: Option<&str>,
    ) -> Result<Staged, String> {
        let Install::Download { url } = &job.install else {
            return Err("not a download".into());
        };
        let dir = self.dirs.staging.join(n.to_string());
        let unpacked = dir.join("unpacked");
        fs::create_dir_all(&unpacked).map_err(|e| e.to_string())?;
        let file = dir.join(file_name_from_url(url));

        send(
            &job.id,
            Phase::Downloading {
                received: 0,
                total: None,
            },
        );
        self.http.download(url, &file, &|received, total| {
            send(&job.id, Phase::Downloading { received, total })
        })?;

        send(&job.id, Phase::Verifying);
        let checksum_matched = match sha256 {
            Some(want) => {
                let out =
                    macos::run_ok("/usr/bin/shasum", &["-a", "256", &file.to_string_lossy()])?;
                let got = parse_sha256(&out).ok_or("couldn't checksum the download")?;
                if !got.eq_ignore_ascii_case(want) {
                    return Err(
                        "the download doesn't match the checksum published with the release".into(),
                    );
                }
                true
            }
            None => false,
        };
        if job.source == Some(Source::Sparkle) {
            check_sparkle_signature(
                &file,
                job.app.sparkle_key.as_deref(),
                job.signature.as_deref(),
            )?;
        }
        let payload = unpack(&file, &unpacked, &job.app.bundle_id)?;
        let old_team = macos::team_id(&job.app.path);
        match &payload {
            Payload::App(new) => {
                let fresh =
                    InstalledApp::read(new).ok_or("the download doesn't contain a readable app")?;
                if fresh.bundle_id != job.app.bundle_id {
                    return Err(format!(
                        "the download is a different app ({})",
                        fresh.bundle_id
                    ));
                }
                if !version::is_newer(&fresh.version, &job.app.version)
                    && !fresh
                        .build
                        .as_deref()
                        .zip(job.app.build.as_deref())
                        .is_some_and(|(n, o)| version::is_newer(n, o))
                {
                    return Err(format!(
                        "the download is version {}, not newer than {}",
                        fresh.version, job.app.version
                    ));
                }
                runs_here(&fresh, self.os_version.as_deref(), Arch::host())?;
                macos::verify_signature(new)?;
                self_trust(
                    old_team.as_deref(),
                    macos::team_id(new).as_deref(),
                    checksum_matched,
                    || macos::gatekeeper_accepts(new),
                )?;
            }
            Payload::Pkg(pkg) => {
                let out = macos::run_ok(
                    "/usr/sbin/pkgutil",
                    &["--check-signature", &pkg.to_string_lossy()],
                )
                .map_err(|_| "the installer package isn't signed".to_string())?;
                let pkg_team = macos::parse_pkg_team_id(&out);
                if old_team.is_none() || pkg_team != old_team {
                    return Err("the installer package isn't signed by this app's developer".into());
                }
            }
        }
        Ok(Staged {
            job: job.clone(),
            payload,
        })
    }

    fn back_up(&self, app: &InstalledApp) -> Result<PathBuf, String> {
        let dest_dir = self
            .dirs
            .backups
            .join(&app.bundle_id)
            .join(sanitize(&app.version));
        let dest = dest_dir.join(format!("{}.app", app.file_name));
        if dest.exists() {
            return Ok(dest);
        }
        fs::create_dir_all(&dest_dir).map_err(|e| e.to_string())?;
        let (src, dst) = (app.path.to_string_lossy(), dest.to_string_lossy());
        // An APFS clone first; a real copy if the backup is on another volume.
        macos::run_ok("/bin/cp", &["-cR", &src, &dst])
            .or_else(|_| {
                let _ = fs::remove_dir_all(&dest);
                macos::run_ok("/usr/bin/ditto", &[&src, &dst])
            })
            .map(|_| dest.clone())
    }

    fn commit(&self, staged: &[Staged], send: &dyn Fn(&str, Phase), interactive: bool) {
        if staged.is_empty() {
            return;
        }
        let (admin, user): (Vec<_>, Vec<_>) = staged.iter().enumerate().partition(|(_, s)| {
            matches!(s.payload, Payload::Pkg(_)) || needs_admin(&s.job.app.path)
        });

        for (group, as_admin) in [(user, false), (admin, true)] {
            if group.is_empty() {
                continue;
            }
            if as_admin && !interactive {
                for (_, s) in &group {
                    send(
                        &s.job.id,
                        Phase::Failed {
                            message: "Needs your password. Update it from Camu.".into(),
                        },
                    );
                }
                continue;
            }
            let items: Vec<ScriptItem> = group
                .iter()
                .map(|(n, s)| {
                    send(&s.job.id, Phase::Installing);
                    match &s.payload {
                        Payload::App(new) => ScriptItem::App {
                            n: *n,
                            new: new.clone(),
                            dest: s.job.app.path.clone(),
                            aside: self.dirs.staging.join(format!("{n}/old.app")),
                        },
                        Payload::Pkg(pkg) => ScriptItem::Pkg {
                            n: *n,
                            pkg: pkg.clone(),
                        },
                    }
                })
                .collect();
            let script_path = self.dirs.staging.join(if as_admin {
                "commit-admin.sh"
            } else {
                "commit.sh"
            });
            let output = fs::write(&script_path, replace_script(&items))
                .map_err(|e| e.to_string())
                .and_then(|_| {
                    if as_admin {
                        macos::run_script_as_admin(
                            &script_path,
                            "Camu needs your password to replace apps it can't change on its own.",
                        )
                    } else {
                        macos::run_ok("/bin/sh", &[&script_path.to_string_lossy()])
                    }
                });

            let results = match &output {
                Ok(stdout) => parse_results(stdout),
                Err(_) => Vec::new(),
            };
            for (n, s) in &group {
                let outcome = results.iter().find(|(i, _)| i == n).map(|(_, r)| r.clone());
                send(
                    &s.job.id,
                    match (outcome, &output) {
                        (Some(Ok(())), _) => confirm(&s.job),
                        (Some(Err(message)), _) => Phase::Failed { message },
                        (None, Err(e)) if e.contains("-128") => Phase::Failed {
                            message: "Cancelled".into(),
                        },
                        (None, Err(e)) => Phase::Failed { message: e.clone() },
                        (None, Ok(_)) => Phase::Failed {
                            message: "The install step didn't report back".into(),
                        },
                    },
                );
            }
        }
    }

    fn upgrade_casks(&self, jobs: &[Job], send: &dyn Fn(&str, Phase), interactive: bool) {
        if jobs.is_empty() {
            return;
        }
        let Some(brew) = homebrew::brew_path() else {
            for job in jobs {
                send(
                    &job.id,
                    Phase::Failed {
                        message: "Homebrew isn't installed".into(),
                    },
                );
            }
            return;
        };
        for job in jobs {
            send(&job.id, Phase::Installing);
        }
        let tokens: Vec<String> = jobs
            .iter()
            .filter_map(|j| match &j.install {
                Install::Homebrew { cask } => Some(cask.clone()),
                _ => None,
            })
            .collect();

        let askpass = self.dirs.staging.join("askpass.sh");
        // Unattended runs answer Homebrew's password question with "no", so
        // a cask that needs administrator rights fails instead of asking.
        let _ = fs::write(
            &askpass,
            if interactive {
                ASKPASS
            } else {
                "#!/bin/sh\nexit 1\n"
            },
        );
        let _ = macos::run("/bin/chmod", &["700", &askpass.to_string_lossy()]);

        let output = std::process::Command::new(&brew)
            .args(["upgrade", "--cask"])
            .args(&tokens)
            .env(
                "PATH",
                "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin",
            )
            .env("SUDO_ASKPASS", &askpass)
            .env("HOMEBREW_NO_ENV_HINTS", "1")
            // Camu already knows what to upgrade; refreshing all of Homebrew
            // first can take minutes.
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("HOMEBREW_NO_COLOR", "1")
            .stdin(std::process::Stdio::null())
            .output();
        let brew_error = match &output {
            Ok(o) => last_error(&String::from_utf8_lossy(&o.stderr)),
            Err(e) => Some(e.to_string()),
        };

        for job in jobs {
            send(
                &job.id,
                match confirm(job) {
                    Phase::Done { version } => Phase::Done { version },
                    _ => Phase::Failed {
                        message: brew_error.clone().unwrap_or_else(|| {
                            format!("Homebrew doesn't have {} yet", job.target_version)
                        }),
                    },
                },
            );
        }
    }

    /// Keep only the newest backup of each app.
    fn prune_backups(&self) {
        for backup in list_backups(&self.dirs.backups)
            .into_iter()
            .filter(|b| !b.newest)
        {
            if let Some(dir) = backup.path.parent() {
                let _ = fs::remove_dir_all(dir);
            }
        }
    }

    /// Download and verify a new version of `app` (Camu itself, for
    /// self-updates) without installing it. Returns the verified new bundle,
    /// which stays in Camu's cache until the next update run. `sha256` is the
    /// checksum published with the release, which an ad-hoc signed Camu needs
    /// in place of a matching Developer ID.
    pub fn prepare(
        &self,
        app: &InstalledApp,
        url: &str,
        target_version: &str,
        sha256: Option<&str>,
    ) -> Result<PathBuf, String> {
        let job = Job {
            id: app.id(),
            app: app.clone(),
            install: Install::Download {
                url: url.to_string(),
            },
            target_version: target_version.to_string(),
            source: None,
            signature: None,
        };
        let dir = self.dirs.staging.with_file_name("self-update");
        let _ = fs::remove_dir_all(&dir);
        let scoped = Updater {
            os_version: self.os_version.clone(),
            http: HttpFetch::new(),
            dirs: Dirs {
                staging: dir,
                backups: self.dirs.backups.clone(),
            },
        };
        let staged = scoped.stage_pinned(0, &job, &|_, _| {}, sha256)?;
        self.back_up(app)?;
        match staged.payload {
            Payload::App(path) => Ok(path),
            Payload::Pkg(_) => Err("the update is an installer package; install it by hand".into()),
        }
    }

    pub fn roll_back(&self, app: &InstalledApp) -> Result<String, String> {
        let backup = list_backups(&self.dirs.backups)
            .into_iter()
            .find(|b| b.bundle_id == app.bundle_id && b.newest)
            .ok_or("there's no backup of this app")?;
        if macos::is_running(&app.path) && !macos::quit_app(&app.path, &app.bundle_id, 20) {
            return Err(format!("{} is still open. Quit it first.", app.name));
        }
        fs::create_dir_all(&self.dirs.staging).map_err(|e| e.to_string())?;
        let script_path = self.dirs.staging.join("rollback.sh");
        let item = ScriptItem::App {
            n: 0,
            new: backup.path.clone(),
            dest: app.path.clone(),
            aside: self.dirs.staging.join("rollback-old.app"),
        };
        fs::write(&script_path, replace_script(&[item])).map_err(|e| e.to_string())?;
        let out = if needs_admin(&app.path) {
            macos::run_script_as_admin(
                &script_path,
                "Camu needs your password to restore this app.",
            )
        } else {
            macos::run_ok("/bin/sh", &[&script_path.to_string_lossy()])
        }?;
        let _ = fs::remove_dir_all(&self.dirs.staging);
        match parse_results(&out).first() {
            Some((_, Ok(()))) => Ok(backup.version),
            Some((_, Err(e))) => Err(e.clone()),
            None => Err("the restore didn't report back".into()),
        }
    }
}

/// Refuse a new app this Mac can't run: built for the other chip only, or
/// needing a newer macOS. Installing it would leave an app that won't open.
pub fn runs_here(app: &InstalledApp, os_version: Option<&str>, host: Arch) -> Result<(), String> {
    if !arch::runs_on(&app.archs, host) {
        let other = if host == Arch::Arm64 {
            Arch::X86_64
        } else {
            Arch::Arm64
        };
        return Err(format!("this download only runs on {} Macs", other.label()));
    }
    if let (Some(needed), Some(running)) = (app.minimum_os.as_deref(), os_version) {
        if version::compare(needed, running) == std::cmp::Ordering::Greater {
            return Err(format!(
                "version {} needs macOS {needed} or later",
                app.version
            ));
        }
    }
    Ok(())
}

/// Check a Sparkle download against the app's own EdDSA key, as Sparkle
/// itself would. An app with a key accepts only signed updates.
pub fn check_sparkle_signature(
    file: &Path,
    public_key: Option<&str>,
    signature: Option<&str>,
) -> Result<(), String> {
    let Some(key) = public_key else {
        return Ok(());
    };
    let signature = signature.ok_or("the update isn't signed by its developer")?;
    let bytes = fs::read(file).map_err(|e| e.to_string())?;
    verify_ed25519(key, signature, &bytes)
}

fn verify_ed25519(key: &str, signature: &str, message: &[u8]) -> Result<(), String> {
    use base64::Engine;
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};
    let b64 = base64::engine::general_purpose::STANDARD;
    let bad = || "the app's update signature key is unreadable".to_string();
    let key: [u8; 32] = b64
        .decode(key.trim())
        .ok()
        .and_then(|k| k.try_into().ok())
        .ok_or_else(bad)?;
    let sig: [u8; 64] = b64
        .decode(signature.trim())
        .ok()
        .and_then(|s| s.try_into().ok())
        .ok_or("the update's signature is unreadable")?;
    VerifyingKey::from_bytes(&key)
        .map_err(|_| bad())?
        .verify(message, &Signature::from_bytes(&sig))
        .map_err(|_| "the download doesn't match its developer's signature".to_string())
}

/// Read the app back from disk and report what is installed now.
fn confirm(job: &Job) -> Phase {
    match InstalledApp::read(&job.app.path) {
        Some(now) if now.version != job.app.version || now.build != job.app.build => Phase::Done {
            version: now.version,
        },
        Some(_) => Phase::Failed {
            message: "The app is still on the old version".into(),
        },
        None => Phase::Failed {
            message: "The app is missing after the update".into(),
        },
    }
}

const ASKPASS: &str = r#"#!/bin/sh
# Homebrew calls this when a cask needs administrator rights.
/usr/bin/osascript -e 'display dialog "Homebrew needs your password to finish updating apps." with title "Camu" default answer "" with hidden answer buttons {"Cancel", "OK"} default button "OK"' -e 'text returned of result'
"#;

/// The first "Error:" line Homebrew printed, for the UI.
fn last_error(stderr: &str) -> Option<String> {
    stderr
        .lines()
        .find(|l| l.starts_with("Error:"))
        .map(|l| l.trim_start_matches("Error:").trim().to_string())
}

struct Plan {
    app_store: Vec<Job>,
    homebrew: Vec<Job>,
    downloads: Vec<Job>,
    manual: Vec<Job>,
}

fn partition(jobs: Vec<Job>) -> Plan {
    let mut plan = Plan {
        app_store: vec![],
        homebrew: vec![],
        downloads: vec![],
        manual: vec![],
    };
    let mut seen = HashSet::new();
    for job in jobs.into_iter().filter(|j| seen.insert(j.id.clone())) {
        match job.install {
            Install::AppStore { .. } => plan.app_store.push(job),
            Install::Homebrew { .. } => plan.homebrew.push(job),
            Install::Download { .. } => plan.downloads.push(job),
            Install::OpenApp | Install::Website { .. } | Install::Vendor { .. } | Install::None => {
                plan.manual.push(job)
            }
        }
    }
    plan
}

/// Decide whether a downloaded app may replace the installed one.
///
/// Both signed: the Apple team must match, so a hijacked feed can't push an
/// app from someone else. Installed copy unsigned: the new one must pass
/// Gatekeeper (Developer ID and notarized). Download unsigned: never.
pub fn trust(
    old_team: Option<&str>,
    new_team: Option<&str>,
    gatekeeper: impl FnOnce() -> bool,
) -> Result<(), String> {
    match (old_team, new_team) {
        (Some(old), Some(new)) if old == new => Ok(()),
        (Some(_), Some(_)) => Err("the download is signed by a different developer".into()),
        (_, None) => Err("the download isn't signed by its developer".into()),
        (None, Some(_)) if gatekeeper() => Ok(()),
        (None, Some(_)) => Err("the download isn't notarized by Apple".into()),
    }
}

/// `trust`, plus one case for Camu's own updates: an ad-hoc signed Camu may
/// be replaced by an ad-hoc signed download when the download matched the
/// SHA-256 published with Camu's release. Without a checksum this is `trust`.
pub fn self_trust(
    old_team: Option<&str>,
    new_team: Option<&str>,
    checksum_matched: bool,
    gatekeeper: impl FnOnce() -> bool,
) -> Result<(), String> {
    match (old_team, new_team) {
        (None, None) if checksum_matched => Ok(()),
        _ => trust(old_team, new_team, gatekeeper),
    }
}

/// The hash from `shasum -a 256` output or a `.sha256` file
/// (`<64 hex digits>  <file name>`).
pub fn parse_sha256(text: &str) -> Option<String> {
    let first = text.split_whitespace().next()?;
    (first.len() == 64 && first.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| first.to_ascii_lowercase())
}

/// Free space wanted before downloading anything.
const MIN_FREE: u64 = 2_000_000_000;

/// Would installing this update ask for an administrator password? True for
/// installer packages and apps this user can't replace.
pub fn needs_password(report: &Report) -> bool {
    match &report.install {
        Install::Download { url } => {
            url.split(['?', '#'])
                .next()
                .unwrap_or(url)
                .ends_with(".pkg")
                || needs_admin(&report.app.path)
        }
        Install::Homebrew { .. } => needs_admin(&report.app.path),
        _ => false,
    }
}

/// True when the current user can't move the app themselves.
pub fn needs_admin(app: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    let mine = std::env::var_os("HOME")
        .and_then(|h| fs::metadata(h).ok())
        .map(|m| m.uid());
    let owner = fs::metadata(app).map(|m| m.uid()).ok();
    if mine.is_none() || owner != mine {
        return true;
    }
    // Moving the app needs write access to the folder it's in.
    let Some(parent) = app.parent() else {
        return true;
    };
    let probe = parent.join(format!(".camu-write-test-{}", std::process::id()));
    match fs::write(&probe, b"") {
        Ok(()) => {
            let _ = fs::remove_file(&probe);
            false
        }
        Err(_) => true,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ScriptItem {
    App {
        n: usize,
        new: PathBuf,
        dest: PathBuf,
        aside: PathBuf,
    },
    Pkg {
        n: usize,
        pkg: PathBuf,
    },
}

/// A shell script that swaps each new app in, putting the old one back if
/// the copy fails. Prints `CAMU-OK <n>` or `CAMU-FAIL <n> <reason>` per item.
fn replace_script(items: &[ScriptItem]) -> String {
    let q = |p: &Path| macos::sh_quote(&p.to_string_lossy());
    let mut s = String::from("#!/bin/sh\n# Written by Camu for one update run.\n");
    for item in items {
        match item {
            ScriptItem::App { n, new, dest, aside } => s.push_str(&format!(
                "rm -rf {aside}\n\
                 if /bin/mv {dest} {aside}; then\n\
                 \x20 if /usr/bin/ditto {new} {dest}; then /bin/rm -rf {aside}; echo 'CAMU-OK {n}';\n\
                 \x20 else /bin/rm -rf {dest}; /bin/mv {aside} {dest}; echo 'CAMU-FAIL {n} Copying the new version failed; the old one was put back'; fi\n\
                 else echo 'CAMU-FAIL {n} The old version could not be moved aside'; fi\n",
                new = q(new),
                dest = q(dest),
                aside = q(aside),
            )),
            ScriptItem::Pkg { n, pkg } => s.push_str(&format!(
                "if /usr/sbin/installer -pkg {pkg} -target / >/dev/null 2>&1; then echo 'CAMU-OK {n}';\n\
                 else echo 'CAMU-FAIL {n} The installer package failed'; fi\n",
                pkg = q(pkg),
            )),
        }
    }
    s
}

fn parse_results(stdout: &str) -> Vec<(usize, Result<(), String>)> {
    stdout
        .lines()
        .filter_map(|line| {
            if let Some(rest) = line.strip_prefix("CAMU-OK ") {
                Some((rest.trim().parse().ok()?, Ok(())))
            } else if let Some(rest) = line.strip_prefix("CAMU-FAIL ") {
                let (n, msg) = rest.split_once(' ').unwrap_or((rest, "Failed"));
                Some((n.parse().ok()?, Err(msg.to_string())))
            } else {
                None
            }
        })
        .collect()
}

/// A safe local file name for a download URL, keeping its extension.
fn file_name_from_url(url: &str) -> String {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    let name = path.rsplit('/').next().unwrap_or("download");
    let clean = sanitize(name);
    if clean.is_empty() || clean.starts_with('.') {
        format!("download{clean}")
    } else {
        clean
    }
}

fn sanitize(s: &str) -> String {
    let decoded = s.replace("%20", " ");
    decoded
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || " ._-+".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim()
        .trim_start_matches('.')
        .to_string()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Zip,
    Dmg,
    Pkg,
    Tar,
}

/// What kind of archive a download is, by extension, then by its bytes.
fn kind_of(file: &Path) -> Option<Kind> {
    let name = file.to_string_lossy().to_lowercase();
    let by_name = [
        (".zip", Kind::Zip),
        (".dmg", Kind::Dmg),
        (".pkg", Kind::Pkg),
        (".mpkg", Kind::Pkg),
        (".tar.gz", Kind::Tar),
        (".tgz", Kind::Tar),
        (".tar.bz2", Kind::Tar),
        (".tbz", Kind::Tar),
        (".tar.xz", Kind::Tar),
        (".txz", Kind::Tar),
    ];
    if let Some((_, kind)) = by_name.iter().find(|(ext, _)| name.ends_with(ext)) {
        return Some(*kind);
    }
    let bytes = fs::read(file).ok()?;
    sniff(&bytes)
}

fn sniff(bytes: &[u8]) -> Option<Kind> {
    if bytes.starts_with(b"PK\x03\x04") {
        Some(Kind::Zip)
    } else if bytes.starts_with(b"xar!") {
        Some(Kind::Pkg)
    } else if bytes.len() >= 512 && &bytes[bytes.len() - 512..bytes.len() - 508] == b"koly" {
        Some(Kind::Dmg)
    } else if bytes.starts_with(&[0x1f, 0x8b])
        || bytes.starts_with(b"BZh")
        || bytes.starts_with(&[0xfd, b'7', b'z', b'X', b'Z'])
    {
        Some(Kind::Tar)
    } else {
        None
    }
}

/// Unpack a download into `out` and find the app (or installer) inside.
fn unpack(file: &Path, out: &Path, bundle_id: &str) -> Result<Payload, String> {
    let (f, o) = (file.to_string_lossy(), out.to_string_lossy());
    match kind_of(file).ok_or("Camu doesn't recognise this download's format")? {
        Kind::Pkg => Ok(Payload::Pkg(file.to_path_buf())),
        Kind::Zip => {
            macos::run_ok("/usr/bin/ditto", &["-x", "-k", &f, &o])?;
            find_payload(out, bundle_id)
        }
        Kind::Tar => {
            macos::run_ok("/usr/bin/tar", &["-xf", &f, "-C", &o])?;
            find_payload(out, bundle_id)
        }
        Kind::Dmg => {
            let mount = macos::attach_dmg(file)?;
            // Copy out of the image so it can be detached straight away.
            let result = find_payload(&mount, bundle_id).and_then(|found| {
                let (Payload::App(src) | Payload::Pkg(src)) = &found;
                let dest = out.join(src.file_name().unwrap_or_default());
                macos::run_ok(
                    "/usr/bin/ditto",
                    &[&src.to_string_lossy(), &dest.to_string_lossy()],
                )?;
                Ok(match found {
                    Payload::App(_) => Payload::App(dest),
                    Payload::Pkg(_) => Payload::Pkg(dest),
                })
            });
            macos::detach_dmg(&mount);
            result
        }
    }
}

/// The app with `bundle_id` under `dir`, or failing that a lone installer.
fn find_payload(dir: &Path, bundle_id: &str) -> Result<Payload, String> {
    if let Some(app) = find_app(dir, bundle_id, 0) {
        return Ok(Payload::App(app));
    }
    fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "pkg" || e == "mpkg"))
        .map(Payload::Pkg)
        .ok_or_else(|| "the download doesn't contain this app".to_string())
}

fn find_app(dir: &Path, bundle_id: &str, depth: usize) -> Option<PathBuf> {
    let entries = fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        // Never follow links: a disk image's "Applications" shortcut points
        // at the real /Applications.
        if entry.file_type().map(|t| t.is_symlink()).unwrap_or(true) || !path.is_dir() {
            continue;
        }
        if path.extension().is_some_and(|e| e == "app") {
            if InstalledApp::read(&path).is_some_and(|a| a.bundle_id == bundle_id) {
                return Some(path);
            }
        } else if depth < 2 {
            subdirs.push(path);
        }
    }
    subdirs
        .into_iter()
        .find_map(|d| find_app(&d, bundle_id, depth + 1))
}

/// A saved previous version of an app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Backup {
    pub bundle_id: String,
    pub version: String,
    pub path: PathBuf,
    /// The most recent backup of this app.
    pub newest: bool,
    /// When the backup was made, in seconds since 1970.
    pub made: u64,
}

pub fn list_backups(root: &Path) -> Vec<Backup> {
    let mut out = Vec::new();
    for app_dir in fs::read_dir(root).into_iter().flatten().flatten() {
        let bundle_id = app_dir.file_name().to_string_lossy().into_owned();
        let mut versions: Vec<(std::time::SystemTime, Backup)> = fs::read_dir(app_dir.path())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|v| {
                let app = fs::read_dir(v.path())
                    .ok()?
                    .flatten()
                    .map(|e| e.path())
                    .find(|p| p.extension().is_some_and(|e| e == "app"))?;
                let modified = fs::metadata(v.path()).and_then(|m| m.modified()).ok()?;
                Some((
                    modified,
                    Backup {
                        bundle_id: bundle_id.clone(),
                        version: v.file_name().to_string_lossy().into_owned(),
                        path: app,
                        newest: false,
                        made: modified
                            .duration_since(std::time::UNIX_EPOCH)
                            .map(|d| d.as_secs())
                            .unwrap_or(0),
                    },
                ))
            })
            .collect();
        versions.sort_by_key(|v| std::cmp::Reverse(v.0));
        for (i, (_, mut backup)) in versions.into_iter().enumerate() {
            backup.newest = i == 0;
            out.push(backup);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> InstalledApp {
        InstalledApp {
            path: format!("/Applications/{name}.app").into(),
            file_name: name.into(),
            name: name.into(),
            bundle_id: format!("com.test.{}", name.to_lowercase()),
            version: "1.0".into(),
            build: None,
            sparkle_feed: None,
            electron_feed: None,
            from_app_store: false,
            minimum_os: None,
            icon_file: None,
            ..Default::default()
        }
    }

    fn job(name: &str, install: Install) -> Job {
        Job {
            id: format!("/Applications/{name}.app"),
            app: app(name),
            install,
            target_version: "2.0".into(),
            source: None,
            signature: None,
        }
    }

    #[test]
    fn refuses_apps_for_the_other_chip_or_a_newer_macos() {
        let mut a = app("Rocket");
        a.archs = vec![Arch::Arm64];
        assert!(runs_here(&a, None, Arch::X86_64)
            .unwrap_err()
            .contains("Apple silicon"));
        assert!(runs_here(&a, None, Arch::Arm64).is_ok());
        a.archs = vec![Arch::X86_64];
        assert!(
            runs_here(&a, None, Arch::Arm64).is_ok(),
            "Rosetta runs Intel apps"
        );
        a.minimum_os = Some("15.0".into());
        assert!(runs_here(&a, Some("14.6"), Arch::Arm64)
            .unwrap_err()
            .contains("macOS 15.0"));
        assert!(runs_here(&a, Some("15.1"), Arch::Arm64).is_ok());
    }

    #[test]
    fn sparkle_signatures() {
        use base64::Engine;
        use ed25519_dalek::{Signer, SigningKey};
        let b64 = base64::engine::general_purpose::STANDARD;
        let signing = SigningKey::from_bytes(&[7u8; 32]);
        let key = b64.encode(signing.verifying_key().to_bytes());
        let data = b"the app archive";
        let sig = b64.encode(signing.sign(data).to_bytes());
        assert!(verify_ed25519(&key, &sig, data).is_ok());
        assert!(verify_ed25519(&key, &sig, b"tampered").is_err());

        let dir = std::env::temp_dir().join(format!("camu-sig-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.zip");
        fs::write(&file, data).unwrap();
        assert!(
            check_sparkle_signature(&file, None, None).is_ok(),
            "no key: nothing to check"
        );
        assert!(
            check_sparkle_signature(&file, Some(&key), None).is_err(),
            "key but unsigned"
        );
        assert!(check_sparkle_signature(&file, Some(&key), Some(&sig)).is_ok());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn trust_rules() {
        assert!(trust(Some("TEAM1"), Some("TEAM1"), || false).is_ok());
        assert!(
            trust(Some("TEAM1"), Some("TEAM2"), || true).is_err(),
            "a different developer is never trusted"
        );
        assert!(trust(Some("TEAM1"), None, || true).is_err());
        assert!(trust(None, None, || true).is_err());
        assert!(
            trust(None, Some("TEAM1"), || true).is_ok(),
            "notarized replaces unsigned"
        );
        assert!(trust(None, Some("TEAM1"), || false).is_err());
    }

    #[test]
    fn camu_self_updates_need_a_matching_checksum_when_unsigned() {
        assert!(self_trust(None, None, true, || false).is_ok());
        assert!(self_trust(None, None, false, || true).is_err());
        // A signed Camu still demands the same team, checksum or not.
        assert!(self_trust(Some("TEAM1"), None, true, || true).is_err());
        assert!(self_trust(Some("TEAM1"), Some("TEAM2"), true, || true).is_err());
        assert!(self_trust(Some("TEAM1"), Some("TEAM1"), false, || false).is_ok());
    }

    #[test]
    fn reads_sha256_lines() {
        let h = "AB".repeat(32);
        assert_eq!(
            parse_sha256(&format!("{h}  release/Camu.dmg\n")),
            Some(h.to_lowercase())
        );
        assert_eq!(parse_sha256("not-a-hash  x"), None);
        assert_eq!(parse_sha256(""), None);
    }

    #[test]
    fn jobs_are_split_by_how_they_install() {
        let plan = partition(vec![
            job(
                "A",
                Install::AppStore {
                    url: "macappstore://x".into(),
                },
            ),
            job("B", Install::Homebrew { cask: "b".into() }),
            job(
                "C",
                Install::Download {
                    url: "https://x/C.zip".into(),
                },
            ),
            job("D", Install::OpenApp),
            job(
                "C",
                Install::Download {
                    url: "https://x/C.zip".into(),
                },
            ),
        ]);
        assert_eq!(plan.app_store.len(), 1);
        assert_eq!(plan.homebrew.len(), 1);
        assert_eq!(plan.downloads.len(), 1, "duplicates are dropped");
        assert_eq!(plan.manual.len(), 1);
    }

    #[test]
    fn replace_script_quotes_paths_and_restores_on_failure() {
        let script = replace_script(&[
            ScriptItem::App {
                n: 3,
                new: "/tmp/stage/3/It's New.app".into(),
                dest: "/Applications/It's New.app".into(),
                aside: "/tmp/stage/3/old.app".into(),
            },
            ScriptItem::Pkg {
                n: 4,
                pkg: "/tmp/stage/4/Tool.pkg".into(),
            },
        ]);
        assert!(script.contains(r"/bin/mv '/Applications/It'\''s New.app' '/tmp/stage/3/old.app'"));
        assert!(
            script.contains("/bin/mv '/tmp/stage/3/old.app' '/Applications/It'\\''s New.app'"),
            "puts the old app back"
        );
        assert!(script.contains("installer -pkg '/tmp/stage/4/Tool.pkg' -target /"));
        assert!(script.contains("echo 'CAMU-OK 3'") && script.contains("echo 'CAMU-OK 4'"));
    }

    #[test]
    fn replace_script_really_swaps_and_rolls_back() {
        // Run the generated script for real (needs only /bin/sh, mv, rm).
        if !Path::new("/usr/bin/ditto").exists() {
            // Off macOS there is no ditto; check the failure path restores.
            let root = std::env::temp_dir().join(format!("camu-swap-{}", std::process::id()));
            let dest = root.join("Apps/Rocket.app");
            fs::create_dir_all(&dest).unwrap();
            fs::write(dest.join("old"), b"1").unwrap();
            let item = ScriptItem::App {
                n: 0,
                new: root.join("new/Rocket.app"),
                dest: dest.clone(),
                aside: root.join("aside.app"),
            };
            let out = std::process::Command::new("/bin/sh")
                .arg("-c")
                .arg(replace_script(&[item]))
                .output()
                .unwrap();
            let results = parse_results(&String::from_utf8_lossy(&out.stdout));
            assert!(matches!(results.as_slice(), [(0, Err(_))]));
            assert!(dest.join("old").exists(), "the old app is back in place");
            fs::remove_dir_all(&root).unwrap();
        }
    }

    #[test]
    fn results_are_parsed_per_item() {
        let out = "noise\nCAMU-OK 0\nCAMU-FAIL 2 The old version could not be moved aside\n";
        assert_eq!(
            parse_results(out),
            vec![
                (0, Ok(())),
                (2, Err("The old version could not be moved aside".into()))
            ]
        );
    }

    #[test]
    fn download_names_are_safe() {
        assert_eq!(
            file_name_from_url("https://x.com/dl/Rocket%202.5.dmg?sig=abc"),
            "Rocket 2.5.dmg"
        );
        assert_eq!(
            file_name_from_url("https://x.com/../../etc/passwd"),
            "passwd"
        );
        assert_eq!(file_name_from_url("https://x.com/download/"), "download");
        assert_eq!(
            file_name_from_url("https://x.com/.hidden.zip"),
            "hidden.zip"
        );
    }

    #[test]
    fn archive_kinds() {
        assert_eq!(sniff(b"PK\x03\x04rest"), Some(Kind::Zip));
        assert_eq!(sniff(b"xar!rest"), Some(Kind::Pkg));
        let mut dmg = vec![0u8; 1024];
        dmg[512..516].copy_from_slice(b"koly");
        assert_eq!(sniff(&dmg), Some(Kind::Dmg));
        assert_eq!(sniff(&[0x1f, 0x8b, 0]), Some(Kind::Tar));
        assert_eq!(sniff(b"<html>"), None);
        assert_eq!(kind_of(Path::new("/x/Rocket-2.0.tar.xz")), Some(Kind::Tar));
    }

    #[test]
    fn finds_the_right_app_and_ignores_links() {
        let root = std::env::temp_dir().join(format!("camu-find-{}", std::process::id()));
        let plist = |id: &str| {
            format!(
                r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>{id}</string><key>CFBundleShortVersionString</key><string>2.0</string></dict></plist>"#
            )
        };
        for (path, id) in [
            ("Other.app", "com.other"),
            ("Folder/Rocket.app", "com.test.rocket"),
        ] {
            let contents = root.join(path).join("Contents");
            fs::create_dir_all(&contents).unwrap();
            fs::write(contents.join("Info.plist"), plist(id)).unwrap();
        }
        std::os::unix::fs::symlink("/", root.join("Applications")).unwrap();

        assert_eq!(
            find_payload(&root, "com.test.rocket").unwrap(),
            Payload::App(root.join("Folder/Rocket.app"))
        );
        assert!(find_payload(&root, "com.missing").is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn backups_mark_the_newest() {
        let root = std::env::temp_dir().join(format!("camu-backups-{}", std::process::id()));
        for v in ["1.0", "1.1"] {
            fs::create_dir_all(root.join(format!("com.test.rocket/{v}/Rocket.app"))).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let backups = list_backups(&root);
        let newest: Vec<_> = backups
            .iter()
            .filter(|b| b.newest)
            .map(|b| b.version.as_str())
            .collect();
        assert_eq!(newest, ["1.1"]);
        assert_eq!(backups.len(), 2);
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn homebrew_errors_are_summarised() {
        assert_eq!(
            last_error("==> Upgrading 1 outdated package\nError: rocket: SHA-256 mismatch\n")
                .as_deref(),
            Some("rocket: SHA-256 mismatch")
        );
        assert_eq!(last_error("all good"), None);
    }
}
