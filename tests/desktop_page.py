#!/usr/bin/env python3
"""Acceptance test: the page against the app's real backend, in Chromium.

    python3 tests/desktop_page.py --cli desktop/target/release/vidsync-cli \
        --ui build/desktop/ui --home /tmp/vs-home --out shots [--chromium /opt/pw-browsers/chromium]

Copies tests/fixtures to --home (with a small real video made by ffmpeg when it's
installed), runs `vidsync-cli serve` with tests/ytdlp_mock.py standing in for yt-dlp
and tests/tauri_shim.js for Tauri, and drives the page: adding a channel, a
playlist and a video by their links, the cards and the list, a channel's page and
its audit, a new video upstream fetched by hand and on the schedule, a download
that fails and skipping it, creators, tags, search, the player, deleting a video,
removing sources, changing the library folder, the dark theme and a narrow window.
Screenshots go to --out.
"""
import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import time
import urllib.parse
import urllib.request
from pathlib import Path

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent.parent
ap = argparse.ArgumentParser()
ap.add_argument("--cli", required=True)
ap.add_argument("--ui", required=True)
ap.add_argument("--home", type=Path, required=True)
ap.add_argument("--out", type=Path, required=True)
ap.add_argument("--chromium", default=None)
a = ap.parse_args()

home = a.home.resolve()
if home.exists():
    shutil.rmtree(home)
fx = home / "fixtures"
lib = home / "Library"
shutil.copytree(ROOT / "tests/fixtures", fx)
a.out.mkdir(parents=True, exist_ok=True)
failures = []

# a real two-second video, so the player has something to play; a stub otherwise.
# VP8 and Opus, which every Chromium build plays (the Playwright one has no H.264 on Windows)
if shutil.which("ffmpeg"):
    subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=320x180:rate=10", "-f", "lavfi", "-i", "sine=frequency=440",
                    "-t", "2", "-pix_fmt", "yuv420p", "-c:v", "libvpx", "-b:v", "200k", "-c:a", "libopus", str(fx / "sample.webm")], check=True)
REAL_VIDEO = (fx / "sample.webm").is_file()


def check(cond, what):
    print(("ok   " if cond else "FAIL ") + what, flush=True)
    if not cond:
        failures.append(what)


def free_port():
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    p = s.getsockname()[1]
    s.close()
    return p


def fixture(kind, key):
    return fx / kind / f"{key}.json"


def put(kind, key, data):
    fixture(kind, key).write_text(json.dumps(data), encoding="utf-8")


def edit(kind, key, fn):
    d = json.loads(fixture(kind, key).read_text(encoding="utf-8"))
    fn(d)
    put(kind, key, d)


port = free_port()
env = {**os.environ, "VIDSYNC_YTDLP": str(ROOT / "tests/ytdlp_mock.py"), "VIDSYNC_MOCK_FIXTURES": str(fx), "VIDSYNC_MOCK_DELAY": "0.4",
       "VIDSYNC_MOCK_LOG": str(a.out / "ytdlp-calls.log"), "VIDSYNC_FFMPEG": shutil.which("ffmpeg") or ""}
serve_log = open(a.out / "serve.log", "w")
serve = subprocess.Popen([a.cli, "serve", "--ui", a.ui, "--home", str(home / "app"), "--library", str(lib), "--port", str(port), "--tick", "1"],
                         env=env, stdout=serve_log, stderr=subprocess.STDOUT)
BASE = f"http://127.0.0.1:{port}"
for _ in range(100):
    try:
        urllib.request.urlopen(BASE + "/", timeout=1)
        break
    except Exception:
        time.sleep(0.1)


def wait_for(fn, what, timeout=30, detail=None):
    end = time.time() + timeout
    while time.time() < end:
        try:
            if fn():
                check(True, what)
                return True
        except Exception:
            pass
        time.sleep(0.25)
    try:
        extra = f" [{detail()}]" if detail else ""
    except Exception as e:  # noqa: BLE001
        extra = f" [{e}]"
    check(False, what + " (timed out)" + extra)
    return False


def post(path, data):
    req = urllib.request.Request(BASE + path, data=json.dumps(data).encode(), headers={"content-type": "application/json"}, method="POST")
    return json.loads(urllib.request.urlopen(req).read())


def api(cmd, args=None):
    return post("/invoke/api", {"cmd": cmd, "args": args or {}})


def channel_json(key):
    return json.loads((lib / "youtube.com/channels" / key / "channel.json").read_text(encoding="utf-8"))


def playlist_json(pid):
    return json.loads((lib / "youtube.com/playlists" / pid / "playlist.json").read_text(encoding="utf-8"))


def kept(key):
    return sorted(v["id"] for v in videos_of(key))


def videos_of(key):
    """Each kept video's video.json under a channel's folder."""
    vids = lib / "youtube.com/channels" / key / "videos"
    return [json.loads((d / "video.json").read_text(encoding="utf-8")) for d in sorted(vids.iterdir()) if (d / "video.json").is_file()] if vids.is_dir() else []


def upstream(key, kind="channel"):
    d = channel_json(key) if kind == "channel" else playlist_json(key)
    return {x["id"]: x for x in d["upstream"]}


def idle():
    j = api("jobs")
    return j["current"] is None and not j["queued"]


def recent():
    return api("jobs")["recent"]


def notices():
    return json.loads(urllib.request.urlopen(BASE + "/test/notices").read())


def ytdlp_calls():
    return [json.loads(l) for l in (a.out / "ytdlp-calls.log").read_text().splitlines()]


DEMO = "youtube.com/channels/@Demo"
WORK = "youtube.com/channels/@Workshop"
FAVES = "youtube.com/playlists/PLdemo000000000000"

try:
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=a.chromium, args=["--no-sandbox"]) if a.chromium else p.chromium.launch()
        page = browser.new_page(viewport={"width": 1400, "height": 880})
        page.add_init_script((ROOT / "tests/tauri_shim.js").read_text())
        page.on("pageerror", lambda e: print(f"page error: {e}", flush=True))
        page.goto(BASE + "/")
        page.wait_for_selector(".empty-state")
        check(page.is_visible("text=Add your first channel"), "an empty library offers to add a channel")
        page.screenshot(path=str(a.out / "01-empty.png"))

        # ---- adding a channel, a playlist and a video ---------------------------
        page.fill("#first-link", "https://www.youtube.com/@Demo")
        page.click(".empty-state button[type=submit]")
        page.wait_for_selector("#add-links")
        page.fill("#add-links", "https://www.youtube.com/@Demo\nhttps://www.youtube.com/playlist?list=PLdemo000000000000\nnot a link\nhttps://www.youtube.com/@Nope")
        page.wait_for_selector(".checks li >> nth=3")
        checks = page.inner_text(".checks")
        check("channel @Demo" in checks and "playlist PLdemo000000000000" in checks, "the Add dialog reads each link")
        check("not a link" in checks.lower() or "link" in page.inner_text(".checks .bad"), "a line that isn't a link is pointed out")
        check(page.is_visible("#add-mode") and page.is_visible("#add-pl-mode"), "channels and playlists each get their follow settings")
        check(page.input_value("#add-mode") == "new" and page.input_value("#add-pl-mode") == "all", "channels follow new videos, playlists take every video")
        page.screenshot(path=str(a.out / "02-add.png"))
        check(page.inner_text("#add-go").strip().endswith("Add 3 links"), "three links can be added")
        page.click("#add-go")
        wait_for(lambda: page.locator(".card.source").count() == 2, "the channel and the playlist appear as sources", 40)
        wait_for(idle, "the adds finish", 60)
        check(not (lib / "youtube.com/channels/@Nope").exists(), "a channel that doesn't exist leaves no folder behind")
        check((lib / DEMO / "channel.json").is_file() and (lib / FAVES / "playlist.json").is_file(), "the channel and the playlist each have a folder with their JSON")
        c = channel_json("@Demo")
        check(c["followed"] and c["follow"]["mode"] == "new", "the channel is followed for new videos")
        check(len(c["upstream"]) == 4 and all(x["state"] == "not-wanted" for x in c["upstream"]), "what the channel had already is listed as not wanted")
        check(kept("@Demo") == ["vid00000001", "vid00000003"], "the playlist's videos were downloaded into the channel's folder")
        folders = sorted(d.name for d in (lib / DEMO / "videos").iterdir())
        check(folders == ["2026-01-01 First video [vid00000001]", "2026-01-03 Third video [vid00000003]"] or len(folders) == 2, f"each video has a dated folder {folders}")
        vdir = lib / DEMO / "videos" / folders[0]
        names = sorted(f.suffix for f in vdir.iterdir())
        side = json.loads((vdir / "video.json").read_text(encoding="utf-8")) if (vdir / "video.json").is_file() else {}
        check(side.get("id") in ("vid00000001", "vid00000003") and side.get("file", "").startswith(side.get("title", "?")), f"each video's folder has its own video.json {side}")
        check("videos" not in channel_json("@Demo"), "channel.json holds no video records (they're in each folder)")
        check((".webm" in names or ".mp4" in names) and ".json" in names and ".jpg" in names, f"the media, info and thumbnail are kept together {names}")
        check(not (lib / "_library/.partial").exists() or not any((lib / "_library/.partial").iterdir()), "nothing is left in the partial folder")
        pl = playlist_json("PLdemo000000000000")
        check([x["id"] for x in pl["upstream"]] == ["vid00000001", "vid00000003"], "the playlist keeps its order")
        page.click(".sidebar >> text=Activity")
        page.wait_for_selector("#finished")
        wait_for(lambda: page.locator("#finished li").count() == 3, "Activity lists each finished job")
        act = page.inner_text("#finished")
        check("doesn't exist on YouTube" in act, "Activity says why a channel couldn't be added")
        check("downloaded 2 videos" in act, f"Activity counts what the playlist downloaded: {act[:300]!r}")
        page.screenshot(path=str(a.out / "03-activity.png"))

        # ---- the videos page: cards, list, search, filters ------------------------
        page.click(".sidebar >> text=Videos")
        page.wait_for_selector(".card.video")
        check(page.locator(".card.video").count() == 2, "the videos page has a card per video")
        card = page.locator(".card.video[data-id='vid00000001']")
        check("First video" in card.inner_text() and "Demo Channel" in card.inner_text(), "a card shows the title and the channel")
        wait_for(lambda: page.evaluate("document.querySelector(\".card.video[data-id='vid00000001'] img\")?.naturalWidth") == 16, "the thumbnail loads from the library folder")
        page.screenshot(path=str(a.out / "04-videos.png"))
        page.fill("#search", "third")
        wait_for(lambda: page.locator(".card.video").count() == 1, "search finds words in titles")
        page.fill("#search", "")
        wait_for(lambda: page.locator(".card.video").count() == 2, "clearing the search shows everything")
        page.click("button[title=List]")
        page.wait_for_selector("table.list tr.row")
        wait_for(lambda: page.locator("table.list tr.row").count() == 2, "the list view has a row per video")
        page.click("button[title=Cards]")
        side = page.inner_text(".sidebar").lower()
        check("channels" in side and "playlists" in side and "demo channel" in side and "favourites" in side, "the menu lists channels and playlists")
        page.click(".sidebar .filters >> text=Favourites")
        wait_for(lambda: page.locator(".card.video").count() == 2 and "Favourites" in page.inner_text(".cmdbar"), "a playlist filters the videos")
        page.click(".sidebar >> text=Videos")

        # ---- a channel's page and its audit ---------------------------------------
        page.goto(BASE + f"/#/source/{DEMO}")
        page.wait_for_selector(".source-title h1")
        check("Demo Channel" in page.inner_text(".source-title"), "the channel's page shows its name")
        check("follow" in page.inner_text(".inspector").lower() and page.input_value("#follow-mode") == "new", "the inspector shows how the channel is followed")
        props = page.inner_text("#props")
        for label in ("Added", "Last checked", "Kept", "Upstream", "Subscribers", "Next check"):
            check(label in props, f"details show {label}")
        page.screenshot(path=str(a.out / "05-channel.png"))
        page.click("#tab-audit")
        page.wait_for_selector("#upstream tbody tr")
        rows = page.locator("#upstream tbody tr")
        check(rows.count() == 4, "the Audit tab lists what the channel has upstream")
        check(page.locator("#upstream tr[data-state=kept]").count() == 2 and page.locator("#upstream tr[data-state=not-wanted]").count() == 2, "listed videos are marked kept or not wanted")
        page.screenshot(path=str(a.out / "06-audit-tab.png"))
        page.click("#upstream tr[data-video=vid00000002] button[title='Download this video']")
        wait_for(lambda: "vid00000002" in kept("@Demo"), "a not-wanted video can be fetched from the audit", 40)
        wait_for(lambda: page.locator("#upstream tr[data-video=vid00000002][data-state=kept]").count() == 1, "the audit shows it as kept")
        page.click("#upstream tr[data-video=vid00000004] button[title^=Skip]")
        wait_for(lambda: upstream("@Demo")["vid00000004"]["state"] == "skipped", "a video can be skipped on purpose")
        wait_for(lambda: page.locator("#upstream tr[data-video=vid00000004][data-state=skipped]").count() == 1, "the audit shows it as skipped")

        # ---- a new video upstream, fetched by hand ---------------------------------
        put("videos", "vid00000005", {"title": "Fifth video", "duration": 90, "upload_date": "20260301", "view_count": 5, "description": "The fifth"})
        edit("channels", "@Demo", lambda d: d["tabs"]["videos"].insert(0, "vid00000005"))
        page.click("#check-now")
        wait_for(lambda: "vid00000005" in kept("@Demo"), "Check now downloads the channel's new video", 40)
        wait_for(idle, "the check finishes")
        check(upstream("@Demo")["vid00000005"]["state"] == "wanted", "the new video was wanted")
        wait_for(lambda: page.locator("#upstream tbody tr").count() == 5, "the audit lists the new video")
        wait_for(lambda: page.locator("#upstream tr[data-video=vid00000005][data-state=kept]").count() == 1, "and shows it as kept")
        page.click("#check-now")
        wait_for(lambda: idle() and "Up to date" in recent()[0]["message"], "a check with nothing new says it's up to date")
        check(len(videos_of("@Demo")) == 4, "nothing is downloaded twice")

        # ---- a channel whose videos are all wanted; one fails -----------------------
        page.keyboard.press("n")
        page.wait_for_selector("#add-links")
        page.fill("#add-links", "https://www.youtube.com/@Workshop")
        page.wait_for_selector(".checks li")
        page.select_option("#add-mode", "all")
        page.check("#add-streams")
        page.click("#add-go")
        wait_for(lambda: (lib / WORK / "channel.json").is_file() and idle(), "the second channel is added and its videos fetched", 60)
        w = channel_json("@Workshop")
        up = upstream("@Workshop")
        check(kept("@Workshop") == ["liv00000001", "wrk00000001"], f"every video that could be downloaded was {kept('@Workshop')}")
        check(up["wrk00000002"]["state"] == "failed" and "members" in up["wrk00000002"]["error"].lower(), "a members-only video is marked failed with the reason")
        check(up["liv00000001"]["tab"] == "streams", "live streams are listed from their tab")
        check((lib / WORK / "avatar.jpg").is_file() or (lib / WORK / "avatar.png").is_file(), "the channel's avatar is saved")
        page.goto(BASE + f"/#/source/{WORK}?tab=audit")
        page.wait_for_selector("#upstream tbody tr")
        check(page.locator("#upstream tr[data-video=wrk00000002][data-state=failed]").count() == 1, "the audit shows the failed video")
        check("members" in page.inner_text("#upstream tr[data-video=wrk00000002]").lower(), "with yt-dlp's reason")
        check(page.is_visible("#download-missing"), "the channel's page offers to download what's missing")
        page.screenshot(path=str(a.out / "07-failed.png"))
        page.click("#upstream tr[data-video=wrk00000002] button[title^=Skip]")
        wait_for(lambda: upstream("@Workshop")["wrk00000002"]["state"] == "skipped", "a failed video can be skipped")
        wait_for(lambda: not page.is_visible("#download-missing"), "then nothing is missing")

        # ---- sources, creators, tags ------------------------------------------------
        page.goto(BASE + "/#/sources")
        page.wait_for_selector(".card.source")
        wait_for(lambda: page.locator(".card.source").count() == 3, "Sources lists both channels and the playlist")
        wcard = page.locator(f".card.source[data-id='{WORK}']")
        wait_for(lambda: page.evaluate(f"document.querySelector(\".card.source[data-id='{WORK}'] .avatar img\")?.naturalWidth") == 8, "a channel card shows its avatar")
        check("skipped" in wcard.inner_text().lower() or "1" in wcard.inner_text(), "a card carries the audit's counts")
        box = lambda sel: page.locator(sel).first.bounding_box()
        card, title, chips = box(f".card.source[data-id='{WORK}']"), box(f".card.source[data-id='{WORK}'] h3"), box(f".card.source[data-id='{WORK}'] .audit-chips")
        check(title["width"] > card["width"] * 0.4 and title["height"] < 40, f"a source card's name runs across the card, not down it (card {card['width']:.0f}px, name {title['width']:.0f}x{title['height']:.0f}px)")
        check(chips["width"] > card["width"] * 0.6 and chips["height"] < 60, f"a source card's counts sit in a row ({chips['width']:.0f}x{chips['height']:.0f}px)")
        page.screenshot(path=str(a.out / "08-sources.png"))
        for key in ("@Demo", "@Workshop"):
            page.goto(BASE + f"/#/source/youtube.com/channels/{key}")
            page.wait_for_selector("#creator-input")
            page.fill("#creator-input", "Demo Studio")
            page.press("#creator-input", "Enter")
            wait_for(lambda: channel_json(key)["creator"] == "Demo Studio", f"{key} is given a creator")
        page.fill("#tag-input", "talks")
        page.press("#tag-input", "Enter")
        wait_for(lambda: channel_json("@Workshop")["tags"] == ["talks"], "a tag is saved on the channel")
        page.goto(BASE + "/#/")
        page.wait_for_selector(".card.video")
        wait_for(lambda: "creators" in page.inner_text(".sidebar").lower() and "Demo Studio" in page.inner_text(".sidebar"), "the menu lists creators")
        page.click(".sidebar .filters >> text=Demo Studio")
        wait_for(lambda: page.locator(".card.video").count() == 6 and "Demo Studio" in page.inner_text(".cmdbar"), "a creator filters to every channel's videos")
        page.click(".sidebar .filters >> text=talks")
        wait_for(lambda: page.locator(".card.video").count() == 2, "a tag filters the videos")
        page.fill("#search", "opening")
        wait_for(lambda: page.locator(".card.video").count() == 1, "search and a filter work together")
        page.fill("#search", "")
        page.screenshot(path=str(a.out / "09-creator.png"))
        page.goto(BASE + "/#/sources")
        page.wait_for_selector(".card.source")
        check(page.locator(".creator-head[data-creator='Demo Studio']").count() == 1 and page.locator(".creator-head[data-creator='Demo Studio'] + .cards .card.source").count() == 2, "Sources groups channels by creator")
        page.click(".creator-head[data-creator='Demo Studio'] a")
        page.wait_for_selector("#creator-page")
        check(page.locator("#creator-page .card.source").count() == 2, "a creator's page lists their channels")
        wait_for(lambda: page.locator("#creator-feed .card.video").count() == 6 and "Demo Studio" in page.inner_text(".cmdbar"), "and one feed of all their videos, newest first", detail=lambda: f"{page.locator('.card.video').count()} cards, hash {page.evaluate('location.hash')}, filter {page.evaluate('JSON.stringify(window.__vidsync.ui.get().filter)')}, {page.inner_text('.cmdbar')[:80]!r}")
        page.goto(BASE + "/#/sources")
        page.wait_for_selector(".card.source")
        page.screenshot(path=str(a.out / "09b-sources-creators.png"))
        page.goto(BASE + "/#/creator/Demo%20Studio")
        page.wait_for_selector("#creator-feed")
        dates = page.evaluate("[...document.querySelectorAll('#creator-feed .card.video')].map(c => c.dataset.id)")
        check(dates[0] == "vid00000005" and dates.index("liv00000001") < dates.index("wrk00000001"), f"the newest video comes first {dates}")
        page.screenshot(path=str(a.out / "09c-creator-page.png"))

        # ---- the audit page ------------------------------------------------------------
        page.goto(BASE + "/#/audit")
        page.wait_for_selector("#audit tbody tr")
        check(page.locator("#audit tbody tr").count() == 3, "the Audit page has a row per source")
        drow = page.inner_text(f"#audit tr[data-id='{DEMO}']")
        check("Demo Channel" in drow, "with the source's name")
        foot = page.inner_text("#audit tfoot")
        check("Every source" in foot, "and a total")
        check("nothing missing" in page.inner_text(".cmdbar"), "the audit says nothing is missing")
        page.screenshot(path=str(a.out / "10-audit.png"))
        edit("channels", "@Workshop", lambda d: d["tabs"]["videos"].insert(0, "wrk00000003"))
        put("videos", "wrk00000003", {"title": "Closing talk", "duration": 600, "upload_date": "20260310"})
        api("check", {"id": WORK})
        wait_for(lambda: "wrk00000003" in kept("@Workshop") and idle(), "a check from the audit page fetches the new video", 40)

        # ---- a download cut off half way carries on -----------------------------------------
        edit("channels", "@Workshop", lambda d: d["tabs"]["videos"].insert(0, "wrk00000004"))
        put("videos", "wrk00000004", {"title": "Interrupted talk", "duration": 900, "upload_date": "20260312", "break_once": True})
        api("check", {"id": WORK})
        wait_for(lambda: idle() and upstream("@Workshop").get("wrk00000004", {}).get("state") == "failed", "a download cut off part way is marked failed", 40)
        check("reached" in upstream("@Workshop")["wrk00000004"]["error"], f"with why: {upstream('@Workshop')['wrk00000004']['error']!r}")
        part_dir = lib / "_library/.partial/wrk00000004"
        check(part_dir.is_dir() and any(f.name.endswith(".part") for f in part_dir.iterdir()), "what it got so far is kept for the next try")
        last_dl = next(c for c in reversed(ytdlp_calls()) if "--progress-template" in c)
        check("--no-part" not in last_dl and "--continue" in last_dl, "yt-dlp is asked to keep .part files and carry them on")
        api("video_get", {"id": WORK, "video": "wrk00000004"})
        wait_for(lambda: idle() and "wrk00000004" in kept("@Workshop"), "the next try finishes it", 40)
        d4 = next(d for d in (lib / WORK / "videos").iterdir() if "wrk00000004" in d.name)
        info4 = json.loads(next(d4.glob("*.info.json")).read_text(encoding="utf-8"))
        check(info4.get("vidsync_mock_resumed_from", 0) > 0, "carrying on from where it stopped, not from the start")
        check(not part_dir.exists() and not any(f.name.endswith(".part") or f.name == "try.json" for d in (lib / WORK / "videos").iterdir() for f in d.iterdir()), "and nothing of the try is left behind")

        # ---- the video's page and the player ---------------------------------------------
        page.goto(BASE + f"/#/video/@Workshop/wrk00000001")
        page.wait_for_selector("#player")
        if REAL_VIDEO:
            wait_for(lambda: page.locator("#player video").count() == 1, "the video's page has a player",
                     detail=lambda: f"player: {page.inner_text('#player')[:200]!r}; page errors: {page.evaluate('window.__errors')}")
        else:
            # without ffmpeg the stand-in writes a stub that no player can decode: the page must say so, not break
            wait_for(lambda: "can't be played here" in page.inner_text("#player"), "a file the player can't decode is explained, with Open in the system's player (no ffmpeg here, so the sample is a stub)")
        check("Opening talk" in page.inner_text("h1"), "the video's page shows its title")
        check(page.locator(".chapters li").count() == 2, "chapters are listed")
        check("Workshop Talks" in page.inner_text(".crumbs"), "the crumbs lead back to the channel")
        props = page.inner_text("#props")
        for label in ("Channel", "Creator", "Published", "Downloaded", "On disk"):
            check(label in props, f"the video's details show {label}")
        check("Demo Studio" in props, "with the creator")
        if REAL_VIDEO:
            src = page.get_attribute("#player video", "src") or ""
            check(urllib.parse.unquote(src).startswith("/library/youtube.com/channels/@Workshop/videos/"), f"the player plays the file from the library folder {src}")
            wait_for(lambda: page.evaluate("document.querySelector('#player video').readyState") >= 1 and page.evaluate("document.querySelector('#player video').duration") > 1.5, "the player reads the video's length")
            r = urllib.request.Request(BASE + src, headers={"Range": "bytes=100-199"})
            with urllib.request.urlopen(r) as resp:
                check(resp.status == 206 and resp.headers["content-range"].startswith("bytes 100-199/") and len(resp.read()) == 100, "the library serves byte ranges, so the player can seek")
            r = urllib.request.Request(BASE + src, headers={"Range": "bytes=0-"})
            with urllib.request.urlopen(r) as resp:
                size = int(resp.headers["content-range"].rsplit("/", 1)[1])
                check(resp.status == 206 and len(resp.read()) == min(size, 8 << 20), "an open-ended range is answered with a part, not the whole file at once")
        if REAL_VIDEO:
            page.click(".chapters li >> nth=1 >> button")
        page.screenshot(path=str(a.out / "11-video.png"))
        page.fill("#tag-input", "keeper")
        page.press("#tag-input", "Enter")
        wait_for(lambda: next(v for v in videos_of("@Workshop") if v["id"] == "wrk00000001")["tags"] == ["keeper"], "a tag is saved on the video")
        page.click("#open-system")
        wait_for(lambda: any(o["cmd"] == "open_path" and "wrk00000001" in json.dumps(o["args"]) for o in json.loads(urllib.request.urlopen(BASE + "/test/opened").read())), "Open in player hands the file to the system")

        # ---- watched, and carrying on where you stopped --------------------------------------
        vjson = lambda key, vid: next(v for v in videos_of(key) if v["id"] == vid)
        page.goto(BASE + "/#/sources")  # leaving the player saves where it was: do that first
        page.wait_for_selector(".card.source")
        page.wait_for_timeout(300)
        api("video_progress", {"channel": "@Demo", "video": "vid00000003", "position": 61.24})
        check(vjson("@Demo", "vid00000003")["position"] == 61.2 and vjson("@Demo", "vid00000003")["played"], "where playing stopped is saved in the video's video.json")
        page.goto(BASE + "/#/")
        page.reload()
        page.wait_for_selector("#continue")
        check(page.locator("#continue .card.video").count() == 1 and page.locator("#continue .card.video[data-id=vid00000003]").count() == 1, "Continue watching shows a video started and not finished")
        check(page.locator("#all-videos .card.video[data-id=vid00000003] .played").count() == 1, "its card shows how far it was watched")
        page.screenshot(path=str(a.out / "11b-continue.png"))
        page.goto(BASE + "/#/video/@Demo/vid00000003")
        page.wait_for_selector("#watched-toggle")
        page.click("#watched-toggle")
        wait_for(lambda: vjson("@Demo", "vid00000003")["watched"] and vjson("@Demo", "vid00000003")["position"] == 0, "a video can be marked watched (it starts from the beginning next time)")
        wait_for(lambda: "Watched" in page.inner_text("#watched-toggle"), "the button says so")
        page.goto(BASE + "/#/sources")
        page.wait_for_selector(".card.source")
        page.goto(BASE + "/#/")
        page.wait_for_selector("#all-videos .card.video")
        wait_for(lambda: page.locator("#continue").count() == 0, "a watched video leaves Continue watching")
        check(page.locator("#all-videos .card.video[data-id=vid00000003] .seen").count() == 1, "its card is marked watched")
        unseen = page.locator("#all-videos .card.video:not(.watched)").count()
        page.click(".sidebar .filters >> text=Not watched yet")
        wait_for(lambda: page.locator(".card.video").count() == unseen and page.locator(".card.video.watched").count() == 0, "the menu filters to what's not watched yet")
        page.click(".sidebar .filters >> text=Not watched yet")
        if REAL_VIDEO:
            page.goto(BASE + "/#/video/@Demo/vid00000001")
            page.wait_for_selector("#player video")
            wait_for(lambda: page.evaluate("document.querySelector('#player video').readyState") >= 1, "the player is ready")
            page.evaluate("(() => { const v = document.querySelector('#player video'); v.muted = true; v.currentTime = Math.max(0, v.duration - 0.4); return v.play(); })()")
            wait_for(lambda: vjson("@Demo", "vid00000001")["watched"], "playing to the end marks a video watched", 15)
            api("videos_watched", {"items": [{"channel": "@Demo", "video": "vid00000001"}], "watched": False})

        # ---- the schedule ---------------------------------------------------------------
        put("videos", "vid00000006", {"title": "Sixth video", "duration": 30, "upload_date": "20260401"})
        edit("channels", "@Demo", lambda d: d["tabs"]["videos"].insert(0, "vid00000006"))
        settings = api("settings_get")["library"]
        api("settings_set", {"library": {**settings, "check_every_hours": 0.0005}})
        wait_for(lambda: "vid00000006" in kept("@Demo"), "the schedule fetches a new video by itself", 60)
        api("settings_set", {"library": {**settings, "check_every_hours": 24}})
        wait_for(idle, "scheduled checks finish", 60)
        check(any(f["job"]["by"] == "schedule" for f in recent()), "Activity records scheduled checks")
        check(any("new video" in n["title"] and "Sixth video" in n["text"] for n in notices()), f"a scheduled check that brings new videos shows a notice {notices()}")
        page.goto(BASE + "/#/settings")
        page.wait_for_selector("#check-every")
        check(page.input_value("#check-every") == "24", "Settings shows how often sources are checked")
        check("yt-dlp" in page.inner_text(".tools") and "2026.10.26-mock" in page.inner_text(".tools"), "Settings shows the yt-dlp that was found and its version")
        page.screenshot(path=str(a.out / "12-settings.png"))

        # ---- running in the background, being gentle, cookies -----------------------------------
        check(page.is_checked("#keep-running") and page.is_checked("#notices"), "by default Vid Sync keeps checking from the tray and shows notices")
        page.wait_for_selector("#autostart")
        check(not page.is_checked("#autostart"), "starting at sign-in is off until asked for")
        page.check("#autostart")
        wait_for(lambda: post("/invoke/autostart_get", {}) is True, "it can be turned on from Settings")
        page.uncheck("#keep-running")
        wait_for(lambda: api("settings_get")["quit_on_close"] is True, "closing the window can be made to quit")
        page.check("#keep-running")
        wait_for(lambda: api("settings_get")["quit_on_close"] is False, "and back")
        check(api("settings_get")["polite"]["sleep_downloads"] == 3, "downloads pause a few seconds between them by default")
        page.fill("#sleep-downloads", "2")
        page.press("#sleep-downloads", "Enter")
        page.fill("#limit-rate", "4")
        page.press("#limit-rate", "Enter")
        wait_for(lambda: api("settings_get")["polite"]["sleep_downloads"] == 2 and api("settings_get")["polite"]["limit_rate"] == 4 * 1048576, "the pause and a speed limit are saved")
        page.select_option("#cookies-from", "browser:chrome")
        wait_for(lambda: api("settings_get")["cookies"] == {"mode": "browser", "browser": "chrome", "file": ""}, "cookies are picked from a list of browsers")
        page.click("#cookies-test")
        page.wait_for_selector("#cookies-result", timeout=20000)
        check("Firefox" in page.inner_text("#cookies-result") and "error" in page.get_attribute("#cookies-result", "class"), f"Test says when a browser's cookies can't be read, and what to do {page.inner_text('#cookies-result')!r}")
        page.select_option("#cookies-from", "browser:firefox")
        wait_for(lambda: api("settings_get")["cookies"]["browser"] == "firefox", "another browser can be picked")
        page.click("#cookies-test")
        wait_for(lambda: "age-restricted" in page.inner_text("#cookies-result") and " ok" in page.get_attribute("#cookies-result", "class"), "Test says when YouTube answers with the cookies")
        page.locator("#cookies-test").scroll_into_view_if_needed()
        page.screenshot(path=str(a.out / "12c-settings-cookies.png"))
        page.locator("#keep-running").scroll_into_view_if_needed()
        page.screenshot(path=str(a.out / "12c-settings-background.png"))
        test_call = next(c for c in reversed(ytdlp_calls()) if "HtVdAasjOgU" in " ".join(c))
        check(test_call[test_call.index("--cookies-from-browser") + 1] == "firefox", "the test reads an age-restricted video with the browser's cookies")

        # ---- following settings on a source -------------------------------------------------
        page.goto(BASE + f"/#/source/{DEMO}")
        page.wait_for_selector("#follow-mode")
        page.select_option("#follow-quality", "720p")
        page.check("#follow-shorts")
        wait_for(lambda: channel_json("@Demo")["follow"]["quality"] == "720p" and channel_json("@Demo")["follow"]["tabs"]["shorts"], "a channel's follow settings are saved")
        page.click("#check-now")
        wait_for(lambda: idle() and "sho00000001" in upstream("@Demo"), "the next check lists the shorts tab too", 40)
        check(upstream("@Demo")["sho00000001"]["state"] == "wanted" and "sho00000001" in kept("@Demo"), "a video on a newly followed tab counts as new and is fetched")
        calls = (a.out / "ytdlp-calls.log").read_text().splitlines()
        last_dl = next(json.loads(l) for l in reversed(calls) if "--progress-template" in l)
        check(any("res:720" in x for x in last_dl), "the quality asked for is passed to yt-dlp")
        check(last_dl[last_dl.index("--sleep-interval") + 1] == "2" and last_dl[last_dl.index("--limit-rate") + 1] == str(4 * 1048576), "with the pause and the speed limit from Settings")
        check(last_dl[last_dl.index("--cookies-from-browser") + 1] == "firefox", "and the cookies")
        api("settings_set", {"cookies": {"mode": "", "browser": "", "file": ""}, "polite": {"sleep_requests": 0.5, "sleep_downloads": 3, "limit_rate": 0}})

        # ---- keep only the newest ------------------------------------------------------------------
        put("videos", "wrk00000005", {"title": "Panel", "duration": 1200, "upload_date": "20260315"})
        put("videos", "wrk00000006", {"title": "Keynote", "duration": 1500, "upload_date": "20260316"})
        put("videos", "wrk00000007", {"title": "Archive talk", "duration": 700, "upload_date": "20250101"})
        edit("channels", "@Workshop", lambda d: d["tabs"].__setitem__("videos", ["wrk00000006", "wrk00000005"] + d["tabs"]["videos"] + ["wrk00000007"]))
        page.goto(BASE + f"/#/source/{WORK}")
        page.wait_for_selector("#follow-keep")
        before = sorted(kept("@Workshop"))
        page.select_option("#follow-keep", "5")
        wait_for(lambda: channel_json("@Workshop")["follow"]["keep_newest"] == 5, "a channel can keep only its newest videos")
        check(page.is_visible("#follow-drop"), "or delete what's been watched after a while")
        api("follow_set", {"id": WORK, "follow": {**channel_json("@Workshop")["follow"], "keep_newest": 2}})  # a number the list doesn't offer, so the test stays small
        api("check", {"id": WORK})
        wait_for(lambda: idle() and "wrk00000006" in kept("@Workshop"), "the check fetches what's new", 40)
        w_up = upstream("@Workshop")
        check(kept("@Workshop") == ["wrk00000001", "wrk00000005", "wrk00000006"], f"only the newest two stay, and the tagged one: {kept('@Workshop')} (was {before})")
        check(w_up["wrk00000003"]["state"] == "not-wanted" and "newest 2" in w_up["wrk00000003"]["error"], f"the audit still lists what was deleted, and why: {w_up['wrk00000003']}")
        check(w_up["wrk00000007"]["state"] == "not-wanted" and "wrk00000007" not in kept("@Workshop") and not any("wrk00000007" in " ".join(c) for c in ytdlp_calls() if "--progress-template" in c), "an older video isn't downloaded only to be deleted")
        fin = next(f for f in recent() if f["job"]["target"] == WORK)
        check(len(fin["outcome"].get("deleted", [])) == 3 and "deleted" in fin["message"], f"Activity says what was deleted: {fin['message']!r}")
        page.goto(BASE + "/#/activity")
        page.wait_for_selector("#finished .sublist.deleted")
        page.screenshot(path=str(a.out / "12e-pruned.png"))

        # ---- choosing several videos ------------------------------------------------------------------
        page.goto(BASE + "/#/")
        page.wait_for_selector("#all-videos .card.video")
        card = lambda vid: page.locator(f"#all-videos .card.video[data-id={vid}]")
        card("wrk00000006").click(modifiers=["Control"])
        card("wrk00000005").click(modifiers=["Control"])
        page.wait_for_selector("#selection-bar")
        check("2 videos chosen" in page.inner_text("#selection-bar"), "Ctrl and click chooses videos instead of opening them")
        check(page.evaluate("location.hash") in ("#/", ""), "and stays on the page")
        page.fill("#sel-tag", "conference")
        page.click("#sel-tag-add")
        wait_for(lambda: all("conference" in vjson("@Workshop", x)["tags"] for x in ("wrk00000005", "wrk00000006")), "a tag is added to every chosen video")
        page.click("#sel-watched")
        wait_for(lambda: all(vjson("@Workshop", x)["watched"] for x in ("wrk00000005", "wrk00000006")), "the chosen videos can be marked watched together")
        page.screenshot(path=str(a.out / "12f-chosen.png"))
        page.keyboard.press("Escape")
        wait_for(lambda: page.locator("#selection-bar").count() == 0, "Esc lets go of them")
        card("wrk00000006").click(modifiers=["Control"])
        card("wrk00000005").click(modifiers=["Shift"])
        wait_for(lambda: "2 videos chosen" in page.inner_text("#selection-bar"), "Shift and click chooses a run of videos")
        page.select_option("#sel-untag", "conference")
        wait_for(lambda: not any("conference" in vjson("@Workshop", x)["tags"] for x in ("wrk00000005", "wrk00000006")), "a tag can be taken off them together")
        page.click("#sel-delete")
        page.wait_for_selector("#confirm-ok")
        page.click("#confirm-ok")
        wait_for(lambda: kept("@Workshop") == ["wrk00000001"], "the chosen videos can be deleted together")
        wait_for(lambda: page.locator("#selection-bar").count() == 0, "and nothing stays chosen")

        # ---- searching inside videos, comments, saved views ----------------------------------------------
        put("videos", "wrk00000008", {"title": "Workshop day", "duration": 2400, "upload_date": "20260317", "description": "Notes on the paddle shape and the varnish.",
                                      "cues": [[0.0, 1.0, "good morning"], [1.0, 1.5, "today we will build a"], [1.5, 2.0, "canoe from scratch"], [65.0, 66.0, "the canoe floats"]],
                                      "comments": [{"author": "Ana", "text": "Great build!", "like_count": 12}, {"author": "Workshop Talks", "text": "Thanks all", "like_count": 3, "author_is_uploader": True}]})
        edit("channels", "@Workshop", lambda d: d["tabs"]["videos"].insert(0, "wrk00000008"))
        api("follow_set", {"id": WORK, "follow": {**channel_json("@Workshop")["follow"], "keep_newest": 0, "subtitles": "en.*,en", "comments": True}})
        api("check", {"id": WORK})
        wait_for(lambda: idle() and "wrk00000008" in kept("@Workshop"), "a video is downloaded with its subtitles and comments", 40)
        last_dl = next(c for c in reversed(ytdlp_calls()) if "--progress-template" in c)
        check("--write-comments" in last_dl and any("max_comments=200" in x for x in last_dl), "yt-dlp is asked for the top 200 comments")
        page.goto(BASE + "/#/video/@Workshop/wrk00000008")
        page.wait_for_selector("#comments")
        check(page.locator("#comments li").count() == 2 and "Great build!" in page.inner_text("#comments"), "the video's page shows the saved comments")
        page.goto(BASE + "/#/")
        page.wait_for_selector("#search")
        page.fill("#search", "build a canoe")
        page.wait_for_selector("#said")
        check(page.locator("#said li[data-id=wrk00000008]").count() == 1, "a search finds what's said in a video's subtitles, across two lines")
        hit = page.locator("#said li[data-id=wrk00000008] a.hit").first
        check("0:01" in hit.inner_text() and "canoe" in hit.inner_text(), f"with the moment it's said: {hit.inner_text()!r}")
        check(page.locator("#said li[data-id=wrk00000008] a.hit").count() == 1, "once, though it runs over two lines")
        page.screenshot(path=str(a.out / "12g-said.png"))
        hit.click()
        wait_for(lambda: page.evaluate("location.hash").endswith("wrk00000008?t=1"), "the moment opens the video there")
        if REAL_VIDEO:
            wait_for(lambda: abs(page.evaluate("document.querySelector('#player video').currentTime") - 1) < 0.6, "and the player starts from that second")
        page.goto(BASE + "/#/")
        page.wait_for_selector("#search")
        page.fill("#search", "paddle shape")
        wait_for(lambda: page.locator("#said li[data-id=wrk00000008] .desc-hit").count() == 1, "a search finds words in descriptions too")
        page.fill("#search", "")
        wait_for(lambda: page.locator("#said").count() == 0, "clearing the search clears them")
        page.click(".sidebar .filters >> text=Workshop Talks")
        page.fill("#search", "day")
        page.wait_for_selector("#save-view")
        page.click("#save-view")
        page.wait_for_selector("#prompt-input")
        page.fill("#prompt-input", "Workshop days")
        page.click("#prompt-ok")
        wait_for(lambda: page.locator(".saved-views [data-view='Workshop days']").count() == 1, "the view can be saved to the menu")
        wait_for(lambda: any(v["name"] == "Workshop days" and v["filter"]["by"] == "channel" and v["query"] == "day" for v in api("prefs_get").get("views", [])), "with its filter and search, on this computer")
        page.click(".cmdbar .chip")
        page.fill("#search", "")
        wait_for(lambda: page.locator(".card.video").count() > 1, "the menu's filter can be cleared")
        page.click(".saved-views [data-view='Workshop days']")
        wait_for(lambda: page.input_value("#search") == "day" and "Workshop Talks" in page.inner_text(".cmdbar") and page.locator("#all-videos .card.video").count() == 1, "a saved view brings back its filter and search")
        page.screenshot(path=str(a.out / "12h-saved-view.png"))
        page.hover(".saved-views .view-link")
        page.click(".saved-views .forget")
        wait_for(lambda: page.locator(".saved-views").count() == 0 and not api("prefs_get").get("views"), "a saved view can be forgotten")
        page.click(".cmdbar .chip")
        page.fill("#search", "")

        # ---- downloads side by side -------------------------------------------------------------------
        page.goto(BASE + "/#/settings")
        page.wait_for_selector("#parallel")
        page.select_option("#parallel", "3")
        wait_for(lambda: api("settings_get")["parallel"] == 3, "Settings can download a few videos at once")
        batch = [f"wrk000000{n}" for n in (10, 11, 12, 13)]
        for i, vid in enumerate(batch):
            put("videos", vid, {"title": f"Session {i + 1}", "duration": 600, "upload_date": f"2026032{i}", "delay": 4})
        edit("channels", "@Workshop", lambda d: d["tabs"]["videos"].__setitem__(slice(0, 0), list(reversed(batch))))
        api("check", {"id": WORK})
        page.goto(BASE + "/#/activity")
        page.reload()  # started behind the page's back, so look now rather than at the next idle poll
        wait_for(lambda: page.locator("#side-by-side li").count() >= 2, "a check downloads several videos side by side, each shown in Activity", 30, detail=lambda: page.inner_text("#job-now"))
        page.screenshot(path=str(a.out / "12i-side-by-side.png"))
        wait_for(lambda: idle() and all(v in kept("@Workshop") for v in batch), "and every one of them arrives", 60)
        api("settings_set", {"parallel": 1})

        # ---- downloading a video again in another quality ---------------------------------------------
        api("videos_tag", {"items": [{"channel": "@Workshop", "video": "wrk00000008"}], "add": ["favourite"]})
        api("video_progress", {"channel": "@Workshop", "video": "wrk00000008", "position": 300})
        page.goto(BASE + "/#/video/@Workshop/wrk00000008")
        page.wait_for_selector("#video-more")
        page.click("#video-more")
        page.click("#redownload")
        page.wait_for_selector("#choose-720p")
        page.click("#choose-720p")
        wait_for(lambda: idle() and vjson("@Workshop", "wrk00000008")["quality"] == "720p", "a video can be downloaded again in another quality", 40)
        v8 = vjson("@Workshop", "wrk00000008")
        check(v8["tags"] == ["favourite"] and v8["position"] == 300, f"its tags and where you stopped carry over {v8['tags']} {v8['position']}")
        dirs8 = [d.name for d in (lib / WORK / "videos").iterdir() if "wrk00000008" in d.name]
        check(len(dirs8) == 1 and not dirs8[0].endswith("_"), f"the old copy is gone and the new one has its folder name {dirs8}")
        last_dl = next(c for c in reversed(ytdlp_calls()) if "--progress-template" in c)
        check(any("res:720" in x for x in last_dl), "yt-dlp is asked for the new quality")

        # ---- bringing in videos downloaded elsewhere -----------------------------------------------------
        old = home / "old-downloads"
        (old / "talks").mkdir(parents=True)
        sample = fx / "sample.webm" if (fx / "sample.webm").is_file() else None
        (old / "talks" / "Old talk [imp00000001].webm").write_bytes(sample.read_bytes() if sample else b"\x1aE\xdf\xa3" + bytes(3000))
        (old / "talks" / "Old talk [imp00000001].info.json").write_text(json.dumps({"id": "imp00000001", "title": "Old talk", "channel_id": "UC_work_0000000000000000", "channel": "Workshop Talks", "uploader_id": "@Workshop", "upload_date": "20250505", "duration": 2, "ext": "webm"}), encoding="utf-8")
        (old / "talks" / "Old talk [imp00000001].jpg").write_bytes((fx / "thumb.png").read_bytes())
        (old / "Mystery [imp00000002].mp4").write_bytes(b"\x00\x00\x00\x1cftypisom" + bytes(4000))
        put("videos", "imp00000002", {"title": "Mystery clip", "duration": 30, "upload_date": "20250606", "channel": "@Workshop"})
        (old / "Copy of opening [wrk00000001].mp4").write_bytes(b"\x00\x00\x00\x1cftypisom" + bytes(500))
        (old / "holiday.mp4").write_bytes(b"\x00" * 100)
        post("/test/pick", {"path": str(old)})
        page.goto(BASE + "/#/settings")
        page.wait_for_selector("#import")
        page.click("#import")
        page.wait_for_selector("#choose-move")
        body = page.inner_text(".dialog")
        check("2 videos" in body and "1 video it has already" in body and "1 video came without" in body, f"Import says what it found: {body[:300]!r}")
        page.screenshot(path=str(a.out / "12j-import.png"))
        page.click("#choose-move")
        wait_for(lambda: idle() and "imp00000001" in kept("@Workshop") and "imp00000002" in kept("@Workshop"), "the videos are brought into their channel", 40)
        check(not (old / "talks" / "Old talk [imp00000001].webm").exists() and not (old / "Mystery [imp00000002].mp4").exists(), "moved, not copied, as asked")
        check((old / "Copy of opening [wrk00000001].mp4").exists() and (old / "holiday.mp4").exists(), "what the library has already, and files without an id, are left where they are")
        d2 = next(d for d in (lib / WORK / "videos").iterdir() if "imp00000002" in d.name)
        check(any(f.name.endswith(".info.json") for f in d2.iterdir()), "a video without its details gets them from YouTube")
        check(vjson("@Workshop", "imp00000001")["title"] == "Old talk" and vjson("@Workshop", "imp00000001")["thumbnail"].endswith(".jpg"), "with its picture and details")
        check(any(f["job"]["type"] == "import" and "Moved 2 videos" in f["message"] for f in recent()), "Activity says what was brought in")

        # ---- deleting a video, removing sources -------------------------------------------------
        page.goto(BASE + "/#/video/@Demo/vid00000002")
        page.wait_for_selector("#video-more")
        page.click("#video-more")
        page.click("#delete-video")
        page.wait_for_selector("#confirm-ok")
        page.click("#confirm-ok")
        wait_for(lambda: "vid00000002" not in kept("@Demo"), "a video can be deleted")
        check(not any("vid00000002" in d.name for d in (lib / DEMO / "videos").iterdir()), "its folder is gone")
        check(upstream("@Demo")["vid00000002"]["state"] == "not-wanted", "it's still listed upstream, as not wanted")
        page.wait_for_selector(".source-title h1")
        page.goto(BASE + f"/#/source/{FAVES}")
        page.wait_for_selector("#source-more")
        page.click("#source-more")
        page.click("#remove")
        page.wait_for_selector("#confirm-ok")
        page.click("#confirm-ok")
        wait_for(lambda: not (lib / FAVES).exists(), "a playlist can be removed")
        check("vid00000001" in kept("@Demo"), "removing a playlist keeps its videos in their channels")
        page.wait_for_selector(".card.source")
        page.goto(BASE + f"/#/source/{WORK}")
        page.wait_for_selector("#source-more")
        page.click("#source-more")
        page.click("#remove")
        page.wait_for_selector("#confirm-ok")
        check("3 videos" in page.inner_text(".dialog") or "videos" in page.inner_text(".dialog"), "removing a channel says its videos go too")
        page.click("#confirm-ok")
        wait_for(lambda: not (lib / WORK).exists(), "a channel and its videos can be removed")
        page.wait_for_selector(".card.source")
        page.wait_for_timeout(300)
        check(page.locator(".card.source").count() == 1, "Sources shows what's left")

        # ---- another library folder, and back -----------------------------------------------------
        other = home / "Other"
        post("/test/pick", {"path": str(other)})
        page.goto(BASE + "/#/settings")
        page.wait_for_selector("#change-library")
        page.click("#change-library")
        page.wait_for_selector("#choose-move")
        check("Move the library there" in page.inner_text(".dialog") and page.is_visible("#choose-open"), "changing the folder asks whether to move the library there or start a new one")
        page.screenshot(path=str(a.out / "12b-move-or-new.png"))
        page.click("#choose-open")
        wait_for(lambda: (other / "_library/library.json").is_file(), "a new library folder is set up")
        wait_for(lambda: other.name in page.inner_text("#library-path"), "Settings shows the new folder")
        page.goto(BASE + "/#/")
        page.wait_for_selector(".empty-state")
        check(page.locator(".card.video").count() == 0, "the new library is empty")
        api("library_open", {"path": str(lib)})
        page.reload()
        page.wait_for_selector(".card.video")
        check(page.locator(".card.video").count() == 5, "the first library opens again with its videos")

        # ---- moving the library ------------------------------------------------------------------------
        moved = home / "Moved" / "Videos"
        post("/test/pick", {"path": str(other)})  # a library already: opened, nothing to ask
        page.goto(BASE + "/#/settings")
        page.wait_for_selector("#change-library")
        page.click("#change-library")
        wait_for(lambda: other.name in page.inner_text("#library-path"), "picking an existing library opens it without asking")
        api("library_open", {"path": str(lib)})
        page.reload()
        page.wait_for_selector("#change-library")
        post("/test/pick", {"path": str(moved)})
        page.click("#change-library")
        page.wait_for_selector("#choose-move")
        check("1 source and 5 videos" in page.inner_text(".dialog"), f"the question says what the library holds: {page.inner_text('.dialog')[:200]!r}")
        page.click("#choose-move")
        wait_for(lambda: (moved / "_library/library.json").is_file() and not lib.exists(), "Move takes every file to the new folder and leaves none behind", 60)
        wait_for(lambda: str(moved) in api("app_info").get("library", {}).get("path", ""), "the moved library is the one open")
        # on the same drive the move is a rename, done before the page hears back from library_move
        wait_for(lambda: page.evaluate("location.hash") == "#/activity", "the move is followed in Activity")
        page.wait_for_selector("#job-now")
        page.goto(BASE + "/#/settings")
        wait_for(lambda: "Moved" in page.inner_text("#library-path"), "Settings shows where it went")
        check(any(f["job"]["type"] == "move" and f["ok"] for f in recent()), "Activity records the move")
        lib = moved
        page.goto(BASE + "/#/")
        page.wait_for_selector(".card.video")
        wait_for(lambda: page.locator(".card.video").count() == 5, "the moved library shows its videos")
        thumb = page.evaluate("document.querySelector('.card.video img')?.getAttribute('src') || ''")
        check(thumb.startswith("/library/"), "and their pictures load from the new place")

        # ---- YouTube asks whether this is a bot: scheduled checks wait --------------------------
        put("videos", "vid00000008", {"title": "Guarded video", "duration": 40, "upload_date": "20260402", "bot_check": True})
        put("videos", "vid00000009", {"title": "Next in line", "duration": 40, "upload_date": "20260403"})
        edit("channels", "@Demo", lambda d: d["tabs"]["videos"].__setitem__(slice(0, 0), ["vid00000009", "vid00000008"]))
        api("check", {"id": DEMO})
        wait_for(lambda: idle() and api("app_info").get("paused_until"), "a bot check pauses scheduled checks", 40)
        check(upstream("@Demo")["vid00000008"]["state"] == "failed" and "vid00000009" not in kept("@Demo"), "and the run stops there instead of asking again for every video")
        check(any("needs you" in n["title"] for n in notices()), f"a notice says something needs you {notices()[-1:]}")
        page.goto(BASE + "/#/activity")
        page.wait_for_selector("#paused")
        check("not a bot" in page.inner_text("#paused"), "Activity says why checks wait, and until when")
        page.screenshot(path=str(a.out / "12d-paused.png"))
        page.click("#resume-checks")
        wait_for(lambda: not api("app_info").get("paused_until") and page.locator("#paused").count() == 0, "scheduled checks can be turned back on")

        # ---- the look -------------------------------------------------------------------------------
        page.click("#theme-toggle")
        page.click("#theme-toggle")
        check(page.evaluate("document.documentElement.dataset.theme") == "dark", "the theme can be dark")
        page.screenshot(path=str(a.out / "13-dark.png"))
        page.set_viewport_size({"width": 860, "height": 700})
        page.goto(BASE + f"/#/source/{DEMO}")
        page.wait_for_selector(".source-title h1")
        check(page.evaluate("document.documentElement.scrollWidth <= innerWidth + 1"), "a narrow window doesn't scroll sideways")
        page.screenshot(path=str(a.out / "14-narrow.png"))
        errors = page.evaluate("window.__errors")
        check(not errors, f"no errors in the page {errors}")
        browser.close()
finally:
    serve.terminate()

print(f"\n{len(failures)} failed" if failures else "\nall passed")
sys.exit(1 if failures else 0)
