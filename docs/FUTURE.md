# Vid Sync: future features

Written 2026-11-02, after 0.1 (pull request #1), at the owner's request. Nothing here is started; each piece gets a section in `PLAN.md` before it's built. Sizes: S an afternoon, M a day or two, L a week.

The groups are by when they pay off. **Soon** is about the download loop being trustworthy without anyone watching. **Next** is about living in the library once it holds a few hundred videos. **Later** is the bigger pieces that change what the app is.

## Soon: an app you can leave running

| Feature | What you get | What it takes | Size |
| --- | --- | --- | --- |
| Tray icon, start with Windows | Close the window and checks keep running; a tray menu with Check all, the next check, Open. | Tauri tray and autostart plugins; the window hides instead of closing; `vidsync-cli` stays for cron. | M |
| Notifications | A system notice when a check finds new videos or a download fails for a reason that needs you (cookies). | Tauri notification plugin, hooked where Activity records a finished job; a setting to turn it off. | S |
| Resume broken downloads | A download cut off by sleep or a dropped connection continues where it stopped. | Keep the `.partial/<id>` folder between tries and pass yt-dlp's `--continue`; the stand-in gets a half-written file case. | S |
| Two or three downloads at once | A big backlog (Every video on a large channel) finishes in a fraction of the time. | The worker runs N video jobs side by side (listing stays one at a time); Activity shows each; a setting for N. | M |
| Politeness and limits | Fewer "sign in to confirm you're not a bot" blocks on long runs. | `--sleep-requests` and `--limit-rate` from settings; back off for an hour after a bot-check error. | S |
| Cookies without typing paths | Pick the browser from a list and press Test; the app says whether YouTube now answers. | Settings UI over the existing `--cookies-from-browser`; Test runs `yt-dlp -J` on a known age-restricted id. | S |
| Import an existing yt-dlp folder | Videos already on disk count as kept, so the audit starts right and nothing downloads twice. | Scan a folder for `[<id>].*` names and `.info.json`, move or link them into the channel layout; a dialog with a preview. | M |
| Re-download in a better quality | A video fetched at 720p before ffmpeg was there can be fetched again at full quality. | A `video` job with `replace: true`; the old folder goes only after the new one is complete. | S |

## Next: living in the library

| Feature | What you get | What it takes | Size |
| --- | --- | --- | --- |
| Watched and resume | A video remembers where you stopped; an Unwatched filter and a Continue watching row. | `watched` and `position` per video in `channel.json`, saved from the player every few seconds; sidebar filter; a card badge. | M |
| A page per creator | All of a creator's channels, their totals, one combined feed of newest videos. | Route `#/creator/<name>`; the data is already in the `creators` and `videos` commands. | S |
| Saved views | "Unwatched talks from Demo Studio" as a one-click entry in the menu. | A saved filter (creator, channel, tag, unwatched, search) in prefs; the sidebar lists them. | M |
| Full-text search in subtitles and descriptions | Find the video where someone said a phrase, and jump to that second. | Index the `.vtt` text and descriptions in the backend (rebuilt on scan); results show "found at 12:34" and seek the player. | M |
| SponsorBlock skips in the player | Sponsor segments are skipped or marked on the timeline. | `--sponsorblock-mark all` at download time puts segments in the info JSON; the player reads them. | M |
| Multi-select | Tag, delete, skip or re-download many videos at once. | Shift/Ctrl selection on cards and rows; a bar with the actions; the commands already exist one at a time. | S |
| Comments on the video page | Top comments saved with the video, readable offline. | `--write-comments` with a cap (such as 200); a Comments section on the video page. | S |
| Keep only the newest | Per channel: keep the newest N or delete watched videos after X days, with the audit still listing them. | The `prune` pattern from Repo Sync; Activity says what was deleted. | M |

## Later: bigger pieces

| Feature | What you get | What it takes | Size |
| --- | --- | --- | --- |
| Other sites yt-dlp knows | Twitch VODs, Vimeo, PeerTube and podcasts in the same library and audit. | The folder layout has room (`<host>/channels/…`); `sources.rs` needs a reader per site; the stand-in grows a host per site. | L |
| Watch from another device | Open the library from a phone or TV on the same network, with the player. | `vidsync-cli serve` is most of it; a read-only mode, a token, a lighter page for small screens. | L |
| Plex and Jellyfin friendly | The library shows up in a media server with titles, dates and thumbnails. | An option for their naming scheme and `.nfo` files beside each video. | M |
| Audio library mode | Audio-only sources shown as a podcast-like list with play-through and mp3 conversion. | `quality: audio` exists; continuous play, a conversion step through ffmpeg, a sidebar group. | M |
| Clips from chapters | Cut a chapter or a marked range to its own file to share. | ffmpeg stream copy between two timestamps; a Clip button on the chapter list. | S |
| The app updating itself | A new release is offered in About and installs on exit, as Game Repo does. | The Tauri updater plugin and a signing key in CI. | M |
| Storage view | Where the space goes: by creator, channel, quality and year, with a one-click trim. | `bytes` are already per video; a chart page and a link into the prune settings. | S |

## A suggested order

First, not on the list: install 0.1 on a Windows machine and run it against real channels for a few days, because the cloud session could not reach YouTube. What breaks there sets the real order. Then:

1. **0.2, trust the loop**: tray icon and start with Windows, notifications, resumed downloads, politeness limits, cookies from a list with Test. All but the tray and the notice can be checked with the yt-dlp stand-in; those two need a hand test on Windows.
2. **0.3, live in the library**: watched and resume, a page per creator, multi-select, keep only the newest.
3. **0.4, find things**: subtitle and description search, saved views, comments.
4. **0.5, more at once** (after 0.2 has shown the single download is solid): downloads side by side, import of an existing folder, re-download in a better quality.
5. **Then decide** between the bigger pieces by what's actually wanted: other sites, watching from a phone, or the media-server layout.

Two things to settle before 0.2:

- **A licence for the repository.** None is chosen yet. Bundling ffmpeg in the installer would make it a GPL question; bundling only yt-dlp (Unlicense), as CI does now, does not.
- **Where per-video state lives.** Watched position and tags go into `channel.json` today, which keeps the folder self-describing but means rewriting one big file often for a large channel. If 0.3's resume position makes that noticeable, a sidecar per video (`<id>.vidsync.json` in its folder) is the alternative; decide once.
