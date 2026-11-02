# Changelog

What was built, when and why. It's written for whoever picks the work up next, people or agents: start with **For agents** below, then read the newest release. Newest first.

**Keeping it up to date:** add a line under **Unreleased** with each change you push (what changed for the user, and anything the next person needs to know). When CI publishes a release, move those lines under a heading for that version. The release notes are the **Unreleased** section, so write it for the user.

---

## For agents

### What this is

A desktop app (Windows first, Linux kept portable; Tauri 2) that keeps a library of YouTube videos downloaded with yt-dlp: add a channel, playlist or video by its link, its videos are organised by channel, playlist and creator (the user's own grouping of channels), followed sources are checked for new videos on a schedule, and an audit shows what each source lists upstream against what the library keeps. The owner ("DrFlGd", Jerred) asked for it on 2026-11-02 with Repo Sync's look and stack as the cue; the plan and the decisions are in `docs/PLAN.md`.

### Where things are

| Path | What |
| --- | --- |
| `web/` | The front end (no build step). `app.js`: start-up and routing (`#/` with `?creator=`/`?channel=`/`?playlist=`/`?tag=` filters, `#/sources`, `#/source/<id>?tab=videos|audit|playlists`, `#/video/<channel key>/<video id>`, `#/audit`, `#/activity`, `#/settings`, `#/about`). `web/ui/`: `shell.js` (the menu with its creator, channel, playlist and tag filters, the status bar with the download in hand, keys, pasting a link anywhere opens Add), `state.js` (shared state, prefs, theme, polling the jobs: every second while there's work, every 15 s otherwise; `DEFAULT_FOLLOW`, the option lists), `videos.js` (cards and list, search, sort, the pending card), `sources.js` (channels grouped by creator, playlists, channels only holding videos), `source.js` (a source's page: videos, the Audit tab with Download and Skip per video, playlists; the inspector with details, Follow, Creator, Your tags, Notes), `video.js` (the player, description with timestamps, chapters, details, tags), `audit.js`, `activity.js`, `settings.js` (library, schedule, running in the background, defaults for new sources, Tools: yt-dlp and ffmpeg found/fetched/updated, being gentle with YouTube, cookies with Test), `dialogs.js` (Add with link checks and follow options, confirm), `follow.js` (the follow fields shared by Add and the inspector), `actions.js`, `about.js`, `icons.js`. `lib/format.js`: sizes, dates, durations. `platform.js` / `platform-desktop.js`: the seam between the page and the backend. |
| `desktop/core/` | Rust, no GUI. `api.rs` (the command table `App::call`, the job queue and its worker thread, the schedule thread, the JSON the page gets), `library.rs` (the library folder, `channel.json`/`playlist.json` shapes, `Follow`, `Listed` upstream entries and their `State`, `audit()`, scanning), `sources.rs` (reading YouTube links: channel handles/ids/paths, playlists, videos; their URLs), `ytdlp.rs` (running yt-dlp: listing with `-J --flat-playlist`, a video's info, downloads with a progress template and Stop, `explain()` turning its errors into short reasons, sorting the files it wrote), `capture.rs` (listing a channel through its tabs or a playlist, `merge()` of the fresh list into upstream by follow mode, `wanted()`, downloading into `_library/.partial/<id>` and `place()` into the channel's dated folder, avatars, deleting), `tools.rs` (finding yt-dlp and ffmpeg: Settings path, `VIDSYNC_YTDLP`/`VIDSYNC_FFMPEG`, `<config>/tools/`, the app's folder, PATH; fetching or updating them from GitHub), `http.rs` (ureq: JSON, downloads with progress, `file://` for tests), `config.rs` (this computer: the open library, tool paths, cookies, politeness, prefs), `textsearch.rs` (searching subtitles and descriptions), `util.rs`. `src/bin/vidsync-cli.rs`: `list`, `audit`, `add`, `check-all`, `serve` (the backend over HTTP for the tests, with `/library/<path>` serving byte ranges). |
| `desktop/src-tauri/` | The Tauri app: a thin layer over `api.rs`, plus the tray, start at sign-in, notices, one copy at a time, the folder and file dialogs, the file manager, web links in the browser and the `library://` protocol (media with Range requests, thumbnails, avatars). Version in `tauri.conf.json`. `bundle.resources` copies `build/tools/` (yt-dlp from `tools/fetch_tools.py`) beside the app. Icons from `icons/icon.svg` (`npx @tauri-apps/cli@2 icon icons/icon.svg -o icons`, then delete the mobile and Square ones). |
| `tools/` | `build_desktop.py` (web/ to `build/desktop/ui`, fonts with `--fetch-fonts`, `THIRD_PARTY.md` and `build-info.json` for About), `fetch_tools.py` (yt-dlp, and ffmpeg with `--ffmpeg`, into `build/tools`), `set_version.py`, `vendor_preact.py`. |
| `tests/` | `desktop_page.py` (acceptance test: the page + `vidsync-cli serve` + `ytdlp_mock.py`, in Chromium; about 200 checks), `ytdlp_mock.py` (a stand-in for yt-dlp answering `--version`, flat listings of channel tabs and playlists, a video's JSON and downloads with progress lines, from JSON fixtures), `fixtures/` (two channels, a playlist, videos; one is members-only so a download fails), `tauri_shim.js`. |
| `.github/workflows/desktop.yml` | Windows app, tests and release; Linux only from Run workflow. |

### Conventions

- **Releases.** The repository keeps `0.<phase>.0` in `desktop/src-tauri/tauri.conf.json` and `desktop/Cargo.toml`. CI picks the next free `0.<phase>.<n>` from the release tags and publishes `v<version>` from `main` only when the Rust tests, the acceptance test and the Windows build pass.
- **Windows first**, as in the owner's other apps; keep the code portable.
- **Plan first** for a big feature: the design goes into `docs/PLAN.md` before the code.
- **Text:** plain words in the interface, no jargon; British spellings (licence, colour), as in the other apps.
- **The library folder is the source of truth** and must make sense without the app. No database in it; unknown keys in the JSON are kept (`#[serde(flatten)] extra`); JSON is written with sorted keys (`util::write_json`). Nothing downloaded is deleted except by the user (Delete the video, Remove the channel, or a channel's own "keep only the newest" or "delete watched after", which the user set).
- **yt-dlp is a program, never linked.** Everything it's asked goes through `ytdlp.rs`; anything new asked of it needs the stand-in (`tests/ytdlp_mock.py`) to answer it too.
- **Videos live in their channel's folder**, whatever brought them in (a playlist, a pasted link). A channel created that way has `followed: false` until the user follows it. Playlists are ordered lists of ids.
- **Tests grow with each feature:** add checks to `tests/desktop_page.py` for everything the user can see.

### Working in a cloud session

- crates.io, PyPI and npm were reachable on 2026-11-02 (`cargo test -p vidsync-core` works; `apt-get install libwebkit2gtk-4.1-dev libgtk-3-dev` let `cargo check -p vid-sync` build the Tauri app too). youtube.com and api.github.com were refused by the session's proxy, so the real yt-dlp can't be exercised there: use the stand-in.
- **Acceptance test locally:** `python3 tools/build_desktop.py --out build/desktop`, then `python3 tests/desktop_page.py --cli desktop/target/debug/vidsync-cli --ui build/desktop/ui --home /tmp/vs-home --out /tmp/vs-shots --chromium /opt/pw-browsers/chromium` (`pip install playwright`; the pre-installed Chromium works; ffmpeg makes the player checks real).
- **By hand:** `VIDSYNC_YTDLP=tests/ytdlp_mock.py VIDSYNC_MOCK_FIXTURES=tests/fixtures vidsync-cli serve --ui build/desktop/ui --home /tmp/vs-home --library /tmp/vs-lib --tick 1`, then open the page in a browser with `tests/tauri_shim.js` injected (the test does this). `VIDSYNC_CONFIG` gives `vidsync-cli` another config folder; `VIDSYNC_MOCK_DELAY` and `VIDSYNC_MOCK_LOG` slow and log the stand-in.

### Known limitations now

- Checks run while the app runs, in its window or the tray (or from `vidsync-cli check-all`); there's no system service.
- One download at a time.
- The app doesn't sign in to YouTube itself; age-restricted and members-only videos need a browser's cookies or a cookies file (Settings → Cookies → Test).
- A channel's listing is one flat `-J` per tab, so very large channels take a while on the first check; there's no "stop listing after the newest N" yet.
- The player uses the system WebView's codecs: an AV1 or VP9 file that WebView2 can't play falls back to "Open in player".
- Deleted or private videos upstream are marked gone but the kept file stays; nothing is re-fetched automatically when YouTube changes a title.
- Installers aren't code-signed, and the app doesn't update itself.

### Next (ideas, not yet chosen by the owner)

See `docs/FUTURE.md` (written 2026-11-02, sized and ordered): a tray icon so checks run with the window closed; SponsorBlock and chapter-based trimming; watching progress and a "watched" mark; a queue with more than one download; notifications; importing an existing yt-dlp folder; per-creator pages; and an export of the audit.

---

## Unreleased

After trying 0.1.0, Jerred asked for these:

- **Channel cards on Sources show properly again.** A layout rule meant for a source's page also caught the cards, squeezing each channel's name into a column one letter wide.
- **Moving the library.** Choosing another folder in Settings now asks whether to move the library there (every file, with progress in Activity; renamed in place when it's on the same drive) or start a new, empty library there and leave this one where it is. Picking a folder that's already a library just opens it.
- **Each video's folder describes itself.** A video's record (title, dates, size, quality, your tags) is now in `video.json` beside its files, not in the channel's `channel.json`. A library from 0.1 is converted the first time it's opened; nothing is lost and its videos stay where they are.
- **Licensed under MIT.**

Then the first part of the roadmap (0.2, "trust the loop"):

- **Vid Sync keeps checking with the window closed.** Closing the window leaves it in the tray (a notice says so the first time); the tray menu shows what's downloading or when the next check is, with Open, Check all now and Quit. Settings can make closing quit instead. Starting Vid Sync again brings back the window of the copy that's running.
- **Start when you sign in.** A switch in Settings starts Vid Sync in the tray, without a window, when you sign in to Windows.
- **Notices.** A system notice when a scheduled check brings new videos, or when something needs you (cookies, a sign-in). It can be turned off in Settings.
- **Broken downloads carry on.** A download cut off by sleep, a dropped connection or Stop keeps what it got, and the next try carries on from there instead of starting again (for a week; a different quality starts afresh).
- **Gentler on YouTube.** A pause before each download (3 seconds unless changed), a short pause between page requests and an optional speed limit, in Settings. When YouTube asks to confirm you're not a bot, the run stops there and scheduled checks wait an hour; Activity says so, with a button to carry on anyway.
- **Cookies from a list, with a Test.** Pick the browser from a list (or a cookies file) and press Test: Vid Sync asks YouTube for an age-restricted video with those cookies and says whether it answered, or why not (for example, that Chrome and Edge lock their cookies on Windows and Firefox works).
- **Seeking in long videos is lighter.** The player's requests read only the part of the file asked for, not the whole video each time.

And the second (0.3, "live in the library"):

- **Watched, and carrying on where you stopped.** The player remembers where you stopped and carries on from there next time. Near the end, or when you press Mark watched, a video counts as watched. Cards show how far you got or a Watched mark, Videos opens with a Continue watching row, and the menu has Not watched yet. All of it is kept in each video's `video.json`.
- **A page for each creator.** It shows all of a creator's channels with their totals and one feed of their newest videos, with a button to check every channel at once. Creator headings on Sources, and the Creator field on a video or channel, lead to it.
- **Choose several videos.** Ctrl-click or Shift-click cards (or tick their box), then mark them watched, add or remove a tag, or delete them, all at once. Esc lets go.
- **Keep only the newest.** Each channel can keep only its newest 5, 10, 25, 50 or 100 videos, and can delete watched videos a day, a week or a month after watching. Old videos aren't downloaded only to be deleted. After each check, Activity lists what was deleted. The audit still lists those videos, so they aren't fetched again, and a video you've tagged is always kept.

And the third (0.4, "find things"):

- **Search inside videos.** From three letters on, a search also looks in the videos' subtitles and descriptions. Each place a phrase is said comes with the second it's said, even when it runs over two lines of subtitles, and clicking it plays the video from there. A match in a description shows the words around it.
- **Saved views.** Any filter and search (for example, one channel plus "day") can be saved to the menu with Save view, so it's one click away later. Hover a view and press × to forget it.
- **Comments.** A source can save the top 200 comments with each video (Follow → Save the top comments). The video's page shows them, with the channel's own replies marked.

For agents: library format 2. `Channel.videos` is filled from `videos/*/video.json` on scan and never written to `channel.json` (`#[serde(skip_serializing)]`; read once from a format 1 file to migrate). Save a video's record with `Library::put_video`/`save_video`; `Video.folder` is where the file was found, not stored. New commands `library_inspect`, `library_move` and the `move` job (`util::move_tree`); the page's `chooseDialog` asks between several actions.

For agents, 0.2: `config.rs` gains `Polite` (`sleep_requests`, `sleep_downloads`, `limit_rate` → `--sleep-requests`, `--sleep-interval`, `--limit-rate`), `quit_on_close`, `notices_off`. Downloads use `--continue` (no more `--no-part`); `capture::download` keeps `.partial/<id>` when a `.part` file is in it, with `try.json` recording the quality asked for (a different one starts afresh), and `clean_partial` drops what's older than a week when a library opens. `ytdlp::BOT_CHECK` from `explain()` sets `App.paused_until` (an hour; in memory), which `queue_due` and `next_check` respect; `checks_resume` clears it. `App::notice_for` decides the notices and `set_notifier` takes the shell's (`vidsync-cli serve` keeps them for `GET /test/notices`). `cookies_test` runs on its own thread (not the job queue) and `cookies_test_result` answers. `App::status_line` is the tray's line. `util::read_file_part` serves ranges from disk, open-ended ones capped at 8 MB. The Tauri app adds the tray (`tray-icon` feature), and the autostart (`--hidden`), notification and single-instance plugins; `autostart_get`/`autostart_set` are its commands (stubbed in `serve`). The stand-in knows `needs_cookies`, `bot_check` and `break_once` in a video's fixture. The tray and real notices still need a hand test on Windows.

For agents, 0.3: `Video` gains `watched`, `watched_at`, `position` and `played` (`played_to`, `set_watched`); the player posts `video_progress` every 10 s, on pause, at the end and on leaving (watched within 5 % or 30 s of the end). Batch commands `videos_watched`, `videos_tag`, `videos_delete` take `items: [{channel, video}]`. `Follow` gains `keep_newest` and `drop_watched_days`; `capture::newest_allowed` stops older wanted videos from downloading (marked not-wanted with the reason), `capture::to_prune`/`prune` run after every channel's downloads and fill `Outcome.deleted`. The creator page (`web/ui/creator.js`, `#/creator/<name>`) is built from the `sources` and `videos` the page already has. Choosing lives in `ui.selected` (`"channel/id"` keys), with `SelectionBar` on the Videos page; cards get the tick box only with `pick`.

For agents, 0.4: `textsearch.rs` parses WebVTT/SRT (rolling YouTube captions de-duplicated), normalises text (lower case, punctuation as spaces) and finds a phrase within a line or across two; `App.texts` caches each video's `Doc` keyed by folder, download time and subtitle files, cleared when a library opens; `search_text` returns `results` with `hits` ({t, text}), `hit_count` and `in_description`. The page links hits as `#/video/<ch>/<id>?t=<s>` (`VideoPage` prop `t`). `Follow.comments` adds `--write-comments --extractor-args youtube:max_comments=200,all,0;comment_sort=top`; the `video` command returns `comments` from the `.info.json`. Saved views are `views` in prefs (this computer), with `promptDialog` for the name. The stand-in writes `cues` and `comments` from a video's fixture.

## 0.1.0 (2026-11-02)

First version of Vid Sync:

- **Add channels, playlists and videos by their links**, one or many at once (paste a link anywhere in the window, or press N). Each link is read as you type, and anything already in the library is pointed out. A video link can also add the playlist it came from.
- **Downloads with yt-dlp** (found on the computer, bundled with the installer, or fetched and updated from Settings → Tools), and ffmpeg beside it when wanted. Quality (best, up to 1080p, up to 720p, audio only), subtitles and the tabs to follow (videos, shorts, live streams) per source, with defaults for new channels and playlists.
- **Followed on a schedule**: every day by default (hourly to weekly, or by hand; per source too). "New videos from now on", "Every video" or "Only list them" per channel; playlists take every video and keep their order. Activity shows the download in hand with its progress and Stop, what's waiting, and what each check found.
- **The audit**: for each source, what YouTube lists against what's kept (kept, missing, failed with yt-dlp's reason, not wanted, skipped, gone), with Download and Skip per video, and an Audit page totalling every source.
- **Organised by channel, playlist, creator and tag**, searched by title, channel and tags; cards or a list; a page for each video (played in the app with seeking, description with clickable timestamps, chapters, details, tags) and for each channel or playlist (videos, audit, follow settings, creator, tags, notes).
- **A library folder that makes sense without the app**: `youtube.com/channels/<@handle>/videos/<date> <title> [<id>]/` with the media, info JSON, thumbnail and subtitles; `channel.json` and `playlist.json` beside them.

For agents: the code map above is this version. The command table is in `api.rs` (`App::call`); the jobs are `add`, `check`, `missing`, `video` and `tool`; `vidsync-cli serve` is the backend the acceptance test drives.
