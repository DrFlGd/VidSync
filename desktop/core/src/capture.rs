//! A check: listing a source upstream, merging the listing into what the audit
//! knows, downloading the videos that are wanted, each into its channel's
//! folder through `_library/.partial/` (docs/PLAN.md, "Following and the audit").

use crate::http::{Client, Progress};
use crate::library::{self, Channel, ChannelDetails, ChannelRef, Follow, Library, Listed, Mode, Playlist, PlaylistDetails, Quality, State, Video, VIDEOS_DIR};
use crate::sources::{self, ChannelKey};
use crate::util::{dir_size, now_iso, now_unix, safe_name, unix_from_iso};
use crate::ytdlp::{date, num, set_stage, sort_files, text, Listing, Stage, YtDlp};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

/// What a job did, for Activity.
#[derive(Debug, Default, Clone, serde::Serialize)]
pub struct Outcome {
    /// Titles downloaded.
    pub downloaded: Vec<String>,
    /// Videos newly listed upstream.
    pub listed_new: usize,
    /// Videos that couldn't be downloaded: "title: why".
    pub failed: Vec<String>,
    pub warnings: Vec<String>,
    /// Kept videos deleted by "keep only the newest" or "delete watched after": "title (why)".
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub deleted: Vec<String>,
    /// What a job that isn't a check did, in words.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

impl Outcome {
    pub fn describe(&self) -> String {
        if let Some(n) = &self.note {
            return n.clone();
        }
        let mut parts = vec![];
        if self.listed_new > 0 {
            parts.push(format!("{} newly listed", plural(self.listed_new, "video")));
        }
        if !self.downloaded.is_empty() {
            parts.push(format!("downloaded {}", plural(self.downloaded.len(), "video")));
        }
        if !self.failed.is_empty() {
            parts.push(format!("{} couldn't be downloaded", plural(self.failed.len(), "video")));
        }
        if !self.deleted.is_empty() {
            parts.push(format!("deleted {} as the channel's settings ask", plural(self.deleted.len(), "older or watched video")));
        }
        let mut msg = if parts.is_empty() { "Up to date".to_string() } else { cap(&parts.join(", ")) };
        for w in &self.warnings {
            msg.push_str(&format!(". {w}"));
        }
        msg
    }
}

pub fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

fn cap(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// A channel's identity and details from a listing's top level (or a video's info).
pub fn channel_from(meta: &Value) -> (ChannelRef, ChannelDetails) {
    let id = text(&meta["channel_id"]);
    let mut handle = text(&meta["uploader_id"]);
    if !handle.starts_with('@') {
        handle = meta["uploader_url"].as_str().or(meta["channel_url"].as_str()).and_then(|u| u.rsplit('/').find(|p| p.starts_with('@'))).map(String::from).unwrap_or_default();
    }
    let url = if !handle.is_empty() {
        format!("https://www.youtube.com/{handle}")
    } else if !id.is_empty() {
        format!("https://www.youtube.com/channel/{id}")
    } else {
        text(&meta["channel_url"])
    };
    let mut name = text(&meta["channel"]);
    if name.is_empty() {
        name = text(&meta["uploader"]);
    }
    if name.is_empty() {
        // a tab's title is "Name - Videos"
        name = text(&meta["title"]).rsplit_once(" - ").map(|(n, _)| n.to_string()).unwrap_or_else(|| text(&meta["title"]));
    }
    let thumbs = meta["thumbnails"].as_array().cloned().unwrap_or_default();
    let avatar_url = thumbs
        .iter()
        .find(|t| t["id"].as_str() == Some("avatar_uncropped"))
        .or_else(|| thumbs.iter().filter(|t| num(&t["width"]) > 0.0 && (num(&t["width"]) - num(&t["height"])).abs() < 2.0).max_by(|a, b| num(&a["width"]).total_cmp(&num(&b["width"]))))
        .map(|t| text(&t["url"]))
        .unwrap_or_default();
    let details = ChannelDetails {
        name,
        description: text(&meta["description"]),
        avatar_url,
        avatar: String::new(),
        subscribers: num(&meta["channel_follower_count"]) as u64,
        video_count: num(&meta["playlist_count"]) as u64,
    };
    (ChannelRef { id, handle, url }, details)
}

pub fn playlist_from(meta: &Value) -> PlaylistDetails {
    let thumbs = meta["thumbnails"].as_array().cloned().unwrap_or_default();
    PlaylistDetails {
        title: text(&meta["title"]),
        description: text(&meta["description"]),
        channel_id: text(&meta["channel_id"]),
        channel: if text(&meta["channel"]).is_empty() { text(&meta["uploader"]) } else { text(&meta["channel"]) },
        uploader_id: text(&meta["uploader_id"]),
        video_count: num(&meta["playlist_count"]) as u64,
        thumbnail_url: thumbs.last().map(|t| text(&t["url"])).unwrap_or_default(),
    }
}

/// One listed entry as the audit keeps it (state to be set by `merge`).
fn listed_from(e: &Value, tab: &str, now: &str) -> Option<Listed> {
    let id = text(&e["id"]);
    if id.len() != 11 {
        return None;
    }
    Some(Listed {
        id,
        title: text(&e["title"]),
        duration: num(&e["duration"]),
        upload_date: date(&e["upload_date"]).max(date(&e["timestamp"])),
        tab: tab.to_string(),
        channel_id: text(&e["channel_id"]),
        channel: if text(&e["channel"]).is_empty() { text(&e["uploader"]) } else { text(&e["channel"]) },
        uploader_id: text(&e["uploader_id"]),
        view_count: num(&e["view_count"]) as u64,
        live: text(&e["live_status"]),
        availability: text(&e["availability"]),
        first_seen: now.to_string(),
        last_seen: now.to_string(),
        ..Default::default()
    })
}

/// List a channel through its tabs. (the first tab's top level, the entries in
/// listing order, the tabs that exist).
pub fn list_channel(y: &YtDlp, url: &str, follow: &Follow, progress: &Progress, stage: &Stage, label: &str) -> Result<(Value, Vec<Listed>)> {
    let now = now_iso();
    let mut meta = Value::Null;
    let mut all = vec![];
    let mut tabs = follow.tabs.list();
    if tabs.is_empty() {
        tabs.push("videos");
    }
    for tab in tabs {
        set_stage(stage, format!("Listing the {tab} of {label}"));
        let listing = match y.list(&sources::tab_url(url, tab), progress)? {
            Some(l) => l,
            None if tab == "videos" => match y.list(url, progress)? {
                Some(l) => l,
                None => continue,
            },
            None => continue,
        };
        if meta.is_null() || tab == "videos" {
            meta = listing.meta.clone();
        }
        all.extend(listing.entries.iter().filter_map(|e| listed_from(e, tab, &now)));
    }
    if meta.is_null() {
        bail!("YouTube lists nothing for this channel");
    }
    Ok((meta, all))
}

pub fn list_playlist(y: &YtDlp, url: &str, progress: &Progress, stage: &Stage, label: &str) -> Result<(Value, Vec<Listed>)> {
    set_stage(stage, format!("Listing {label}"));
    let now = now_iso();
    let Listing { meta, entries } = y.list(url, progress)?.context("YouTube lists nothing for this playlist")?;
    Ok((meta, entries.iter().filter_map(|e| listed_from(e, "", &now)).collect()))
}

/// Merge a fresh listing into `upstream`. New videos are marked by the follow
/// mode (at a source's first listing, "new" means from now on, so what's there
/// already isn't wanted); known ones keep their state, gone ones are kept and
/// marked. With `ordered` (a playlist) the fresh order is taken. How many were new.
pub fn merge(upstream: &mut Vec<Listed>, fresh: Vec<Listed>, mode: Mode, first: bool, ordered: bool) -> usize {
    let state_for_new = match mode {
        Mode::All => State::Wanted,
        Mode::None => State::NotWanted,
        Mode::New => {
            if first {
                State::NotWanted
            } else {
                State::Wanted
            }
        }
    };
    let mut old: Vec<Listed> = std::mem::take(upstream);
    let mut out: Vec<Listed> = Vec::with_capacity(fresh.len() + old.len());
    let mut new = 0;
    for mut f in fresh {
        if let Some(i) = old.iter().position(|o| o.id == f.id) {
            let mut o = old.remove(i);
            o.title = if f.title.is_empty() { o.title } else { f.title };
            o.duration = if f.duration > 0.0 { f.duration } else { o.duration };
            o.upload_date = if f.upload_date.is_empty() { o.upload_date } else { f.upload_date };
            o.view_count = f.view_count.max(o.view_count);
            o.live = f.live;
            o.availability = f.availability;
            o.tab = if f.tab.is_empty() { o.tab } else { f.tab };
            o.channel_id = if f.channel_id.is_empty() { o.channel_id } else { f.channel_id };
            o.channel = if f.channel.is_empty() { o.channel } else { f.channel };
            o.uploader_id = if f.uploader_id.is_empty() { o.uploader_id } else { f.uploader_id };
            o.last_seen = f.last_seen;
            o.gone = false;
            out.push(o);
        } else if !out.iter().any(|x| x.id == f.id) {
            f.state = state_for_new;
            new += 1;
            out.push(f);
        }
    }
    // what wasn't listed this time
    for mut o in old {
        o.gone = true;
        out.push(o);
    }
    if !ordered {
        // a channel: the newest first, as its tabs list them; new ones ahead of known ones
        out.sort_by_key(|l| l.gone);
    }
    *upstream = out;
    new
}

/// The videos a check should download now, in the order to fetch them (oldest
/// first for a channel, the playlist's order for a playlist). Failed ones are
/// tried again after a day, or always with `retry`.
pub fn wanted(upstream: &[Listed], kept: &dyn Fn(&str) -> bool, ordered: bool, retry: bool) -> Vec<Listed> {
    let now = now_unix();
    let mut v: Vec<Listed> = upstream
        .iter()
        .filter(|l| !l.gone && !kept(&l.id) && l.live != "is_live" && l.live != "is_upcoming")
        .filter(|l| match l.state {
            State::Wanted => true,
            State::Failed => retry || unix_from_iso(&l.last_try).unwrap_or(0) + 86400 <= now,
            _ => false,
        })
        .cloned()
        .collect();
    if !ordered {
        v.reverse();
    }
    v
}

/// What one download left in `.partial/<id>/`.
pub struct Downloaded {
    pub dir: PathBuf,
    pub info: Value,
    pub media: String,
    pub info_file: String,
    pub picture: String,
    pub subtitles: Vec<String>,
}

/// What a try at a download was asked for, kept in its `.partial/<id>` folder:
/// a later try carries on from the files there only when it asks for the same.
const TRY_FILE: &str = "try.json";

/// How long a download cut off part way is kept for a later try to carry on.
pub const PARTIAL_KEEP_DAYS: u64 = 7;

/// Remove what downloads left in `.partial/` more than [`PARTIAL_KEEP_DAYS`] ago.
pub fn clean_partial(partial: &Path) {
    let keep = std::time::Duration::from_secs(PARTIAL_KEEP_DAYS * 86_400);
    for e in std::fs::read_dir(partial).into_iter().flatten().flatten() {
        let age = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok());
        if age.is_none_or(|a| a > keep) {
            let p = e.path();
            let _ = if p.is_dir() { std::fs::remove_dir_all(&p) } else { std::fs::remove_file(&p) };
        }
    }
}

/// Download one video into its folder under `partial` and read what came. A
/// folder left by an earlier try that was cut off (sleep, a dropped connection,
/// Stop) is carried on from, when that try asked for the same quality.
pub fn download(y: &YtDlp, partial: &Path, video_id: &str, follow: &Follow, progress: &Progress, stage: &Stage) -> Result<Downloaded> {
    let dir = partial.join(video_id);
    let asked = json!({ "quality": follow.quality.key(), "subtitles": follow.subtitles.trim(), "ffmpeg": y.ffmpeg.is_some(), "comments": follow.comments });
    let before: Value = std::fs::read(dir.join(TRY_FILE)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null);
    if before != asked {
        let _ = std::fs::remove_dir_all(&dir);
    }
    std::fs::create_dir_all(&dir).with_context(|| format!("couldn't make {}", dir.display()))?;
    std::fs::write(dir.join(TRY_FILE), serde_json::to_vec(&asked)?)?;
    set_stage(stage, "Downloading");
    let res = y.download(&sources::video_url(video_id), &dir, follow.quality, &follow.subtitles, follow.comments, progress, stage);
    if let Err(e) = res {
        // a half-written file is kept for the next try; otherwise nothing is worth keeping
        let resumable = std::fs::read_dir(&dir).into_iter().flatten().flatten().any(|f| f.file_name().to_string_lossy().ends_with(".part"));
        if !resumable {
            let _ = std::fs::remove_dir_all(&dir);
        }
        return Err(e);
    }
    let _ = std::fs::remove_file(dir.join(TRY_FILE));
    let (media, info_file, picture, subtitles) = sort_files(&dir)?;
    let media = media.context("yt-dlp finished but left no video file")?;
    let info_file = info_file.unwrap_or_default();
    let info = if info_file.is_empty() { Value::Null } else { std::fs::read(dir.join(&info_file)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null) };
    Ok(Downloaded { dir, info, media, info_file, picture: picture.unwrap_or_default(), subtitles })
}

/// Put a download into its channel's folder (making the channel when it's new,
/// not followed) and record it. (the channel's key, the video).
pub fn place(lib: &mut Library, d: Downloaded, video_id: &str, quality: Quality, fallback: Option<&Listed>) -> Result<(String, Video)> {
    let info = &d.info;
    let (mut cref, details) = channel_from(info);
    if cref.id.is_empty() && cref.handle.is_empty() {
        if let Some(l) = fallback {
            cref = ChannelRef { id: l.channel_id.clone(), handle: if l.uploader_id.starts_with('@') { l.uploader_id.clone() } else { String::new() }, url: String::new() };
        }
    }
    // the channel this video belongs to: by id, else by handle, else made now
    let key = match lib.channel_by_id(&cref.id).map(Channel::key).or_else(|| lib.channels.contains_key(&cref.key()).then(|| cref.key())) {
        Some(k) => k,
        None => {
            if cref.key().is_empty() {
                bail!("yt-dlp didn't say which channel the video belongs to");
            }
            if cref.url.is_empty() {
                cref.url = format!("https://www.youtube.com/{}", cref.key());
            }
            let mut c = Channel::new(cref, false, lib.settings.new_follow.clone());
            c.details = ChannelDetails { name: details.name, ..Default::default() };
            if let Some(l) = fallback {
                if c.details.name.is_empty() {
                    c.details.name = l.channel.clone();
                }
            }
            lib.put_channel(c)?
        }
    };
    let title = if text(&info["title"]).is_empty() { fallback.map(|l| l.title.clone()).unwrap_or_else(|| video_id.to_string()) } else { text(&info["title"]) };
    let upload_date = date(&info["upload_date"]);
    let mut folder = safe_name(&format!("{}{} [{video_id}]", if upload_date.is_empty() { String::new() } else { format!("{upload_date} ") }, title.chars().take(80).collect::<String>().trim()));
    let videos_dir = lib.channel_folder(&key).join(VIDEOS_DIR);
    std::fs::create_dir_all(&videos_dir)?;
    while videos_dir.join(&folder).exists() {
        folder.push('_');
    }
    let dest = videos_dir.join(&folder);
    std::fs::rename(&d.dir, &dest).with_context(|| format!("couldn't move the video into {}", dest.display()))?;
    let (bytes, _) = dir_size(&dest);
    let v = Video {
        id: video_id.to_string(),
        folder,
        file: d.media.clone(),
        title,
        upload_date,
        duration: num(&info["duration"]).max(fallback.map(|l| l.duration).unwrap_or(0.0)),
        bytes,
        width: num(&info["width"]) as u32,
        height: num(&info["height"]) as u32,
        ext: d.media.rsplit('.').next().unwrap_or("").to_ascii_lowercase(),
        vcodec: text(&info["vcodec"]),
        acodec: text(&info["acodec"]),
        downloaded: now_iso(),
        thumbnail: d.picture,
        subtitles: d.subtitles,
        info: d.info_file,
        view_count: num(&info["view_count"]) as u64,
        quality: quality.key().to_string(),
        tags: vec![],
        ..Default::default()
    };
    lib.save_video(&key, &v)?;
    let mut c = lib.channels.get(&key).cloned().context("the channel vanished")?;
    c.videos.retain(|x| x.id != video_id || videos_dir.join(&x.folder).is_dir());
    c.videos.push(v.clone());
    c.last_changed = now_iso();
    // the channel's own listing learns of it
    if let Some(l) = c.upstream.iter_mut().find(|l| l.id == video_id) {
        l.state = if l.state == State::Failed { State::Wanted } else { l.state };
        l.error.clear();
    }
    lib.put_channel(c)?;
    Ok((key, v))
}

/// Fetch a channel's avatar into its folder when it's missing or has changed.
pub fn fetch_avatar(client: &Client, folder: &Path, details: &mut ChannelDetails, progress: &Progress) {
    if details.avatar_url.is_empty() {
        return;
    }
    let file = "avatar.jpg";
    if !details.avatar.is_empty() && folder.join(&details.avatar).is_file() && details.extra_avatar_url_same(folder) {
        return;
    }
    let tmp = folder.join("avatar.jpg.part");
    if client.download(&details.avatar_url, &tmp, progress, "the channel's picture").is_ok() && std::fs::rename(&tmp, folder.join(file)).is_ok() {
        details.avatar = file.to_string();
        let _ = std::fs::write(folder.join(".avatar-url"), &details.avatar_url);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

impl ChannelDetails {
    /// Whether the kept avatar came from the current address (noted beside it).
    fn extra_avatar_url_same(&self, folder: &Path) -> bool {
        std::fs::read_to_string(folder.join(".avatar-url")).map(|u| u == self.avatar_url).unwrap_or(false)
    }
}

/// Which channel a link to one names, after yt-dlp has listed it: the handle
/// typed may differ in case from the one YouTube answers with.
pub fn channel_key_matches(key: &ChannelKey, c: &Channel) -> bool {
    match key {
        ChannelKey::Handle(h) => c.source.handle.eq_ignore_ascii_case(h),
        ChannelKey::Id(id) => c.source.id == *id,
        ChannelKey::Path(_) => false,
    }
}

/// Remove a video's folder and its record. The playlists keep listing it (the
/// audit shows it as missing again, or not wanted).
/// Which listed videos "keep only the newest N" allows: the first N in the
/// listing (newest first) that are kept or wanted, skipping what's gone or
/// skipped. None when there's no limit.
pub fn newest_allowed(upstream: &[Listed], kept: &dyn Fn(&str) -> bool, n: u32) -> Option<std::collections::HashSet<String>> {
    (n > 0).then(|| upstream.iter().filter(|l| !l.gone && l.state != State::Skipped && (kept(&l.id) || l.state == State::Wanted)).take(n as usize).map(|l| l.id.clone()).collect())
}

/// What a channel's follow settings say should go: kept videos beyond the newest
/// `keep_newest` (by date published), and watched ones older than
/// `drop_watched_days`. A video with one of the user's tags stays. (id, title, why).
pub fn to_prune(c: &Channel, now: i64) -> Vec<(String, String, String)> {
    let f = &c.follow;
    let mut out = vec![];
    let mut by_date: Vec<&Video> = c.videos.iter().collect();
    by_date.sort_by(|a, b| b.upload_date.cmp(&a.upload_date).then(b.downloaded.cmp(&a.downloaded)));
    for (i, v) in by_date.iter().enumerate() {
        if !v.tags.is_empty() {
            continue;
        }
        if f.keep_newest > 0 && i >= f.keep_newest as usize {
            out.push((v.id.clone(), v.title.clone(), format!("Deleted: only the newest {} are kept", f.keep_newest)));
        } else if f.drop_watched_days > 0 && v.watched && unix_from_iso(&v.watched_at).is_some_and(|t| t + f.drop_watched_days as i64 * 86_400 <= now) {
            out.push((v.id.clone(), v.title.clone(), format!("Deleted: watched more than {} ago", plural(f.drop_watched_days as usize, "day"))));
        }
    }
    out
}

/// Delete what [`to_prune`] names; the audit keeps listing each, as not wanted
/// with the reason, so nothing downloads it again. Titles with the reason.
pub fn prune(lib: &mut Library, key: &str) -> Result<Vec<String>> {
    let c = lib.channels.get(key).context("That channel isn't in the library.")?;
    let doomed = to_prune(c, now_unix());
    let mut out = vec![];
    for (id, title, why) in doomed {
        delete_video(lib, key, &id)?;
        let mut c = lib.channels.get(key).cloned().context("gone")?;
        if let Some(l) = c.upstream.iter_mut().find(|l| l.id == id) {
            l.state = State::NotWanted;
            l.error = why.clone();
        }
        lib.put_channel(c)?;
        out.push(format!("{title} ({})", why.trim_start_matches("Deleted: ")));
    }
    Ok(out)
}

pub fn delete_video(lib: &mut Library, key: &str, video_id: &str) -> Result<()> {
    let mut c = lib.channels.get(key).cloned().context("That channel isn't in the library.")?;
    let i = c.videos.iter().position(|v| v.id == video_id).context("That video isn't in the library.")?;
    let v = c.videos.remove(i);
    let dir = lib.channel_folder(key).join(VIDEOS_DIR).join(crate::util::safe_rel(Path::new(&v.folder)).context("not a video's folder")?);
    if dir.is_dir() {
        std::fs::remove_dir_all(&dir).with_context(|| format!("couldn't remove {}", dir.display()))?;
    }
    if let Some(l) = c.upstream.iter_mut().find(|l| l.id == video_id) {
        if l.state == State::Wanted {
            l.state = State::NotWanted;
        }
    }
    lib.put_channel(c)?;
    Ok(())
}

pub fn playlist_label(p: &Playlist) -> String {
    p.name()
}

pub fn due_at(last_checked: &str, added: &str, every: Option<f64>, s: &library::Settings) -> Option<i64> {
    let hours = every.unwrap_or(s.check_every_hours);
    if hours <= 0.0 {
        return None;
    }
    let last = unix_from_iso(last_checked).or_else(|| unix_from_iso(added)).unwrap_or(0);
    Some(last + (hours * 3600.0) as i64)
}

#[cfg(test)]
mod prune_tests {
    use super::*;

    fn video(id: &str, date: &str) -> Video {
        Video { id: id.into(), title: id.into(), upload_date: date.into(), ..Default::default() }
    }

    #[test]
    fn the_newest_stay_and_old_watched_ones_go() {
        let mut c = Channel::new(ChannelRef { id: "UC1".into(), handle: "@a".into(), url: String::new() }, true, Follow { keep_newest: 2, ..Follow::default() });
        c.videos = vec![video("a", "2026-01-01"), video("b", "2026-03-01"), video("c", "2026-02-01"), video("d", "2025-12-01")];
        c.videos[3].tags = vec!["keep".into()];
        let gone: Vec<String> = to_prune(&c, now_unix()).into_iter().map(|x| x.0).collect();
        assert_eq!(gone, vec!["a".to_string()], "the oldest untagged beyond two go; a tagged one stays");
        c.follow = Follow { drop_watched_days: 7, ..Follow::default() };
        c.videos[1].set_watched(true);
        c.videos[2].watched = true;
        c.videos[2].watched_at = "2026-01-01T00:00:00Z".into();
        let gone: Vec<String> = to_prune(&c, now_unix()).into_iter().map(|x| x.0).collect();
        assert_eq!(gone, vec!["c".to_string()], "only what was watched more than a week ago");
        let up: Vec<Listed> = ["n1", "n2", "n3"].iter().map(|id| Listed { id: id.to_string(), state: State::Wanted, ..Default::default() }).collect();
        let allowed = newest_allowed(&up, &|_| false, 2).unwrap();
        assert!(allowed.contains("n1") && allowed.contains("n2") && !allowed.contains("n3"));
    }

    #[test]
    fn playing_near_the_end_counts_as_watched() {
        let mut v = Video { duration: 600.0, ..Default::default() };
        v.played_to(3.0, false);
        assert_eq!((v.position, v.watched), (0.0, false));
        v.played_to(125.04, false);
        assert_eq!((v.position, v.watched), (125.0, false));
        v.played_to(580.0, false);
        assert!(v.watched && v.position == 0.0 && !v.watched_at.is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn l(id: &str) -> Listed {
        Listed { id: id.into(), title: id.into(), ..Default::default() }
    }

    #[test]
    fn merging_marks_new_videos_by_the_mode() {
        let mut up = vec![];
        // first listing in "new" mode: what's there isn't wanted
        assert_eq!(merge(&mut up, vec![l("a"), l("b")], Mode::New, true, false), 2);
        assert!(up.iter().all(|x| x.state == State::NotWanted));
        // a later listing: the new one is wanted, b has gone
        assert_eq!(merge(&mut up, vec![l("c"), l("a")], Mode::New, false, false), 1);
        assert_eq!(up.iter().map(|x| (x.id.as_str(), x.state, x.gone)).collect::<Vec<_>>(), vec![("c", State::Wanted, false), ("a", State::NotWanted, false), ("b", State::NotWanted, true)]);
        // b comes back; a failed one stays failed
        up[1].state = State::Failed;
        merge(&mut up, vec![l("c"), l("a"), l("b")], Mode::New, false, false);
        assert!(!up[2].gone && up[1].state == State::Failed);
        // "all" wants everything at the first listing; a playlist keeps its order
        let mut pl = vec![];
        merge(&mut pl, vec![l("x"), l("y")], Mode::All, true, true);
        merge(&mut pl, vec![l("y"), l("z"), l("x")], Mode::All, false, true);
        assert_eq!(pl.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["y", "z", "x"]);
        assert!(pl.iter().all(|x| x.state == State::Wanted));
        let w = wanted(&pl, &|id| id == "z", true, false);
        assert_eq!(w.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["y", "x"]);
        let w = wanted(&up, &|_| false, false, false);
        assert_eq!(w.iter().map(|x| x.id.as_str()).collect::<Vec<_>>(), vec!["a", "c"]); // the failed one is old enough to retry; oldest first
    }

    #[test]
    fn a_channel_is_read_from_a_listing() {
        let meta = json!({ "channel_id": "UC12345678901234567890ab", "uploader_id": "@Demo", "channel": "Demo Channel", "title": "Demo Channel - Videos", "channel_follower_count": 1200, "playlist_count": 3, "description": "Hi", "thumbnails": [{ "id": "avatar_uncropped", "url": "https://x/avatar.jpg" }, { "id": "banner", "url": "https://x/b.jpg" }] });
        let (r, d) = channel_from(&meta);
        assert_eq!(r.key(), "@Demo");
        assert_eq!(r.url, "https://www.youtube.com/@Demo");
        assert_eq!(d.name, "Demo Channel");
        assert_eq!(d.avatar_url, "https://x/avatar.jpg");
        assert_eq!(d.subscribers, 1200);
        let e = json!({ "id": "abcdefghijk", "title": "T", "duration": 61, "view_count": 5, "live_status": null });
        let x = listed_from(&e, "videos", "2026-01-01T00:00:00Z").unwrap();
        assert_eq!((x.title.as_str(), x.duration, x.view_count, x.tab.as_str()), ("T", 61.0, 5, "videos"));
        assert!(listed_from(&json!({ "id": "short" }), "videos", "").is_none());
    }
}
