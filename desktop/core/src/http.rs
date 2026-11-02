//! Web requests: downloads to a file with progress and Stop (yt-dlp and ffmpeg
//! updates, channel avatars), and the progress that yt-dlp's own downloads
//! report into.

use anyhow::{anyhow, bail, Context, Result};
use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Progress of the download in hand, read by the Activity page.
#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    /// 0 when the size isn't known.
    pub total: AtomicU64,
    /// Bytes a second (0 when unknown).
    pub speed: AtomicU64,
    /// Seconds left (0 when unknown).
    pub eta: AtomicU64,
    pub stop: AtomicBool,
}

impl Progress {
    pub fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }
    pub fn reset(&self) {
        self.done.store(0, Ordering::Relaxed);
        self.total.store(0, Ordering::Relaxed);
        self.speed.store(0, Ordering::Relaxed);
        self.eta.store(0, Ordering::Relaxed);
    }
}

pub type Shared = Arc<Progress>;

/// The app's web client: one agent, so connections are kept between requests.
pub struct Client {
    agent: ureq::Agent,
}

impl Client {
    pub fn new() -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(20))
            .timeout_read(std::time::Duration::from_secs(120))
            .try_proxy_from_env(true)
            .user_agent(&format!("Vid-Sync/{}", crate::VERSION))
            .build();
        Self { agent }
    }

    /// GET and read JSON. `what` names the thing asked for, for error messages.
    pub fn json(&self, url: &str, what: &str) -> Result<serde_json::Value> {
        let r = self.agent.get(url).call().map_err(|e| explain(e, what))?;
        let text = r.into_string().with_context(|| format!("couldn't read the answer for {what}"))?;
        serde_json::from_str(&text).with_context(|| format!("the answer for {what} wasn't JSON"))
    }

    /// Download to `dest`, counting into `progress` and stopping when asked.
    /// A `file://` address is copied (the tests' pictures).
    pub fn download(&self, url: &str, dest: &Path, progress: &Progress, what: &str) -> Result<u64> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(p) = url.strip_prefix("file://") {
            let p = crate::util::percent_decode(p);
            let n = std::fs::copy(&p, dest).with_context(|| format!("couldn't copy {p}"))?;
            return Ok(n);
        }
        let r = self.agent.get(url).call().map_err(|e| explain(e, what))?;
        let total = r.header("content-length").and_then(|v| v.parse::<u64>().ok()).unwrap_or(0);
        progress.reset();
        progress.total.store(total, Ordering::Relaxed);
        let mut reader = r.into_reader();
        let mut f = std::fs::File::create(dest).with_context(|| format!("couldn't create {}", dest.display()))?;
        let mut buf = vec![0u8; 256 * 1024];
        let mut n: u64 = 0;
        let started = std::time::Instant::now();
        loop {
            if progress.stopped() {
                drop(f);
                let _ = std::fs::remove_file(dest);
                bail!("Stopped.");
            }
            let k = reader.read(&mut buf).with_context(|| format!("the download of {what} broke off"))?;
            if k == 0 {
                break;
            }
            f.write_all(&buf[..k]).with_context(|| format!("couldn't write {}", dest.display()))?;
            n += k as u64;
            progress.done.store(n, Ordering::Relaxed);
            let secs = started.elapsed().as_secs_f64();
            if secs > 0.5 {
                let speed = (n as f64 / secs) as u64;
                progress.speed.store(speed, Ordering::Relaxed);
                if total > n && speed > 0 {
                    progress.eta.store((total - n) / speed, Ordering::Relaxed);
                }
            }
        }
        f.sync_all().ok();
        if total > 0 && n != total {
            let _ = std::fs::remove_file(dest);
            bail!("the download of {what} stopped short ({n} of {total} bytes)");
        }
        Ok(n)
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}

/// What the server said, in words for the user.
fn explain(e: ureq::Error, what: &str) -> anyhow::Error {
    match e {
        ureq::Error::Status(404, _) => anyhow!("{what} wasn't found on the server (404)."),
        ureq::Error::Status(403 | 429, _) => anyhow!("the server refused {what} for now (too many requests); try again later."),
        ureq::Error::Status(code, _) => anyhow!("the server answered {code} for {what}."),
        ureq::Error::Transport(t) => anyhow!("couldn't reach the server for {what}: {}", t.message().unwrap_or("no connection")),
    }
}
