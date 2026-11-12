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

#[cfg(test)]
mod tests {
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
