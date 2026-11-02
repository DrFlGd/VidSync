// Shared state for the interface: the platform (the backend), the library's
// videos and sources as last read, jobs, preferences, the theme, and a toast.
import { createStore } from "../lib/store.js";

export const ui = createStore({
  ready: false,
  route: { page: "videos" },
  info: {},
  videos: [],          // summaries (api "videos")
  channels: [],        // every channel, for the menu: { id, key, name, followed, creator, videos, tags }
  playlists: [],       // { id, key, name, videos }
  sources: null,       // api "sources": { channels, playlists }
  totals: { count: 0, bytes: 0, unreadable: [], next_check: null },
  downloading: null,   // the video in hand: { video, place, stage, label, target }
  query: "",
  filter: null,        // { by: "channel" | "playlist" | "creator" | "tag", value, label }
  jobs: { current: null, queued: [], recent: [] },
  dialog: null,        // { type: "add", text } | { type: "confirm", ... }
  view: "cards",
  sort: "published",
  theme: "system",
});

let platform = null;
export const ctx = () => platform;
export const api = (cmd, args = {}) => platform.api(cmd, args);

export function initState(p) {
  platform = p;
  const prefs = p.store.prefs;
  ui.set({ info: p.info || {}, view: prefs.get("view", "cards"), sort: prefs.get("sort", "published"), theme: prefs.get("theme", "system") });
  applyTheme();
  matchMedia("(prefers-color-scheme: dark)").addEventListener("change", applyTheme);
}

export function setPref(patch) {
  ui.set(patch);
  for (const [k, v] of Object.entries(patch)) platform.store.prefs.set(k, v);
  if ("theme" in patch) applyTheme();
}

function applyTheme() {
  const t = ui.get().theme;
  const dark = t === "dark" || (t === "system" && matchMedia("(prefers-color-scheme: dark)").matches);
  document.documentElement.dataset.theme = dark ? "dark" : "light";
}

let toastTimer = null;
export function toast(text, ms = 3200) {
  const el = document.getElementById("toast");
  if (!el) return;
  el.textContent = text;
  el.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { el.hidden = true; }, ms);
}

/** An error from the backend in words. */
export const errText = (e) => (typeof e === "string" ? e : e?.message || String(e));

// ---- the library and jobs, read again while anything is happening ----------

let videosSeq = 0;
export async function loadVideos() {
  const seq = ++videosSeq;
  try {
    const r = await api("videos", { query: ui.get().query });
    if (seq !== videosSeq) return;
    ui.set({ videos: r.videos, channels: r.channels, playlists: r.playlists, downloading: r.downloading, totals: { count: r.count, bytes: r.bytes, unreadable: r.unreadable || [], next_check: r.next_check } });
  } catch (e) {
    ui.set({ videos: [], channels: [], playlists: [], downloading: null, totals: { count: 0, bytes: 0, unreadable: [], next_check: null } });
  }
}

export async function loadSources() {
  try {
    ui.set({ sources: await api("sources") });
  } catch {
    ui.set({ sources: { channels: [], playlists: [] } });
  }
}

export const reloadAll = () => Promise.all([loadVideos(), loadSources()]);

const busy = (j) => !!j.current || (j.queued || []).length > 0;
let lastFinished; // undefined until the first read: what finished before isn't announced
let pollTimer = null;

export async function loadJobs() {
  try {
    const j = await api("jobs");
    const prevBusy = busy(ui.get().jobs);
    ui.set({ jobs: j });
    const newest = j.recent?.[0];
    const key = newest ? `${newest.job.id}:${newest.finished}` : null;
    // something finished (or started): the library has changed
    if (key !== lastFinished || prevBusy !== busy(j) || busy(j)) {
      if (lastFinished !== undefined && key !== lastFinished && newest) {
        announce(newest);
        // the library may be somewhere else now
        if (newest.job.type === "move") { try { ui.set({ info: { ...(await platform.refreshInfo()) }, filter: null }); } catch { /* keep */ } }
      }
      lastFinished = key;
      await reloadAll();
    }
  } catch { /* no library */ }
}

function announce(f) {
  if (!f.ok && !f.outcome) toast(`${f.job.label}: ${f.message}`, 6000);
  else if (f.outcome && (f.outcome.downloaded?.length || f.outcome.note)) toast(`${f.job.label}: ${f.message}`);
}

/** Read jobs every second while there's work, every 15 s otherwise (the schedule may start some). */
export function startPolling() {
  const tick = async () => {
    await loadJobs();
    pollTimer = setTimeout(tick, busy(ui.get().jobs) ? 1000 : 15000);
  };
  clearTimeout(pollTimer);
  tick();
}

/** After starting work: look again soon. */
export function poke() {
  clearTimeout(pollTimer);
  setTimeout(startPolling, 250);
}

// ---- choices shown in Follow, Add and Settings ------------------------------
export const MODES = [["new", "New videos from now on"], ["all", "Every video"], ["none", "Only list them (nothing downloaded)"]];
export const modeLabel = (m) => (MODES.find(([k]) => k === m) || [m, m])[1];
export const QUALITIES = [["best", "Best available"], ["1080p", "Up to 1080p"], ["720p", "Up to 720p"], ["audio", "Audio only"]];
export const qualityLabel = (q) => (QUALITIES.find(([k]) => k === q) || [q, q])[1];
export const SUBS = [["", "None"], ["en.*,en", "English"], ["all", "Every language"]];
export const EVERY = [[0, "Only by hand"], [1, "Every hour"], [3, "Every 3 hours"], [6, "Every 6 hours"], [12, "Every 12 hours"], [24, "Every day"], [168, "Every week"]];
export const DEFAULT_FOLLOW = { mode: "new", tabs: { videos: true, shorts: false, streams: false }, quality: "best", subtitles: "", every_hours: null };
