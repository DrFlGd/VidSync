//! The app's commands: one table, [`App::call`], used by the desktop app
//! (desktop/src-tauri) and by `vidsync-cli serve` (the tests). Checks and
//! downloads run as jobs, one at a time, on a worker thread; a second thread
//! queues the checks that are due (docs/PLAN.md, "Following and the audit").

use crate::capture::{self, plural, Outcome};
use crate::config::{AppConfig, Cookies, Polite, Prefs};
use crate::http::{Client, Progress};
use crate::library::{self, audit, source_id, split_id, Audit, Channel, Follow, Kind, Library, Listed, Mode, Playlist, PlaylistRef, Settings, State, Video, VIDEOS_DIR};
use crate::sources::{self, Link};
use crate::tools;
use crate::util::{now_iso, now_unix, percent_encode};
use crate::ytdlp::{set_stage, Stage, YtDlp, BOT_CHECK, COOKIE_TEST_VIDEO};
use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Condvar, Mutex};

pub struct AppPaths {
    pub config_dir: PathBuf,
    /// The library folder's files as the page reaches them ("library://localhost/").
    pub library_url: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum JobKind {
    /// Add a source (or one video) from a link, list it and download what's wanted.
    Add { link: Link, follow: Follow },
    /// List a source again and download what's new.
    Check { id: String },
    /// List a source again and download everything missing, failed ones too.
    Missing { id: String },
    /// Download one listed video of a source (from the audit).
    Video { id: String, video: String },
    /// Fetch or update yt-dlp or ffmpeg.
    Tool { name: String },
    /// Move the whole library folder to another place, then open it there.
    Move { to: String },
}

impl JobKind {
    fn same(&self, other: &JobKind) -> bool {
        match (self, other) {
            (JobKind::Video { video: a, .. }, JobKind::Video { video: b, .. }) => a == b,
            (JobKind::Video { .. }, _) | (_, JobKind::Video { .. }) => false,
            (JobKind::Tool { name: a }, JobKind::Tool { name: b }) => a == b,
            _ => true,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub id: u64,
    #[serde(flatten)]
    pub kind: JobKind,
    /// The source's id (or "video:<id>", "tool:<name>").
    pub target: String,
    /// What to call it on the page.
    pub label: String,
    /// "you" or "schedule".
    pub by: &'static str,
    pub queued: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Finished {
    pub job: Job,
    pub finished: String,
    pub ok: bool,
    pub message: String,
    pub outcome: Option<Outcome>,
}

struct Running {
    job: Job,
    started: String,
    progress: Arc<Progress>,
    stage: Arc<Stage>,
    /// "title" of the video in hand and its place in the run ("3 of 7").
    item: Arc<Mutex<(String, String)>>,
}

#[derive(Default)]
struct Jobs {
    next: u64,
    queue: VecDeque<Job>,
    current: Option<Running>,
    /// Newest first, at most 50.
    recent: VecDeque<Finished>,
}

pub struct App {
    paths: AppPaths,
    config: Mutex<AppConfig>,
    prefs: Prefs,
    lib: Mutex<Option<Library>>,
    lib_error: Mutex<Option<String>>,
    jobs: Mutex<Jobs>,
    wake: Condvar,
    client: Arc<Client>,
    /// yt-dlp and ffmpeg as last found (looked for again when Settings change).
    tools: Mutex<(tools::Tool, tools::Tool)>,
    /// Scheduled checks wait until then (unix seconds) after YouTube asked for a sign-in.
    paused_until: Mutex<i64>,
    /// Shows a system notice (title, text); set by the desktop app.
    notifier: Mutex<Option<Notifier>>,
    /// The last cookies test: {"state": "testing"|"ok"|"failed", "message", "at"}.
    cookie_test: Arc<Mutex<Value>>,
    /// Each kept video's subtitles and description, read on the first search:
    /// "channel/id" -> (what it was read from, the text).
    texts: Mutex<std::collections::HashMap<String, (String, Arc<crate::textsearch::Doc>)>>,
}

/// Shows a system notice: (title, text).
pub type Notifier = Box<dyn Fn(&str, &str) + Send + Sync>;

/// How long scheduled checks wait after YouTube asks to confirm it isn't a bot.
pub const BOT_PAUSE_SECS: i64 = 3600;

impl App {
    /// Open the library from this computer's config and start the worker and the
    /// schedule. `tick` is how often the schedule looks for checks that are due.
    pub fn new(paths: AppPaths, tick: std::time::Duration) -> Result<Arc<Self>> {
        let cfg_file = paths.config_dir.join("config.json");
        let config = AppConfig::load(&cfg_file);
        let prefs = Prefs::new(paths.config_dir.join("prefs.json"));
        let found = (tools::find_ytdlp(&config, &paths.config_dir), tools::find_ffmpeg(&config, &paths.config_dir));
        let app = Arc::new(Self {
            paths,
            config: Mutex::new(config),
            prefs,
            lib: Mutex::new(None),
            lib_error: Mutex::new(None),
            jobs: Mutex::new(Jobs::default()),
            wake: Condvar::new(),
            client: Arc::new(Client::new()),
            tools: Mutex::new(found),
            paused_until: Mutex::new(0),
            notifier: Mutex::new(None),
            cookie_test: Arc::new(Mutex::new(Value::Null)),
            texts: Mutex::default(),
        });
        let path = app.config.lock().unwrap().library_path();
        app.open_library(path);
        let worker = app.clone();
        std::thread::Builder::new().name("jobs".into()).spawn(move || worker.work())?;
        let sched = app.clone();
        std::thread::Builder::new().name("schedule".into()).spawn(move || loop {
            std::thread::sleep(tick);
            sched.queue_due();
        })?;
        Ok(app)
    }

    fn open_library(&self, path: PathBuf) {
        match Library::open(&path) {
            Ok(l) => {
                capture::clean_partial(&l.partial_dir());
                self.texts.lock().unwrap().clear();
                *self.lib.lock().unwrap() = Some(l);
                *self.lib_error.lock().unwrap() = None;
                let mut c = self.config.lock().unwrap();
                c.set_library(&path);
                let _ = c.save(&self.paths.config_dir.join("config.json"));
            }
            Err(e) => {
                *self.lib.lock().unwrap() = None;
                *self.lib_error.lock().unwrap() = Some(format!("{e:#}"));
            }
        }
    }

    fn with_lib<T>(&self, f: impl FnOnce(&mut Library) -> Result<T>) -> Result<T> {
        let mut g = self.lib.lock().unwrap();
        let lib = g.as_mut().ok_or_else(|| anyhow!(self.lib_error.lock().unwrap().clone().unwrap_or_else(|| "No library is open.".into())))?;
        f(lib)
    }

    fn refind_tools(&self) {
        let (c, dir) = (self.config.lock().unwrap().clone(), self.paths.config_dir.clone());
        *self.tools.lock().unwrap() = (tools::find_ytdlp(&c, &dir), tools::find_ffmpeg(&c, &dir));
    }

    /// yt-dlp as found, ready to run, or why it can't.
    fn ytdlp(&self) -> Result<YtDlp> {
        let (y, f) = self.tools.lock().unwrap().clone();
        if y.path.is_empty() {
            bail!("yt-dlp wasn't found on this computer. Settings can download it.");
        }
        let c = self.config.lock().unwrap();
        Ok(YtDlp { exe: PathBuf::from(y.path), ffmpeg: if f.path.is_empty() { None } else { Some(PathBuf::from(f.path)) }, cookies: c.cookies.clone(), polite: c.polite.clone() })
    }

    /// This computer's settings as they stand (the desktop app reads quit_on_close).
    pub fn config(&self) -> AppConfig {
        self.config.lock().unwrap().clone()
    }

    /// Where system notices go (the desktop app); none until it's set.
    pub fn set_notifier(&self, f: Notifier) {
        *self.notifier.lock().unwrap() = Some(f);
    }

    /// Until when scheduled checks wait (unix seconds), if they do.
    fn paused(&self) -> Option<i64> {
        let t = *self.paused_until.lock().unwrap();
        (t > now_unix()).then_some(t)
    }

    /// One line for the tray: what's happening, or when the next check is.
    pub fn status_line(&self) -> String {
        if let Some(r) = self.jobs.lock().unwrap().current.as_ref() {
            let (_, place) = r.item.lock().unwrap().clone();
            return format!("{}{}", r.job.label, if place.is_empty() { String::new() } else { format!(" ({place})") });
        }
        if let Some(t) = self.paused() {
            return format!("Checks paused for {} (YouTube asked for a sign-in)", in_words(t - now_unix()));
        }
        let next = self.lib.lock().unwrap().as_ref().map(|l| self.next_check(l));
        match next.as_ref().and_then(|v| v.as_str()).and_then(crate::util::unix_from_iso) {
            Some(t) if t <= now_unix() + 60 => "Next check: now".into(),
            Some(t) => format!("Next check in {}", in_words(t - now_unix())),
            None => "No checks scheduled".into(),
        }
    }

    /// The notice a finished job deserves, if any: new videos found by the
    /// schedule, or something only the user can fix (cookies, a sign-in).
    pub fn notice_for(f: &Finished) -> Option<(String, String)> {
        let needs_you = |t: &str| t.contains("add cookies") || t.contains("cookies file") || t.contains("its cookies") || t.contains("their cookies");
        let o = f.outcome.as_ref();
        let reason = if needs_you(&f.message) {
            Some(f.message.clone())
        } else {
            o.and_then(|o| o.failed.iter().find(|x| needs_you(x)).cloned())
        };
        if let Some(r) = reason {
            return Some((format!("{} needs you", f.job.label), r.chars().take(240).collect()));
        }
        let got = o.map(|o| o.downloaded.as_slice()).unwrap_or_default();
        if f.job.by == "schedule" && !got.is_empty() {
            let mut text = got.iter().take(3).cloned().collect::<Vec<_>>().join(" · ");
            if got.len() > 3 {
                text.push_str(&format!(" and {} more", got.len() - 3));
            }
            return Some((format!("{}: {}", f.job.label, plural(got.len(), "new video")), text));
        }
        None
    }

    /// One command, by name, with its arguments as JSON.
    pub fn call(&self, cmd: &str, args: Value) -> Result<Value, String> {
        self.dispatch(cmd, &args).map_err(|e| format!("{e:#}"))
    }

    fn dispatch(&self, cmd: &str, a: &Value) -> Result<Value> {
        let id = || a["id"].as_str().map(String::from).context("no source given");
        let vid = || a["video"].as_str().map(String::from).filter(|v| !v.is_empty()).context("no video given");
        match cmd {
            "app_info" => Ok(self.app_info()),
            "library_open" => {
                let p = a["path"].as_str().context("no folder given")?;
                if self.jobs.lock().unwrap().current.is_some() {
                    bail!("Wait for the download in hand to finish (or stop it) before changing library.");
                }
                self.jobs.lock().unwrap().queue.clear();
                self.open_library(PathBuf::from(p));
                if let Some(e) = self.lib_error.lock().unwrap().clone() {
                    bail!(e);
                }
                Ok(self.app_info())
            }
            "library_inspect" => self.library_inspect(Path::new(a["path"].as_str().context("no folder given")?)),
            "library_move" => {
                let to = a["path"].as_str().context("no folder given")?.trim().to_string();
                let i = self.library_inspect(Path::new(&to))?;
                if !i["can_move"].as_bool().unwrap_or(false) {
                    bail!("{}", i["why_not"].as_str().unwrap_or("The library can't be moved there."));
                }
                {
                    let j = self.jobs.lock().unwrap();
                    if j.current.is_some() || !j.queue.is_empty() {
                        bail!("Wait for the jobs in Activity to finish (or stop them) before moving the library.");
                    }
                }
                Ok(json!({ "job": self.enqueue(JobKind::Move { to: to.clone() }, "library".into(), format!("Moving the library to {to}"), "you") }))
            }
            "checks_resume" => {
                *self.paused_until.lock().unwrap() = 0;
                self.queue_due();
                Ok(self.app_info())
            }
            "cookies_test" => self.cookies_test(a),
            "cookies_test_result" => Ok(self.cookie_test.lock().unwrap().clone()),
            "prefs_get" => Ok(self.prefs.get()),
            "prefs_set" => {
                self.prefs.set(&a["prefs"])?;
                Ok(json!(true))
            }
            "settings_get" => self.settings_get(),
            "settings_set" => self.settings_set(a),
            "tools_find" => {
                self.refind_tools();
                self.settings_get()
            }
            "tool_update" => {
                let name = a["name"].as_str().unwrap_or("yt-dlp").to_string();
                if name != "yt-dlp" && name != "ffmpeg" {
                    bail!("unknown tool {name}");
                }
                Ok(json!({ "job": self.enqueue(JobKind::Tool { name: name.clone() }, format!("tool:{name}"), name, "you") }))
            }
            "videos" => self.videos(a["query"].as_str().unwrap_or("")),
            "search_text" => self.search_text(a["query"].as_str().unwrap_or("")),
            "video" => self.video(a["channel"].as_str().context("no channel given")?, &vid()?),
            "sources" => self.sources(),
            "source" => self.source(&id()?),
            "creators" => self.with_lib(|l| {
                let mut m: BTreeMap<String, usize> = BTreeMap::new();
                for c in l.channels.values().filter(|c| !c.creator.is_empty()) {
                    *m.entry(c.creator.clone()).or_default() += 1;
                }
                Ok(json!(m.into_iter().map(|(name, n)| json!({ "name": name, "channels": n })).collect::<Vec<_>>()))
            }),
            "creator_rename" => {
                let from = a["from"].as_str().unwrap_or("").trim().to_string();
                let to = a["to"].as_str().unwrap_or("").trim().to_string();
                self.with_lib(|l| {
                    let keys: Vec<String> = l.channels.values().filter(|c| c.creator == from).map(Channel::key).collect();
                    for k in keys {
                        let mut c = l.channels[&k].clone();
                        c.creator = to.clone();
                        l.put_channel(c)?;
                    }
                    Ok(json!(true))
                })
            }
            "audit" => self.audit_table(),
            "link_check" => self.link_check(a["input"].as_str().unwrap_or("")),
            "add" => self.add(a),
            "check" => {
                let id = id()?;
                let label = self.label_of(&id)?;
                Ok(json!({ "job": self.enqueue(JobKind::Check { id: id.clone() }, id, label, "you") }))
            }
            "check_all" => {
                let list: Vec<(String, String)> = self.with_lib(|l| Ok(l.channels.values().filter(|c| c.followed).map(|c| (c.id(), c.name())).chain(l.playlists.values().map(|p| (p.id(), p.name()))).collect()))?;
                let n = list.into_iter().filter_map(|(id, label)| self.enqueue(JobKind::Check { id: id.clone() }, id, label, "you")).count();
                Ok(json!({ "queued": n }))
            }
            "download_missing" => {
                let id = id()?;
                let label = self.label_of(&id)?;
                Ok(json!({ "job": self.enqueue(JobKind::Missing { id: id.clone() }, id, label, "you") }))
            }
            "video_get" => {
                let (id, video) = (id()?, vid()?);
                let label = self.with_lib(|l| Ok(upstream_of(l, &id)?.iter().find(|x| x.id == video).map(|x| x.title.clone()).unwrap_or_else(|| video.clone())))?;
                self.set_state(&id, &video, State::Wanted, "")?;
                Ok(json!({ "job": self.enqueue(JobKind::Video { id: id.clone(), video: video.clone() }, format!("video:{video}"), label, "you") }))
            }
            "video_skip" => {
                let (id, video) = (id()?, vid()?);
                let skip = a["skip"].as_bool().unwrap_or(true);
                self.set_state(&id, &video, if skip { State::Skipped } else { State::Wanted }, "")?;
                self.source(&id)
            }
            "follow_set" => {
                let id = id()?;
                let f: Follow = serde_json::from_value(a["follow"].clone()).context("those settings can't be read")?;
                self.edit_source(&id, |c, p| {
                    if let Some(c) = c {
                        c.follow = f.clone();
                        // a channel listed but not followed starts following when its mode asks for downloads
                        if !c.followed && f.mode != Mode::None {
                            c.followed = true;
                        }
                    }
                    if let Some(p) = p {
                        p.follow = f.clone();
                    }
                    Ok(())
                })?;
                self.source(&id)
            }
            "followed_set" => {
                let id = id()?;
                let on = a["followed"].as_bool().unwrap_or(true);
                self.edit_source(&id, |c, _| {
                    if let Some(c) = c {
                        c.followed = on;
                        if on && c.upstream.is_empty() {
                            c.last_checked.clear();
                        }
                    }
                    Ok(())
                })?;
                if on {
                    let label = self.label_of(&id)?;
                    self.enqueue(JobKind::Check { id: id.clone() }, id.clone(), label, "you");
                }
                self.source(&id)
            }
            "tags_set" => {
                let id = id()?;
                let tags = tags_of(&a["tags"]);
                self.edit_source(&id, |c, p| {
                    if let Some(c) = c {
                        c.tags = tags.clone();
                    }
                    if let Some(p) = p {
                        p.tags = tags.clone();
                    }
                    Ok(())
                })?;
                self.source(&id)
            }
            "notes_set" => {
                let id = id()?;
                let notes = a["notes"].as_str().unwrap_or("").to_string();
                self.edit_source(&id, |c, p| {
                    if let Some(c) = c {
                        c.notes = notes.clone();
                    }
                    if let Some(p) = p {
                        p.notes = notes.clone();
                    }
                    Ok(())
                })?;
                self.source(&id)
            }
            "creator_set" => {
                let id = id()?;
                let creator = a["creator"].as_str().unwrap_or("").trim().to_string();
                self.edit_source(&id, |c, _| {
                    if let Some(c) = c {
                        c.creator = creator.clone();
                    }
                    Ok(())
                })?;
                self.source(&id)
            }
            "video_tags_set" => {
                let key = a["channel"].as_str().context("no channel given")?.to_string();
                let video = vid()?;
                let tags = tags_of(&a["tags"]);
                self.with_lib(|l| {
                    let mut v = l.channels.get(&key).and_then(|c| c.videos.iter().find(|v| v.id == video)).cloned().context("That video isn't in the library.")?;
                    v.tags = tags;
                    l.put_video(&key, v)
                })?;
                self.video(&key, &video)
            }
            "video_progress" => {
                let key = a["channel"].as_str().context("no channel given")?.to_string();
                let video = vid()?;
                let pos = a["position"].as_f64().unwrap_or(0.0);
                let ended = a["ended"].as_bool().unwrap_or(false);
                self.with_lib(|l| {
                    let mut v = l.channels.get(&key).and_then(|c| c.videos.iter().find(|v| v.id == video)).cloned().context("That video isn't in the library.")?;
                    v.played_to(pos, ended);
                    let out = json!({ "watched": v.watched, "position": v.position });
                    l.put_video(&key, v)?;
                    Ok(out)
                })
            }
            "videos_watched" => {
                let on = a["watched"].as_bool().unwrap_or(true);
                self.each_video(&a["items"], |v| {
                    v.set_watched(on);
                    Ok(())
                })
            }
            "videos_tag" => {
                let add = tags_of(&a["add"]);
                let remove = tags_of(&a["remove"]);
                self.each_video(&a["items"], |v| {
                    v.tags.retain(|t| !remove.contains(t));
                    for t in &add {
                        if !v.tags.contains(t) {
                            v.tags.push(t.clone());
                        }
                    }
                    Ok(())
                })
            }
            "videos_delete" => {
                let items = items_of(&a["items"])?;
                let busy = self.jobs.lock().unwrap().current.as_ref().map(|c| c.item.lock().unwrap().0.clone()).unwrap_or_default();
                let mut n = 0;
                for (key, video) in items {
                    if video == busy {
                        continue;
                    }
                    self.with_lib(|l| capture::delete_video(l, &key, &video))?;
                    n += 1;
                }
                Ok(json!({ "deleted": n }))
            }
            "remove" => self.remove(&id()?),
            "video_delete" => {
                let key = a["channel"].as_str().context("no channel given")?.to_string();
                let video = vid()?;
                if self.jobs.lock().unwrap().current.as_ref().is_some_and(|c| c.item.lock().unwrap().0 == video) {
                    bail!("That video is downloading: stop it first.");
                }
                self.with_lib(|l| capture::delete_video(l, &key, &video))?;
                Ok(json!(true))
            }
            "jobs" => Ok(self.jobs_json()),
            "job_stop" => {
                let job = a["job"].as_u64();
                let mut j = self.jobs.lock().unwrap();
                if let Some(c) = &j.current {
                    if job.is_none() || job == Some(c.job.id) {
                        c.progress.stop.store(true, Ordering::Relaxed);
                    }
                }
                if let Some(job) = job {
                    j.queue.retain(|q| q.id != job);
                } else {
                    j.queue.clear();
                }
                Ok(json!(true))
            }
            "path" => {
                let id = id()?;
                let sub = a["sub"].as_str().unwrap_or("");
                self.with_lib(|l| {
                    let base = l.folder_of(&id).context("not a source's id")?;
                    let p = if sub.is_empty() { base } else { base.join(crate::util::safe_rel(std::path::Path::new(sub)).context("not a folder of the source")?) };
                    Ok(json!(p.display().to_string()))
                })
            }
            other => bail!("unknown command {other}"),
        }
    }

    /// Change several kept videos the same way ([{channel, video}, …]), saving each video.json.
    fn each_video(&self, items: &Value, mut f: impl FnMut(&mut Video) -> Result<()>) -> Result<Value> {
        let items = items_of(items)?;
        self.with_lib(|l| {
            let mut n = 0;
            for (key, video) in &items {
                let mut v = l.channels.get(key).and_then(|c| c.videos.iter().find(|v| &v.id == video)).cloned().with_context(|| format!("{video} isn't in the library."))?;
                f(&mut v)?;
                l.put_video(key, v)?;
                n += 1;
            }
            Ok(json!({ "changed": n }))
        })
    }

    fn app_info(&self) -> Value {
        let lib = self.lib.lock().unwrap();
        let cfg = self.config.lock().unwrap();
        let (y, f) = self.tools.lock().unwrap().clone();
        json!({
            "version": crate::VERSION,
            "os": std::env::consts::OS,
            "library_url": self.paths.library_url,
            "library": lib.as_ref().map(|l| json!({ "path": l.root.display().to_string(), "name": l.name, "id": l.id, "videos": l.video_count() })),
            "library_error": *self.lib_error.lock().unwrap(),
            "recent_libraries": cfg.recent_libraries.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "default_library": crate::config::default_library_path().display().to_string(),
            "ytdlp": !y.path.is_empty(),
            "ffmpeg": !f.path.is_empty(),
            "paused_until": self.paused().map(crate::util::iso_from_unix),
        })
    }

    fn settings_get(&self) -> Result<Value> {
        let cfg = self.config.lock().unwrap().clone();
        let (y, f) = self.tools.lock().unwrap().clone();
        let lib = self.lib.lock().unwrap();
        Ok(json!({
            "library": lib.as_ref().map(|l| l.settings.clone()),
            "next_check": lib.as_ref().map(|l| self.next_check(l)).unwrap_or(Value::Null),
            "tools": { "ytdlp": y, "ffmpeg": f, "folder": tools::tools_dir(&self.paths.config_dir).display().to_string() },
            "ytdlp_path": cfg.ytdlp_path,
            "ffmpeg_path": cfg.ffmpeg_path,
            "cookies": cfg.cookies,
            "polite": cfg.polite,
            "quit_on_close": cfg.quit_on_close,
            "notices_off": cfg.notices_off,
            "cookie_test_video": sources::video_url(COOKIE_TEST_VIDEO),
        }))
    }

    fn settings_set(&self, a: &Value) -> Result<Value> {
        let mut tools_changed = false;
        {
            let mut c = self.config.lock().unwrap();
            if let Some(p) = a["ytdlp_path"].as_str() {
                c.ytdlp_path = p.trim().to_string();
                tools_changed = true;
            }
            if let Some(p) = a["ffmpeg_path"].as_str() {
                c.ffmpeg_path = p.trim().to_string();
                tools_changed = true;
            }
            if a["cookies"].is_object() {
                c.cookies = serde_json::from_value::<Cookies>(a["cookies"].clone()).context("those cookie settings can't be read")?;
            }
            if a["polite"].is_object() {
                let p: Polite = serde_json::from_value(a["polite"].clone()).context("those settings can't be read")?;
                if !(0.0..=600.0).contains(&p.sleep_requests) || !(0.0..=3600.0).contains(&p.sleep_downloads) {
                    bail!("Pauses are between 0 and 600 seconds (between requests) or 3600 seconds (between downloads).");
                }
                c.polite = p;
            }
            if let Some(b) = a["quit_on_close"].as_bool() {
                c.quit_on_close = b;
            }
            if let Some(b) = a["notices_off"].as_bool() {
                c.notices_off = b;
            }
            c.save(&self.paths.config_dir.join("config.json"))?;
        }
        if tools_changed {
            self.refind_tools();
        }
        if !a["library"].is_null() {
            let s: Settings = serde_json::from_value(a["library"].clone()).context("those settings can't be read")?;
            self.with_lib(|l| l.save_settings(s))?;
        }
        self.settings_get()
    }

    /// When the schedule will next check something (ISO 8601), or null.
    fn next_check(&self, l: &Library) -> Value {
        let s = &l.settings;
        let c = l.channels.values().filter(|c| c.followed).filter_map(|c| capture::due_at(&c.last_checked, &c.added, c.follow.every_hours, s));
        let p = l.playlists.values().filter_map(|p| capture::due_at(&p.last_checked, &p.added, p.follow.every_hours, s));
        let floor = self.paused().unwrap_or(0).max(now_unix());
        c.chain(p).min().map(|t| json!(crate::util::iso_from_unix(t.max(floor)))).unwrap_or(Value::Null)
    }

    fn status_of(&self, target: &str) -> &'static str {
        let j = self.jobs.lock().unwrap();
        if j.current.as_ref().is_some_and(|c| c.job.target == target) {
            "working"
        } else if j.queue.iter().any(|q| q.target == target) {
            "queued"
        } else {
            ""
        }
    }

    fn label_of(&self, id: &str) -> Result<String> {
        self.with_lib(|l| {
            let (kind, key) = split_id(id).context("not a source's id")?;
            Ok(match kind {
                Kind::Channel => l.channels.get(key).context("That channel isn't in the library.")?.name(),
                Kind::Playlist => l.playlists.get(key).context("That playlist isn't in the library.")?.name(),
            })
        })
    }

    // ------------------------------------------------------------- reading

    fn file_url(&self, parts: &[&str]) -> String {
        let enc: Vec<String> = parts.iter().map(|p| percent_encode(p)).collect();
        format!("{}{}", self.paths.library_url, enc.join("/"))
    }

    fn video_summary(&self, l: &Library, c: &Channel, v: &Video) -> Value {
        let key = c.key();
        let base = [library::HOST, library::CHANNELS_DIR, key.as_str(), VIDEOS_DIR, v.folder.as_str()];
        let url = |f: &str| if f.is_empty() { Value::Null } else { json!(self.file_url(&[&base[..], &[f]].concat())) };
        json!({
            "id": v.id,
            "channel": key,
            "source": c.id(),
            "channel_name": c.name(),
            "creator": c.creator,
            "title": v.title,
            "upload_date": v.upload_date,
            "duration": v.duration,
            "bytes": v.bytes,
            "width": v.width,
            "height": v.height,
            "ext": v.ext,
            "downloaded": v.downloaded,
            "thumbnail_url": url(&v.thumbnail),
            "file_url": url(&v.file),
            "file": v.file,
            "folder": v.folder,
            "quality": v.quality,
            "tags": v.tags,
            "watched": v.watched,
            "position": v.position,
            "played": v.played,
            "view_count": v.view_count,
            "subtitles": v.subtitles,
            "playlists": l.playlists_of(&v.id).iter().map(|p| p.id()).collect::<Vec<_>>(),
        })
    }

    fn videos(&self, query: &str) -> Result<Value> {
        let words: Vec<String> = query.to_lowercase().split_whitespace().map(String::from).collect();
        self.with_lib(|l| {
            let mut out = vec![];
            for c in l.channels.values() {
                for v in &c.videos {
                    if !words.is_empty() {
                        let meta = format!("{} {} {} {} {} {} {}", v.title, c.name(), c.source.handle, c.creator, v.tags.join(" "), c.tags.join(" "), v.upload_date).to_lowercase();
                        if !words.iter().all(|w| meta.contains(w.as_str())) {
                            continue;
                        }
                    }
                    out.push(self.video_summary(l, c, v));
                }
            }
            // what's downloading now, for the dashed card
            let now = self.jobs.lock().unwrap().current.as_ref().map(|r| {
                let (video, place) = r.item.lock().unwrap().clone();
                json!({ "video": video, "place": place, "stage": *r.stage.lock().unwrap(), "label": r.job.label, "target": r.job.target })
            });
            Ok(json!({
                "videos": out,
                "count": l.video_count(),
                "bytes": l.bytes(),
                "channels": l.channels.values().map(|c| json!({ "id": c.id(), "key": c.key(), "name": c.name(), "followed": c.followed, "creator": c.creator, "videos": c.videos.len(), "tags": c.tags })).collect::<Vec<_>>(),
                "playlists": l.playlists.values().map(|p| json!({ "id": p.id(), "key": p.key(), "name": p.name(), "videos": p.upstream.iter().filter(|x| l.has_video(&x.id)).count() })).collect::<Vec<_>>(),
                "unreadable": l.unreadable.iter().map(|(f, e)| json!({ "folder": f, "error": e })).collect::<Vec<_>>(),
                "next_check": self.next_check(l),
                "downloading": now,
            }))
        })
    }

    /// Videos whose subtitles or description hold a phrase: where it's said
    /// (seconds and the line), how often, and a bit of the description.
    fn search_text(&self, query: &str) -> Result<Value> {
        let phrase = crate::textsearch::normalise(query);
        if phrase.chars().count() < 3 {
            return Ok(json!({ "query": query, "results": [], "videos_searched": 0 }));
        }
        self.with_lib(|l| {
            let mut results = vec![];
            let mut searched = 0;
            let mut texts = self.texts.lock().unwrap();
            for c in l.channels.values() {
                let dir = l.channel_folder(&c.key()).join(VIDEOS_DIR);
                for v in &c.videos {
                    let key = format!("{}/{}", c.key(), v.id);
                    let stamp = format!("{}|{}|{}", v.folder, v.downloaded, v.subtitles.join(","));
                    let doc = match texts.get(&key) {
                        Some((st, d)) if *st == stamp => d.clone(),
                        _ => {
                            let d = Arc::new(crate::textsearch::Doc::load(&dir.join(&v.folder), v));
                            texts.insert(key, (stamp, d.clone()));
                            d
                        }
                    };
                    searched += 1;
                    let (hits, count, snippet) = doc.find(&phrase, 5);
                    if count > 0 || snippet.is_some() {
                        let mut s = self.video_summary(l, c, v);
                        s["hits"] = json!(hits);
                        s["hit_count"] = json!(count);
                        s["in_description"] = json!(snippet);
                        results.push(s);
                    }
                }
            }
            // most said first, then newest
            results.sort_by(|a, b| b["hit_count"].as_u64().cmp(&a["hit_count"].as_u64()).then_with(|| b["upload_date"].as_str().cmp(&a["upload_date"].as_str())));
            let total = results.len();
            results.truncate(100);
            Ok(json!({ "query": query, "results": results, "total": total, "videos_searched": searched }))
        })
    }

    fn video(&self, key: &str, video: &str) -> Result<Value> {
        self.with_lib(|l| {
            let c = l.channels.get(key).context("That channel isn't in the library.")?;
            let v = c.videos.iter().find(|v| v.id == video).context("That video isn't in the library.")?;
            let mut s = self.video_summary(l, c, v);
            let dir = l.channel_folder(key).join(VIDEOS_DIR).join(&v.folder);
            let info: Value = if v.info.is_empty() { Value::Null } else { std::fs::read(dir.join(&v.info)).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(Value::Null) };
            s["description"] = json!(info["description"].as_str().unwrap_or(""));
            s["like_count"] = json!(info["like_count"].as_u64());
            s["uploader"] = json!(info["uploader"].as_str().unwrap_or(""));
            s["categories"] = info["categories"].clone();
            s["youtube_tags"] = json!(info["tags"].as_array().map(|a| a.iter().filter_map(|t| t.as_str()).take(30).collect::<Vec<_>>()).unwrap_or_default());
            s["fps"] = info["fps"].clone();
            s["vcodec"] = json!(if v.vcodec.is_empty() { info["vcodec"].as_str().unwrap_or("") } else { &v.vcodec });
            s["acodec"] = json!(if v.acodec.is_empty() { info["acodec"].as_str().unwrap_or("") } else { &v.acodec });
            s["format"] = json!(info["format"].as_str().unwrap_or(""));
            // top comments, when they were saved (Follow → Comments)
            s["comments"] = json!(info["comments"].as_array().map(|a| a.iter().filter(|c| c["parent"].as_str().unwrap_or("root") == "root").take(200).map(|c| json!({
                "author": c["author"].as_str().unwrap_or(""),
                "text": c["text"].as_str().unwrap_or(""),
                "likes": c["like_count"].as_u64().unwrap_or(0),
                "when": c["timestamp"].as_i64().map(crate::util::iso_from_unix),
                "pinned": c["is_pinned"].as_bool().unwrap_or(false),
                "by_uploader": c["author_is_uploader"].as_bool().unwrap_or(false),
            })).collect::<Vec<_>>()));
            s["comment_count"] = info["comment_count"].clone();
            s["chapters"] = json!(info["chapters"].as_array().map(|a| a.iter().map(|ch| json!({ "title": ch["title"], "start": ch["start_time"] })).collect::<Vec<_>>()).unwrap_or_default());
            s["path"] = json!(dir.display().to_string());
            s["web_url"] = json!(sources::video_url(video));
            s["playlists"] = json!(l.playlists_of(video).iter().map(|p| json!({ "id": p.id(), "name": p.name() })).collect::<Vec<_>>());
            s["channel_avatar"] = json!(if c.details.avatar.is_empty() { None } else { Some(self.file_url(&[library::HOST, library::CHANNELS_DIR, key, &c.details.avatar])) });
            Ok(s)
        })
    }

    fn channel_summary(&self, l: &Library, c: &Channel) -> Value {
        let id = c.id();
        let a = audit(&c.upstream, &|v| c.has(v), c.videos.len());
        let status = self.status_of(&id);
        json!({
            "id": id,
            "kind": "channel",
            "key": c.key(),
            "name": c.name(),
            "handle": c.source.handle,
            "channel_id": c.source.id,
            "url": if c.source.url.is_empty() { format!("https://www.youtube.com/{}", c.key()) } else { c.source.url.clone() },
            "description": c.details.description,
            "avatar_url": if c.details.avatar.is_empty() { Value::Null } else { json!(self.file_url(&[library::HOST, library::CHANNELS_DIR, &c.key(), &c.details.avatar])) },
            "subscribers": c.details.subscribers,
            "video_count": c.details.video_count,
            "followed": c.followed,
            "follow": c.follow,
            "creator": c.creator,
            "tags": c.tags,
            "notes": c.notes,
            "added": c.added,
            "last_checked": c.last_checked,
            "last_changed": c.last_changed,
            "error": c.last_error,
            "status": if !status.is_empty() { status } else if !c.last_error.is_empty() { "error" } else { "ok" },
            "due": if c.followed { capture::due_at(&c.last_checked, &c.added, c.follow.every_hours, &l.settings).map(crate::util::iso_from_unix) } else { None },
            "videos": c.videos.len(),
            "bytes": c.bytes(),
            "audit": a,
            "newest": c.videos.iter().map(|v| v.upload_date.as_str()).max().unwrap_or(""),
        })
    }

    fn playlist_summary(&self, l: &Library, p: &Playlist) -> Value {
        let id = p.id();
        let a = audit(&p.upstream, &|v| l.has_video(v), p.upstream.iter().filter(|x| l.has_video(&x.id)).count());
        let status = self.status_of(&id);
        let bytes: u64 = p.upstream.iter().filter_map(|x| l.find_video(&x.id)).map(|(_, v)| v.bytes).sum();
        json!({
            "id": id,
            "kind": "playlist",
            "key": p.key(),
            "name": p.name(),
            "url": if p.source.url.is_empty() { sources::playlist_url(&p.source.id) } else { p.source.url.clone() },
            "description": p.details.description,
            "channel": p.details.channel,
            "channel_source": l.channel_by_id(&p.details.channel_id).map(Channel::id),
            "thumbnail_url": p.details.thumbnail_url,
            "video_count": p.details.video_count,
            "followed": true,
            "follow": p.follow,
            "tags": p.tags,
            "notes": p.notes,
            "added": p.added,
            "last_checked": p.last_checked,
            "last_changed": p.last_changed,
            "error": p.last_error,
            "status": if !status.is_empty() { status } else if !p.last_error.is_empty() { "error" } else { "ok" },
            "due": capture::due_at(&p.last_checked, &p.added, p.follow.every_hours, &l.settings).map(crate::util::iso_from_unix),
            "videos": a.kept,
            "bytes": bytes,
            "audit": a,
        })
    }

    fn sources(&self) -> Result<Value> {
        self.with_lib(|l| {
            let channels: Vec<Value> = l.channels.values().map(|c| self.channel_summary(l, c)).collect();
            let playlists: Vec<Value> = l.playlists.values().map(|p| self.playlist_summary(l, p)).collect();
            Ok(json!({ "channels": channels, "playlists": playlists, "next_check": self.next_check(l) }))
        })
    }

    /// A listed video with what the audit says of it.
    fn listed_json(&self, l: &Library, x: &Listed) -> Value {
        let kept = l.find_video(&x.id).map(|(k, v)| json!({ "channel": k, "folder": v.folder, "bytes": v.bytes, "quality": v.quality }));
        let waiting = {
            let j = self.jobs.lock().unwrap();
            j.queue.iter().chain(j.current.as_ref().map(|c| &c.job)).any(|q| matches!(&q.kind, JobKind::Video { video, .. } if *video == x.id))
                || j.current.as_ref().is_some_and(|c| c.item.lock().unwrap().0 == x.id)
        };
        let mut v = serde_json::to_value(x).unwrap_or(Value::Null);
        v["kept"] = kept.unwrap_or(Value::Null);
        v["waiting"] = json!(waiting);
        v["web_url"] = json!(sources::video_url(&x.id));
        v
    }

    fn source(&self, id: &str) -> Result<Value> {
        self.with_lib(|l| {
            let (kind, key) = split_id(id).context("not a source's id")?;
            match kind {
                Kind::Channel => {
                    let c = l.channels.get(key).context("That channel isn't in the library.")?;
                    let mut v = self.channel_summary(l, c);
                    v["videos_kept"] = json!(c.videos.iter().rev().map(|x| self.video_summary(l, c, x)).collect::<Vec<_>>());
                    v["upstream"] = json!(c.upstream.iter().map(|x| self.listed_json(l, x)).collect::<Vec<_>>());
                    v["playlists"] = json!(l.playlists.values().filter(|p| p.details.channel_id == c.source.id || p.upstream.iter().any(|x| c.has(&x.id))).map(|p| self.playlist_summary(l, p)).collect::<Vec<_>>());
                    v["path"] = json!(l.channel_folder(key).display().to_string());
                    Ok(v)
                }
                Kind::Playlist => {
                    let p = l.playlists.get(key).context("That playlist isn't in the library.")?;
                    let mut v = self.playlist_summary(l, p);
                    v["videos_kept"] = json!(p.upstream.iter().filter_map(|x| l.find_video(&x.id).map(|(k, vv)| self.video_summary(l, &l.channels[k], vv))).collect::<Vec<_>>());
                    v["upstream"] = json!(p.upstream.iter().map(|x| self.listed_json(l, x)).collect::<Vec<_>>());
                    v["path"] = json!(l.playlist_folder(key).display().to_string());
                    Ok(v)
                }
            }
        })
    }

    fn audit_table(&self) -> Result<Value> {
        self.with_lib(|l| {
            let mut rows: Vec<Value> = l.channels.values().filter(|c| c.followed || !c.upstream.is_empty()).map(|c| self.channel_summary(l, c)).collect();
            rows.extend(l.playlists.values().map(|p| self.playlist_summary(l, p)));
            let mut total = Audit::default();
            for r in &rows {
                if let Ok(a) = serde_json::from_value::<Audit>(r["audit"].clone()) {
                    total.listed += a.listed;
                    total.kept += a.kept;
                    total.missing += a.missing;
                    total.not_wanted += a.not_wanted;
                    total.failed += a.failed;
                    total.skipped += a.skipped;
                    total.waiting += a.waiting;
                    total.gone += a.gone;
                    total.kept_unlisted += a.kept_unlisted;
                }
            }
            Ok(json!({ "rows": rows, "total": total }))
        })
    }

    /// What a typed link names, without asking YouTube (for the Add dialog).
    fn link_check(&self, input: &str) -> Result<Value> {
        let mut out = vec![];
        for line in input.lines().map(str::trim).filter(|l| !l.is_empty()) {
            out.push(match sources::parse(line) {
                Err(e) => json!({ "input": line, "ok": false, "error": e.to_string() }),
                Ok(link) => {
                    let have = self.with_lib(|l| Ok(self.have(l, &link))).unwrap_or(None);
                    let mut v = json!({ "input": line, "ok": true, "link": link, "label": label_of_link(&link), "in_library": have });
                    if let Link::Video { playlist: Some(p), .. } = &link {
                        v["also_playlist"] = json!(p);
                    }
                    v
                }
            });
        }
        Ok(json!(out))
    }

    /// The source's id when a link's thing is in the library already (a kept video: its channel's id).
    fn have(&self, l: &Library, link: &Link) -> Option<String> {
        match link {
            Link::Channel { key, .. } => l.channels.values().find(|c| c.followed && capture::channel_key_matches(key, c)).map(Channel::id),
            Link::Playlist { id } => l.playlists.get(id).map(Playlist::id),
            Link::Video { id, .. } => l.find_video(id).map(|(k, _)| source_id(Kind::Channel, k)),
        }
    }

    fn add(&self, a: &Value) -> Result<Value> {
        let inputs: Vec<String> = match &a["inputs"] {
            Value::Array(v) => v.iter().filter_map(|x| x.as_str()).map(String::from).collect(),
            _ => a["input"].as_str().unwrap_or("").lines().map(String::from).collect(),
        };
        let inputs: Vec<String> = inputs.into_iter().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
        if inputs.is_empty() {
            bail!("Paste a link to a YouTube video, channel or playlist.");
        }
        let (default_c, default_p) = self.with_lib(|l| Ok((l.settings.new_follow.clone(), l.settings.new_follow_playlist.clone())))?;
        let follow_c: Follow = if a["follow"].is_object() { serde_json::from_value(a["follow"].clone()).context("those settings can't be read")? } else { default_c };
        let follow_p: Follow = if a["playlist_follow"].is_object() { serde_json::from_value(a["playlist_follow"].clone()).context("those settings can't be read")? } else { default_p };
        self.ytdlp()?;
        let mut out = vec![];
        for input in inputs {
            match sources::parse(&input) {
                Err(e) => out.push(json!({ "input": input, "error": e.to_string() })),
                Ok(link) => {
                    let have = self.with_lib(|l| Ok(self.have(l, &link)))?;
                    if let Some(id) = have {
                        out.push(json!({ "input": input, "id": id, "error": "already in the library" }));
                        continue;
                    }
                    let (target, follow) = match &link {
                        Link::Channel { key, .. } => (format!("add:{}", key.label()), follow_c.clone()),
                        Link::Playlist { id } => (source_id(Kind::Playlist, id), follow_p.clone()),
                        Link::Video { id, .. } => (format!("video:{id}"), follow_c.clone()),
                    };
                    let label = label_of_link(&link);
                    let job = self.enqueue(JobKind::Add { link: link.clone(), follow }, target.clone(), label, "you");
                    out.push(json!({ "input": input, "target": target, "job": job }));
                }
            }
        }
        Ok(json!(out))
    }

    fn edit_source(&self, id: &str, f: impl FnOnce(Option<&mut Channel>, Option<&mut Playlist>) -> Result<()>) -> Result<()> {
        self.with_lib(|l| {
            let (kind, key) = split_id(id).context("not a source's id")?;
            match kind {
                Kind::Channel => {
                    let mut c = l.channels.get(key).cloned().context("That channel isn't in the library.")?;
                    f(Some(&mut c), None)?;
                    l.put_channel(c)?;
                }
                Kind::Playlist => {
                    let mut p = l.playlists.get(key).cloned().context("That playlist isn't in the library.")?;
                    f(None, Some(&mut p))?;
                    l.put_playlist(p)?;
                }
            }
            Ok(())
        })
    }

    /// Set a listed video's audit state in a source.
    fn set_state(&self, id: &str, video: &str, state: State, error: &str) -> Result<()> {
        self.edit_source(id, |c, p| {
            let up = match (c, p) {
                (Some(c), _) => &mut c.upstream,
                (_, Some(p)) => &mut p.upstream,
                _ => unreachable!(),
            };
            let x = up.iter_mut().find(|x| x.id == video).context("That video isn't listed for this source.")?;
            x.state = state;
            x.error = error.to_string();
            if state == State::Failed {
                x.tries += 1;
                x.last_try = now_iso();
            }
            Ok(())
        })
    }

    fn remove(&self, id: &str) -> Result<Value> {
        if self.status_of(id) == "working" {
            bail!("That source is being checked: stop it first.");
        }
        self.jobs.lock().unwrap().queue.retain(|q| q.target != id);
        self.with_lib(|l| {
            let (kind, key) = split_id(id).context("not a source's id")?;
            let dir = match kind {
                Kind::Channel => {
                    l.channels.remove(key).context("That channel isn't in the library.")?;
                    l.channel_folder(key)
                }
                Kind::Playlist => {
                    l.playlists.remove(key).context("That playlist isn't in the library.")?;
                    l.playlist_folder(key)
                }
            };
            std::fs::remove_dir_all(&dir).with_context(|| format!("couldn't remove {}", dir.display()))?;
            Ok(json!(true))
        })
    }

    // ------------------------------------------------------------------ jobs

    /// Queue a job, unless the same one is queued or running. Its id, or None.
    fn enqueue(&self, kind: JobKind, target: String, label: String, by: &'static str) -> Option<u64> {
        let mut j = self.jobs.lock().unwrap();
        if j.queue.iter().any(|q| q.target == target && q.kind.same(&kind)) || j.current.as_ref().is_some_and(|c| c.job.target == target && c.job.kind.same(&kind)) {
            return None;
        }
        j.next += 1;
        let id = j.next;
        j.queue.push_back(Job { id, kind, target, label, by, queued: now_iso() });
        self.wake.notify_all();
        Some(id)
    }

    fn queue_due(&self) {
        if self.paused().is_some() {
            return;
        }
        let now = now_unix();
        let due: Vec<(String, String)> = match self.lib.lock().unwrap().as_ref() {
            Some(l) => {
                let s = &l.settings;
                let c = l.channels.values().filter(|c| c.followed && capture::due_at(&c.last_checked, &c.added, c.follow.every_hours, s).is_some_and(|t| t <= now)).map(|c| (c.id(), c.name()));
                let p = l.playlists.values().filter(|p| capture::due_at(&p.last_checked, &p.added, p.follow.every_hours, s).is_some_and(|t| t <= now)).map(|p| (p.id(), p.name()));
                c.chain(p).collect()
            }
            None => return,
        };
        for (id, label) in due {
            self.enqueue(JobKind::Check { id: id.clone() }, id, label, "schedule");
        }
    }

    fn jobs_json(&self) -> Value {
        let j = self.jobs.lock().unwrap();
        json!({
            "current": j.current.as_ref().map(|c| {
                let (video, place) = c.item.lock().unwrap().clone();
                json!({
                    "job": c.job,
                    "started": c.started,
                    "stage": *c.stage.lock().unwrap(),
                    "video": video,
                    "place": place,
                    "done": c.progress.done.load(Ordering::Relaxed),
                    "total": c.progress.total.load(Ordering::Relaxed),
                    "speed": c.progress.speed.load(Ordering::Relaxed),
                    "eta": c.progress.eta.load(Ordering::Relaxed),
                })
            }),
            "queued": j.queue,
            "recent": j.recent,
        })
    }

    fn work(self: Arc<Self>) {
        loop {
            let (job, progress, stage, item) = {
                let mut j = self.jobs.lock().unwrap();
                while j.queue.is_empty() {
                    j = self.wake.wait(j).unwrap();
                }
                let job = j.queue.pop_front().unwrap();
                let progress = Arc::new(Progress::default());
                let stage = Arc::new(Mutex::new(String::from("Starting")));
                let item = Arc::new(Mutex::new((String::new(), String::new())));
                j.current = Some(Running { job: job.clone(), started: now_iso(), progress: progress.clone(), stage: stage.clone(), item: item.clone() });
                (job, progress, stage, item)
            };
            let run = Run { progress: &progress, stage: &stage, item: &item };
            let res = match &job.kind {
                JobKind::Add { link, follow } => self.run_add(link, follow, &run),
                JobKind::Check { id } => self.run_check(id, false, &run),
                JobKind::Missing { id } => self.run_check(id, true, &run),
                JobKind::Video { id, video } => self.run_video(id, video, &run),
                JobKind::Tool { name } => self.run_tool(name, &run),
                JobKind::Move { to } => self.run_move(to, &run),
            };
            let mut j = self.jobs.lock().unwrap();
            j.current = None;
            let (ok, message, outcome) = match res {
                Ok(o) => (o.warnings.is_empty() && o.failed.is_empty(), o.describe(), Some(o)),
                Err(e) => (false, format!("{e:#}"), None),
            };
            let bot = message.contains(BOT_CHECK) || outcome.as_ref().is_some_and(|o| o.failed.iter().chain(&o.warnings).any(|x| x.contains(BOT_CHECK)));
            let f = Finished { job, finished: now_iso(), ok, message, outcome };
            let notice = Self::notice_for(&f);
            j.recent.push_front(f);
            j.recent.truncate(50);
            if bot {
                // the schedule leaves YouTube alone for a while; what's queued by it waits too
                *self.paused_until.lock().unwrap() = now_unix() + BOT_PAUSE_SECS;
                j.queue.retain(|q| q.by != "schedule");
            }
            drop(j);
            if let Some((title, text)) = notice {
                if !self.config.lock().unwrap().notices_off {
                    if let Some(n) = self.notifier.lock().unwrap().as_ref() {
                        n(&title, &text);
                    }
                }
            }
        }
    }

    /// What's at a folder the user picked for the library, so the page can ask the
    /// right question: open the library there, start one, or move this one there.
    fn library_inspect(&self, p: &Path) -> Result<Value> {
        let current = self.lib.lock().unwrap().as_ref().map(|l| (l.root.clone(), l.channels.len() + l.playlists.len(), l.video_count()));
        let exists = p.is_dir();
        let empty = !exists || std::fs::read_dir(p).map(|mut d| d.next().is_none()).unwrap_or(false);
        let is_library = p.join(library::META_DIR).join("library.json").is_file();
        let (same, inside, has_content) = match &current {
            Some((root, sources, videos)) => {
                let canon = |x: &Path| std::fs::canonicalize(x).unwrap_or_else(|_| x.to_path_buf());
                let (r, t) = (canon(root), canon(p));
                let t_parent = p.parent().map(canon);
                (r == t, t.starts_with(&r) || t_parent.is_some_and(|tp| tp.starts_with(&r)), sources + videos > 0)
            }
            None => (false, false, false),
        };
        let why_not = if current.is_none() {
            "No library is open to move."
        } else if same {
            "That's the library's folder already."
        } else if inside {
            "A folder inside the library can't hold it."
        } else if !empty {
            "Pick an empty (or new) folder to move the library into."
        } else {
            ""
        };
        Ok(json!({
            "path": p.display().to_string(),
            "exists": exists,
            "empty": empty,
            "is_library": is_library,
            "same": same,
            "current_has_content": has_content,
            "current": current.as_ref().map(|c| json!({ "path": c.0.display().to_string(), "sources": c.1, "videos": c.2 })),
            "can_move": why_not.is_empty(),
            "why_not": why_not,
        }))
    }

    /// Read an age-restricted video with the cookies given (or the saved ones), on
    /// a thread of its own so a download in hand doesn't hold it up. The answer
    /// comes from `cookies_test_result`.
    fn cookies_test(&self, a: &Value) -> Result<Value> {
        let mut y = self.ytdlp()?;
        if a["cookies"].is_object() {
            y.cookies = serde_json::from_value(a["cookies"].clone()).context("those cookie settings can't be read")?;
        }
        y.polite = Polite { sleep_requests: 0.0, ..y.polite };
        let started = now_iso();
        let state = json!({ "state": "testing", "at": started, "cookies": y.cookies });
        *self.cookie_test.lock().unwrap() = state.clone();
        let out = self.cookie_test.clone();
        std::thread::Builder::new().name("cookie test".into()).spawn(move || {
            let res = y.info(&sources::video_url(COOKIE_TEST_VIDEO), &Progress::default());
            let v = match res {
                Ok(info) => json!({ "state": "ok", "message": format!("YouTube answered: “{}”, an age-restricted video, can be read. Members-only and private videos you can watch in the browser will download too.", info["title"].as_str().unwrap_or(COOKIE_TEST_VIDEO)) }),
                Err(e) => {
                    let why = format!("{e:#}");
                    let hint = if y.cookies.mode.is_empty() { " (no cookies are set, so this is expected)" } else { "" };
                    json!({ "state": "failed", "message": format!("{why}{hint}") })
                }
            };
            let mut g = out.lock().unwrap();
            if g["at"].as_str() == Some(started.as_str()) {
                let mut v = v;
                v["at"] = json!(started);
                v["cookies"] = g["cookies"].clone();
                *g = v;
            }
        })?;
        Ok(state)
    }

    fn run_move(&self, to: &str, run: &Run) -> Result<Outcome> {
        let from = self.lib.lock().unwrap().as_ref().map(|l| l.root.clone()).context("No library is open.")?;
        let to = PathBuf::from(to);
        set_stage(run.stage, format!("Moving the library to {}", to.display()));
        // closed while it moves, so nothing writes into it halfway
        *self.lib.lock().unwrap() = None;
        *self.lib_error.lock().unwrap() = Some(format!("The library is being moved to {}. It opens there when the move is done.", to.display()));
        match crate::util::move_tree(&from, &to, run.progress) {
            Ok(()) => {
                self.open_library(to.clone());
                if let Some(e) = self.lib_error.lock().unwrap().clone() {
                    bail!("The files were moved to {}, but the library there couldn't be opened: {e}", to.display());
                }
                Ok(Outcome { note: Some(format!("Moved the library to {}", to.display())), ..Default::default() })
            }
            Err(e) => {
                self.open_library(from.clone());
                Err(e.context(format!("The library stays at {}", from.display())))
            }
        }
    }

    fn run_tool(&self, name: &str, run: &Run) -> Result<Outcome> {
        set_stage(run.stage, format!("Downloading {name}"));
        let v = if name == "ffmpeg" { tools::get_ffmpeg(&self.client, &self.paths.config_dir, run.progress)? } else { tools::update_ytdlp(&self.client, &self.paths.config_dir, run.progress)? };
        self.refind_tools();
        Ok(Outcome { note: Some(format!("{name} {v} is ready")), ..Default::default() })
    }

    fn run_add(&self, link: &Link, follow: &Follow, run: &Run) -> Result<Outcome> {
        let y = self.ytdlp()?;
        match link {
            Link::Channel { key, .. } => {
                let (meta, fresh) = capture::list_channel(&y, &key.url(), follow, run.progress, run.stage, &key.label())?;
                let (cref, mut details) = capture::channel_from(&meta);
                if cref.key().is_empty() {
                    bail!("YouTube didn't say which channel that is");
                }
                let id = self.with_lib(|l| {
                    // the channel may be in the library already, holding videos from a playlist
                    let mut c = match l.channel_by_id(&cref.id).or_else(|| l.channels.get(&cref.key())) {
                        Some(c) => c.clone(),
                        None => Channel::new(cref.clone(), true, follow.clone()),
                    };
                    let first = c.upstream.is_empty();
                    c.followed = true;
                    c.follow = follow.clone();
                    c.source.url = cref.url.clone();
                    if c.source.id.is_empty() {
                        c.source.id = cref.id.clone();
                    }
                    details.avatar = c.details.avatar.clone();
                    capture::fetch_avatar(&self.client, &l.channel_folder(&c.key()), &mut details, run.progress);
                    c.details = details.clone();
                    capture::merge(&mut c.upstream, fresh, c.follow.mode, first, false);
                    c.last_checked = now_iso();
                    c.last_error.clear();
                    let id = c.id();
                    l.put_channel(c)?;
                    Ok(id)
                })?;
                let mut out = self.download_wanted(&id, false, run)?;
                out.note = Some(format!("Added: {}{}", details.name, if out.downloaded.is_empty() { String::new() } else { format!(", downloaded {}", plural(out.downloaded.len(), "video")) }));
                Ok(out)
            }
            Link::Playlist { id: pl } => {
                let (meta, fresh) = capture::list_playlist(&y, &sources::playlist_url(pl), run.progress, run.stage, pl)?;
                let details = capture::playlist_from(&meta);
                let id = self.with_lib(|l| {
                    let mut p = l.playlists.get(pl).cloned().unwrap_or_else(|| Playlist::new(PlaylistRef { id: pl.clone(), url: sources::playlist_url(pl) }, follow.clone()));
                    let first = p.upstream.is_empty();
                    p.follow = follow.clone();
                    p.details = details.clone();
                    capture::merge(&mut p.upstream, fresh, p.follow.mode, first, true);
                    p.last_checked = now_iso();
                    p.last_error.clear();
                    let id = p.id();
                    l.put_playlist(p)?;
                    Ok(id)
                })?;
                let mut out = self.download_wanted(&id, false, run)?;
                out.note = Some(format!("Added: {}{}", details.title, if out.downloaded.is_empty() { String::new() } else { format!(", downloaded {}", plural(out.downloaded.len(), "video")) }));
                Ok(out)
            }
            Link::Video { id, .. } => {
                *run.item.lock().unwrap() = (id.clone(), String::new());
                let partial = self.with_lib(|l| Ok(l.partial_dir()))?;
                let d = capture::download(&y, &partial, id, follow, run.progress, run.stage)?;
                let (key, v) = self.with_lib(|l| capture::place(l, d, id, follow.quality, None))?;
                Ok(Outcome { downloaded: vec![v.title.clone()], note: Some(format!("Downloaded {} into {key}", v.title)), ..Default::default() })
            }
        }
    }

    fn run_check(&self, id: &str, missing: bool, run: &Run) -> Result<Outcome> {
        let y = self.ytdlp()?;
        let (kind, key) = split_id(id).context("not a source's id")?;
        let listed = match kind {
            Kind::Channel => {
                let c = self.with_lib(|l| l.channels.get(key).cloned().context("That channel isn't in the library any more."))?;
                let url = if c.source.url.is_empty() { format!("https://www.youtube.com/{}", c.key()) } else { c.source.url.clone() };
                match capture::list_channel(&y, &url, &c.follow, run.progress, run.stage, &c.name()) {
                    Ok((meta, fresh)) => {
                        let (_, mut details) = capture::channel_from(&meta);
                        self.with_lib(|l| {
                            let mut c = l.channels.get(key).cloned().context("gone")?;
                            details.avatar = c.details.avatar.clone();
                            capture::fetch_avatar(&self.client, &l.channel_folder(key), &mut details, run.progress);
                            if details.name.is_empty() {
                                details.name = c.details.name.clone();
                            }
                            c.details = details;
                            let first = c.upstream.is_empty();
                            let n = capture::merge(&mut c.upstream, fresh, c.follow.mode, first, false);
                            c.last_checked = now_iso();
                            c.last_error.clear();
                            l.put_channel(c)?;
                            Ok(n)
                        })?
                    }
                    Err(e) => {
                        if run.progress.stopped() {
                            return Err(e);
                        }
                        self.edit_source(id, |c, _| {
                            if let Some(c) = c {
                                c.last_checked = now_iso();
                                c.last_error = format!("{e:#}");
                            }
                            Ok(())
                        })?;
                        return Err(e);
                    }
                }
            }
            Kind::Playlist => {
                let p = self.with_lib(|l| l.playlists.get(key).cloned().context("That playlist isn't in the library any more."))?;
                let url = if p.source.url.is_empty() { sources::playlist_url(key) } else { p.source.url.clone() };
                match capture::list_playlist(&y, &url, run.progress, run.stage, &p.name()) {
                    Ok((meta, fresh)) => self.with_lib(|l| {
                        let mut p = l.playlists.get(key).cloned().context("gone")?;
                        p.details = capture::playlist_from(&meta);
                        let first = p.upstream.is_empty();
                        let n = capture::merge(&mut p.upstream, fresh, p.follow.mode, first, true);
                        p.last_checked = now_iso();
                        p.last_error.clear();
                        l.put_playlist(p)?;
                        Ok(n)
                    })?,
                    Err(e) => {
                        if run.progress.stopped() {
                            return Err(e);
                        }
                        self.edit_source(id, |_, p| {
                            if let Some(p) = p {
                                p.last_checked = now_iso();
                                p.last_error = format!("{e:#}");
                            }
                            Ok(())
                        })?;
                        return Err(e);
                    }
                }
            }
        };
        let mut out = self.download_wanted(id, missing, run)?;
        out.listed_new = listed;
        Ok(out)
    }

    /// Download what a source wants and hasn't got, one video at a time.
    fn download_wanted(&self, id: &str, retry: bool, run: &Run) -> Result<Outcome> {
        let y = self.ytdlp()?;
        let (kind, key) = split_id(id).context("not a source's id")?;
        let (list, follow, partial) = self.with_lib(|l| {
            let (up, follow, ordered) = match kind {
                Kind::Channel => {
                    let c = l.channels.get(key).context("gone")?;
                    (c.upstream.clone(), c.follow.clone(), false)
                }
                Kind::Playlist => {
                    let p = l.playlists.get(key).context("gone")?;
                    (p.upstream.clone(), p.follow.clone(), true)
                }
            };
            let mut list = capture::wanted(&up, &|v| l.has_video(v), ordered, retry);
            // "keep only the newest N": what's older isn't downloaded only to be deleted
            if kind == Kind::Channel {
                if let Some(allowed) = capture::newest_allowed(&up, &|v| l.has_video(v), follow.keep_newest) {
                    let older: Vec<String> = list.iter().filter(|x| !allowed.contains(&x.id)).map(|x| x.id.clone()).collect();
                    if !older.is_empty() {
                        let mut c = l.channels.get(key).cloned().context("gone")?;
                        for x in c.upstream.iter_mut().filter(|x| older.contains(&x.id)) {
                            x.state = State::NotWanted;
                            x.error = format!("Older than the newest {} kept", follow.keep_newest);
                        }
                        l.put_channel(c)?;
                        list.retain(|x| allowed.contains(&x.id));
                    }
                }
            }
            Ok((list, follow, l.partial_dir()))
        })?;
        let mut out = Outcome::default();
        if follow.mode == Mode::None && !retry {
            if kind == Kind::Channel {
                out.deleted = self.with_lib(|l| capture::prune(l, key))?;
            }
            return Ok(out);
        }
        let n = list.len();
        for (i, x) in list.iter().enumerate() {
            *run.item.lock().unwrap() = (x.id.clone(), format!("{} of {n}", i + 1));
            set_stage(run.stage, format!("Downloading {}", x.title));
            match capture::download(&y, &partial, &x.id, &follow, run.progress, run.stage) {
                Ok(d) => match self.with_lib(|l| capture::place(l, d, &x.id, follow.quality, Some(x))) {
                    Ok((_, v)) => {
                        out.downloaded.push(v.title);
                        let _ = self.edit_source(id, |c, p| {
                            if let Some(c) = c {
                                c.last_changed = now_iso();
                            }
                            if let Some(p) = p {
                                p.last_changed = now_iso();
                            }
                            Ok(())
                        });
                    }
                    Err(e) => out.warnings.push(format!("{}: {e:#}", x.title)),
                },
                Err(e) => {
                    if run.progress.stopped() {
                        return Err(e);
                    }
                    let why = format!("{e:#}");
                    let _ = self.set_state(id, &x.id, State::Failed, &why);
                    out.failed.push(format!("{}: {why}", x.title));
                    if why.contains(BOT_CHECK) && i + 1 < n {
                        let left = n - i - 1;
                        out.warnings.push(format!("Stopped after YouTube asked for a sign-in; {} {} for the next check.", plural(left, "video"), if left == 1 { "waits" } else { "wait" }));
                        break;
                    }
                }
            }
        }
        *run.item.lock().unwrap() = (String::new(), String::new());
        if kind == Kind::Channel {
            out.deleted = self.with_lib(|l| capture::prune(l, key))?;
        }
        Ok(out)
    }

    fn run_video(&self, id: &str, video: &str, run: &Run) -> Result<Outcome> {
        let y = self.ytdlp()?;
        let (kind, key) = split_id(id).context("not a source's id")?;
        let (x, follow, partial) = self.with_lib(|l| {
            let (up, follow) = match kind {
                Kind::Channel => {
                    let c = l.channels.get(key).context("That channel isn't in the library any more.")?;
                    (c.upstream.clone(), c.follow.clone())
                }
                Kind::Playlist => {
                    let p = l.playlists.get(key).context("That playlist isn't in the library any more.")?;
                    (p.upstream.clone(), p.follow.clone())
                }
            };
            let x = up.into_iter().find(|x| x.id == video).context("That video isn't listed for this source.")?;
            Ok((x, follow, l.partial_dir()))
        })?;
        if self.with_lib(|l| Ok(l.has_video(video)))? {
            return Ok(Outcome { note: Some(format!("{} is in the library already", x.title)), ..Default::default() });
        }
        *run.item.lock().unwrap() = (video.to_string(), String::new());
        set_stage(run.stage, format!("Downloading {}", x.title));
        match capture::download(&y, &partial, video, &follow, run.progress, run.stage) {
            Ok(d) => {
                let (_, v) = self.with_lib(|l| capture::place(l, d, video, follow.quality, Some(&x)))?;
                Ok(Outcome { downloaded: vec![v.title], ..Default::default() })
            }
            Err(e) => {
                if !run.progress.stopped() {
                    let _ = self.set_state(id, video, State::Failed, &format!("{e:#}"));
                }
                Err(e)
            }
        }
    }
}

/// The progress, stage and item of the job in hand.
struct Run<'a> {
    progress: &'a Progress,
    stage: &'a Stage,
    item: &'a Mutex<(String, String)>,
}

fn upstream_of<'a>(l: &'a Library, id: &str) -> Result<&'a Vec<Listed>> {
    let (kind, key) = split_id(id).context("not a source's id")?;
    Ok(match kind {
        Kind::Channel => &l.channels.get(key).context("That channel isn't in the library.")?.upstream,
        Kind::Playlist => &l.playlists.get(key).context("That playlist isn't in the library.")?.upstream,
    })
}

/// "3 h 20 min", "45 min", "under a minute".
fn in_words(secs: i64) -> String {
    let m = (secs.max(0) + 59) / 60;
    match (m / 60, m % 60) {
        (0, 0) => "under a minute".into(),
        (0, m) => format!("{m} min"),
        (h, _) if h >= 24 => format!("{} d {} h", h / 24, h % 24),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// [{channel, video}, …] from the page.
fn items_of(v: &Value) -> Result<Vec<(String, String)>> {
    let items: Vec<(String, String)> = v.as_array().map(|a| a.iter().filter_map(|x| Some((x["channel"].as_str()?.to_string(), x["video"].as_str()?.to_string()))).collect()).unwrap_or_default();
    if items.is_empty() {
        bail!("No videos were chosen.");
    }
    Ok(items)
}

fn tags_of(v: &Value) -> Vec<String> {
    let mut tags: Vec<String> = v.as_array().map(|t| t.iter().filter_map(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default();
    tags.dedup();
    tags
}

fn label_of_link(link: &Link) -> String {
    match link {
        Link::Channel { key, .. } => key.label(),
        Link::Playlist { id } => format!("playlist {id}"),
        Link::Video { id, .. } => format!("video {id}"),
    }
}

impl App {
    /// A file in the open library, for library:// addresses (pictures, videos).
    pub fn library_path(&self, rel: &str) -> Result<PathBuf> {
        let p = self.with_lib(|l| l.inside(&crate::util::percent_decode(rel.trim_start_matches('/'))))?;
        if !p.is_file() {
            bail!("{} isn't there", p.display());
        }
        Ok(p)
    }

    pub fn library_file(&self, rel: &str) -> Result<Vec<u8>> {
        let p = self.library_path(rel)?;
        std::fs::read(&p).with_context(|| format!("couldn't read {}", p.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sources::ChannelKey;

    #[test]
    fn link_labels_and_tags() {
        assert_eq!(label_of_link(&Link::Channel { key: ChannelKey::Handle("@x".into()), tab: None }), "@x");
        assert_eq!(tags_of(&json!(["a", " b ", "", "b"])), vec!["a".to_string(), "b".to_string()]);
        assert_eq!(in_words(3 * 3600 + 20 * 60), "3 h 20 min");
        assert_eq!(in_words(45 * 60 - 30), "45 min");
        assert_eq!(in_words(26 * 3600), "1 d 2 h");
    }

    fn finished(by: &'static str, message: &str, outcome: Option<Outcome>) -> Finished {
        let job = Job { id: 1, kind: JobKind::Check { id: "channel:@a".into() }, target: "channel:@a".into(), label: "Ahoy".into(), by, queued: String::new() };
        Finished { job, finished: String::new(), ok: true, message: message.into(), outcome }
    }

    #[test]
    fn notices_for_new_videos_and_what_needs_you() {
        let got = Outcome { downloaded: vec!["One".into(), "Two".into()], ..Default::default() };
        assert_eq!(App::notice_for(&finished("schedule", "", Some(got.clone()))), Some(("Ahoy: 2 new videos".into(), "One · Two".into())));
        assert_eq!(App::notice_for(&finished("you", "", Some(got))), None);
        let n = App::notice_for(&finished("you", BOT_CHECK, None)).unwrap();
        assert_eq!(n.0, "Ahoy needs you");
        let failed = Outcome { failed: vec![format!("Clip: {BOT_CHECK}")], ..Default::default() };
        assert!(App::notice_for(&finished("schedule", "", Some(failed))).is_some());
        assert_eq!(App::notice_for(&finished("schedule", "", Some(Outcome::default()))), None);
    }
}
