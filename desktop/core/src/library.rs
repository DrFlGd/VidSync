//! The library folder: `_library/library.json` (its id, name and settings), one
//! folder per channel at `youtube.com/channels/<@handle>/` holding `channel.json`
//! and `videos/<date> <title> [<id>]/` (one folder per kept video), and one per
//! playlist at `youtube.com/playlists/<id>/playlist.json`. Everything the app
//! shows comes from these files, so a library can be copied, synced or opened
//! on another computer (docs/PLAN.md, "The library folder").

use crate::util::{now_iso, read_json_object, write_json};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub const FORMAT: u64 = 1;
pub const META_DIR: &str = "_library";
pub const HOST: &str = "youtube.com";
pub const CHANNELS_DIR: &str = "channels";
pub const PLAYLISTS_DIR: &str = "playlists";
pub const CHANNEL_FILE: &str = "channel.json";
pub const PLAYLIST_FILE: &str = "playlist.json";
pub const VIDEOS_DIR: &str = "videos";
/// Downloads in progress, under `_library/` (removed before each job).
pub const PARTIAL_DIR: &str = ".partial";

/// What a check downloads of a source.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    /// Videos that appear upstream after the source was added.
    #[default]
    New,
    /// Everything upstream.
    All,
    /// Nothing: only list, for the audit.
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Quality {
    #[serde(rename = "best")]
    #[default]
    Best,
    #[serde(rename = "1080p")]
    P1080,
    #[serde(rename = "720p")]
    P720,
    #[serde(rename = "audio")]
    Audio,
}

impl Quality {
    pub fn label(self) -> &'static str {
        match self {
            Quality::Best => "Best available",
            Quality::P1080 => "Up to 1080p",
            Quality::P720 => "Up to 720p",
            Quality::Audio => "Audio only",
        }
    }
    pub fn key(self) -> &'static str {
        match self {
            Quality::Best => "best",
            Quality::P1080 => "1080p",
            Quality::P720 => "720p",
            Quality::Audio => "audio",
        }
    }
}

/// Which tabs of a channel are listed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tabs {
    pub videos: bool,
    pub shorts: bool,
    pub streams: bool,
}

impl Default for Tabs {
    fn default() -> Self {
        Self { videos: true, shorts: false, streams: false }
    }
}

impl Tabs {
    pub fn list(&self) -> Vec<&'static str> {
        let mut v = vec![];
        if self.videos {
            v.push("videos");
        }
        if self.shorts {
            v.push("shorts");
        }
        if self.streams {
            v.push("streams");
        }
        v
    }
}

/// What a source follows, and how often it's checked.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Follow {
    pub mode: Mode,
    pub tabs: Tabs,
    pub quality: Quality,
    /// yt-dlp's `--sub-langs` ("" for none; "en.*,en", "all").
    pub subtitles: String,
    /// Hours between checks: None follows the library's setting, 0 is never (only by hand).
    pub every_hours: Option<f64>,
}

impl Default for Follow {
    fn default() -> Self {
        Self { mode: Mode::New, tabs: Tabs::default(), quality: Quality::Best, subtitles: String::new(), every_hours: None }
    }
}

/// What the audit knows of a video listed upstream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// To be downloaded (when it isn't kept yet).
    #[default]
    Wanted,
    /// Outside the follow mode: listed, not downloaded unless asked.
    NotWanted,
    /// Left out by the user on purpose.
    Skipped,
    /// yt-dlp couldn't fetch it; `error` says why.
    Failed,
}

/// One video as listed upstream (a channel's tab or a playlist).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Listed {
    pub id: String,
    pub title: String,
    /// Seconds (0 when unknown).
    pub duration: f64,
    /// "YYYY-MM-DD" when known (flat listings often don't say).
    pub upload_date: String,
    /// The channel's tab it was listed in ("videos", "shorts", "streams"; "" for a playlist).
    pub tab: String,
    pub channel_id: String,
    pub channel: String,
    /// The channel's handle ("@x"), when the listing had it.
    pub uploader_id: String,
    pub view_count: u64,
    /// "", "is_live", "was_live", "is_upcoming", "post_live".
    pub live: String,
    pub availability: String,
    pub first_seen: String,
    pub last_seen: String,
    /// Not in the latest listing any more (removed, private, or a tab no longer followed).
    pub gone: bool,
    pub state: State,
    pub error: String,
    pub tries: u32,
    pub last_try: String,
}

/// One kept video, in `videos/<folder>/`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Video {
    pub id: String,
    /// Under `videos/`.
    pub folder: String,
    /// The media file, relative to the folder.
    pub file: String,
    pub title: String,
    pub upload_date: String,
    pub duration: f64,
    pub bytes: u64,
    pub width: u32,
    pub height: u32,
    pub ext: String,
    /// The video codec and audio codec yt-dlp chose ("avc1.64001F", "mp4a.40.2").
    pub vcodec: String,
    pub acodec: String,
    pub downloaded: String,
    /// The picture, relative to the folder ("" when there's none).
    pub thumbnail: String,
    /// Subtitle files, relative to the folder.
    pub subtitles: Vec<String>,
    /// yt-dlp's metadata file, relative to the folder ("" when there's none).
    pub info: String,
    pub view_count: u64,
    /// The quality asked for when it was downloaded ("best", "1080p", "720p", "audio").
    pub quality: String,
    pub tags: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Which channel this is.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelRef {
    /// "UC…".
    pub id: String,
    /// "@handle", or "" for the few channels without one.
    pub handle: String,
    pub url: String,
}

impl ChannelRef {
    /// The folder's name under channels/: the handle, else the id.
    pub fn key(&self) -> String {
        if self.handle.is_empty() { self.id.clone() } else { self.handle.clone() }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelDetails {
    pub name: String,
    pub description: String,
    pub avatar_url: String,
    /// The avatar as kept, relative to the channel's folder ("" when none).
    pub avatar: String,
    pub subscribers: u64,
    /// Videos upstream, as the channel reports it.
    pub video_count: u64,
}

/// `channel.json`. Keys the app doesn't know are kept when it's rewritten.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Channel {
    pub source: ChannelRef,
    pub details: ChannelDetails,
    pub added: String,
    /// Listed and downloaded on the schedule. A channel that only holds videos
    /// (from a playlist, or a pasted link) isn't followed until asked.
    pub followed: bool,
    pub follow: Follow,
    /// The user's group this channel belongs to ("" for none).
    pub creator: String,
    pub tags: Vec<String>,
    pub notes: String,
    pub last_checked: String,
    /// When a video was last downloaded.
    pub last_changed: String,
    pub last_error: String,
    /// Every video listed upstream, newest first.
    pub upstream: Vec<Listed>,
    /// Every video kept, in download order.
    pub videos: Vec<Video>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Channel {
    fn default() -> Self {
        Self {
            source: ChannelRef::default(),
            details: ChannelDetails::default(),
            added: now_iso(),
            followed: false,
            follow: Follow::default(),
            creator: String::new(),
            tags: vec![],
            notes: String::new(),
            last_checked: String::new(),
            last_changed: String::new(),
            last_error: String::new(),
            upstream: vec![],
            videos: vec![],
            extra: Map::new(),
        }
    }
}

impl Channel {
    pub fn new(source: ChannelRef, followed: bool, follow: Follow) -> Self {
        Self { source, followed, follow, ..Default::default() }
    }
    pub fn key(&self) -> String {
        self.source.key()
    }
    pub fn id(&self) -> String {
        source_id(Kind::Channel, &self.key())
    }
    pub fn name(&self) -> String {
        if self.details.name.is_empty() { self.source.key() } else { self.details.name.clone() }
    }
    pub fn bytes(&self) -> u64 {
        self.videos.iter().map(|v| v.bytes).sum()
    }
    pub fn has(&self, video_id: &str) -> bool {
        self.videos.iter().any(|v| v.id == video_id)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaylistRef {
    pub id: String,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlaylistDetails {
    pub title: String,
    pub description: String,
    /// Whose playlist it is.
    pub channel_id: String,
    pub channel: String,
    pub uploader_id: String,
    pub video_count: u64,
    pub thumbnail_url: String,
}

/// `playlist.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Playlist {
    pub source: PlaylistRef,
    pub details: PlaylistDetails,
    pub added: String,
    pub follow: Follow,
    pub tags: Vec<String>,
    pub notes: String,
    pub last_checked: String,
    pub last_changed: String,
    pub last_error: String,
    /// Every video listed upstream, in the playlist's order.
    pub upstream: Vec<Listed>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Playlist {
    fn default() -> Self {
        Self {
            source: PlaylistRef::default(),
            details: PlaylistDetails::default(),
            added: now_iso(),
            follow: Follow { mode: Mode::All, ..Follow::default() },
            tags: vec![],
            notes: String::new(),
            last_checked: String::new(),
            last_changed: String::new(),
            last_error: String::new(),
            upstream: vec![],
            extra: Map::new(),
        }
    }
}

impl Playlist {
    pub fn new(source: PlaylistRef, follow: Follow) -> Self {
        Self { source, follow, ..Default::default() }
    }
    pub fn key(&self) -> String {
        self.source.id.clone()
    }
    pub fn id(&self) -> String {
        source_id(Kind::Playlist, &self.key())
    }
    pub fn name(&self) -> String {
        if self.details.title.is_empty() { self.source.id.clone() } else { self.details.title.clone() }
    }
}

/// The library's own settings, in library.json ("settings"), so they travel with it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Hours between checks for every source that follows the library (0: only by hand).
    pub check_every_hours: f64,
    /// What a newly added channel follows.
    pub new_follow: Follow,
    /// What a newly added playlist follows.
    pub new_follow_playlist: Follow,
}

impl Default for Settings {
    fn default() -> Self {
        Self { check_every_hours: 24.0, new_follow: Follow::default(), new_follow_playlist: Follow { mode: Mode::All, ..Follow::default() } }
    }
}

/// A channel or a playlist.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Channel,
    Playlist,
}

/// "youtube.com/channels/@handle" or "youtube.com/playlists/PL…": a source's id,
/// also its folder in the library.
pub fn source_id(kind: Kind, key: &str) -> String {
    format!("{HOST}/{}/{key}", match kind {
        Kind::Channel => CHANNELS_DIR,
        Kind::Playlist => PLAYLISTS_DIR,
    })
}

/// The kind and key of a source's id, or None.
pub fn split_id(id: &str) -> Option<(Kind, &str)> {
    let rest = id.strip_prefix(HOST)?.strip_prefix('/')?;
    let (dir, key) = rest.split_once('/')?;
    if key.is_empty() || key.contains('/') {
        return None;
    }
    match dir {
        CHANNELS_DIR => Some((Kind::Channel, key)),
        PLAYLISTS_DIR => Some((Kind::Playlist, key)),
        _ => None,
    }
}

/// Upstream against kept, for one source.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Audit {
    /// Videos listed upstream (the ones gone from it aside).
    pub listed: usize,
    /// Listed and kept.
    pub kept: usize,
    /// Wanted and not kept: what Download missing fetches.
    pub missing: usize,
    /// Outside the follow mode and not kept.
    pub not_wanted: usize,
    pub failed: usize,
    pub skipped: usize,
    /// Live now or upcoming: can't be downloaded yet.
    pub waiting: usize,
    /// No longer listed upstream (kept or not).
    pub gone: usize,
    /// Kept videos that aren't in the listing (added by a link or a playlist, or gone upstream).
    pub kept_unlisted: usize,
}

pub fn audit(upstream: &[Listed], kept: &dyn Fn(&str) -> bool, kept_total: usize) -> Audit {
    let mut a = Audit::default();
    let mut kept_listed = 0;
    for l in upstream {
        if l.gone {
            a.gone += 1;
            continue;
        }
        a.listed += 1;
        if kept(&l.id) {
            a.kept += 1;
            kept_listed += 1;
            continue;
        }
        if l.live == "is_live" || l.live == "is_upcoming" {
            a.waiting += 1;
            continue;
        }
        match l.state {
            State::Wanted => a.missing += 1,
            State::NotWanted => a.not_wanted += 1,
            State::Skipped => a.skipped += 1,
            State::Failed => a.failed += 1,
        }
    }
    // kept videos listed only among the gone ones still count as kept
    for l in upstream.iter().filter(|l| l.gone) {
        if kept(&l.id) {
            kept_listed += 1;
        }
    }
    a.kept_unlisted = kept_total.saturating_sub(kept_listed);
    a
}

pub struct Library {
    pub root: PathBuf,
    pub id: String,
    pub name: String,
    pub settings: Settings,
    /// Every channel, by key ("@handle" or "UC…").
    pub channels: BTreeMap<String, Channel>,
    /// Every playlist, by id.
    pub playlists: BTreeMap<String, Playlist>,
    /// Folders with a channel.json or playlist.json that couldn't be read: (folder, why).
    pub unreadable: Vec<(String, String)>,
}

impl Library {
    /// Open the library at `root`, making it (and library.json) when it's new.
    pub fn open(root: &Path) -> Result<Self> {
        std::fs::create_dir_all(root).with_context(|| format!("couldn't make the library folder {}", root.display()))?;
        let file = root.join(META_DIR).join("library.json");
        let mut v = read_json_object(&file);
        let fresh = v.get("format").is_none();
        if let Some(f) = v["format"].as_u64() {
            if f > FORMAT {
                bail!("This library was made by a newer Vid Sync (format {f}); update the app to open it.");
            }
        }
        if fresh {
            let name = root.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Vid Sync".into());
            v = json!({ "format": FORMAT, "id": new_id(), "name": name, "created": now_iso(), "settings": Settings::default() });
            write_json(&file, &v)?;
        }
        let settings: Settings = serde_json::from_value(v["settings"].clone()).unwrap_or_default();
        let mut lib = Self {
            root: root.to_path_buf(),
            id: v["id"].as_str().unwrap_or("").to_string(),
            name: v["name"].as_str().unwrap_or("").to_string(),
            settings,
            channels: BTreeMap::new(),
            playlists: BTreeMap::new(),
            unreadable: vec![],
        };
        lib.scan();
        Ok(lib)
    }

    /// Read every channel.json and playlist.json again.
    pub fn scan(&mut self) {
        self.channels.clear();
        self.playlists.clear();
        self.unreadable.clear();
        let host = self.root.join(HOST);
        for e in std::fs::read_dir(host.join(CHANNELS_DIR)).into_iter().flatten().flatten() {
            let dir = e.path();
            let file = dir.join(CHANNEL_FILE);
            if !file.is_file() {
                continue;
            }
            let folder = e.file_name().to_string_lossy().into_owned();
            match std::fs::read(&file).map_err(anyhow::Error::from).and_then(|b| Ok(serde_json::from_slice::<Channel>(&b)?)) {
                Ok(c) if c.key() == folder => {
                    self.channels.insert(folder, c);
                }
                Ok(c) => self.unreadable.push((format!("{HOST}/{CHANNELS_DIR}/{folder}"), format!("its channel.json is for {}", c.key()))),
                Err(err) => self.unreadable.push((format!("{HOST}/{CHANNELS_DIR}/{folder}"), err.to_string())),
            }
        }
        for e in std::fs::read_dir(host.join(PLAYLISTS_DIR)).into_iter().flatten().flatten() {
            let dir = e.path();
            let file = dir.join(PLAYLIST_FILE);
            if !file.is_file() {
                continue;
            }
            let folder = e.file_name().to_string_lossy().into_owned();
            match std::fs::read(&file).map_err(anyhow::Error::from).and_then(|b| Ok(serde_json::from_slice::<Playlist>(&b)?)) {
                Ok(p) if p.key() == folder => {
                    self.playlists.insert(folder, p);
                }
                Ok(p) => self.unreadable.push((format!("{HOST}/{PLAYLISTS_DIR}/{folder}"), format!("its playlist.json is for {}", p.key()))),
                Err(err) => self.unreadable.push((format!("{HOST}/{PLAYLISTS_DIR}/{folder}"), err.to_string())),
            }
        }
    }

    pub fn channel_folder(&self, key: &str) -> PathBuf {
        self.root.join(HOST).join(CHANNELS_DIR).join(key)
    }

    pub fn playlist_folder(&self, key: &str) -> PathBuf {
        self.root.join(HOST).join(PLAYLISTS_DIR).join(key)
    }

    /// A source's folder from its id.
    pub fn folder_of(&self, id: &str) -> Option<PathBuf> {
        let (kind, key) = split_id(id)?;
        Some(match kind {
            Kind::Channel => self.channel_folder(key),
            Kind::Playlist => self.playlist_folder(key),
        })
    }

    pub fn partial_dir(&self) -> PathBuf {
        self.root.join(META_DIR).join(PARTIAL_DIR)
    }

    pub fn save_channel(&self, c: &Channel) -> Result<()> {
        write_json(&self.channel_folder(&c.key()).join(CHANNEL_FILE), &serde_json::to_value(c)?)
    }

    pub fn save_playlist(&self, p: &Playlist) -> Result<()> {
        write_json(&self.playlist_folder(&p.key()).join(PLAYLIST_FILE), &serde_json::to_value(p)?)
    }

    /// Put a channel in (saving it) and return its key.
    pub fn put_channel(&mut self, c: Channel) -> Result<String> {
        let key = c.key();
        self.save_channel(&c)?;
        self.channels.insert(key.clone(), c);
        Ok(key)
    }

    pub fn put_playlist(&mut self, p: Playlist) -> Result<String> {
        let key = p.key();
        self.save_playlist(&p)?;
        self.playlists.insert(key.clone(), p);
        Ok(key)
    }

    pub fn save_settings(&mut self, s: Settings) -> Result<()> {
        let file = self.root.join(META_DIR).join("library.json");
        let mut v = read_json_object(&file);
        v["settings"] = serde_json::to_value(&s)?;
        write_json(&file, &v)?;
        self.settings = s;
        Ok(())
    }

    /// The channel with this YouTube id ("UC…"), whatever its folder.
    pub fn channel_by_id(&self, channel_id: &str) -> Option<&Channel> {
        if channel_id.is_empty() {
            return None;
        }
        self.channels.values().find(|c| c.source.id == channel_id)
    }

    /// Where a video is kept: (channel key, the video).
    pub fn find_video(&self, video_id: &str) -> Option<(&str, &Video)> {
        for (k, c) in &self.channels {
            if let Some(v) = c.videos.iter().find(|v| v.id == video_id) {
                return Some((k.as_str(), v));
            }
        }
        None
    }

    pub fn has_video(&self, video_id: &str) -> bool {
        self.find_video(video_id).is_some()
    }

    /// The playlists a video is in.
    pub fn playlists_of(&self, video_id: &str) -> Vec<&Playlist> {
        self.playlists.values().filter(|p| p.upstream.iter().any(|l| l.id == video_id)).collect()
    }

    /// A path inside the library from a relative one with "/" (library:// addresses), or an error.
    pub fn inside(&self, rel_path: &str) -> Result<PathBuf> {
        let r = crate::util::safe_rel(Path::new(rel_path)).context("not a path inside the library")?;
        Ok(self.root.join(r))
    }

    pub fn bytes(&self) -> u64 {
        self.channels.values().map(Channel::bytes).sum()
    }

    pub fn video_count(&self) -> usize {
        self.channels.values().map(|c| c.videos.len()).sum()
    }
}

fn new_id() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    format!("{:016x}", h.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_round_trip_keeps_unknown_keys() {
        let dir = std::env::temp_dir().join(format!("vidsync-lib-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut lib = Library::open(&dir).unwrap();
        assert!(dir.join("_library/library.json").is_file());
        let mut c = Channel::new(ChannelRef { id: "UC123".into(), handle: "@demo".into(), url: "https://www.youtube.com/@demo".into() }, true, Follow::default());
        c.extra.insert("mine".into(), json!(42));
        c.videos.push(Video { id: "abcdefghijk".into(), folder: "2026-01-01 A [abcdefghijk]".into(), bytes: 10, ..Default::default() });
        lib.put_channel(c).unwrap();
        lib.put_playlist(Playlist::new(PlaylistRef { id: "PL1234567890abc".into(), url: String::new() }, Follow::default())).unwrap();
        assert!(dir.join("youtube.com/channels/@demo/channel.json").is_file());
        assert!(dir.join("youtube.com/playlists/PL1234567890abc/playlist.json").is_file());
        // a stray folder with a broken channel.json is reported, not fatal
        std::fs::create_dir_all(dir.join("youtube.com/channels/@broken")).unwrap();
        std::fs::write(dir.join("youtube.com/channels/@broken/channel.json"), "{").unwrap();
        let lib = Library::open(&dir).unwrap();
        assert_eq!(lib.channels.len(), 1);
        assert_eq!(lib.playlists.len(), 1);
        assert_eq!(lib.channels["@demo"].extra["mine"], json!(42));
        assert_eq!(lib.channels["@demo"].id(), "youtube.com/channels/@demo");
        assert_eq!(split_id("youtube.com/channels/@demo"), Some((Kind::Channel, "@demo")));
        assert_eq!(split_id("youtube.com/playlists/PL1234567890abc"), Some((Kind::Playlist, "PL1234567890abc")));
        assert_eq!(split_id("youtube.com/other/x"), None);
        assert_eq!(lib.find_video("abcdefghijk").map(|(k, _)| k), Some("@demo"));
        assert_eq!(lib.channel_by_id("UC123").map(|c| c.key()), Some("@demo".into()));
        assert_eq!(lib.unreadable.len(), 1);
        assert!(lib.inside("../etc").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn audit_counts_each_state() {
        let l = |id: &str, state: State, gone: bool, live: &str| Listed { id: id.into(), state, gone, live: live.into(), ..Default::default() };
        let up = vec![
            l("a", State::Wanted, false, ""),
            l("b", State::Wanted, false, ""),
            l("c", State::NotWanted, false, ""),
            l("d", State::Failed, false, ""),
            l("e", State::Skipped, false, ""),
            l("f", State::Wanted, true, ""),
            l("g", State::Wanted, false, "is_upcoming"),
        ];
        let kept = |id: &str| id == "a" || id == "f";
        let a = audit(&up, &kept, 3);
        assert_eq!(a, Audit { listed: 6, kept: 1, missing: 1, not_wanted: 1, failed: 1, skipped: 1, waiting: 1, gone: 1, kept_unlisted: 1 });
    }
}
