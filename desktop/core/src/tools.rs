//! Finding yt-dlp and ffmpeg on this computer, and fetching or updating them
//! into the app's `tools/` folder (docs/PLAN.md, "yt-dlp and ffmpeg").

use crate::config::AppConfig;
use crate::http::{Client, Progress};
use crate::ytdlp::version_of;
use anyhow::{bail, Context, Result};
use serde::Serialize;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Tool {
    pub name: String,
    /// Where it was found ("" when it wasn't).
    pub path: String,
    pub version: String,
    /// "settings", "environment", "tools folder", "app folder", "PATH" or "".
    pub from: String,
    /// Why the one pointed at in Settings couldn't be used.
    pub error: String,
}

pub fn tools_dir(config_dir: &Path) -> PathBuf {
    config_dir.join("tools")
}

fn exe_name(name: &str) -> String {
    if cfg!(windows) { format!("{name}.exe") } else { name.to_string() }
}

fn on_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let names: Vec<String> = if cfg!(windows) { vec![format!("{name}.exe"), format!("{name}.cmd"), format!("{name}.bat"), name.to_string()] } else { vec![name.to_string()] };
    for dir in std::env::split_paths(&path) {
        for n in &names {
            let p = dir.join(n);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

fn app_dir() -> Option<PathBuf> {
    std::env::current_exe().ok().and_then(|p| p.parent().map(Path::to_path_buf))
}

/// Find `name` (yt-dlp or ffmpeg): the setting, the environment
/// (VIDSYNC_YTDLP, VIDSYNC_FFMPEG), the tools folder, the app's folder, PATH.
pub fn find(name: &str, setting: &str, config_dir: &Path) -> Tool {
    let flag = if name == "ffmpeg" { "-version" } else { "--version" };
    let mut t = Tool { name: name.to_string(), ..Default::default() };
    let env_name = format!("VIDSYNC_{}", name.to_ascii_uppercase().replace('-', ""));
    let mut tries: Vec<(PathBuf, &str)> = vec![];
    if !setting.trim().is_empty() {
        tries.push((PathBuf::from(setting.trim()), "settings"));
    }
    if let Some(p) = std::env::var_os(&env_name).filter(|p| !p.is_empty()) {
        tries.push((PathBuf::from(p), "environment"));
    }
    let tools = tools_dir(config_dir);
    tries.push((tools.join(exe_name(name)), "tools folder"));
    if name == "ffmpeg" {
        tries.push((tools.join("ffmpeg").join("bin").join(exe_name(name)), "tools folder"));
    }
    if let Some(d) = app_dir() {
        tries.push((d.join(exe_name(name)), "app folder"));
    }
    if let Some(p) = on_path(name) {
        tries.push((p, "PATH"));
    }
    for (p, from) in tries {
        if !p.is_file() {
            if from == "settings" {
                t.error = format!("{} isn't there", p.display());
            }
            continue;
        }
        match version_of(&p, flag) {
            Ok(v) => {
                t.path = p.display().to_string();
                t.version = v;
                t.from = from.to_string();
                return t;
            }
            Err(e) => {
                if from == "settings" {
                    t.error = format!("{e:#}");
                }
            }
        }
    }
    t
}

pub fn find_ytdlp(cfg: &AppConfig, config_dir: &Path) -> Tool {
    find("yt-dlp", &cfg.ytdlp_path, config_dir)
}

pub fn find_ffmpeg(cfg: &AppConfig, config_dir: &Path) -> Tool {
    find("ffmpeg", &cfg.ffmpeg_path, config_dir)
}

/// The yt-dlp build for this computer on its releases page.
pub fn ytdlp_url() -> Result<&'static str> {
    Ok(match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        ("windows", "x86") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_x86.exe",
        ("linux", "x86_64") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
        ("linux", "aarch64") => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux_aarch64",
        ("macos", _) => "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
        (os, arch) => bail!("there's no yt-dlp build to download for {os} on {arch}; install it yourself and point Settings at it"),
    })
}

/// Download the newest yt-dlp into the tools folder. The old one is replaced
/// only once the new one answers --version. Its version.
pub fn update_ytdlp(client: &Client, config_dir: &Path, progress: &Progress) -> Result<String> {
    let url = std::env::var("VIDSYNC_YTDLP_URL").ok().unwrap_or_else(|| ytdlp_url().map(String::from).unwrap_or_default());
    if url.is_empty() {
        ytdlp_url()?;
    }
    let dir = tools_dir(config_dir);
    std::fs::create_dir_all(&dir)?;
    let fresh = dir.join(exe_name("yt-dlp.new"));
    client.download(&url, &fresh, progress, "yt-dlp")?;
    make_executable(&fresh)?;
    let v = version_of(&fresh, "--version").context("the downloaded yt-dlp doesn't run")?;
    let dest = dir.join(exe_name("yt-dlp"));
    replace(&fresh, &dest)?;
    Ok(v)
}

/// Fetch ffmpeg (and ffprobe) into the tools folder: BtbN's build on Windows.
pub fn get_ffmpeg(client: &Client, config_dir: &Path, progress: &Progress) -> Result<String> {
    let url = match std::env::var("VIDSYNC_FFMPEG_URL") {
        Ok(u) if !u.is_empty() => u,
        _ => match (std::env::consts::OS, std::env::consts::ARCH) {
            ("windows", "x86_64") => "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip".to_string(),
            ("linux", _) => bail!("on Linux, install ffmpeg with your package manager (apt install ffmpeg); Vid Sync finds it on PATH"),
            ("macos", _) => bail!("on a Mac, install ffmpeg with Homebrew (brew install ffmpeg); Vid Sync finds it on PATH"),
            (os, arch) => bail!("there's no ffmpeg build to download for {os} on {arch}"),
        },
    };
    let dir = tools_dir(config_dir);
    std::fs::create_dir_all(&dir)?;
    let zip_path = dir.join("ffmpeg.zip.part");
    client.download(&url, &zip_path, progress, "ffmpeg")?;
    let f = std::fs::File::open(&zip_path)?;
    let mut z = zip::ZipArchive::new(f).context("the ffmpeg download isn't a zip")?;
    let mut got = vec![];
    for i in 0..z.len() {
        let mut entry = z.by_index(i)?;
        let name = entry.name().rsplit('/').next().unwrap_or("").to_string();
        if entry.is_file() && (name == exe_name("ffmpeg") || name == exe_name("ffprobe")) {
            let tmp = dir.join(format!("{name}.new"));
            let mut out = std::fs::File::create(&tmp)?;
            std::io::copy(&mut entry, &mut out)?;
            drop(out);
            make_executable(&tmp)?;
            replace(&tmp, &dir.join(&name))?;
            got.push(name);
        }
    }
    let _ = std::fs::remove_file(&zip_path);
    if !got.iter().any(|n| n == &exe_name("ffmpeg")) {
        bail!("the download had no ffmpeg in it");
    }
    version_of(&dir.join(exe_name("ffmpeg")), "-version").context("the downloaded ffmpeg doesn't run")
}

fn make_executable(p: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perm = std::fs::metadata(p)?.permissions();
        perm.set_mode(0o755);
        std::fs::set_permissions(p, perm)?;
    }
    let _ = p;
    Ok(())
}

/// Move `fresh` over `dest` (a running program on Windows can be renamed away first).
fn replace(fresh: &Path, dest: &Path) -> Result<()> {
    if dest.exists() {
        let old = dest.with_extension("old");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(dest, &old).with_context(|| format!("couldn't set the old {} aside", dest.display()))?;
        if let Err(e) = std::fs::rename(fresh, dest) {
            let _ = std::fs::rename(&old, dest);
            return Err(e).with_context(|| format!("couldn't put the new {} in place", dest.display()));
        }
        let _ = std::fs::remove_file(&old);
    } else {
        std::fs::rename(fresh, dest).with_context(|| format!("couldn't put {} in place", dest.display()))?;
    }
    Ok(())
}
