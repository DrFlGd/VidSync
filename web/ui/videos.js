// The Videos page: every kept video as a card or a row, with search, the
// filters from the menu, sorting, Continue watching, Check all and Add. Videos
// can be chosen (Ctrl or Shift and click, or their tick box) to mark watched,
// tag or delete them together.
import { html, useEffect, useMemo, useRef, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, toast, errText, setPref, loadVideos, reloadAll } from "./state.js";
import { Icon } from "./icons.js";
import { openAdd, confirmDialog } from "./dialogs.js";
import { checkAll } from "./actions.js";
import { bytes, ago, when, day, duration, res, plural } from "../lib/format.js";

const SORTS = [
  ["published", "Published"],
  ["downloaded", "Downloaded"],
  ["title", "Title"],
  ["channel", "Channel"],
  ["duration", "Duration"],
  ["size", "Size"],
];

function sorter(by) {
  const t = (s) => (s ? Date.parse(s) || 0 : 0);
  switch (by) {
    case "title": return (a, b) => a.title.localeCompare(b.title, undefined, { sensitivity: "base" });
    case "channel": return (a, b) => a.channel_name.localeCompare(b.channel_name, undefined, { sensitivity: "base" }) || (b.upload_date || "").localeCompare(a.upload_date || "");
    case "downloaded": return (a, b) => t(b.downloaded) - t(a.downloaded);
    case "duration": return (a, b) => b.duration - a.duration;
    case "size": return (a, b) => b.bytes - a.bytes;
    default: return (a, b) => (b.upload_date || "").localeCompare(a.upload_date || "") || t(b.downloaded) - t(a.downloaded);
  }
}

function matches(v, f) {
  if (!f) return true;
  switch (f.by) {
    case "channel": return v.channel === f.value;
    case "playlist": return v.playlists.includes(f.value);
    case "creator": return v.creator === f.value;
    case "tag": return v.tags.includes(f.value);
    case "unwatched": return !v.watched;
    default: return true;
  }
}

export const videoHref = (v) => `#/video/${encodeURIComponent(v.channel)}/${v.id}`;
const keyOf = (v) => `${v.channel}/${v.id}`;

// ---- choosing several videos ---------------------------------------------------
let anchor = null; // the last video clicked, for Shift ranges

/** A click on a card or row: Ctrl/⌘ adds or takes it out, Shift takes the run from the last one; a plain click opens it (or, while some are chosen, adds it). */
function clickVideo(e, v, list) {
  const sel = ui.get().selected;
  const k = keyOf(v);
  if (e.shiftKey && anchor && list) {
    const keys = list.map(keyOf);
    const [a, b] = [keys.indexOf(anchor), keys.indexOf(k)].sort((x, y) => x - y);
    if (a >= 0 && b >= 0) { ui.set({ selected: [...new Set([...sel, ...keys.slice(a, b + 1)])] }); e.preventDefault(); return; }
  }
  if (e.ctrlKey || e.metaKey || sel.length > 0) {
    ui.set({ selected: sel.includes(k) ? sel.filter((x) => x !== k) : [...sel, k] });
    anchor = k;
    e.preventDefault();
    return;
  }
  anchor = k;
  location.hash = videoHref(v);
}

function Pick({ v, list }) {
  const on = useStore(ui, (s) => s.selected.includes(keyOf(v)));
  return html`<label class="pick" title="Choose" onClick=${(e) => e.stopPropagation()}>
    <input type="checkbox" checked=${on} aria-label=${`Choose ${v.title}`} onClick=${(e) => { e.stopPropagation(); clickVideo({ ctrlKey: true, shiftKey: e.shiftKey, preventDefault() {} }, v, list); }} /></label>`;
}

/** The bar shown while videos are chosen: mark watched, tag, delete. */
export function SelectionBar({ videos }) {
  const sel = useStore(ui, (s) => s.selected);
  const [tag, setTag] = useState("");
  const chosen = videos.filter((v) => sel.includes(keyOf(v)));
  useEffect(() => {
    const esc = (e) => { if (e.key === "Escape" && ui.get().selected.length && !ui.get().dialog) ui.set({ selected: [] }); };
    addEventListener("keydown", esc);
    return () => removeEventListener("keydown", esc);
  }, []);
  if (!chosen.length) return null;
  const items = chosen.map((v) => ({ channel: v.channel, video: v.id }));
  const run = async (cmd, args, done) => {
    try { await api(cmd, { items, ...args }); await reloadAll(); toast(done); } catch (e) { toast(errText(e), 6000); }
  };
  const tags = [...new Set(chosen.flatMap((v) => v.tags))].sort();
  const addTag = (e) => { e.preventDefault(); const t = tag.trim(); if (t) { run("videos_tag", { add: [t] }, `Tagged ${plural(chosen.length, "video")} “${t}”`); setTag(""); } };
  const del = async () => {
    const size = chosen.reduce((s, v) => s + v.bytes, 0);
    if (!(await confirmDialog({ title: `Delete ${plural(chosen.length, "video")}?`, body: `${chosen.length === 1 ? `${chosen[0].title} (${bytes(size)}) is` : `${plural(chosen.length, "video")} (${bytes(size)}) are`} deleted from the library folder. Their channels still list them in the audit, so they can be downloaded again.`, ok: "Delete them" }))) return;
    await run("videos_delete", {}, `Deleted ${plural(chosen.length, "video")}`);
    ui.set({ selected: [] });
  };
  const allWatched = chosen.every((v) => v.watched);
  return html`<div class="selection-bar" id="selection-bar" role="toolbar" aria-label="Chosen videos">
    <b>${plural(chosen.length, "video")} chosen</b>
    <button type="button" class="small" id="sel-watched" onClick=${() => run("videos_watched", { watched: !allWatched }, allWatched ? "Marked not watched" : "Marked watched")}>${Icon.eye(13)} ${allWatched ? "Mark not watched" : "Mark watched"}</button>
    <form class="row" onSubmit=${addTag}><input type="text" id="sel-tag" placeholder="Tag" value=${tag} onInput=${(e) => setTag(e.target.value)} style="width:120px" aria-label="Tag to add" />
      <button type="submit" class="small" id="sel-tag-add">Add tag</button></form>
    ${tags.length > 0 && html`<select class="small" id="sel-untag" value="" aria-label="Remove a tag" onChange=${(e) => e.target.value && run("videos_tag", { remove: [e.target.value] }, `Removed “${e.target.value}”`)}>
      <option value="">Remove a tag…</option>${tags.map((t) => html`<option value=${t}>${t}</option>`)}</select>`}
    <button type="button" class="small danger" id="sel-delete" onClick=${del}>${Icon.trash(13)} Delete…</button>
    <span class="spacer"></span>
    <button type="button" class="small ghost" id="sel-all" onClick=${() => ui.set({ selected: [...new Set([...sel, ...videos.map(keyOf)])] })}>Choose all ${videos.length}</button>
    <button type="button" class="small ghost" id="sel-clear" title="Esc" onClick=${() => ui.set({ selected: [] })}>${Icon.x(12)} Clear</button>
  </div>`;
}

/** A video's picture with its length in the corner, how far it was watched, and a mark once watched. */
export function Thumb({ v, big = false }) {
  const pct = !v.watched && v.position > 0 && v.duration > 0 ? Math.min(100, (v.position / v.duration) * 100) : 0;
  return html`<div class=${"thumb" + (big ? " big" : "")}>
    ${v.thumbnail_url ? html`<img src=${v.thumbnail_url} alt="" loading="lazy" />` : html`<div class="thumb-none">${Icon.video(28)}</div>`}
    ${v.duration > 0 && html`<span class="dur">${duration(v.duration)}</span>`}
    ${res(v) && html`<span class="res">${res(v)}</span>`}
    ${v.watched && html`<span class="seen" title="Watched">${Icon.check(11)} Watched</span>`}
    ${pct > 0 && html`<span class="played" title=${`Watched to ${duration(v.position)}`}><i style=${`width:${pct}%`}></i></span>`}
  </div>`;
}

/** A video's card. With `pick`, it can be chosen (the page shows the SelectionBar). */
export function VideoCard({ v, showChannel = true, list = null, pick = false }) {
  const chosen = useStore(ui, (s) => pick && s.selected.includes(keyOf(v)));
  const click = (e) => (pick ? clickVideo(e, v, list) : (location.hash = videoHref(v)));
  return html`<article class=${"card video" + (v.watched ? " watched" : "") + (chosen ? " chosen" : "")} tabindex="0" data-id=${v.id} aria-selected=${chosen ? "true" : null}
      onClick=${click} onKeyDown=${(e) => e.key === "Enter" && click(e)}>
    ${pick && html`<${Pick} v=${v} list=${list} />`}
    <${Thumb} v=${v} />
    <div class="card-title"><h3 title=${v.title}>${v.title}</h3>
      ${showChannel && html`<div class="card-sub"><a href=${`#/source/${encodeURIComponent(v.source)}`} onClick=${(e) => e.stopPropagation()}>${v.channel_name}</a></div>`}</div>
    <div class="card-foot">
      <span class="when" title=${`Published ${day(v.upload_date)}\nDownloaded ${when(v.downloaded)}`}>${v.upload_date ? day(v.upload_date) : "Date unknown"}</span>
      <span class="mono muted">${bytes(v.bytes)}</span>
    </div>
  </article>`;
}

function PendingCard({ d }) {
  return html`<article class="card video pending" data-pending=${d.video}>
    <div class="thumb"><div class="thumb-none">${Icon.download(28)}</div></div>
    <div class="card-title"><h3>${d.stage.replace(/^Downloading /, "")}</h3><div class="card-sub">${d.label}${d.place ? ` · ${d.place}` : ""}</div></div>
    <p class="card-desc muted"><span class="badge working">Downloading…</span></p>
  </article>`;
}

export function Rows({ list, sort, showChannel = true }) {
  const head = (key, label, cls = "") => html`<th class=${cls}>${key ? html`<button type="button" onClick=${() => setPref({ sort: key })}>${label}${sort === key ? " ▾" : ""}</button>` : label}</th>`;
  return html`<table class="list">
    <thead><tr>${head("title", "Video")}${showChannel && head("channel", "Channel")}${head("published", "Published")}${head("duration", "Length", "num")}${head(null, "Quality", "num")}${head("size", "Size", "num")}</tr></thead>
    <tbody>${list.map((v) => html`<${Row} key=${v.channel + v.id} v=${v} list=${list} showChannel=${showChannel} />`)}</tbody>
  </table>`;
}

function Row({ v, list, showChannel }) {
  const chosen = useStore(ui, (s) => s.selected.includes(keyOf(v)));
  return html`<tr class=${"row" + (v.watched ? " watched" : "") + (chosen ? " chosen" : "")} data-id=${v.id} aria-selected=${chosen ? "true" : null} onClick=${(e) => clickVideo(e, v, list)}>
      <td><div class="name">${v.watched ? html`<span class="seen-dot" title="Watched">✓</span> ` : ""}${v.title}</div>${v.tags.length > 0 && html`<div class="sub">${v.tags.join(", ")}</div>`}</td>
      ${showChannel && html`<td>${v.channel_name}</td>`}
      <td class="date" title=${when(v.downloaded)}>${day(v.upload_date) || "?"}</td>
      <td class="num">${duration(v.duration)}</td>
      <td class="num">${res(v)}</td>
      <td class="num">${bytes(v.bytes)}</td>
    </tr>`;
}

function EmptyLibrary() {
  const [text, setText] = useState("");
  const info = useStore(ui, (s) => s.info);
  return html`<div class="empty-state">
    <h2>Add your first channel</h2>
    <p class="muted">Paste a link to a YouTube channel, playlist or video. Vid Sync lists it, downloads its videos with yt-dlp, and keeps checking for new ones on a schedule. The audit shows what's upstream that you don't have.</p>
    ${info.ytdlp === false && html`<p class="notice">yt-dlp wasn't found on this computer. <a href="#/settings">Settings</a> can download it.</p>`}
    <form onSubmit=${(e) => { e.preventDefault(); openAdd(text); }}>
      <input id="first-link" type="text" placeholder="https://www.youtube.com/@channel" value=${text} onInput=${(e) => setText(e.target.value)} aria-label="Link to a channel, playlist or video" />
      <button type="submit" class="primary">${Icon.plus(15)} Add</button>
    </form>
  </div>`;
}

export function VideosPage() {
  const videos = useStore(ui, (s) => s.videos);
  const totals = useStore(ui, (s) => s.totals);
  const filter = useStore(ui, (s) => s.filter);
  const view = useStore(ui, (s) => s.view);
  const sort = useStore(ui, (s) => s.sort);
  const downloading = useStore(ui, (s) => s.downloading);
  const info = useStore(ui, (s) => s.info);
  const [query, setQuery] = useState(ui.get().query);
  // the search runs in the backend, a moment after typing stops; leaving the page
  // mid-pause applies what was typed, so the list isn't stale when it's shown again
  const typed = useRef(query);
  typed.current = query;
  useEffect(() => {
    if (query === ui.get().query) return;
    const t = setTimeout(() => { ui.set({ query }); loadVideos(); }, 200);
    return () => clearTimeout(t);
  }, [query]);
  useEffect(() => () => { if (typed.current !== ui.get().query) { ui.set({ query: typed.current }); loadVideos(); } }, []);
  const list = useMemo(() => videos.filter((v) => matches(v, filter)).sort(sorter(sort)), [videos, filter, sort]);
  // started and not finished, most recently played first
  const going = useMemo(() => videos.filter((v) => !v.watched && v.position > 0).sort((a, b) => (b.played || "").localeCompare(a.played || "")).slice(0, 6), [videos]);
  const empty = totals.count === 0 && !downloading;
  const bytesShown = list.reduce((s, v) => s + v.bytes, 0);
  return html`<div class="cmdbar">
      <h1>Videos</h1>
      <span class="count">${filter || query ? `${list.length} of ${totals.count}` : totals.count}${list.length ? ` · ${bytes(bytesShown)}` : ""}</span>
      ${filter && html`<button type="button" class="chip" onClick=${() => ui.set({ filter: null })} title="Show everything">${filter.label || filter.value} ${Icon.x(11)}</button>`}
      <label class="search"><span class="visually-hidden">Search</span>${Icon.search(15)}
        <input id="search" type="search" placeholder="Search titles, channels, tags…" value=${query} onInput=${(e) => setQuery(e.target.value)} /></label>
      <span class="spacer"></span>
      <select aria-label="Sort by" value=${sort} onChange=${(e) => setPref({ sort: e.target.value })}>
        ${SORTS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}</select>
      <div class="seg" role="group" aria-label="View">
        <button type="button" aria-pressed=${view === "cards" ? "true" : "false"} title="Cards" onClick=${() => setPref({ view: "cards" })}>${Icon.grid(16)}</button>
        <button type="button" aria-pressed=${view === "list" ? "true" : "false"} title="List" onClick=${() => setPref({ view: "list" })}>${Icon.list(16)}</button>
      </div>
      <button type="button" id="check-all" onClick=${checkAll} title="Check every source for new videos now">${Icon.refresh(15)} Check all</button>
      <button type="button" class="primary" id="add" onClick=${() => openAdd("")} title="Add channels, playlists or videos by their links (N)">${Icon.plus(15)} Add</button>
    </div>
    <div class="page">
      ${info.ytdlp === false && !empty && html`<p class="notice">yt-dlp wasn't found on this computer, so nothing can be downloaded. <a href="#/settings">Settings</a> can download it.</p>`}
      ${totals.unreadable.length > 0 && html`<p class="notice">${plural(totals.unreadable.length, "folder")} in the library couldn't be read: ${totals.unreadable.map((u) => `${u.folder} (${u.error})`).join("; ")}</p>`}
      ${empty ? html`<${EmptyLibrary} />`
        : list.length === 0 && !downloading ? html`<p class="muted">Nothing matches${query ? ` “${query}”` : ""}${filter ? ` in ${filter.label || filter.value}` : ""}.</p>`
        : view === "list" ? html`<${Rows} list=${list} sort=${sort} />`
        : html`${going.length > 0 && !filter && !query && html`<h2 class="section-head">Continue watching <span class="n">${going.length}</span></h2>
            <div class="cards videos continue" id="continue">${going.map((v) => html`<${VideoCard} key=${"c" + v.channel + v.id} v=${v} pick=${true} />`)}</div>
            <h2 class="section-head">Everything</h2>`}
          <div class="cards videos" id="all-videos">${downloading && !filter && !query && html`<${PendingCard} d=${downloading} />`}${list.map((v) => html`<${VideoCard} key=${v.channel + v.id} v=${v} list=${list} pick=${true} />`)}</div>`}
    </div>
    <${SelectionBar} videos=${list} />`;
}
