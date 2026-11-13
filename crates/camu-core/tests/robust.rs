//! Every parser that reads text from the internet or from an app's files
//! must never panic: Camu is built with `panic = "abort"`, so one panic on a
//! worker thread closes the whole app. These feed them random text full of
//! multi-byte characters and the punctuation the parsers split on.

use camu_core::arch::{build_of, Arch};
use camu_core::sources::{appstore, electron, github, homebrew, sparkle, Source};
use camu_core::{notes, version, InstalledApp};

/// A small deterministic generator, so a failure always reproduces.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick<'a>(&mut self, items: &[&'a str]) -> &'a str {
        items[(self.next() % items.len() as u64) as usize]
    }
}

const PIECES: &[&str] = &[
    "1",
    "2",
    "10",
    "26",
    ".",
    "..",
    "-",
    "_",
    ",",
    "+",
    "&",
    ";",
    "<",
    ">",
    "/",
    " ",
    "\n",
    "beta",
    "rc1",
    "stable",
    "build",
    "v",
    "a",
    "Z",
    "arm64",
    "x64",
    "universal",
    "&amp;",
    "&#x4e2d;",
    "&#20013;",
    "&nbsp",
    "<p>",
    "</p>",
    "<script>",
    "<br/>",
    "改善",
    "新機能",
    "修正",
    "🚀",
    "✨",
    "é",
    "ü",
    "ß",
    "—",
    "\u{200b}",
    "\u{0}",
    "%",
    "\"",
    "'",
    "(",
    ")",
    "5a3070e4",
    "0x",
    "deadbeef-1234",
    "dmg",
    "zip",
    ".app",
    "http://",
    "https://",
];

fn random_text(rng: &mut Rng, max: u64) -> String {
    let n = rng.next() % max;
    (0..n).map(|_| rng.pick(PIECES)).collect()
}

fn app(version: &str) -> InstalledApp {
    InstalledApp {
        name: "Rocket".into(),
        bundle_id: "com.acme.rocket".into(),
        version: version.into(),
        ..Default::default()
    }
}

#[test]
fn text_parsers_never_panic() {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20_000 {
        let a = random_text(&mut rng, 24);
        let b = random_text(&mut rng, 24);
        let _ = notes::to_text(&a);
        let _ = version::clean(&a);
        let _ = version::compare(&a, &b);
        let _ = version::is_newer(&a, &b);
        let _ = version::split_release(&a, &b);
        let _ = version::is_update(&a, Some(&b), &b, None);
        let _ = version::is_update(&a, None, &b, Some(&a));
        let _ = version::major(&a);
        let _ = version::is_major_jump(&a, &b);
        let _ = build_of(&a);
        let _ = github::repo_from_url(&a);
    }
}

#[test]
fn notes_with_an_ampersand_before_wide_characters() {
    // "&" then multi-byte characters: cutting the entity search at a fixed
    // byte count used to land inside a character and panic.
    assert_eq!(notes::to_text("R& 新機能を追加"), "R& 新機能を追加");
    assert_eq!(notes::to_text("Fixes & 🚀🚀🚀"), "Fixes & 🚀🚀🚀");
    assert_eq!(notes::to_text("a &amp; b"), "a & b");
}

#[test]
fn feed_parsers_never_panic() {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    for _ in 0..3_000 {
        let v = random_text(&mut rng, 8);
        let installed = app(&random_text(&mut rng, 8));
        let notes_text = random_text(&mut rng, 30);
        let url = random_text(&mut rng, 8);

        let appcast = format!(
            r#"<?xml version="1.0"?><rss xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle"><channel><item><title>{v}</title><sparkle:version>{v}</sparkle:version><sparkle:shortVersionString>{v}</sparkle:shortVersionString><description><![CDATA[{notes_text}]]></description><enclosure url="https://x/{url}" sparkle:version="{v}" length="1"/></item></channel></rss>"#
        );
        if let Ok(items) = sparkle::parse(&appcast) {
            let filter = sparkle::Filter {
                os_version: Some("26.7.1"),
                include_beta: true,
                max_major: None,
            };
            if let Some(item) = sparkle::newest(&items, &filter) {
                let _ = sparkle::finding(&installed, item);
            }
            for item in &items {
                let _ = sparkle::finding(&installed, item);
            }
        }

        let release = serde_json::json!({
            "tag_name": v, "name": v, "body": notes_text, "html_url": "https://github.com/a/b",
            "assets": [{ "name": format!("{url}.dmg"), "browser_download_url": format!("https://x/{url}.dmg") }]
        })
        .to_string();
        for arch in [Arch::Arm64, Arch::X86_64] {
            let _ = github::parse_for(&release, &installed, Source::Github, arch);
        }

        let lookup = serde_json::json!({
            "resultCount": 1,
            "results": [{ "version": v, "releaseNotes": notes_text, "trackViewUrl": "https://apps.apple.com/x", "minimumOsVersion": url }]
        })
        .to_string();
        let _ = appstore::parse(&lookup, &installed);

        let yml = format!(
            "version: {v}\nfiles:\n  - url: {url}.zip\npath: {url}\nreleaseNotes: {notes_text}\n"
        );
        let _ = electron::parse(&yml, "https://x/", &installed);

        let casks = serde_json::json!([{
            "token": "rocket", "version": v, "url": format!("https://x/{url}"), "homepage": "https://x",
            "artifacts": [{ "app": ["Rocket.app"] }, { "uninstall": [{ "quit": "com.acme.rocket" }] }],
            "auto_updates": false, "caveats": notes_text
        }])
        .to_string();
        let _ = homebrew::CaskIndex::parse(&casks);
    }
}
