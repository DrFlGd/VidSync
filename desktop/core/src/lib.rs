//! Vid Sync's core: the library folder (channels, playlists, videos), reading
//! YouTube links, running yt-dlp, checks and the audit, and the app's commands.
//! No window here: desktop/src-tauri puts one around it, and `vidsync-cli serve`
//! puts a local web server around it for the tests.

pub mod api;
pub mod capture;
pub mod config;
pub mod http;
pub mod library;
pub mod sources;
pub mod textsearch;
pub mod tools;
pub mod util;
pub mod ytdlp;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
