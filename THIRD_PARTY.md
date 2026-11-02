# Third-party code

| What | Used for | License |
| --- | --- | --- |
| [yt-dlp](https://github.com/yt-dlp/yt-dlp) | Listing channels and playlists and downloading videos. Run as a separate program, never linked; fetched or updated by the app on request (Settings → Tools) | Unlicense |
| [FFmpeg](https://ffmpeg.org) | Joining video and audio streams and converting thumbnails. Run as a separate program; the Windows build comes from [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds) (GPL build) | LGPL 2.1+ / GPL 2+ |
| [Preact](https://github.com/preactjs/preact) 10.27.2 @ `0dbe636` | Interface components (`web/vendor/preact`; copied by `tools/vendor_preact.py`, only import paths changed) | MIT |
| [htm](https://github.com/developit/htm) @ `d62dcfd` | JSX-like templates without a build step (`web/vendor/htm`) | Apache-2.0 |
| [ureq](https://crates.io/crates/ureq) (with rustls and webpki-roots) | Fetching yt-dlp and ffmpeg releases, channel avatars | MIT or Apache-2.0 (webpki-roots: CDLA-Permissive-2.0) |
| [zip](https://crates.io/crates/zip) | Unpacking the ffmpeg release archive | MIT or Apache-2.0 |
| [serde](https://crates.io/crates/serde), [serde_json](https://crates.io/crates/serde_json), [anyhow](https://crates.io/crates/anyhow) | Reading and writing the library's JSON | MIT or Apache-2.0 |
| [Overpass](https://github.com/google/fonts/tree/main/ofl/overpass) and [Overpass Mono](https://github.com/google/fonts/tree/main/ofl/overpassmono) | The interface's fonts (bundled into the app by `tools/build_desktop.py --fetch-fonts`) | SIL OFL 1.1 |
| [Tauri](https://tauri.app) 2 and its dialog, opener, notification, autostart and single-instance plugins | The desktop app's window | MIT or Apache-2.0 |

The app's shell, interface pieces, look (Drafting Sheet) and tests follow [Repo Sync](https://github.com/DrFlGd/Repo-Sync), [Model Library](https://github.com/DrFlGd/Model-Library) and [SCAD Workshop](https://github.com/DrFlGd/SCAD-Workshop) (same owner).
