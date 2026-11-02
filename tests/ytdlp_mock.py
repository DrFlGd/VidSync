#!/usr/bin/env python3
"""A stand-in for yt-dlp, so adding sources, checks and the audit can be tested
without YouTube. Vid Sync runs it when VIDSYNC_YTDLP points at this file.

    VIDSYNC_YTDLP=tests/ytdlp_mock.py VIDSYNC_MOCK_FIXTURES=<folder> vidsync-cli ...

<folder>/channels/<@handle>.json   {"channel_id", "name", "description", "subscribers",
                                    "avatar": "<file in the folder>", "tabs": {"videos": [ids], "shorts": [ids]}}
<folder>/playlists/<id>.json       {"title", "description", "channel": "<@handle>", "videos": [ids]}
<folder>/videos/<id>.json          {"title", "duration", "upload_date": "20260101", "channel": "<@handle>",
                                    "view_count", "description", "live_status", "error": "<yt-dlp's error text>",
                                    "needs_cookies": true (age restricted: only with Firefox's cookies or a cookies file),
                                    "bot_check": true (YouTube asks to confirm it isn't a bot, unless cookies are given),
                                    "break_once": true (the first download is cut off half way, leaving a .part file;
                                                        the next one carries on and says where from in its .info.json),
                                    "cues": [[start, end, "text"], ...] (the subtitles written with --write-subs),
                                    "comments": [{"author", "text", "like_count", ...}] (written with --write-comments),
                                    "delay": seconds a download of it takes}
<folder>/sample.webm or sample.mp4 the file every download gets (a few bytes are written when neither is there)

A test edits these files to simulate a change upstream. It answers what Vid Sync
asks: --version, -J --flat-playlist <channel tab or playlist>, -J <video>, and a
download (files in -P, progress lines in the --progress-template shape).
VIDSYNC_MOCK_DELAY (seconds) slows each download down; VIDSYNC_MOCK_LOG appends
each call's arguments to a file.
"""
import json
import os
import re
import shutil
import sys
import time
import urllib.parse
from pathlib import Path

FIX = Path(os.environ.get("VIDSYNC_MOCK_FIXTURES", "."))
args = sys.argv[1:]
if os.environ.get("VIDSYNC_MOCK_LOG"):
    with open(os.environ["VIDSYNC_MOCK_LOG"], "a", encoding="utf-8") as f:
        f.write(json.dumps(args) + "\n")

if args == ["--version"]:
    print("2026.10.26-mock")
    sys.exit(0)


def opt(name, default=None):
    if name in args:
        i = args.index(name)
        return args[i + 1] if i + 1 < len(args) else default
    return default


url = next((a for a in reversed(args) if a.startswith("http") or a.startswith("@")), "")
u = urllib.parse.urlparse(url)
q = urllib.parse.parse_qs(u.query)
path = [p for p in u.path.split("/") if p]


def fail(msg, ie="youtube"):
    sys.stderr.write(f"ERROR: [{ie}] {msg}\n")
    sys.exit(1)


def load(kind, key):
    p = FIX / kind / f"{key}.json"
    return json.loads(p.read_text(encoding="utf-8")) if p.exists() else None


def channel_by(key):
    """A channel by handle, id, or /c/ and /user/ path."""
    if key.startswith("@"):
        for f in (FIX / "channels").glob("*.json"):
            if f.stem.lower() == key.lower():
                return f.stem, load("channels", f.stem)
        return None, None
    for f in (FIX / "channels").glob("*.json"):
        c = load("channels", f.stem)
        if c.get("channel_id") == key or key in c.get("aliases", []):
            return f.stem, c
    return None, None


def gate(vid, v):
    """Fail the way YouTube does for a video that wants a sign-in."""
    browser, cookie_file = opt("--cookies-from-browser"), opt("--cookies")
    signed_in = (browser == "firefox") or bool(cookie_file and Path(cookie_file).is_file())
    if browser in ("chrome", "edge"):
        sys.stderr.write("ERROR: Failed to decrypt with DPAPI. See  https://github.com/yt-dlp/yt-dlp/issues/10927  for more info\n")
        sys.exit(1)
    if v.get("needs_cookies") and not signed_in:
        fail(f"{vid}: Sign in to confirm your age. This video may be inappropriate for some users.")
    if v.get("bot_check") and not signed_in:
        fail(f"{vid}: Sign in to confirm you’re not a bot. Use --cookies-from-browser or --cookies for the authentication.")


def avatar_url(c):
    a = c.get("avatar")
    return f"file://{(FIX / a).resolve()}" if a and (FIX / a).exists() else ""


def video_json(vid, handle, c, flat):
    v = load("videos", vid) or {"title": vid}
    d = {
        "id": vid, "title": v.get("title", vid), "duration": v.get("duration", 60), "view_count": v.get("view_count", 0),
        "live_status": v.get("live_status"), "availability": v.get("availability", "public"),
        "channel_id": c.get("channel_id", ""), "channel": c.get("name", handle), "uploader_id": handle, "uploader": c.get("name", handle),
        "channel_url": f"https://www.youtube.com/{handle}", "uploader_url": f"https://www.youtube.com/{handle}",
        "webpage_url": f"https://www.youtube.com/watch?v={vid}", "url": f"https://www.youtube.com/watch?v={vid}",
        "_type": "url" if flat else "video", "ie_key": "Youtube",
    }
    if not flat:
        d.update({
            "upload_date": v.get("upload_date", "20260101"), "description": v.get("description", ""), "width": v.get("width", 1280), "height": v.get("height", 720),
            "ext": "mp4", "vcodec": "avc1.64001F", "acodec": "mp4a.40.2", "fps": 30, "like_count": v.get("like_count", 0), "tags": v.get("tags", []),
            "categories": v.get("categories", ["Education"]), "format": "137+140", "chapters": v.get("chapters"),
        })
    return d


def video_channel(vid):
    for f in (FIX / "channels").glob("*.json"):
        c = load("channels", f.stem)
        for ids in c.get("tabs", {}).values():
            if vid in ids:
                return f.stem, c
    v = load("videos", vid) or {}
    if v.get("channel"):
        return channel_by(v["channel"])
    return None, None


if "-J" in args or "--dump-single-json" in args:
    flat = "--flat-playlist" in args
    if u.netloc == "youtu.be" or (path and path[0] == "watch") or (path and path[0] in ("shorts", "live")):
        vid = q.get("v", [None])[0] or (path[1] if len(path) > 1 else path[0])
        v = load("videos", vid)
        if v is None:
            fail(f"{vid}: Video unavailable")
        if v.get("error"):
            fail(f"{vid}: {v['error']}")
        gate(vid, v)
        handle, c = video_channel(vid)
        print(json.dumps(video_json(vid, handle or "@unknown", c or {}, False)))
        sys.exit(0)
    if path and path[0] == "playlist":
        pid = q.get("list", [""])[0]
        p = load("playlists", pid)
        if p is None:
            fail(f"{pid}: The playlist does not exist.", "youtube:tab")
        handle, c = channel_by(p.get("channel", ""))
        entries = []
        for vid in p.get("videos", []):
            h, cc = video_channel(vid)
            entries.append(video_json(vid, h or "@unknown", cc or {}, True))
        print(json.dumps({
            "_type": "playlist", "id": pid, "title": p.get("title", pid), "description": p.get("description", ""),
            "channel_id": (c or {}).get("channel_id", ""), "channel": (c or {}).get("name", ""), "uploader_id": handle, "playlist_count": len(entries),
            "thumbnails": [{"url": avatar_url(c or {})}], "entries": entries,
        }))
        sys.exit(0)
    # a channel: /@handle[/tab], /channel/UC…[/tab], /c/Name[/tab], /user/Name[/tab]
    if path and path[0] in ("channel", "c", "user"):
        key = path[1] if len(path) > 1 else ""
        if path[0] != "channel":
            key = f"{path[0]}/{key}"
        tab = path[2] if len(path) > 2 else None
    else:
        key = path[0] if path else ""
        tab = path[1] if len(path) > 1 else None
    handle, c = channel_by(key)
    if c is None:
        fail("Unable to download webpage: HTTP Error 404: Not Found (the channel doesn't exist)", "youtube:tab")
    tabs = c.get("tabs", {})
    if tab is None:
        tab = "videos" if "videos" in tabs else next(iter(tabs), "videos")
    if tab not in tabs:
        fail(f"This channel does not have a {tab} tab", "youtube:tab")
    entries = [video_json(vid, handle, c, True) for vid in tabs[tab]]
    print(json.dumps({
        "_type": "playlist", "id": f"{c.get('channel_id', '')}", "title": f"{c.get('name', handle)} - {tab.capitalize()}",
        "description": c.get("description", ""), "channel_id": c.get("channel_id", ""), "channel": c.get("name", handle),
        "uploader_id": handle, "uploader_url": f"https://www.youtube.com/{handle}", "channel_url": f"https://www.youtube.com/{handle}",
        "channel_follower_count": c.get("subscribers", 0), "playlist_count": len(entries),
        "thumbnails": [{"id": "avatar_uncropped", "url": avatar_url(c)}, {"id": "banner_uncropped", "url": ""}],
        "entries": entries,
    }))
    sys.exit(0)

# ---- a download -----------------------------------------------------------
vid = q.get("v", [None])[0] or (path[1] if len(path) > 1 else (path[0] if path else ""))
v = load("videos", vid)
if v is None:
    fail(f"{vid}: Video unavailable")
if v.get("error"):
    fail(f"{vid}: {v['error']}")
gate(vid, v)
handle, c = video_channel(vid)
info = video_json(vid, handle or "@unknown", c or {}, False)
dest = Path(opt("-P", "."))
dest.mkdir(parents=True, exist_ok=True)
tmpl = opt("--progress-template", "")
tmpl = tmpl.split(":", 1)[1] if tmpl.startswith("download:") else tmpl
title = re.sub(r'[/\\:*?"<>|]', "_", info["title"])[:120]
base = f"{title} [{vid}]"
sample = next((FIX / n for n in ("sample.webm", "sample.mp4") if (FIX / n).exists()), None)
data = sample.read_bytes() if sample else b"\x00\x00\x00\x1cftypisom" + bytes(2000)
ext = sample.suffix.lstrip(".") if sample else "mp4"
if "ba" in (opt("-f", "") or "") and "bv" not in (opt("-f", "") or ""):
    media = dest / f"{base}.m4a"
    info["ext"] = "m4a"
    info["vcodec"] = "none"
else:
    media = dest / f"{base}.{ext}"
    info["ext"] = ext
    if ext == "webm":
        info["vcodec"], info["acodec"] = "vp8", "opus"
delay = float(v.get("delay") or os.environ.get("VIDSYNC_MOCK_DELAY", "0.3"))
print(f"[info] {vid}: Downloading 1 format(s): 137+140", flush=True)
print(f"[download] Destination: {media}", flush=True)
total = len(data)
steps = 5
part = Path(f"{media}.part")
first = 1
if v.get("break_once") and not part.exists():
    # cut off half way, as a dropped connection or sleep leaves it
    part.write_bytes(data[: total // 2])
    print(tmpl.replace("%(progress.downloaded_bytes)s", str(total // 2)).replace("%(progress.total_bytes)s", str(total)).replace("%(progress.total_bytes_estimate)s", "NA").replace("%(progress.speed)s", "1048576.0").replace("%(progress.eta)s", "1"), flush=True)
    fail("Unable to download video data: <urlopen error [Errno 104] Connection reset by peer>", "download")
if part.exists():
    if "--no-part" in args or "--no-continue" in args:
        part.unlink()
    else:
        info["vidsync_mock_resumed_from"] = part.stat().st_size
        print(f"[download] Resuming download at byte {part.stat().st_size}", flush=True)
        first = steps // 2 + 1
        part.unlink()
for i in range(first, steps + 1):
    done = total * i // steps
    line = tmpl.replace("%(progress.downloaded_bytes)s", str(done)).replace("%(progress.total_bytes)s", str(total)).replace("%(progress.total_bytes_estimate)s", "NA").replace("%(progress.speed)s", "1048576.0").replace("%(progress.eta)s", str(steps - i))
    print(line or f"[download] {done * 100 / total:5.1f}% of {total}", flush=True)
    time.sleep(delay / steps)
print(f"[Merger] Merging formats into \"{media}\"", flush=True)
media.write_bytes(data)
if "--write-comments" in args:
    info["comments"] = [{"parent": "root", "timestamp": 1767225600, **c} for c in v.get("comments", [])]
    info["comment_count"] = len(info["comments"])
(dest / f"{base}.info.json").write_text(json.dumps(info), encoding="utf-8")
pic = FIX / "thumb.png"
(dest / f"{base}.jpg").write_bytes(pic.read_bytes() if pic.exists() else b"")
if "--write-subs" in args:
    def vtt_time(t):
        return f"{int(t // 3600):02d}:{int(t % 3600 // 60):02d}:{t % 60:06.3f}"
    cues = v.get("cues") or [[0, 1, "Hello"]]
    (dest / f"{base}.en.vtt").write_text("WEBVTT\n\n" + "".join(f"{vtt_time(a)} --> {vtt_time(b)}\n{t}\n\n" for a, b, t in cues), encoding="utf-8")
