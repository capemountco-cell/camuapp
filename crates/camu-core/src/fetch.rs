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
    /// For feeds and API calls: small replies, so a whole request gets 20s.
    agent: ureq::Agent,
    /// For downloads: no limit on the total time, since a large app on a slow
    /// line can take many minutes, but the server must keep sending.
    downloads: ureq::Agent,
}

const USER_AGENT: &str = concat!(
    "Camu/",
    env!("CARGO_PKG_VERSION"),
    " (macOS update checker)"
);

/// A download fails only when the server stops sending for this long.
const STALL: Duration = Duration::from_secs(60);

impl HttpFetch {
    pub fn new() -> Self {
        Self::with_stall(STALL)
    }

    fn with_stall(stall: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(20)))
            .user_agent(USER_AGENT)
            .build();
        let downloads = ureq::Agent::config_builder()
            .timeout_global(None)
            .timeout_connect(Some(Duration::from_secs(30)))
            .timeout_recv_response(Some(stall))
            .timeout_per_read(Some(stall))
            .user_agent(USER_AGENT)
            .build();
        Self {
            agent: agent.into(),
            downloads: downloads.into(),
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
    ///
    /// A connection that drops part-way is picked up again where it stopped
    /// (an HTTP range request), up to a few times, so a flaky network doesn't
    /// throw away a large download.
    pub fn download(
        &self,
        url: &str,
        dest: &std::path::Path,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<(), String> {
        if !url.starts_with("https://") {
            return Err(format!(
                "refusing to download over an insecure connection: {url}"
            ));
        }
        self.download_any(url, dest, progress)
    }

    /// `download` without the HTTPS check, so tests can use a local server.
    fn download_any(
        &self,
        url: &str,
        dest: &std::path::Path,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<(), String> {
        let mut file =
            std::fs::File::create(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        let mut received = 0u64;
        let mut total = None;
        let mut attempt = 0;
        loop {
            match self.download_from(url, &mut file, &mut received, &mut total, progress) {
                Ok(()) => break,
                Err(Step::Fatal(e)) => return Err(e),
                Err(Step::Retry(e)) => {
                    attempt += 1;
                    if attempt > RETRIES {
                        return Err(e);
                    }
                    std::thread::sleep(Duration::from_secs(2 * attempt as u64));
                }
            }
        }
        progress(received, total);
        if total.is_some_and(|t| t != received) {
            return Err("download ended early".to_string());
        }
        Ok(())
    }

    /// One attempt: (re)start at `received` bytes and append to `file`.
    fn download_from(
        &self,
        url: &str,
        file: &mut std::fs::File,
        received: &mut u64,
        total: &mut Option<u64>,
        progress: &dyn Fn(u64, Option<u64>),
    ) -> Result<(), Step> {
        use std::io::{Read, Seek, SeekFrom, Write};

        let mut request = self.downloads.get(url);
        if *received > 0 {
            request = request.header("Range", &format!("bytes={received}-"));
        }
        let response = request
            .call()
            .map_err(|e| Step::Retry(describe(&e.to_string())))?;
        let length = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.parse::<u64>().ok());
        if *received > 0 && response.status().as_u16() != 206 {
            // The server ignored the range: start over from the beginning.
            *received = 0;
            file.set_len(0).map_err(|e| Step::Fatal(e.to_string()))?;
        }
        file.seek(SeekFrom::Start(*received))
            .map_err(|e| Step::Fatal(e.to_string()))?;
        if total.is_none() || *received == 0 {
            *total = length.map(|l| l + *received);
        }

        let mut reader = response
            .into_body()
            .into_with_config()
            .limit(MAX_DOWNLOAD)
            .reader();
        let mut buf = vec![0u8; 256 * 1024];
        let mut last_report = *received;
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| Step::Retry(describe(&e.to_string())))?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n])
                .map_err(|e| Step::Fatal(e.to_string()))?;
            *received += n as u64;
            if *received - last_report >= 512 * 1024 {
                progress(*received, *total);
                last_report = *received;
            }
        }
        if total.is_some_and(|t| t > *received) {
            return Err(Step::Retry("the connection closed early".into()));
        }
        Ok(())
    }
}

/// How many times an interrupted download is picked up again.
const RETRIES: u32 = 4;

enum Step {
    /// Worth trying again from where it stopped.
    Retry(String),
    /// Trying again won't help (disk full, file unwritable).
    Fatal(String),
}

/// A download error in words a person can act on.
fn describe(raw: &str) -> String {
    if raw.contains("timeout") {
        "the download stalled: the server stopped sending for a minute".into()
    } else {
        format!("download interrupted: {raw}")
    }
}

#[cfg(test)]
mod download_tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    /// A tiny HTTP server that answers each connection with `respond`,
    /// given the request's Range start (if any) and the connection number.
    fn serve(
        respond: impl Fn(Option<u64>, usize, &mut std::net::TcpStream) + Send + 'static,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            for (n, stream) in listener.incoming().enumerate() {
                let mut stream = stream.unwrap();
                let mut range = None;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(v) = line.to_ascii_lowercase().strip_prefix("range: bytes=") {
                        range = v.trim().trim_end_matches('-').parse().ok();
                    }
                }
                respond(range, n, &mut stream);
            }
        });
        format!("http://{addr}/app.dmg")
    }

    fn scratch() -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        std::env::temp_dir().join(format!("camu-dl-test-{}-{n}.dmg", std::process::id()))
    }

    fn body() -> Vec<u8> {
        (0..300_000u32).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn resumes_a_dropped_download_where_it_stopped() {
        let url = serve(|range, n, s| {
            let all = body();
            match (n, range) {
                (0, None) => {
                    // Promise everything, send a third, then drop.
                    write!(
                        s,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        all.len()
                    )
                    .unwrap();
                    s.write_all(&all[..100_000]).unwrap();
                }
                (_, Some(from)) => {
                    let rest = &all[from as usize..];
                    write!(
                        s,
                        "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\n\r\n",
                        rest.len()
                    )
                    .unwrap();
                    s.write_all(rest).unwrap();
                }
                _ => {}
            }
        });
        let dest = scratch();
        HttpFetch::new()
            .download_any(&url, &dest, &|_, _| {})
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body());
    }

    #[test]
    fn starts_over_when_the_server_ignores_ranges() {
        let url = serve(|_range, n, s| {
            let all = body();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                all.len()
            )
            .unwrap();
            let upto = if n == 0 { 50_000 } else { all.len() };
            s.write_all(&all[..upto]).unwrap();
        });
        let dest = scratch();
        HttpFetch::new()
            .download_any(&url, &dest, &|_, _| {})
            .unwrap();
        assert_eq!(std::fs::read(&dest).unwrap(), body());
    }

    #[test]
    fn a_slow_download_longer_than_the_feed_timeout_finishes() {
        // Trickles for ~3s overall with short pauses: no total time limit.
        let url = serve(|_range, _n, s| {
            let all = body();
            write!(
                s,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                all.len()
            )
            .unwrap();
            for chunk in all.chunks(30_000) {
                s.write_all(chunk).unwrap();
                std::thread::sleep(Duration::from_millis(300));
            }
        });
        let dest = scratch();
        let fetch = HttpFetch::with_stall(Duration::from_secs(2));
        fetch.download_any(&url, &dest, &|_, _| {}).unwrap();
        assert_eq!(std::fs::read(&dest).unwrap().len(), body().len());
    }

    #[test]
    fn a_stalled_server_fails_with_a_clear_message() {
        let url = serve(|_range, _n, s| {
            write!(s, "HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n").unwrap();
            s.write_all(&[0; 10]).unwrap();
            std::thread::sleep(Duration::from_secs(30));
        });
        let dest = scratch();
        let fetch = HttpFetch::with_stall(Duration::from_millis(300));
        let err = fetch.download_any(&url, &dest, &|_, _| {}).unwrap_err();
        assert!(
            err.contains("stalled") || err.contains("closed early"),
            "{err}"
        );
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
