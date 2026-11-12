//! The one place Camu talks to the network.
//!
//! Every checker takes a `&dyn Fetch`, so tests hand in canned responses and
//! never touch the internet.

use std::time::Duration;

/// Fetch a URL as text.
pub trait Fetch: Send + Sync {
    fn get(&self, url: &str) -> Result<String, String>;
}

/// The real thing, over HTTPS.
pub struct HttpFetch {
    agent: ureq::Agent,
}

impl HttpFetch {
    pub fn new() -> Self {
        let config = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(concat!(
                "Camu/",
                env!("CARGO_PKG_VERSION"),
                " (macOS update checker)"
            ))
            .build();
        Self {
            agent: config.into(),
        }
    }
}

impl Default for HttpFetch {
    fn default() -> Self {
        Self::new()
    }
}

/// Bodies larger than this are refused. The Homebrew cask index is the
/// biggest thing Camu downloads, at roughly 15 MB.
const MAX_BODY: u64 = 64 * 1024 * 1024;

impl Fetch for HttpFetch {
    fn get(&self, url: &str) -> Result<String, String> {
        if !(url.starts_with("https://") || url.starts_with("http://")) {
            return Err(format!("not a web address: {url}"));
        }
        let mut response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_BODY)
            .read_to_string()
            .map_err(|e| format!("{url}: {e}"))
    }
}

/// Downloads larger than this are refused (Xcode-sized apps aside, nothing
/// Camu updates comes close).
const MAX_DOWNLOAD: u64 = 8 * 1024 * 1024 * 1024;

impl HttpFetch {
    /// Download `url` to `dest`, calling `progress(received, total)` as bytes
    /// arrive. Only HTTPS: an update must not be tampered with on the way.
    pub fn download(
        &self,
        url: &str,
        dest: &std::path::Path,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<(), String> {
        use std::io::{Read, Write};

        if !url.starts_with("https://") {
            return Err(format!(
                "refusing to download over an insecure connection: {url}"
            ));
        }
        let response = self
            .agent
            .get(url)
            .call()
            .map_err(|e| format!("{url}: {e}"))?;
        let total = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        let mut reader = response
            .into_body()
            .into_with_config()
            .limit(MAX_DOWNLOAD)
            .reader();
        let mut file =
            std::fs::File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;

        let mut buf = vec![0u8; 256 * 1024];
        let mut received = 0u64;
        let mut last_report = 0u64;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("download interrupted: {e}"))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| format!("{}: {e}", dest.display()))?;
            received += n as u64;
            if received - last_report >= 512 * 1024 {
                progress(received, total);
                last_report = received;
            }
        }
        progress(received, total);
        if total.is_some_and(|t| t != received) {
            return Err("download ended early".to_string());
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod fake {
    use super::Fetch;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// Canned responses keyed by URL prefix. Records every URL asked for.
    #[derive(Default)]
    pub struct FakeFetch {
        pub responses: HashMap<String, String>,
        pub asked: Mutex<Vec<String>>,
    }

    impl FakeFetch {
        pub fn with(mut self, url_prefix: &str, body: &str) -> Self {
            self.responses
                .insert(url_prefix.to_string(), body.to_string());
            self
        }
    }

    impl Fetch for FakeFetch {
        fn get(&self, url: &str) -> Result<String, String> {
            self.asked.lock().unwrap().push(url.to_string());
            self.responses
                .iter()
                .filter(|(prefix, _)| url.starts_with(prefix.as_str()))
                .max_by_key(|(prefix, _)| prefix.len())
                .map(|(_, body)| body.clone())
                .ok_or_else(|| format!("no canned response for {url}"))
        }
    }
}
