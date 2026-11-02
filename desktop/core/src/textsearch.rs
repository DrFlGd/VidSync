//! Search inside videos: what's said in them (their subtitle files) and what's
//! written under them (the description in yt-dlp's `.info.json`). Each video's
//! text is read once and kept in memory until the library changes; a hit in
//! the subtitles comes with the second it was said, so the player can go there.

use crate::library::Video;
use serde::Serialize;
use serde_json::Value;
use std::path::Path;

/// One line of subtitles and when it starts (seconds).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Cue {
    pub t: f64,
    pub text: String,
}

/// A video's searchable text.
#[derive(Debug, Default)]
pub struct Doc {
    pub description: String,
    pub cues: Vec<Cue>,
    /// The same, lower case with punctuation as spaces, for matching.
    norm_description: String,
    norm_cues: Vec<String>,
}

/// "01:02:03.500", "02:03.500" or "00:00:01,000" (SRT) in seconds.
fn timestamp(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let mut t = 0.0;
    for p in &parts {
        t = t * 60.0 + p.trim().parse::<f64>().ok()?;
    }
    (parts.len() >= 2).then_some(t)
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&").replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", " ")
}

/// Text without tags (`<c>`, `<00:00:01.000>`, `<i>`), entities decoded, spaces collapsed.
fn clean(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_tag = false;
    for ch in line.chars() {
        match ch {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    unescape(&out).split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Lower case, letters and digits only (apostrophes dropped), single spaces.
pub fn normalise(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut space = true;
    for ch in s.chars().flat_map(char::to_lowercase) {
        if ch.is_alphanumeric() {
            out.push(ch);
            space = false;
        } else if ch == '\'' || ch == '’' {
            continue;
        } else if !space {
            out.push(' ');
            space = true;
        }
    }
    out.trim_end().to_string()
}

/// The lines of a WebVTT or SRT file with their start times. YouTube's own
/// captions repeat the previous line in each cue as they roll; a line is kept
/// once, at the cue where it first appears.
pub fn parse_subtitles(text: &str) -> Vec<Cue> {
    let mut out: Vec<Cue> = vec![];
    let mut start: Option<f64> = None;
    let mut recent: Vec<String> = vec![];
    for raw in text.lines() {
        let line = raw.trim();
        if let Some((a, _)) = line.split_once("-->") {
            start = timestamp(a);
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let Some(t) = start else { continue };
        let c = clean(line);
        if c.is_empty() || recent.contains(&c) {
            continue;
        }
        recent.push(c.clone());
        if recent.len() > 3 {
            recent.remove(0);
        }
        out.push(Cue { t, text: c });
    }
    out
}

impl Doc {
    pub fn new(description: String, cues: Vec<Cue>) -> Self {
        let norm_description = normalise(&description);
        let norm_cues = cues.iter().map(|c| normalise(&c.text)).collect();
        Self { description, cues, norm_description, norm_cues }
    }

    /// Read a kept video's description and subtitles from its folder. English
    /// subtitles are preferred when there are several languages.
    pub fn load(dir: &Path, v: &Video) -> Self {
        let description = if v.info.is_empty() {
            String::new()
        } else {
            std::fs::read(dir.join(&v.info)).ok().and_then(|b| serde_json::from_slice::<Value>(&b).ok()).and_then(|i| i["description"].as_str().map(String::from)).unwrap_or_default()
        };
        let sub = v.subtitles.iter().find(|s| s.contains(".en")).or_else(|| v.subtitles.first());
        let cues = sub.and_then(|s| std::fs::read(dir.join(s)).ok()).map(|b| parse_subtitles(&String::from_utf8_lossy(&b))).unwrap_or_default();
        Self::new(description, cues)
    }

    /// Where a phrase is said (a line, or running on into the next one), at most
    /// `max` places, and the whole count; and a bit of the description around it.
    pub fn find(&self, phrase: &str, max: usize) -> (Vec<Cue>, usize, Option<String>) {
        let mut hits = vec![];
        let mut count = 0;
        let mut i = 0;
        while i < self.norm_cues.len() {
            let here = &self.norm_cues[i];
            let found = here.contains(phrase) || self.norm_cues.get(i + 1).is_some_and(|next| format!("{here} {next}").contains(phrase) && !next.contains(phrase));
            if found {
                count += 1;
                if hits.len() < max {
                    let mut text = self.cues[i].text.clone();
                    if !here.contains(phrase) {
                        text = format!("{text} {}", self.cues[i + 1].text);
                    }
                    hits.push(Cue { t: self.cues[i].t, text });
                }
            }
            i += 1;
        }
        let snippet = self.norm_description.contains(phrase).then(|| snippet(&self.description, phrase));
        (hits, count, snippet)
    }
}

/// About 160 characters of the description around the first place the phrase is.
fn snippet(text: &str, phrase: &str) -> String {
    let first = phrase.split(' ').next().unwrap_or(phrase);
    let lower = text.to_lowercase();
    let at = lower.find(first).unwrap_or(0);
    let chars: Vec<char> = text.chars().collect();
    let at_char = text[..at.min(text.len())].chars().count();
    let from = at_char.saturating_sub(60);
    let to = (at_char + 100).min(chars.len());
    let mut s: String = chars[from..to].iter().collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ");
    if from > 0 {
        s = format!("…{s}");
    }
    if to < chars.len() {
        s.push('…');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitles_are_read_once_per_line() {
        let vtt = "WEBVTT\nKind: captions\n\n00:00:01.000 --> 00:00:03.000 align:start\nhello <c.colorE5E5E5>everyone</c>\n\n00:00:03.000 --> 00:00:05.000\nhello everyone\nwelcome to the &amp; workshop\n\n00:01:02.500 --> 00:01:04.000\nthe plan for today\n";
        let cues = parse_subtitles(vtt);
        assert_eq!(cues.iter().map(|c| (c.t, c.text.as_str())).collect::<Vec<_>>(), vec![(1.0, "hello everyone"), (3.0, "welcome to the & workshop"), (62.5, "the plan for today")]);
        let srt = "1\n00:00:01,500 --> 00:00:02,000\nIt's a test\n";
        assert_eq!(parse_subtitles(srt), vec![Cue { t: 1.5, text: "It's a test".into() }]);
    }

    #[test]
    fn phrases_are_found_across_lines_and_in_the_description() {
        let cues = parse_subtitles("00:10.000 --> 00:12.000\nwe will build a\n\n00:12.000 --> 00:14.000\ncanoe today. It's great\n\n01:00.000 --> 01:02.000\nthe Canoe floats\n");
        let d = Doc::new("A long description that mentions a canoe build, among other things.".into(), cues);
        let (hits, n, snip) = d.find(&normalise("canoe"), 5);
        assert_eq!(n, 2);
        assert_eq!(hits[0].t, 12.0);
        assert!(snip.unwrap().contains("canoe build"));
        let (hits, n, _) = d.find(&normalise("build a canoe"), 5);
        assert_eq!((n, hits[0].t, hits[0].text.as_str()), (1, 10.0, "we will build a canoe today. It's great"));
        assert_eq!(normalise("Its  great!"), normalise("it's great"));
    }
}
