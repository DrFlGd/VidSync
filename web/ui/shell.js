// Mounts the interface: the menu on the left (with the creators, channels,
// playlists and tags that narrow the videos), the page for the current route,
// the status bar and dialogs. Also the keys and pasting a link anywhere.
import { html, render, useMemo } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, ctx, setPref, loadVideos } from "./state.js";
import { Icon, Mark } from "./icons.js";
import { VideosPage } from "./videos.js";
import { SourcesPage } from "./sources.js";
import { SourcePage } from "./source.js";
import { VideoPage } from "./video.js";
import { AuditPage } from "./audit.js";
import { ActivityPage } from "./activity.js";
import { SettingsPage } from "./settings.js";
import { AboutPage } from "./about.js";
import { CreatorPage } from "./creator.js";
import { Dialogs, openAdd } from "./dialogs.js";
import { bytes, ago, plural } from "../lib/format.js";

function NavLink({ href, icon, label, count, active, warn }) {
  return html`<li><a class=${"nav-link" + (active ? " active" : "")} href=${href} aria-current=${active ? "page" : null} title=${label}>
    ${icon}<span class="label">${label}</span><span class=${"nav-count" + (warn ? " warn" : "")}>${count ?? ""}</span></a></li>`;
}

function FilterLink({ by, value, label, count }) {
  const f = useStore(ui, (s) => s.filter);
  const page = useStore(ui, (s) => s.route.page);
  const active = page === "videos" && f?.by === by && f?.value === value;
  const pick = () => { ui.set({ filter: active ? null : { by, value, label } }); if (page !== "videos") location.hash = "#/"; };
  return html`<li><button type="button" class=${"nav-link" + (active ? " active" : "")} aria-pressed=${active ? "true" : "false"} onClick=${pick} title=${label}>
    <span class="label">${label}</span><span class="nav-count">${count}</span></button></li>`;
}

/** A saved view in the menu: applies its filter, search and order; × forgets it. */
function ViewLink({ v }) {
  const f = useStore(ui, (s) => s.filter);
  const query = useStore(ui, (s) => s.query);
  const page = useStore(ui, (s) => s.route.page);
  const active = page === "videos" && query === (v.query || "") && JSON.stringify(f || null) === JSON.stringify(v.filter || null);
  const apply = () => {
    ui.set({ filter: v.filter || null, query: v.query || "" });
    if (v.sort) setPref({ sort: v.sort });
    loadVideos();
    if (page !== "videos") location.hash = "#/";
  };
  const forget = (e) => { e.stopPropagation(); setPref({ views: ui.get().views.filter((x) => x.name !== v.name) }); };
  return html`<li class="view-link"><button type="button" class=${"nav-link" + (active ? " active" : "")} data-view=${v.name} aria-pressed=${active ? "true" : "false"} onClick=${apply} title=${v.name}>
    <span class="label">${v.name}</span></button><button type="button" class="ghost forget" title=${`Forget “${v.name}”`} aria-label=${`Forget ${v.name}`} onClick=${forget}>×</button></li>`;
}

/** Counts of each value across the videos, most first. */
function tally(list, get) {
  const m = new Map();
  for (const r of list) for (const v of [].concat(get(r) || [])) if (v) m.set(v, (m.get(v) || 0) + 1);
  return [...m.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]));
}

function Sidebar() {
  const route = useStore(ui, (s) => s.route);
  const videos = useStore(ui, (s) => s.videos);
  const channels = useStore(ui, (s) => s.channels);
  const playlists = useStore(ui, (s) => s.playlists);
  const sources = useStore(ui, (s) => s.sources);
  const count = useStore(ui, (s) => s.totals.count);
  const jobs = useStore(ui, (s) => s.jobs);
  const query = useStore(ui, (s) => s.query);
  const theme = useStore(ui, (s) => s.theme);
  const views = useStore(ui, (s) => s.views);
  const working = (jobs.current ? 1 : 0) + (jobs.queued?.length || 0);
  const nsources = sources ? sources.channels.filter((c) => c.followed).length + sources.playlists.length : 0;
  const missing = sources ? [...sources.channels, ...sources.playlists].reduce((n, s) => n + (s.audit?.missing || 0) + (s.audit?.failed || 0), 0) : 0;
  // filters count the whole library, not the search
  const groups = useMemo(() => query ? null : {
    creators: tally(channels, (c) => c.creator).map(([v, n]) => [v, channels.filter((c) => c.creator === v).reduce((s, c) => s + c.videos, 0)]),
    channels: channels.filter((c) => c.videos > 0).sort((a, b) => b.videos - a.videos || a.name.localeCompare(b.name)),
    playlists: playlists.filter((p) => p.videos > 0),
    tags: tally(videos, (v) => v.tags),
    unwatched: videos.filter((v) => !v.watched).length,
  }, [videos, channels, playlists, query]);
  const nextTheme = { system: "light", light: "dark", dark: "system" }[theme] || "system";
  const themeIcon = theme === "system" ? Icon.auto(16) : theme === "light" ? Icon.sun(16) : Icon.moon(16);
  return html`<nav class="sidebar" aria-label="Menu">
    <a class="brand" href="#/"><${Mark} /><span>Vid Sync</span></a>
    <ul class="nav">
      <${NavLink} href="#/" icon=${Icon.video(16)} label="Videos" count=${count || ""} active=${route.page === "videos" || route.page === "video"} />
      <${NavLink} href="#/sources" icon=${Icon.sources(16)} label="Sources" count=${nsources || ""} active=${route.page === "sources" || route.page === "source" || route.page === "creator"} />
      <${NavLink} href="#/audit" icon=${Icon.audit(16)} label="Audit" count=${missing || ""} warn=${missing > 0} active=${route.page === "audit"} />
      <${NavLink} href="#/activity" icon=${Icon.activity(16)} label="Activity" count=${working || ""} active=${route.page === "activity"} />
      <${NavLink} href="#/settings" icon=${Icon.cog(16)} label="Settings" active=${route.page === "settings"} />
      <${NavLink} href="#/about" icon=${Icon.info(16)} label="About" active=${route.page === "about"} />
    </ul>
    ${views?.length > 0 && html`<div class="filters saved-views"><div class="nav-head">Saved views</div><ul class="nav">${views.map((v) => html`<${ViewLink} key=${v.name} v=${v} />`)}</ul></div>`}
    ${groups && html`<div class="filters">
      ${videos.length > 0 && html`<div class="nav-head">Watching</div><ul class="nav"><${FilterLink} by="unwatched" value="1" label="Not watched yet" count=${groups.unwatched} /></ul>`}
      ${groups.creators.length > 0 && html`<div class="nav-head">Creators</div><ul class="nav">${groups.creators.map(([v, n]) => html`<${FilterLink} key=${v} by="creator" value=${v} label=${v} count=${n} />`)}</ul>`}
      ${groups.channels.length > 0 && html`<div class="nav-head">Channels</div><ul class="nav">${groups.channels.slice(0, 14).map((c) => html`<${FilterLink} key=${c.key} by="channel" value=${c.key} label=${c.name} count=${c.videos} />`)}</ul>`}
      ${groups.playlists.length > 0 && html`<div class="nav-head">Playlists</div><ul class="nav">${groups.playlists.slice(0, 10).map((p) => html`<${FilterLink} key=${p.id} by="playlist" value=${p.id} label=${p.name} count=${p.videos} />`)}</ul>`}
      ${groups.tags.length > 0 && html`<div class="nav-head">Tags</div><ul class="nav">${groups.tags.slice(0, 12).map(([v, n]) => html`<${FilterLink} key=${v} by="tag" value=${v} label=${v} count=${n} />`)}</ul>`}
    </div>`}
    <ul class="nav nav-foot">
      <li><button type="button" class="nav-link" id="theme-toggle" title=${`Theme: ${theme}. Click for ${nextTheme}.`} onClick=${() => setPref({ theme: nextTheme })}>
        ${themeIcon}<span class="label">Theme: ${theme[0].toUpperCase() + theme.slice(1)}</span></button></li>
    </ul>
  </nav>`;
}

function Main() {
  const route = useStore(ui, (s) => s.route);
  const info = useStore(ui, (s) => s.info);
  if (!info.library && route.page !== "settings" && route.page !== "about") {
    return html`<div class="page"><div class="empty-state">
      <h2>No library is open</h2>
      <p class="notice error">${info.library_error || "The library folder couldn't be opened."}</p>
      <div><a class="button primary" href="#/settings">Choose a library folder…</a></div>
    </div></div>`;
  }
  if (route.page === "source") return html`<${SourcePage} key=${route.id} id=${route.id} tab=${route.tab} />`;
  if (route.page === "video") return html`<${VideoPage} key=${route.channel + "/" + route.video} channel=${route.channel} video=${route.video} t=${route.t} />`;
  if (route.page === "sources") return html`<${SourcesPage} />`;
  if (route.page === "creator") return html`<${CreatorPage} key=${route.name} name=${route.name} />`;
  if (route.page === "audit") return html`<${AuditPage} />`;
  if (route.page === "activity") return html`<${ActivityPage} />`;
  if (route.page === "settings") return html`<${SettingsPage} />`;
  if (route.page === "about") return html`<${AboutPage} />`;
  return html`<${VideosPage} />`;
}

function StatusBar() {
  const s = useStore(ui, (st) => ({ info: st.info, totals: st.totals, jobs: st.jobs, ready: st.ready }));
  if (!s.ready) return null;
  const cur = s.jobs.current;
  const queued = s.jobs.queued?.length || 0;
  const pct = cur && cur.total > 0 ? Math.min(100, (cur.done / cur.total) * 100) : null;
  const lib = s.info.library;
  return html`<div class="statusbar-inner">
    <span class="lib" title=${lib?.path || ""}>${lib ? lib.path : "No library"}</span>
    <span>${plural(s.totals.count, "video")}</span>
    <span>${bytes(s.totals.bytes || 0)}</span>
    ${s.totals.next_check && html`<span title="The next scheduled check">Next check ${ago(s.totals.next_check)}</span>`}
    <span class="job">${cur
      ? html`<div class=${"meter" + (pct === null ? " busy" : "")}><i style=${pct === null ? "" : `width:${pct}%`}></i></div>
          <a href="#/activity">${cur.job.label}: ${cur.stage}${cur.place ? ` (${cur.place})` : ""}${pct !== null ? ` · ${Math.round(pct)}%${cur.speed ? ` at ${bytes(cur.speed)}/s` : ""}` : cur.done ? ` (${bytes(cur.done)})` : ""}${queued ? ` · ${queued} waiting` : ""}</a>`
      : html`<span class="muted">Idle</span>`}</span>
  </div>`;
}

export function mountShell() {
  const mount = (id, C) => { const el = document.getElementById(id); if (el) render(html`<${C} />`, el); };
  mount("sidebar-root", Sidebar);
  mount("main-root", Main);
  mount("statusbar", StatusBar);
  mount("dialog-root", Dialogs);
  window.__vidsync = { ui, ctx }; // for tests
  addEventListener("keydown", (e) => {
    const typing = e.target.closest?.("input, textarea, select, [contenteditable]");
    if (e.key === "Escape" && ui.get().dialog) { ui.get().dialog.resolve?.(false); ui.set({ dialog: null }); return; }
    if (ui.get().dialog || typing) return;
    if ((e.key === "/" || (e.key === "k" && (e.ctrlKey || e.metaKey)))) {
      const s = document.getElementById("search");
      if (s) { e.preventDefault(); s.focus(); s.select(); }
    } else if (e.key === "n" && !e.ctrlKey && !e.metaKey && !e.altKey && ui.get().info.library) {
      e.preventDefault();
      openAdd("");
    }
  });
  // a link pasted anywhere but a text box opens Add with it
  addEventListener("paste", (e) => {
    if (e.target.closest?.("input, textarea") || ui.get().dialog || !ui.get().info.library) return;
    const text = e.clipboardData?.getData("text") || "";
    if (/^\s*(https?:\/\/|youtu|@)\S+/m.test(text)) { e.preventDefault(); openAdd(text.trim()); }
  });
}
