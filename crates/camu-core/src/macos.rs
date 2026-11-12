//! Small wrappers around macOS command-line tools, and the parsers for what
//! they print. The parsers are pure and tested on any OS; the wrappers only do
//! anything useful on a Mac.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Run a program and capture its output. Never inherits Camu's stdin.
pub fn run(program: &str, args: &[&str]) -> Result<Output, String> {
    Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{program}: {e}"))
}

/// Run a program; fail with its stderr if it exits non-zero.
pub fn run_ok(program: &str, args: &[&str]) -> Result<String, String> {
    let out = run(program, args)?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        let name = Path::new(program)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        Err(format!("{name}: {}", err.trim()))
    }
}

/// Quote a string for /bin/sh: wrap in single quotes, escape single quotes.
pub fn sh_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// Quote a string for AppleScript: wrap in double quotes, escape `\` and `"`.
pub fn applescript_quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', r"\\").replace('"', "\\\""))
}

/// Run a shell script file as root. macOS shows its own password dialog,
/// once, naming Camu. Returns the script's stdout.
pub fn run_script_as_admin(script: &Path, prompt: &str) -> Result<String, String> {
    let command = format!("/bin/sh {}", sh_quote(&script.to_string_lossy()));
    let apple = format!(
        "do shell script {} with prompt {} with administrator privileges",
        applescript_quote(&command),
        applescript_quote(prompt)
    );
    run_ok("/usr/bin/osascript", &["-e", &apple])
}

/// The Apple team that signed `path`, from `codesign -dv`. `None` when the
/// app is unsigned or ad-hoc signed.
pub fn team_id(path: &Path) -> Option<String> {
    let out = run(
        "/usr/bin/codesign",
        &["-dv", "--verbose=2", &path.to_string_lossy()],
    )
    .ok()?;
    // codesign prints its description on stderr.
    parse_team_id(&String::from_utf8_lossy(&out.stderr))
}

pub fn parse_team_id(codesign_output: &str) -> Option<String> {
    codesign_output
        .lines()
        .find_map(|l| l.strip_prefix("TeamIdentifier="))
        .map(str::trim)
        .filter(|t| !t.is_empty() && *t != "not set")
        .map(str::to_string)
}

/// The team id in `pkgutil --check-signature` output, from the line naming
/// the Developer ID Installer certificate: "... Developer ID Installer: Acme (ABCDE12345)".
pub fn parse_pkg_team_id(pkgutil_output: &str) -> Option<String> {
    pkgutil_output
        .lines()
        .find(|l| l.contains("Developer ID Installer:"))
        .and_then(|l| {
            let open = l.rfind('(')?;
            let close = l.rfind(')')?;
            (close > open + 1).then(|| l[open + 1..close].to_string())
        })
}

/// `codesign --verify --deep --strict`: the signature is intact.
pub fn verify_signature(path: &Path) -> Result<(), String> {
    run_ok(
        "/usr/bin/codesign",
        &["--verify", "--deep", "--strict", &path.to_string_lossy()],
    )
    .map(|_| ())
    .map_err(|e| format!("signature check failed: {e}"))
}

/// Gatekeeper's verdict: signed with a Developer ID and notarized.
pub fn gatekeeper_accepts(path: &Path) -> bool {
    run(
        "/usr/sbin/spctl",
        &["--assess", "--type", "execute", &path.to_string_lossy()],
    )
    .map(|o| o.status.success())
    .unwrap_or(false)
}

/// Mount a disk image read-only, without showing it in the Finder.
pub fn attach_dmg(dmg: &Path) -> Result<PathBuf, String> {
    // Some images show a licence first; "Y" accepts it on stdin.
    let mut child = Command::new("/usr/bin/hdiutil")
        .args([
            "attach",
            "-plist",
            "-nobrowse",
            "-readonly",
            "-noautoopen",
            "-noverify",
        ])
        .arg(dmg)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("hdiutil: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() {
        use std::io::Write;
        let _ = stdin.write_all(b"Y\n");
    }
    let out = child
        .wait_with_output()
        .map_err(|e| format!("hdiutil: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "could not open the disk image: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    parse_mount_point(&out.stdout).ok_or_else(|| "the disk image has no volume".to_string())
}

/// The mount point in `hdiutil attach -plist` output. A licence agreement may
/// be printed before the plist, so parsing starts at `<?xml`.
pub fn parse_mount_point(stdout: &[u8]) -> Option<PathBuf> {
    let text = String::from_utf8_lossy(stdout);
    let start = text.find("<?xml")?;
    let value = plist::Value::from_reader_xml(text[start..].as_bytes()).ok()?;
    value
        .as_dictionary()?
        .get("system-entities")?
        .as_array()?
        .iter()
        .filter_map(|e| e.as_dictionary()?.get("mount-point")?.as_string())
        .map(PathBuf::from)
        .next()
}

pub fn detach_dmg(mount: &Path) {
    let _ = run(
        "/usr/bin/hdiutil",
        &["detach", "-force", &mount.to_string_lossy()],
    );
}

/// Is the app at `path` running? Looks for its executable in the process list.
pub fn is_running(path: &Path) -> bool {
    let pattern = format!("{}/Contents/MacOS/", path.to_string_lossy());
    run("/usr/bin/pgrep", &["-f", &pattern])
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Ask an app to quit, the way the Dock does, and wait up to `seconds`.
/// Never forces it: an app with unsaved work may ask the user first.
pub fn quit_app(path: &Path, bundle_id: &str, seconds: u64) -> bool {
    let script = format!(
        "tell application id {} to quit",
        applescript_quote(bundle_id)
    );
    let _ = run("/usr/bin/osascript", &["-e", &script]);
    for _ in 0..seconds * 4 {
        if !is_running(path) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    !is_running(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoting_survives_hostile_names() {
        assert_eq!(sh_quote("It's Here.app"), r"'It'\''s Here.app'");
        assert_eq!(applescript_quote(r#"say "hi" \o/"#), r#""say \"hi\" \\o/""#);
    }

    #[test]
    fn team_ids_from_codesign() {
        let signed = "Executable=/Applications/Rocket.app/Contents/MacOS/Rocket\nIdentifier=com.acme.rocket\nTeamIdentifier=ABCDE12345\n";
        assert_eq!(parse_team_id(signed).as_deref(), Some("ABCDE12345"));
        assert_eq!(
            parse_team_id("Identifier=x\nTeamIdentifier=not set\n"),
            None
        );
        assert_eq!(parse_team_id("code object is not signed at all"), None);
    }

    #[test]
    fn team_ids_from_pkgutil() {
        let out = "Package \"Rocket.pkg\":\n   Status: signed by a developer certificate issued by Apple for distribution\n   Notarization: trusted by the Apple notary service\n   Certificate Chain:\n    1. Developer ID Installer: Acme Inc (ABCDE12345)\n       Expires: 2030-01-01\n    2. Developer ID Certification Authority\n";
        assert_eq!(parse_pkg_team_id(out).as_deref(), Some("ABCDE12345"));
        assert_eq!(parse_pkg_team_id("Status: no signature"), None);
    }

    #[test]
    fn mount_point_after_a_licence() {
        let out = br#"Agreement text... Y
<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0"><dict><key>system-entities</key><array>
<dict><key>dev-entry</key><string>/dev/disk4</string></dict>
<dict><key>dev-entry</key><string>/dev/disk4s1</string><key>mount-point</key><string>/Volumes/Rocket 2.5</string></dict>
</array></dict></plist>"#;
        assert_eq!(
            parse_mount_point(out),
            Some(PathBuf::from("/Volumes/Rocket 2.5"))
        );
        assert_eq!(parse_mount_point(b"hdiutil: attach failed"), None);
    }
}
