# Vid Sync: plan and decisions

## 1. The ask (2026-11-02)

Jerred: "build a plan and then create an application that uses YTDL to download youtube videos, organize/view them in different ways (creator, channel, playlist, etc), and track and download new channel releases and show an audit of what videos from a source (channel/playlist) are missing. I like the current theme and aesthetic of repo sync and I think we can reuse that here."

"YTDL" is read as [yt-dlp](https://github.com/yt-dlp/yt-dlp), the maintained fork of youtube-dl. "Repo sync" is [Repo Sync](https://github.com/DrFlGd/Repo-Sync), whose stack and look (the Drafting Sheet skin) this app takes over.

## 2. Taken from Repo Sync

| From | Taken |
| --- | --- |
| The stack | A Rust core with one command table (`App::call`), a thin Tauri 2 window, a Preact + htm page with no build step, `platform.js` as the seam, `tests/tauri_shim.js` + `<cli> serve` for the acceptance test in Chromium, `CHANGELOG.md` with **For agents**, numbered releases from CI (`tools/set_version.py`), Windows first. |
| The look | The Drafting Sheet skin as it is (square corners, fine rules, drafting blue, Overpass and Overpass Mono, corner ticks on cards, the status bar as a title block, mono section heads), the menu on the left with counts in brackets, the inspector on the right of a page, light and dark. `styles.css`, `lib/`, `icons.js` and the shell are carried over and extended; the mark is redrawn for video. |
| The model | A library folder as the source of truth (sorted JSON, unknown keys kept, nothing deleted except by the user), jobs one at a time with progress and Stop, a schedule thread, Activity, following with per-source settings, tags, search in the backend. |

## 3. What's new here

The thing kept is a **video**, and it has three homes: the **channel** that published it, any **playlists** it's in, and the **creator** (a group of channels the user makes, for a person or studio with several channels). A channel or a playlist is a **source** that can be followed: it's listed upstream on a schedule, new videos are downloaded, and the difference between what's upstream and what's kept is the **audit**.

### yt-dlp and ffmpeg

The core shells out to `yt-dlp` (JSON out: `-J --flat-playlist` to list a source, `-J` for a video, `--newline --progress-template` for progress) and lets it call `ffmpeg` to merge video and audio. Neither is linked in; both live in the app's config folder under `tools/` and are found in this order: a path set in Settings, `tools/`, the folder the app runs from (the installer puts yt-dlp there), then PATH. Settings shows which was found and its version, with **Update yt-dlp** (from its GitHub releases; the file is swapped only after the new one answers `--version`) and **Get ffmpeg** on Windows (BtbN's build). Without ffmpeg, yt-dlp downloads the best single file (up to 720p) and the app says so. `VIDSYNC_YTDLP` points the core at another program (the tests' stand-in).

### The library folder

```
Vid Sync/                                   (~/Vid Sync until another folder is chosen)
  _library/library.json                     { format: 1, id, name, created, settings }
  _library/creators.json                    the user's groups of channels
  youtube.com/channels/@handle/
    channel.json                            details, follow, upstream listing, kept videos, tags
    avatar.jpg
    videos/2026-10-29 Title of the video [dQw4w9WgXcQ]/
      Title of the video [dQw4w9WgXcQ].mp4  (or .mkv/.webm/.m4a: whatever yt-dlp made)
      Title of the video [dQw4w9WgXcQ].info.json
      Title of the video [dQw4w9WgXcQ].jpg
      Title of the video [dQw4w9WgXcQ].en.vtt   (subtitles, when asked)
    .partial/                               a download in progress (removed before each check)
  youtube.com/playlists/PLxxxx/playlist.json  details, follow, upstream listing, the videos in order
```

- **A video lives in its channel's folder**, whoever asked for it: a channel being followed, a playlist being followed, or a link pasted by hand. A playlist is a list of video ids (in the playlist's order) pointing into channel folders, so a video in three playlists is on disk once. A channel that isn't followed but holds videos (from a playlist, or a pasted link) has `channel.json` with `follow.mode = "none"`; it's listed under Channels like the others and can be followed later.
- A channel's folder is named for its handle (`@handle`, what the user sees in links), a playlist's for its id. The id in the JSON is what counts; the folder is found by scanning, as in Repo Sync.
- `channel.json`: `source` (kind, id, handle, url), `details` (name, description, avatar, subscribers, video count, as last listed), `follow` (mode, which tabs, quality, subtitles, hours between checks), `creator`, `tags`, `notes`, `last_checked`, `last_changed`, `last_error`, `upstream` (every video listed, newest first: id, title, duration, upload date, when it was first seen, `state`: wanted / not-wanted / members-only / live / failed with the error / skipped by the user), `videos` (what's kept: id, folder, file, title, upload date, duration, bytes, width, height, downloaded, formats). Nothing in `videos/` is deleted except by the user.
- **Nothing is replaced.** A download goes into `.partial/` and is moved into place when complete, so Stop or a crash never leaves half a video among the kept ones. A video downloaded again (a better quality asked for) gets a new folder stamped with its quality beside the old one.

### Sources and links

`sources::parse` reads what's pasted: a video (`watch?v=`, `youtu.be/`, `/shorts/`, `/live/`), a channel (`/@handle`, `/channel/UC…`, `/c/…`, `/user/…`, with or without `/videos`, `/shorts`, `/streams`), a playlist (`playlist?list=`, or a video link with `&list=`, which is offered as both). A channel is listed through its tabs: `/videos` always, `/shorts` and `/streams` when the follow says so (default: videos only). The channel's handle and id come from the first listing (yt-dlp's `uploader_id` and `channel_id`).

### Following and the audit

`follow.mode`: **new** (videos published after the source was added: the default, so adding a big channel doesn't fetch ten years of it), **all** (everything upstream), **none** (only list: nothing downloaded, the audit still works). Other follow settings: tabs, quality (best / up to 1080p / up to 720p / audio only; H.264 in mp4 is preferred so the app's own player can show it), subtitles (none / the channel's language / all), check every N hours (or the library's schedule, or by hand).

A check: list the source upstream (flat, fast, no video pages), merge into `upstream` (new ids are marked by the follow mode: wanted or not-wanted; ids gone from upstream are kept and marked "no longer listed"), then download every `wanted` video that isn't kept, oldest first, one at a time, each its own stage in Activity. A video yt-dlp can't fetch is marked `failed` with the reason (members only, private, age restricted without cookies, removed) and isn't tried again until the user asks or the next day. For a playlist, each video is downloaded into its own channel's folder, and the channel's `channel.json` gains the video too.

**The audit** is `upstream` against `videos`, for one source or for the whole library: listed, kept, missing (wanted and not kept), not wanted (outside the follow mode), failed, skipped by the user, no longer listed upstream. The Audit page lists every source with these counts and **Download missing**; a source's Audit tab lists the videos themselves with what's known of each, a box to download one, and **Skip** to leave one out on purpose (it stops counting as missing). The schedule thread queues the checks that are due (every 30 s it looks; a check is due `last_checked` + hours), as Repo Sync does. `vidsync-cli check-all` does the same from Task Scheduler while the app is closed.

### The interface

- **Videos** (the library's home): every kept video as a card (thumbnail with the duration in the corner, title, channel, published, size, quality) or a row. Search (titles, descriptions, channels, tags; in the backend), sort (published, downloaded, title, channel, duration, size), and the menu narrows by **Channel**, **Playlist**, **Creator**, **Tag** or state (failed downloads). A video being downloaded shows as a dashed card.
- **Sources**: the followed channels and playlists as cards (avatar, name, kept / upstream counts, next check, missing in red when any) with Check now and Download missing. Add is one dialog for every kind of link.
- **A channel's page**: banner with the avatar, name, handle, subscribers; Check now, Download missing, Folder, YouTube, ⋯ (copy link, stop following, remove). Tabs: Videos (kept, newest first), Audit (upstream vs kept), Playlists (this channel's videos appear in). Inspector: details, Follow, Creator, Your tags, notes. A playlist's page is the same without the Playlists tab, its Videos in the playlist's order.
- **A video's page**: the app's own player (the file through `library://`, with range requests so seeking works), the title, channel, published, views, duration, description (links open in the browser), the file's facts (container, resolution, size, formats), what else it's in; Open with the system player, Folder, YouTube, Download again at another quality, Delete. Tags.
- **Audit**: the whole library's table.
- **Activity**: the download in hand (title, channel, stage, speed and time left from yt-dlp's progress, a meter) with Stop, the queue, what each check found.
- **Settings**: library folder; schedule; what new sources follow (mode, tabs, quality, subtitles); yt-dlp and ffmpeg (where found, versions, Update, Get ffmpeg); cookies for members-only and age-restricted videos (from a browser, or a cookies file; kept on this computer); theme.

### Tests

`tests/ytdlp_mock.py` stands in for yt-dlp: it answers `--version`, lists channels and playlists from a fixtures folder (`<fixtures>/channels/<handle>.json`, `<fixtures>/playlists/<id>.json`), answers `-J` for a video, and "downloads" by writing a small file to the asked output path while printing progress lines in yt-dlp's shapes. `tests/desktop_page.py` runs the page against `vidsync-cli serve` with the stand-in: adding a channel and a playlist, cards and rows, the channel page, the audit after a new upstream video, Check now and the schedule, a video that fails, Skip, creators, tags, search, the player's range requests, deleting a video, removing a source, changing the library, dark theme, a narrow window.

## 4. Decisions

- **yt-dlp as a program, not a library.** It changes weekly as YouTube does; a program the app can update by itself keeps working without a new release of Vid Sync.
- **Videos by channel, playlists as lists.** One copy of each video, and a channel's folder makes sense without the app.
- **"New" is the default follow mode.** Downloading a whole channel is a choice, made in Add or Follow.
- **Audit states are kept, not computed on the fly**, so a video that failed says why, one the user skipped stays skipped, and one that left YouTube is still remembered.
- **mp4 with H.264 preferred** (yt-dlp's format sorting) so the app's own player, WebView2 on Windows, plays what it downloads; what can't be played in the app opens in the system player.
- **Windows first**, Linux kept portable (as the other apps): the acceptance test runs on both.

## 5. Phases

1. **0.1**: everything above. Checked in a cloud session with the stand-in for yt-dlp (YouTube itself can't be reached from there), the Rust tests and the acceptance test.
2. Later, if asked: downloading comments or chapters, more sites yt-dlp knows (the folder layout leaves room: `<host>/channels/…`), a tray icon so checks run with the window closed, the app updating itself.
