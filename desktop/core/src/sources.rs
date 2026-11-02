//! Reading what the user pastes: a link to a video, a channel or a playlist on
//! YouTube, in the shapes YouTube hands out, and the addresses yt-dlp is given
//! for each.

use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

/// The tabs of a channel that can be listed.
pub const TABS: [&str; 3] = ["videos", "shorts", "streams"];

/// How a channel is named in a link.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "by", content = "value", rename_all = "lowercase")]
pub enum ChannelKey {
    /// `@handle` (with the @).
    Handle(String),
    /// `UC…`.
    Id(String),
    /// `/c/Name` or `/user/Name`: the path as given, which YouTube still answers.
    Path(String),
}

impl ChannelKey {
    /// The channel's web address, without a tab.
    pub fn url(&self) -> String {
        match self {
            ChannelKey::Handle(h) => format!("https://www.youtube.com/{h}"),
            ChannelKey::Id(id) => format!("https://www.youtube.com/channel/{id}"),
            ChannelKey::Path(p) => format!("https://www.youtube.com/{p}"),
        }
    }
    pub fn label(&self) -> String {
        match self {
            ChannelKey::Handle(h) => h.clone(),
            ChannelKey::Id(id) => id.clone(),
            ChannelKey::Path(p) => p.rsplit('/').next().unwrap_or(p).to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Link {
    Video {
        id: String,
        /// The playlist the link was in, when it had one (`&list=`).
        #[serde(skip_serializing_if = "Option::is_none")]
        playlist: Option<String>,
    },
    Channel {
        key: ChannelKey,
        /// The tab the link opened, when it named one.
        #[serde(skip_serializing_if = "Option::is_none")]
        tab: Option<String>,
    },
    Playlist {
        id: String,
    },
}

impl Link {
    pub fn url(&self) -> String {
        match self {
            Link::Video { id, .. } => video_url(id),
            Link::Channel { key, .. } => key.url(),
            Link::Playlist { id } => playlist_url(id),
        }
    }
}

pub fn video_url(id: &str) -> String {
    format!("https://www.youtube.com/watch?v={id}")
}

pub fn playlist_url(id: &str) -> String {
    format!("https://www.youtube.com/playlist?list={id}")
}

/// The address yt-dlp lists for one tab of a channel.
pub fn tab_url(channel_url: &str, tab: &str) -> String {
    format!("{}/{tab}", channel_url.trim_end_matches('/'))
}

fn is_video_id(s: &str) -> bool {
    s.len() == 11 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_playlist_id(s: &str) -> bool {
    s.len() >= 13 && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') && (s.starts_with("PL") || s.starts_with("UU") || s.starts_with("OL") || s.starts_with("FL") || s.starts_with("RD") || s.starts_with("LL") || s.starts_with("WL"))
}

fn is_channel_id(s: &str) -> bool {
    s.len() == 24 && s.starts_with("UC") && s.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

fn is_handle(s: &str) -> bool {
    let h = s.strip_prefix('@').unwrap_or("");
    (3..=30).contains(&h.len()) && h.chars().all(|c| c.is_alphanumeric() || c == '.' || c == '-' || c == '_')
}

/// A query string's values: "v=abc&list=PL" -> [("v","abc"),("list","PL")].
fn query(s: &str) -> Vec<(String, String)> {
    s.split('&').filter(|p| !p.is_empty()).map(|p| {
        let (k, v) = p.split_once('=').unwrap_or((p, ""));
        (k.to_string(), crate::util::percent_decode(&v.replace('+', " ")))
    }).collect()
}

/// Read one pasted line. A plain `@handle`, a video id's `watch?v=`, `youtu.be`,
/// `/shorts/`, `/live/`, `/embed/`, a `/channel/UC…`, `/c/…`, `/user/…`,
/// `playlist?list=`; `www.`, `m.`, `music.` and no scheme are all fine.
pub fn parse(input: &str) -> Result<Link> {
    let s = input.trim();
    if s.is_empty() {
        bail!("nothing to read");
    }
    if is_handle(s) {
        return Ok(Link::Channel { key: ChannelKey::Handle(s.to_string()), tab: None });
    }
    if is_channel_id(s) {
        return Ok(Link::Channel { key: ChannelKey::Id(s.to_string()), tab: None });
    }
    if is_playlist_id(s) {
        return Ok(Link::Playlist { id: s.to_string() });
    }
    let rest = s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")).unwrap_or(s);
    let (host_part, path_q) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host_part.rsplit_once('@').map(|(_, h)| h).unwrap_or(host_part).split(':').next().unwrap_or("").to_ascii_lowercase();
    let host = host.trim_start_matches("www.").trim_start_matches("m.").trim_start_matches("music.");
    let (path, q) = path_q.split_once('?').unwrap_or((path_q, ""));
    let path = path.split('#').next().unwrap_or("").trim_matches('/');
    let q = query(q.split('#').next().unwrap_or(""));
    let list = q.iter().find(|(k, _)| k == "list").map(|(_, v)| v.clone()).filter(|v| is_playlist_id(v));
    if host == "youtu.be" {
        let id = path.split('/').next().unwrap_or("");
        if is_video_id(id) {
            return Ok(Link::Video { id: id.to_string(), playlist: list });
        }
        bail!("that youtu.be link has no video id");
    }
    if host != "youtube.com" && host != "youtube-nocookie.com" {
        if host.is_empty() {
            bail!("that doesn't look like a link");
        }
        bail!("only YouTube links can be added (this one is on {host})");
    }
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    match parts.first().copied() {
        Some("watch") | None => {
            if let Some((_, v)) = q.iter().find(|(k, _)| k == "v") {
                if is_video_id(v) {
                    return Ok(Link::Video { id: v.clone(), playlist: list });
                }
                bail!("that link's video id doesn't look right");
            }
            if let Some(l) = list {
                return Ok(Link::Playlist { id: l });
            }
            bail!("that link names no video, channel or playlist");
        }
        Some("playlist") => match list {
            Some(l) => Ok(Link::Playlist { id: l }),
            None => bail!("that playlist link has no list id"),
        },
        Some("shorts") | Some("live") | Some("embed") | Some("v") | Some("e") => {
            let id = parts.get(1).copied().unwrap_or("");
            if is_video_id(id) {
                return Ok(Link::Video { id: id.to_string(), playlist: list });
            }
            bail!("that link has no video id")
        }
        Some("channel") => {
            let id = parts.get(1).copied().unwrap_or("");
            if is_channel_id(id) {
                return Ok(Link::Channel { key: ChannelKey::Id(id.to_string()), tab: tab_of(parts.get(2)) });
            }
            bail!("that channel link has no channel id")
        }
        Some("c") | Some("user") => {
            let name = parts.get(1).copied().unwrap_or("");
            if name.is_empty() {
                bail!("that channel link has no name");
            }
            Ok(Link::Channel { key: ChannelKey::Path(format!("{}/{name}", parts[0])), tab: tab_of(parts.get(2)) })
        }
        Some(h) if h.starts_with('@') => {
            if !is_handle(h) {
                bail!("that handle doesn't look right");
            }
            Ok(Link::Channel { key: ChannelKey::Handle(h.to_string()), tab: tab_of(parts.get(1)) })
        }
        Some("feed") | Some("results") | Some("gaming") | Some("premium") => bail!("that's a YouTube page, not a video, channel or playlist"),
        Some(_) => bail!("that link names no video, channel or playlist"),
    }
}

fn tab_of(p: Option<&&str>) -> Option<String> {
    p.map(|t| t.to_ascii_lowercase()).filter(|t| TABS.contains(&t.as_str()) || t == "playlists")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_shapes_youtube_hands_out() {
        assert_eq!(parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ").unwrap(), Link::Video { id: "dQw4w9WgXcQ".into(), playlist: None });
        assert_eq!(parse("youtu.be/dQw4w9WgXcQ?t=43").unwrap(), Link::Video { id: "dQw4w9WgXcQ".into(), playlist: None });
        assert_eq!(parse("https://m.youtube.com/shorts/dQw4w9WgXcQ").unwrap(), Link::Video { id: "dQw4w9WgXcQ".into(), playlist: None });
        assert_eq!(parse("https://www.youtube.com/watch?v=dQw4w9WgXcQ&list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf&index=2").unwrap(), Link::Video { id: "dQw4w9WgXcQ".into(), playlist: Some("PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf".into()) });
        assert_eq!(parse("https://www.youtube.com/playlist?list=PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf").unwrap(), Link::Playlist { id: "PLrAXtmErZgOeiKm4sgNOknGvNjby9efdf".into() });
        assert_eq!(parse("https://www.youtube.com/@LinusTechTips/videos").unwrap(), Link::Channel { key: ChannelKey::Handle("@LinusTechTips".into()), tab: Some("videos".into()) });
        assert_eq!(parse("@LinusTechTips").unwrap(), Link::Channel { key: ChannelKey::Handle("@LinusTechTips".into()), tab: None });
        assert_eq!(parse("https://www.youtube.com/channel/UCXuqSBlHAE6Xw-yeJA0Tunw/shorts").unwrap(), Link::Channel { key: ChannelKey::Id("UCXuqSBlHAE6Xw-yeJA0Tunw".into()), tab: Some("shorts".into()) });
        assert_eq!(parse("https://www.youtube.com/c/Computerphile").unwrap(), Link::Channel { key: ChannelKey::Path("c/Computerphile".into()), tab: None });
        assert_eq!(parse("https://www.youtube.com/user/Computerphile/videos").unwrap(), Link::Channel { key: ChannelKey::Path("user/Computerphile".into()), tab: Some("videos".into()) });
        assert!(parse("https://vimeo.com/123").unwrap_err().to_string().contains("vimeo.com"));
        assert!(parse("not a link").is_err());
        assert!(parse("https://www.youtube.com/feed/subscriptions").is_err());
        assert_eq!(ChannelKey::Handle("@x".into()).url(), "https://www.youtube.com/@x");
        assert_eq!(tab_url("https://www.youtube.com/@x", "videos"), "https://www.youtube.com/@x/videos");
    }
}
