//! Comparing version strings the way people write them.
//!
//! Developers write versions as "2.4.1", "2.4", "2.4.1b3", "v2.4 (512)",
//! "2024.11" or Homebrew's "2.4.1,512". There is no single standard, so the
//! comparison is deliberately forgiving:
//!
//! - Numbers compare as numbers, so "1.10" is newer than "1.9".
//! - Missing trailing zeros do not matter, so "1.0" equals "1.0.0".
//! - A pre-release word makes a version older than the same version without
//!   it, so "1.0b2" is older than "1.0", and dev < alpha < beta < rc.
//! - Anything after a space or a comma (a build number in brackets, a
//!   Homebrew build suffix) is ignored.

use std::cmp::Ordering;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Number(u64),
    Word(String),
}

/// Strip the parts of a version string that are not the version itself.
pub fn clean(raw: &str) -> &str {
    let raw = raw.trim();
    let raw = raw.split([',', ' ', '(']).next().unwrap_or("");
    raw.strip_prefix(['v', 'V'])
        .filter(|rest| rest.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(raw)
}

fn tokens(raw: &str) -> Vec<Token> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut digits = false;

    let flush = |current: &mut String, digits: bool, out: &mut Vec<Token>| {
        if current.is_empty() {
            return;
        }
        if digits {
            // A number too large for u64 is not a version anyone ships; cap it.
            out.push(Token::Number(current.parse().unwrap_or(u64::MAX)));
        } else {
            out.push(Token::Word(current.to_ascii_lowercase()));
        }
        current.clear();
    };

    for c in clean(raw).chars() {
        if c.is_ascii_digit() {
            if !digits {
                flush(&mut current, digits, &mut out);
            }
            digits = true;
            current.push(c);
        } else if c.is_alphanumeric() {
            if digits {
                flush(&mut current, digits, &mut out);
            }
            digits = false;
            current.push(c);
        } else {
            flush(&mut current, digits, &mut out);
        }
    }
    flush(&mut current, digits, &mut out);
    out
}

/// Rank of a pre-release word. Unknown words sort after the known ones.
fn word_rank(word: &str) -> u8 {
    match word {
        "dev" | "d" | "nightly" | "snapshot" => 0,
        "alpha" | "a" => 1,
        "beta" | "b" => 2,
        "rc" | "pre" | "preview" | "candidate" => 3,
        _ => 4,
    }
}

/// Compare two version strings. `Greater` means `a` is newer than `b`.
pub fn compare(a: &str, b: &str) -> Ordering {
    let a = tokens(a);
    let b = tokens(b);
    let len = a.len().max(b.len());

    for i in 0..len {
        let ordering = match (a.get(i), b.get(i)) {
            (Some(Token::Number(x)), Some(Token::Number(y))) => x.cmp(y),
            (Some(Token::Word(x)), Some(Token::Word(y))) => {
                word_rank(x).cmp(&word_rank(y)).then_with(|| x.cmp(y))
            }
            // "1.0.1" against "1.0b1": the release number wins.
            (Some(Token::Number(_)), Some(Token::Word(_))) => Ordering::Greater,
            (Some(Token::Word(_)), Some(Token::Number(_))) => Ordering::Less,
            // One side has run out. Extra zeros change nothing, extra numbers
            // make a version newer, and an extra word makes it a pre-release.
            (Some(Token::Number(n)), None) => n.cmp(&0),
            (None, Some(Token::Number(n))) => 0.cmp(n),
            (Some(Token::Word(_)), None) => Ordering::Less,
            (None, Some(Token::Word(_))) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        };
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    Ordering::Equal
}

/// True when `candidate` is a newer version than `installed`.
pub fn is_newer(candidate: &str, installed: &str) -> bool {
    // A blank or unparseable remote version is never an update.
    !tokens(candidate).is_empty() && compare(candidate, installed) == Ordering::Greater
}

/// Split a published version into its release and, when one is attached,
/// a build. Publishers attach a lot to the release number, and none of it
/// means "newer release":
///
/// - `2.5.0,530`: Homebrew's build after a comma.
/// - `3.12.1059+next.0122b4`: semver build metadata after a plus.
/// - `27.1.0-58756`, `2.26.3_1`: a build number after a dash or underscore.
/// - `3.6.11-0102f49a`, `1.52.0-36aa9fba`: a commit hash, dropped.
/// - `6.7.0.3`: an extra fourth number on a three-part `installed` version,
///   read as a build when the rest matches exactly.
///
/// Pre-release words (`1.0.0-beta.2`) stay part of the release.
pub fn split_release<'a>(raw: &'a str, installed: &str) -> (&'a str, Option<&'a str>) {
    let raw = raw.trim();
    let (main, mut build) = match raw.split_once(',') {
        Some((main, rest)) => (main, rest.split(',').next().filter(|b| !b.is_empty())),
        None => (raw, None),
    };
    let mut main = clean(main);

    // Semver build metadata never affects which version is newer.
    if let Some((head, meta)) = main.split_once('+') {
        main = head;
        if build.is_none() && is_number(meta) {
            build = Some(meta);
        }
    }
    // Commit hashes, build codes and build numbers after a dash or
    // underscore, peeled off from the end ("2.1.20-14-5a3070e4").
    for _ in 0..8 {
        let Some(i) = main.rfind(['-', '_']) else {
            break;
        };
        let (head, tail) = (&main[..i], &main[i + 1..]);
        if !head.contains('.') {
            break;
        }
        let revision =
            head.matches('.').count() >= 2 && tail.split('.').all(is_number) && tail.contains('.');
        if is_commit(tail) || is_build_code(tail) || is_hex_chunk(tail) || revision {
            main = head;
        } else if is_number(tail) {
            main = head;
            build = build.or(Some(tail));
        } else {
            break;
        }
    }
    // A release-channel word that isn't a pre-release: "26.10.2.103-stable".
    if let Some(i) = main.rfind(['-', '.', '_']) {
        let tail = main[i + 1..].to_ascii_lowercase();
        if i > 0 && ["stable", "release", "final", "latest", "lts", "ga"].contains(&tail.as_str()) {
            main = &main[..i];
        }
    }
    // An extra number on top of exactly the installed three-part version.
    let installed = clean(installed);
    if installed.split('.').count() >= 3 {
        if let Some(extra) = main
            .strip_prefix(installed)
            .and_then(|rest| rest.strip_prefix('.'))
            .filter(|n| is_number(n))
        {
            main = &main[..installed.len()];
            // With a comma build as well, it's unclear which one the app
            // calls its build, so compare neither.
            build = if build.is_none() { Some(extra) } else { None };
        }
    }
    (main, build)
}

fn is_number(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// A build code like "8A90" or "105c": starts with a digit and has letters,
/// so it can't be a pre-release word such as "beta2" or "rc1".
fn is_build_code(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_digit())
        && s.bytes().all(|b| b.is_ascii_alphanumeric())
        && s.bytes().any(|b| b.is_ascii_alphabetic())
}

/// A chunk of a UUID or short hash like "a4d9": four or more hex digits
/// mixing letters and numbers.
fn is_hex_chunk(s: &str) -> bool {
    s.len() >= 4
        && s.bytes().all(|b| b.is_ascii_hexdigit())
        && s.bytes().any(|b| b.is_ascii_alphabetic())
        && s.bytes().any(|b| b.is_ascii_digit())
}

/// A git commit hash: seven or more hex digits, at least one a letter.
fn is_commit(s: &str) -> bool {
    s.len() >= 7
        && s.bytes().all(|b| b.is_ascii_hexdigit())
        && s.bytes().any(|b| b.is_ascii_alphabetic())
}

/// True when a published version is an update to the installed app, for
/// every source. The remote release must be newer, or the same release with
/// a newer build. Builds are compared only when both are plain numbers,
/// since some apps put their version, not a counter, in CFBundleVersion.
pub fn is_update(
    remote: &str,
    remote_build: Option<&str>,
    installed: &str,
    installed_build: Option<&str>,
) -> bool {
    let (release, attached) = split_release(remote, installed);
    let (current, current_attached) = split_release(installed, "");
    if is_newer(release, current) {
        return true;
    }
    let ours = installed_build.or(current_attached);
    compare(release, current) == Ordering::Equal
        && match (remote_build.or(attached), ours) {
            (Some(new), Some(old)) if is_number(new) && is_number(old) => is_newer(new, old),
            _ => false,
        }
}

#[cfg(test)]
mod tests {

    /// Formats found across Homebrew's catalog and in real apps. None of these
    /// may read as an update when the app is current.
    #[test]
    fn attached_builds_and_hashes_are_not_updates() {
        let same = [
            ("27.1.0-58756", "27.1.0", Some("58756")),     // Parallels
            ("6.7.0.3", "6.7.0", None),                    // MEGAsync
            ("3.6.11-0102f49a", "3.6.11", Some("3.6.11")), // GitHub Desktop
            ("1.52.0-36aa9fba", "1.52.0", None),
            ("2026.11.11-043ef28", "2026.11.11", None),
            ("3.12.1059+next.0122b42e15", "3.12.1059", None),
            ("1.0.2115+e16833f284", "1.0.2115", None),
            ("0.14.2+239", "0.14.2", None),
            ("2.10.0+0,021000.00", "2.10.0", None),
            ("2.26.3_1", "2.26.3", None),
            ("9.8.2_1", "9.8.2", None),
            ("2.5.0,530", "2.5.0", Some("530")),
            ("4.21.0_4588", "4.21.0", Some("4588")),
            ("v1.4.2", "1.4.2", None),
            ("114.0.4-release.20250509.32955", "114.0.4", None),
            ("26.10.2.103-stable", "26.10.2.103", None),
            ("26.10.2.103-stable", "26.10.2", None),
            ("6.8.8.2,131593", "6.8.8", Some("131593")),
            ("3.0.24.84.Release", "3.0.24", None),
            ("1.1.0.6142,-final", "1.1.0", None),
            ("2.1.20-14-5a3070e4", "2.1.20", None),
            ("8.0.11-8A90", "8.0.11", None),
            ("156.0.8078.79-1.1", "156.0.8078.79", None),
            ("150.2.1-105c", "150.2.1", None),
            (
                "1.10.0.1489-68f77abf-a4d9-36cb-8be3-b50f96446ef9",
                "1.10.0.1489",
                None,
            ),
        ];
        for (remote, installed, build) in same {
            assert!(
                !is_update(remote, None, installed, build),
                "{remote} vs {installed}"
            );
        }
    }

    #[test]
    fn real_updates_still_count() {
        let newer = [
            ("27.1.0-58756", "27.1.0", Some("58700")), // newer build, same release
            ("27.1.0-58756", "26.2.1", Some("57300")),
            ("6.7.0.3", "6.6.2", None),
            ("3.6.11-0102f49a", "3.6.10", Some("3.6.10")),
            ("1.2.1", "1.2", None), // a real point release
            ("1.2.4", "1.2.3", None),
            ("2.5.0,530", "2.5.0", Some("520")),
            ("2.26.4_1", "2.26.3", None),
            ("1.0.0", "1.0.0-beta.1", None),
            ("3.12.1060+abc1234", "3.12.1059", None),
            ("2.1.21-14-5a3070e4", "2.1.20", None),
            ("1.0.0-rc1", "1.0.0-beta2", None),
        ];
        for (remote, installed, build) in newer {
            assert!(
                is_update(remote, None, installed, build),
                "{remote} vs {installed}"
            );
        }
        // An explicit remote build (Sparkle) is used when given.
        assert!(is_update("5.2", Some("5200"), "5.2", Some("5100")));
        assert!(!is_update("5.2", Some("5100"), "5.2", Some("5100")));
        // Older is never an update.
        assert!(!is_update("1.9", None, "1.10", None));
        assert!(!is_update("1.0.0-beta.2", None, "1.0.0", None));
    }
    use super::*;

    #[test]
    fn numbers_compare_numerically() {
        assert!(is_newer("1.10", "1.9"));
        assert!(is_newer("2.0", "1.99.99"));
        assert!(!is_newer("1.9", "1.10"));
    }

    #[test]
    fn trailing_zeros_are_equal() {
        assert_eq!(compare("1.0", "1.0.0"), Ordering::Equal);
        assert_eq!(compare("3", "3.0.0.0"), Ordering::Equal);
        assert!(is_newer("1.0.1", "1.0"));
    }

    #[test]
    fn pre_releases_are_older_than_releases() {
        assert!(is_newer("1.0", "1.0b2"));
        assert!(is_newer("1.0b3", "1.0b2"));
        assert!(is_newer("1.0rc1", "1.0beta5"));
        assert!(is_newer("1.0beta1", "1.0alpha9"));
        assert!(is_newer("1.0.1", "1.0b1"));
        assert!(is_newer("2.0-beta.2", "2.0-beta.1"));
    }

    #[test]
    fn decorations_are_ignored() {
        assert_eq!(compare("v2.4.1", "2.4.1"), Ordering::Equal);
        assert_eq!(compare("2.4.1 (512)", "2.4.1"), Ordering::Equal);
        assert_eq!(compare("2.4.1,512", "2.4.1"), Ordering::Equal);
        assert_eq!(clean("  v3.2 (99) "), "3.2");
    }

    #[test]
    fn blank_remote_is_never_an_update() {
        assert!(!is_newer("", "1.0"));
        assert!(!is_newer("   ", "1.0"));
    }

    #[test]
    fn date_style_versions() {
        assert!(is_newer("2024.11.2", "2024.9.30"));
    }
}
