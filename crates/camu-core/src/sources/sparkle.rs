//! Sparkle appcasts: an RSS feed an app points to with `SUFeedURL`.
//!
//! Format reference: https://sparkle-project.org/documentation/publishing/

use std::cmp::Ordering;

use super::{Finding, Source};
use crate::app::InstalledApp;
use crate::{notes, version};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Item {
    /// `sparkle:version`: the build number, compared with CFBundleVersion.
    pub version: Option<String>,
    /// `sparkle:shortVersionString`: the version people see.
    pub short_version: Option<String>,
    pub title: Option<String>,
    pub minimum_os: Option<String>,
    pub channel: Option<String>,
    pub description: Option<String>,
    pub release_notes_url: Option<String>,
    pub download_url: Option<String>,
    pub date: Option<String>,
    pub os: Option<String>,
    /// `sparkle:edSignature` on the enclosure.
    pub ed_signature: Option<String>,
}

impl Item {
    /// The version used to order items in the feed.
    fn sort_key(&self) -> Option<&str> {
        self.version.as_deref().or(self.short_version.as_deref())
    }

    fn display_version(&self) -> Option<&str> {
        self.short_version.as_deref().or(self.version.as_deref())
    }
}

/// Read every `<item>` in an appcast.
pub fn parse(xml: &str) -> Result<Vec<Item>, String> {
    let doc =
        roxmltree::Document::parse(xml).map_err(|e| format!("appcast is not valid XML: {e}"))?;
    let attr = |node: roxmltree::Node, name: &str| {
        node.attributes()
            .find(|a| a.name() == name)
            .map(|a| a.value().trim().to_string())
            .filter(|v| !v.is_empty())
    };
    let child_text = |node: roxmltree::Node, name: &str| {
        node.children()
            .find(|c| c.is_element() && c.tag_name().name() == name)
            .and_then(|c| c.text())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    };

    let items = doc
        .descendants()
        .filter(|n| n.is_element() && n.tag_name().name() == "item")
        .map(|item| {
            let enclosure = item
                .children()
                .find(|c| c.is_element() && c.tag_name().name() == "enclosure");
            let from_enclosure = |name: &str| enclosure.and_then(|e| attr(e, name));
            Item {
                version: child_text(item, "version").or_else(|| from_enclosure("version")),
                short_version: child_text(item, "shortVersionString")
                    .or_else(|| from_enclosure("shortVersionString")),
                title: child_text(item, "title"),
                minimum_os: child_text(item, "minimumSystemVersion"),
                channel: child_text(item, "channel"),
                description: child_text(item, "description"),
                release_notes_url: child_text(item, "releaseNotesLink"),
                download_url: from_enclosure("url"),
                date: child_text(item, "pubDate"),
                os: from_enclosure("os"),
                ed_signature: from_enclosure("edSignature"),
            }
        })
        .collect();
    Ok(items)
}

/// What to accept from a feed.
#[derive(Debug, Clone, Default)]
pub struct Filter<'a> {
    /// The running macOS version, e.g. "15.1". Items that need a newer macOS
    /// are skipped; `None` accepts every item.
    pub os_version: Option<&'a str>,
    /// Accept items on a named channel (Sparkle's betas).
    pub include_beta: bool,
    /// Only accept versions with this major number or lower ("stay on 3.x").
    pub max_major: Option<u64>,
}

/// The newest item this Mac can install.
pub fn newest<'a>(items: &'a [Item], filter: &Filter) -> Option<&'a Item> {
    items
        .iter()
        .filter(|i| i.sort_key().is_some())
        .filter(|i| filter.include_beta || i.channel.is_none())
        .filter(|i| {
            i.os.as_deref()
                .is_none_or(|os| os.eq_ignore_ascii_case("macos"))
        })
        .filter(|i| match (filter.os_version, i.minimum_os.as_deref()) {
            (Some(running), Some(needed)) => version::compare(needed, running) != Ordering::Greater,
            _ => true,
        })
        .filter(|i| {
            match (
                filter.max_major,
                i.display_version().and_then(version::major),
            ) {
                (Some(max), Some(m)) => m <= max,
                _ => true,
            }
        })
        .max_by(|a, b| version::compare(a.sort_key().unwrap(), b.sort_key().unwrap()))
}

/// Turn the newest item into a finding about `app`.
pub fn finding(app: &InstalledApp, item: &Item) -> Option<Finding> {
    let display = item.display_version()?.to_string();
    // Sparkle compares builds when both sides have one, as the app's own
    // updater would.
    let is_update = match (item.version.as_deref(), app.build.as_deref()) {
        (Some(remote), Some(local)) => version::is_newer(remote, local),
        _ => version::is_update(&display, None, &app.version, app.build.as_deref()),
    };
    let mut f = Finding::new(Source::Sparkle, display);
    f.build = item.version.clone();
    f.signature = item.ed_signature.clone();
    f.is_update = is_update;
    f.release_notes = item
        .description
        .as_deref()
        .map(notes::to_text)
        .filter(|t| !t.is_empty());
    f.release_notes_url = item.release_notes_url.clone();
    f.download_url = item.download_url.clone();
    f.minimum_os = item.minimum_os.clone();
    f.date = item.date.clone();
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    const APPCAST: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<rss version="2.0" xmlns:sparkle="http://www.andymatuschak.org/xml-namespaces/sparkle">
  <channel>
    <title>Rocket</title>
    <item>
      <title>Version 2.5 beta</title>
      <sparkle:channel>beta</sparkle:channel>
      <sparkle:version>530</sparkle:version>
      <sparkle:shortVersionString>2.5b1</sparkle:shortVersionString>
      <enclosure url="https://acme.com/rocket-530.zip" length="1"/>
    </item>
    <item>
      <title>Version 2.4.2</title>
      <pubDate>Mon, 03 Nov 2026 10:00:00 +0000</pubDate>
      <sparkle:minimumSystemVersion>13.0</sparkle:minimumSystemVersion>
      <description><![CDATA[<ul><li>Faster launch</li></ul>]]></description>
      <enclosure url="https://acme.com/rocket-520.zip" sparkle:version="520"
                 sparkle:shortVersionString="2.4.2" length="1" type="application/octet-stream"/>
    </item>
    <item>
      <title>Version 3.0</title>
      <sparkle:minimumSystemVersion>27.0</sparkle:minimumSystemVersion>
      <enclosure url="https://acme.com/rocket-600.zip" sparkle:version="600" sparkle:shortVersionString="3.0"/>
    </item>
    <item>
      <title>Version 2.4.1</title>
      <enclosure url="https://acme.com/rocket-512.zip" sparkle:version="512" sparkle:shortVersionString="2.4.1"/>
    </item>
  </channel>
</rss>"#;

    fn rocket() -> InstalledApp {
        InstalledApp {
            path: "/Applications/Rocket.app".into(),
            file_name: "Rocket".into(),
            name: "Rocket".into(),
            bundle_id: "com.acme.rocket".into(),
            version: "2.4.1".into(),
            build: Some("512".into()),
            sparkle_feed: Some("https://acme.com/appcast.xml".into()),
            electron_feed: None,
            from_app_store: false,
            minimum_os: None,
            icon_file: None,
            ..Default::default()
        }
    }

    #[test]
    fn reads_items_from_attributes_and_elements() {
        let items = parse(APPCAST).unwrap();
        assert_eq!(items.len(), 4);
        assert_eq!(items[0].version.as_deref(), Some("530"));
        assert_eq!(items[0].channel.as_deref(), Some("beta"));
        assert_eq!(items[1].short_version.as_deref(), Some("2.4.2"));
        assert_eq!(
            items[1].download_url.as_deref(),
            Some("https://acme.com/rocket-520.zip")
        );
    }

    #[test]
    fn skips_betas_and_items_this_mac_cannot_run() {
        let items = parse(APPCAST).unwrap();
        let filter = Filter {
            os_version: Some("15.1"),
            include_beta: false,
            ..Default::default()
        };
        let item = newest(&items, &filter).unwrap();
        assert_eq!(item.short_version.as_deref(), Some("2.4.2"));

        let finding = finding(&rocket(), item).unwrap();
        assert!(finding.is_update);
        assert_eq!(finding.version, "2.4.2");
        assert_eq!(finding.release_notes.as_deref(), Some("• Faster launch"));
    }

    #[test]
    fn beta_opt_in_and_unknown_os() {
        let items = parse(APPCAST).unwrap();
        let beta = Filter {
            os_version: Some("15.1"),
            include_beta: true,
            ..Default::default()
        };
        assert_eq!(
            newest(&items, &beta).unwrap().version.as_deref(),
            Some("530")
        );
        let any_os = Filter::default();
        assert_eq!(
            newest(&items, &any_os).unwrap().version.as_deref(),
            Some("600")
        );
    }

    #[test]
    fn up_to_date_when_builds_match() {
        let mut app = rocket();
        app.version = "2.4.2".into();
        app.build = Some("520".into());
        let items = parse(APPCAST).unwrap();
        let item = newest(
            &items,
            &Filter {
                os_version: Some("15.1"),
                include_beta: false,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(!finding(&app, item).unwrap().is_update);
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse("<html><body>404").is_err());
    }
}
