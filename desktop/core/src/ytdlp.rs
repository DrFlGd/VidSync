//! Running yt-dlp: listing a channel's tab or a playlist (flat, fast), reading
//! one video's details, and downloading a video with its metadata and picture
//! into a folder, with progress and Stop. yt-dlp is a program beside the app
//! (tools.rs finds it), never linked in, so it can be updated by itself.

use crate::config::{Cookies, Polite};
use crate::http::Progress;
use crate::library::Quality;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::Ordering;
use std::sync::Mutex;

/// The step in hand, for the Activity page.
pub type Stage = Mutex<String>;

pub fn set_stage(stage: &Stage, s: impl Into<String>) {
    *stage.lock().unwrap() = s.into();
}

#[derive(Clone, Debug)]
pub struct YtDlp {
    pub exe: PathBuf,
    pub ffmpeg: Option<PathBuf>,
    pub cookies: Cookies,
    pub polite: Polite,
}

/// A video YouTube shows only to a signed-in adult, for Settings → Cookies → Test
/// (the age-gated trailer yt-dlp's own tests use).
pub const COOKIE_TEST_VIDEO: &str = "HtVdAasjOgU";

/// What explain() says when YouTube wants a sign-in to go on at all: the
/// schedule pauses for a while when a job meets it.
pub const BOT_CHECK: &str = "YouTube asks for a sign-in: add cookies in Settings";

/// What a flat listing gave: the source's own details and its entries.
#[derive(Debug, Default)]
pub struct Listing {
    pub meta: Value,
    pub entries: Vec<Value>,
}

/// A command for a program, with the console window kept hidden on Windows, and
/// a `.py` run through Python (the tests' stand-in).
pub fn command(exe: &Path) -> Command {
    let mut c = if exe.extension().is_some_and(|e| e == "py") {
        let py = std::env::var("VIDSYNC_PYTHON").unwrap_or_else(|_| if cfg!(windows) { "python".into() } else { "python3".into() });
        let mut c = Command::new(py);
        c.arg(exe);
        c
    } else {
        Command::new(exe)
    };
    c.stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    c
}

/// `<program> --version`'s first line, or an error.
pub fn version_of(exe: &Path, flag: &str) -> Result<String> {
    let out = command(exe).arg(flag).stdout(Stdio::piped()).stderr(Stdio::piped()).output().with_context(|| format!("couldn't run {}", exe.display()))?;
    let text = String::from_utf8_lossy(if out.stdout.is_empty() { &out.stderr } else { &out.stdout }).into_owned();
    let line = text.lines().next().unwrap_or("").trim().to_string();
    if !out.status.success() && line.is_empty() {
        bail!("{} didn't answer {flag}", exe.display());
    }
    Ok(line.strip_prefix("ffmpeg version ").map(|v| v.split_whitespace().next().unwrap_or(v).to_string()).unwrap_or(line))
}

/// What yt-dlp's error means, in a few words for the audit.
pub fn explain(stderr: &str) -> String {
    let errors: Vec<&str> = stderr.lines().filter(|l| l.starts_with("ERROR:")).collect();
    let text = errors.join("\n");
    let t = text.to_lowercase();
    let reason = if t.contains("could not copy") && t.contains("cookie database") {
        "The browser holds its cookies while it's open: close it and try again, or use Firefox"
    } else if t.contains("dpapi") || t.contains("app-bound") || t.contains("failed to decrypt") {
        "Chrome and Edge lock their cookies on Windows: use Firefox, or a cookies file"
    } else if (t.contains("could not find") || t.contains("couldn't find")) && t.contains("cookies database") {
        "That browser has no cookies on this computer: is it installed and signed in to YouTube?"
    } else if t.contains("unsupported browser") {
        "yt-dlp can't read that browser's cookies"
    } else if t.contains("cookies") && (t.contains("does not look like a netscape format") || t.contains("no such file")) {
        "The cookies file can't be read: export it again in Netscape (cookies.txt) format"
    } else if t.contains("members-only") || t.contains("join this channel") || t.contains("members only") {
        "Members only"
    } else if t.contains("private video") || t.contains("this video is private") {
        "Private video"
    } else if t.contains("confirm your age") || t.contains("age-restricted") || t.contains("inappropriate for some users") {
        "Age restricted: add cookies in Settings to download it"
    } else if t.contains("not a bot") || t.contains("sign in to confirm") {
        BOT_CHECK
    } else if t.contains("premieres in") || t.contains("live event will begin") || t.contains("is not yet available") {
        "Not started yet"
    } else if t.contains("video unavailable") || t.contains("has been removed") || t.contains("no longer available") || t.contains("account associated with this video has been terminated") {
        "No longer available on YouTube"
    } else if t.contains("requested format is not available") {
        "No format matched the quality asked for"
    } else if t.contains("http error 404") || t.contains("does not exist") || t.contains("doesn't exist") || t.contains("this channel does not exist") {
        "That doesn't exist on YouTube (404): check the link"
    } else if t.contains("unable to download webpage") || t.contains("name or service not known") || t.contains("getaddrinfo") || t.contains("timed out") || t.contains("network is unreachable") || t.contains("connection") {
        "YouTube couldn't be reached"
    } else if t.contains("does not have a") && t.contains("tab") {
        "The channel has no such tab"
    } else {
        ""
    };
    if !reason.is_empty() {
        return reason.to_string();
    }
    let first = errors.first().map(|l| l.trim_start_matches("ERROR:").trim()).unwrap_or("");
    let first = first.strip_prefix("[youtube] ").unwrap_or(first);
    let first = first.split_once(": ").filter(|(id, _)| id.len() == 11 && !id.contains(' ')).map(|(_, r)| r).unwrap_or(first);
    if first.is_empty() {
        "yt-dlp didn't say why".to_string()
    } else {
        first.chars().take(200).collect()
    }
}

impl YtDlp {
    fn base(&self) -> Command {
        let mut c = command(&self.exe);
        c.args(["--ignore-config", "--no-colors", "--no-playlist-reverse"]);
        if let Some(f) = &self.ffmpeg {
            c.arg("--ffmpeg-location").arg(f);
        }
        match self.cookies.mode.as_str() {
            "browser" if !self.cookies.browser.is_empty() => {
                c.arg("--cookies-from-browser").arg(&self.cookies.browser);
            }
            "file" if !self.cookies.file.is_empty() => {
                c.arg("--cookies").arg(&self.cookies.file);
            }
            _ => {}
        }
        if self.polite.sleep_requests > 0.0 {
            c.arg("--sleep-requests").arg(fmt_secs(self.polite.sleep_requests));
        }
        c
    }

    /// Run to the end, with Stop watched. (stdout, stderr, ok).
    fn run(&self, mut cmd: Command, progress: &Progress, mut on_line: impl FnMut(&str)) -> Result<(String, String, bool)> {
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd.spawn().with_context(|| format!("couldn't start {}", self.exe.display()))?;
        let stdout = child.stdout.take().context("no output from yt-dlp")?;
        let mut stderr = child.stderr.take().context("no output from yt-dlp")?;
        let child = Mutex::new(child);
        let done = std::sync::atomic::AtomicBool::new(false);
        let (out, err, status) = std::thread::scope(|s| {
            let err_reader = s.spawn(move || {
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                buf
            });
            let done_ref = &done;
            let child_ref = &child;
            let watcher = s.spawn(move || {
                while !done_ref.load(Ordering::Relaxed) {
                    if progress.stopped() {
                        let _ = child_ref.lock().unwrap().kill();
                        return;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(150));
                }
            });
            let mut out = String::new();
            let mut reader = BufReader::new(stdout);
            let mut line = Vec::new();
            loop {
                line.clear();
                match reader.read_until(b'\n', &mut line) {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        let l = String::from_utf8_lossy(&line);
                        let l = l.trim_end_matches(['\r', '\n']);
                        on_line(l);
                        if out.len() < 64 << 20 {
                            out.push_str(l);
                            out.push('\n');
                        }
                    }
                }
            }
            let status = child.lock().unwrap().wait();
            done.store(true, Ordering::Relaxed);
            let _ = watcher.join();
            let err = err_reader.join().unwrap_or_default();
            (out, err, status)
        });
        if progress.stopped() {
            bail!("Stopped.");
        }
        let ok = status.map(|s| s.success()).unwrap_or(false);
        Ok((out, err, ok))
    }

    /// List a channel's tab or a playlist without opening each video. None when
    /// the channel has no such tab.
    pub fn list(&self, url: &str, progress: &Progress) -> Result<Option<Listing>> {
        let mut cmd = self.base();
        cmd.args(["-J", "--flat-playlist", "--no-warnings"]).arg(url);
        let (out, err, ok) = self.run(cmd, progress, |_| {})?;
        if !ok || out.trim().is_empty() {
            let why = explain(&err);
            if why == "The channel has no such tab" {
                return Ok(None);
            }
            bail!("{why}");
        }
        let mut v: Value = serde_json::from_str(out.trim()).context("yt-dlp's listing wasn't JSON")?;
        let entries = match v.get_mut("entries").map(Value::take) {
            Some(Value::Array(a)) => a.into_iter().filter(|e| e.is_object()).collect(),
            _ if v["_type"].as_str() != Some("playlist") => vec![v.clone()], // a single video's page
            _ => vec![],
        };
        Ok(Some(Listing { meta: v, entries }))
    }

    /// One video's details, in full.
    pub fn info(&self, url: &str, progress: &Progress) -> Result<Value> {
        let mut cmd = self.base();
        cmd.args(["-J", "--no-playlist", "--no-warnings"]).arg(url);
        let (out, err, ok) = self.run(cmd, progress, |_| {})?;
        if !ok || out.trim().is_empty() {
            bail!("{}", explain(&err));
        }
        serde_json::from_str(out.trim()).context("yt-dlp's answer wasn't JSON")
    }

    /// yt-dlp's format arguments for a quality; without ffmpeg only a single file
    /// can be fetched (no merging), so the best one is asked for.
    pub fn format_args(&self, q: Quality) -> Vec<String> {
        let s = |x: &str| x.to_string();
        let merge = self.ffmpeg.is_some();
        let mut a = vec![];
        match q {
            Quality::Audio => a.extend([s("-f"), s("ba[ext=m4a]/ba/b")]),
            _ => {
                let res = match q {
                    Quality::P1080 => "res:1080",
                    Quality::P720 => "res:720",
                    _ => "res",
                };
                a.extend([s("-S"), format!("{res},codec:avc:m4a,ext")]);
                if merge {
                    a.extend([s("-f"), s("bv*+ba/b"), s("--merge-output-format"), s("mp4")]);
                } else {
                    a.extend([s("-f"), s("b")]);
                }
            }
        }
        a
    }

    /// Download one video into `dest`: the media file, its `.info.json`, its
    /// picture and subtitles when asked. Progress goes to `progress`, the step to
    /// `stage`. A `.part` file left in `dest` by a try that was cut off is
    /// carried on from where it stopped.
    pub fn download(&self, url: &str, dest: &Path, quality: Quality, subtitles: &str, comments: bool, progress: &Progress, stage: &Stage) -> Result<()> {
        std::fs::create_dir_all(dest)?;
        let mut cmd = self.base();
        cmd.args(["--no-playlist", "--newline", "--progress", "--continue", "--retries", "3", "--fragment-retries", "5"]);
        if self.polite.sleep_downloads > 0.0 {
            cmd.arg("--sleep-interval").arg(fmt_secs(self.polite.sleep_downloads));
        }
        if self.polite.limit_rate > 0 {
            cmd.arg("--limit-rate").arg(self.polite.limit_rate.to_string());
        }
        cmd.args(["--progress-template", "download:VS>%(progress.downloaded_bytes)s|%(progress.total_bytes)s|%(progress.total_bytes_estimate)s|%(progress.speed)s|%(progress.eta)s"]);
        cmd.args(["--write-info-json", "--no-write-playlist-metafiles", "--write-thumbnail"]);
        if self.ffmpeg.is_some() {
            cmd.args(["--convert-thumbnails", "jpg"]);
        }
        if !subtitles.trim().is_empty() {
            cmd.args(["--write-subs", "--sub-langs", subtitles.trim()]);
        }
        if comments {
            // the top 200, without replies, into the .info.json
            cmd.args(["--write-comments", "--extractor-args", "youtube:max_comments=200,all,0;comment_sort=top"]);
        }
        cmd.args(self.format_args(quality));
        cmd.arg("-P").arg(dest);
        cmd.args(["-o", "%(title).120B [%(id)s].%(ext)s"]);
        cmd.arg(url);
        progress.reset();
        let (_, err, ok) = self.run(cmd, progress, |line| {
            if let Some(p) = line.strip_prefix("VS>") {
                let f: Vec<f64> = p.split('|').map(|x| x.trim().parse::<f64>().unwrap_or(0.0)).collect();
                if f.len() >= 5 {
                    progress.done.store(f[0] as u64, Ordering::Relaxed);
                    progress.total.store(if f[1] > 0.0 { f[1] } else { f[2] } as u64, Ordering::Relaxed);
                    progress.speed.store(f[3] as u64, Ordering::Relaxed);
                    progress.eta.store(f[4].max(0.0) as u64, Ordering::Relaxed);
                }
            } else if line.starts_with("[Merger]") {
                set_stage(stage, "Merging video and audio");
            } else if line.starts_with("[ThumbnailsConvertor]") || line.starts_with("[info] Downloading video thumbnail") {
                set_stage(stage, "Saving the picture");
            } else if line.starts_with("[download] Resuming download") {
                set_stage(stage, "Carrying on where the last try stopped");
            } else if line.contains("Downloading comment") {
                set_stage(stage, "Saving the comments");
            } else if line.starts_with("[download] Sleeping") {
                set_stage(stage, "Pausing between downloads");
            } else if line.starts_with("[download] Destination:") && !line.contains("Downloading") {
                set_stage(stage, "Downloading");
            }
        })?;
        if !ok {
            bail!("{}", explain(&err));
        }
        Ok(())
    }
}

/// Seconds for yt-dlp: "3", "0.5".
fn fmt_secs(s: f64) -> String {
    let t = format!("{s:.2}");
    t.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// What a download left in its folder: (media file, info.json, picture, subtitles).
pub fn sort_files(dir: &Path) -> Result<(Option<String>, Option<String>, Option<String>, Vec<String>)> {
    let (mut media, mut info, mut pic, mut subs) = (None, None, None, vec![]);
    let mut media_size = 0u64;
    for e in std::fs::read_dir(dir)?.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let lower = name.to_ascii_lowercase();
        let size = e.metadata().map(|m| m.len()).unwrap_or(0);
        if lower.ends_with(".info.json") {
            info = Some(name);
        } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") || lower.ends_with(".webp") || lower.ends_with(".png") {
            pic = Some(name);
        } else if lower.ends_with(".vtt") || lower.ends_with(".srt") || lower.ends_with(".ass") || lower.ends_with(".lrc") {
            subs.push(name);
        } else if lower.ends_with(".part") || lower.ends_with(".ytdl") || lower.ends_with(".temp") || lower.ends_with(".description") || lower.ends_with(".json") {
            continue;
        } else if e.file_type().map(|t| t.is_file()).unwrap_or(false) && size >= media_size {
            media_size = size;
            media = Some(name);
        }
    }
    subs.sort();
    Ok((media, info, pic, subs))
}

/// A number from JSON that may be an integer, a float, a string or missing.
pub fn num(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

pub fn text(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// "20261029" (yt-dlp's upload_date) -> "2026-10-29".
pub fn date(v: &Value) -> String {
    let s = text(v);
    if s.len() == 8 && s.bytes().all(|b| b.is_ascii_digit()) {
        format!("{}-{}-{}", &s[0..4], &s[4..6], &s[6..8])
    } else if s.len() >= 10 {
        s[..10].to_string()
    } else if let Some(t) = v.as_f64().filter(|t| *t > 0.0) {
        crate::util::iso_from_unix(t as i64)[..10].to_string()
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_are_explained() {
        assert_eq!(explain("ERROR: [youtube] abc: Join this channel to get access to members-only content"), "Members only");
        assert_eq!(explain("WARNING: x\nERROR: [youtube] abcdefghijk: Private video. Sign in if you've been granted access"), "Private video");
        assert_eq!(explain("ERROR: [youtube] abcdefghijk: Sign in to confirm you’re not a bot."), "YouTube asks for a sign-in: add cookies in Settings");
        assert_eq!(explain("ERROR: [youtube:tab] This channel does not have a shorts tab"), "The channel has no such tab");
        assert_eq!(explain("ERROR: [youtube] abcdefghijk: Something odd happened"), "Something odd happened");
        assert_eq!(explain(""), "yt-dlp didn't say why");
        assert_eq!(explain("ERROR: Could not copy Chrome cookie database. See https://github.com/yt-dlp/yt-dlp/issues/7271"), "The browser holds its cookies while it's open: close it and try again, or use Firefox");
        assert_eq!(explain("ERROR: Failed to decrypt with DPAPI. See https://github.com/yt-dlp/yt-dlp/issues/10927"), "Chrome and Edge lock their cookies on Windows: use Firefox, or a cookies file");
        assert_eq!(fmt_secs(3.0), "3");
        assert_eq!(fmt_secs(0.5), "0.5");
        assert_eq!(date(&serde_json::json!("20261029")), "2026-10-29");
        assert_eq!(date(&serde_json::json!(1_793_244_300)), "2026-10-29");
    }

    #[test]
    fn formats_follow_the_quality() {
        let y = YtDlp { exe: PathBuf::from("yt-dlp"), ffmpeg: Some(PathBuf::from("ffmpeg")), cookies: Cookies::default(), polite: Polite::default() };
        assert_eq!(y.format_args(Quality::P1080).join(" "), "-S res:1080,codec:avc:m4a,ext -f bv*+ba/b --merge-output-format mp4");
        let y = YtDlp { ffmpeg: None, ..y };
        assert_eq!(y.format_args(Quality::Best).join(" "), "-S res,codec:avc:m4a,ext -f b");
        assert_eq!(y.format_args(Quality::Audio).join(" "), "-f ba[ext=m4a]/ba/b");
    }
}
