//! Bringing in videos already downloaded with yt-dlp elsewhere: a folder is
//! searched for files named the way yt-dlp names them (`… [<video id>].ext`),
//! grouped by video with what lies beside them (`.info.json`, picture,
//! subtitles), and each group is moved or copied into its channel's folder like
//! a fresh download (capture::place), so the audit counts it as kept.

use anyhow::{Context, Result};
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const MEDIA: &[&str] = &["mp4", "mkv", "webm", "m4a", "mp3", "opus", "mov", "avi", "flv", "ogg", "wav", "aac", "m4v"];

/// One video found in the folder.
#[derive(Clone, Debug, Serialize)]
pub struct Found {
    pub id: String,
    pub title: String,
    /// The folder its files are in.
    pub dir: PathBuf,
    /// Its files there (the media, and whatever else carries the id).
    pub files: Vec<String>,
    pub bytes: u64,
    /// yt-dlp's `.info.json` beside it, when there is one.
    #[serde(skip)]
    pub info: Option<Value>,
    pub has_info: bool,
    pub channel: String,
}

/// The YouTube id in a yt-dlp file name: the last `[...]` holding 11 id characters.
pub fn id_in_name(name: &str) -> Option<String> {
    let mut rest = name;
    let mut found = None;
    while let Some(open) = rest.find('[') {
        let after = &rest[open + 1..];
        let Some(close) = after.find(']') else { break };
        let inner = &after[..close];
        if inner.len() == 11 && inner.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            found = Some(inner.to_string());
        }
        rest = &after[close + 1..];
    }
    found
}

fn ext_of(name: &str) -> String {
    name.rsplit('.').next().unwrap_or("").to_ascii_lowercase()
}

/// Every video under `root` (searched down to `depth` folders), skipping the
/// folder `skip` (the library itself) and hidden folders.
pub fn scan(root: &Path, skip: Option<&Path>, depth: usize) -> Result<Vec<Found>> {
    let mut groups: BTreeMap<(PathBuf, String), Vec<(String, u64)>> = BTreeMap::new();
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    let skip = skip.map(|p| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
    while let Some((dir, d)) = stack.pop() {
        let entries = std::fs::read_dir(&dir).with_context(|| format!("couldn't read {}", dir.display()))?;
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                let p = e.path();
                let canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
                if d < depth && !name.starts_with('.') && skip.as_ref().is_none_or(|s| &canon != s) {
                    stack.push((p, d + 1));
                }
            } else if ft.is_file() && !name.ends_with(".part") && !name.ends_with(".ytdl") {
                if let Some(id) = id_in_name(&name) {
                    let size = e.metadata().map(|m| m.len()).unwrap_or(0);
                    groups.entry((dir.clone(), id)).or_default().push((name, size));
                }
            }
        }
    }
    let mut out = vec![];
    for ((dir, id), files) in groups {
        let Some((media, _)) = files.iter().filter(|(n, _)| MEDIA.contains(&ext_of(n).as_str())).max_by_key(|(_, s)| *s) else { continue };
        let info = files.iter().find(|(n, _)| n.ends_with(".info.json")).and_then(|(n, _)| std::fs::read(dir.join(n)).ok()).and_then(|b| serde_json::from_slice::<Value>(&b).ok());
        let title = info.as_ref().and_then(|i| i["title"].as_str().map(String::from)).unwrap_or_else(|| {
            let stem = media.rsplit_once('.').map(|x| x.0).unwrap_or(media);
            stem.replace(&format!("[{id}]"), "").trim().trim_end_matches(['-', '_']).trim().to_string()
        });
        let channel = info.as_ref().and_then(|i| i["channel"].as_str().or(i["uploader"].as_str()).map(String::from)).unwrap_or_default();
        out.push(Found {
            id,
            title,
            bytes: files.iter().map(|f| f.1).sum(),
            files: files.into_iter().map(|f| f.0).collect(),
            has_info: info.is_some(),
            info,
            channel,
            dir,
        });
    }
    out.sort_by(|a, b| a.channel.cmp(&b.channel).then(a.title.cmp(&b.title)));
    Ok(out)
}

/// Move a file (a rename when it can, else copy and delete) or copy it.
pub fn bring(from: &Path, to: &Path, copy: bool) -> Result<()> {
    if !copy && std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    std::fs::copy(from, to).with_context(|| format!("couldn't copy {}", from.display()))?;
    if !copy {
        std::fs::remove_file(from).with_context(|| format!("couldn't remove {}", from.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_read_from_names_and_files_grouped() {
        assert_eq!(id_in_name("Talk [part 2] [abcdefghijk].mp4").as_deref(), Some("abcdefghijk"));
        assert_eq!(id_in_name("Talk [abcdefghijk].en.vtt").as_deref(), Some("abcdefghijk"));
        assert_eq!(id_in_name("holiday.mp4"), None);
        let dir = std::env::temp_dir().join(format!("vs-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("A talk [abcdefghijk].mp4"), b"video").unwrap();
        std::fs::write(dir.join("A talk [abcdefghijk].info.json"), br#"{"title": "A talk!", "channel": "Chan"}"#).unwrap();
        std::fs::write(dir.join("A talk [abcdefghijk].jpg"), b"pic").unwrap();
        std::fs::write(dir.join("sub/Other [bcdefghijkl].webm"), b"video2").unwrap();
        std::fs::write(dir.join("sub/notes [bcdefghijkl].txt.part"), b"x").unwrap();
        std::fs::write(dir.join("Only a picture [cdefghijklm].jpg"), b"pic").unwrap();
        let found = scan(&dir, None, 3).unwrap();
        assert_eq!(found.len(), 2);
        let a = found.iter().find(|f| f.id == "abcdefghijk").unwrap();
        assert_eq!((a.title.as_str(), a.channel.as_str(), a.files.len(), a.has_info), ("A talk!", "Chan", 3, true));
        let b = found.iter().find(|f| f.id == "bcdefghijkl").unwrap();
        assert_eq!((b.title.as_str(), b.files.len(), b.has_info), ("Other", 1, false));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
