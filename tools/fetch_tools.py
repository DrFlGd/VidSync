#!/usr/bin/env python3
"""Put yt-dlp (and, if asked, ffmpeg) into build/tools, where the Tauri build
bundles them beside the app so the installer works without a download.

    python3 tools/fetch_tools.py [--out build/tools] [--ffmpeg] [--os windows|linux]

yt-dlp is the latest release from github.com/yt-dlp/yt-dlp (Unlicense).
--ffmpeg fetches a build of ffmpeg too (Windows: BtbN/FFmpeg-Builds, GPL; Linux:
johnvansickle static builds, GPL) and keeps only the ffmpeg program and its
licence. Without --ffmpeg, the app offers to fetch it from Settings → Tools.
The folder is created even when nothing is fetched, so the build always has it.
VIDSYNC_YTDLP_URL and VIDSYNC_FFMPEG_URL override where they come from.
"""
import argparse
import os
import platform
import shutil
import sys
import tarfile
import tempfile
import urllib.request
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ap = argparse.ArgumentParser()
ap.add_argument("--out", type=Path, default=ROOT / "build/tools")
ap.add_argument("--ffmpeg", action="store_true")
ap.add_argument("--os", default={"Windows": "windows", "Linux": "linux", "Darwin": "macos"}.get(platform.system(), "linux"))
ap.add_argument("--skip-download", action="store_true", help="only make the folder")
a = ap.parse_args()

a.out.mkdir(parents=True, exist_ok=True)
(a.out / "README.txt").write_text(
    "Programs Vid Sync runs: yt-dlp (and ffmpeg when bundled). The app can update\n"
    "them from Settings > Tools; updated copies go to the app's config folder.\n", encoding="utf-8")
if a.skip_download:
    sys.exit(0)

YTDLP = {
    "windows": "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
    "linux": "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_linux",
    "macos": "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos",
}
FFMPEG = {
    "windows": "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip",
    "linux": "https://johnvansickle.com/ffmpeg/releases/ffmpeg-release-amd64-static.tar.xz",
}


def fetch(url, dest):
    print(f"fetching {url}", flush=True)
    req = urllib.request.Request(url, headers={"User-Agent": "vidsync-fetch-tools"})
    with urllib.request.urlopen(req, timeout=120) as r, open(dest, "wb") as f:
        shutil.copyfileobj(r, f)
    print(f"  {dest.name}: {dest.stat().st_size / 1e6:.1f} MB")


exe = ".exe" if a.os == "windows" else ""
url = os.environ.get("VIDSYNC_YTDLP_URL") or YTDLP[a.os]
fetch(url, a.out / f"yt-dlp{exe}")
if a.os != "windows":
    os.chmod(a.out / "yt-dlp", 0o755)

if a.ffmpeg:
    url = os.environ.get("VIDSYNC_FFMPEG_URL") or FFMPEG.get(a.os)
    if not url:
        sys.exit(f"no ffmpeg build known for {a.os}")
    with tempfile.TemporaryDirectory() as td:
        archive = Path(td) / url.rsplit("/", 1)[-1]
        fetch(url, archive)
        wanted = {f"ffmpeg{exe}", "LICENSE", "LICENSE.txt", "GPLv3.txt"}
        if archive.suffix == ".zip":
            with zipfile.ZipFile(archive) as z:
                for m in z.infolist():
                    name = Path(m.filename).name
                    if name in wanted and not m.is_dir():
                        with z.open(m) as src, open(a.out / (name if name.startswith("ffmpeg") else f"ffmpeg-{name}"), "wb") as dst:
                            shutil.copyfileobj(src, dst)
        else:
            with tarfile.open(archive) as t:
                for m in t.getmembers():
                    name = Path(m.name).name
                    if name in wanted and m.isfile():
                        with t.extractfile(m) as src, open(a.out / (name if name.startswith("ffmpeg") else f"ffmpeg-{name}"), "wb") as dst:
                            shutil.copyfileobj(src, dst)
        if a.os != "windows":
            os.chmod(a.out / "ffmpeg", 0o755)
print("tools in", a.out)
