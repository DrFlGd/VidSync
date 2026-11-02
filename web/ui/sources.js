// The Sources page: the channels and playlists followed, as cards with what the
// audit says of each, and the channels that only hold videos.
import { html, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui } from "./state.js";
import { Icon } from "./icons.js";
import { openAdd } from "./dialogs.js";
import { check, checkAll, downloadMissing } from "./actions.js";
import { bytes, ago, when, count, plural } from "../lib/format.js";

export const sourceHref = (s) => `#/source/${encodeURIComponent(s.id)}`;

export function Avatar({ s, size = 40 }) {
  const url = s.kind === "playlist" ? s.thumbnail_url : s.avatar_url;
  return html`<span class=${"avatar" + (s.kind === "playlist" ? " playlist" : "")} style=${`width:${size}px;height:${size}px`}>
    ${url ? html`<img src=${url} alt="" />` : (s.kind === "playlist" ? Icon.playlist(Math.round(size * 0.5)) : Icon.channel(Math.round(size * 0.5)))}</span>`;
}

export const statusBadge = (s) => {
  if (s.status === "working") return html`<span class="badge working">Checking…</span>`;
  if (s.status === "queued") return html`<span class="badge queued">Waiting</span>`;
  if (s.status === "error") return html`<span class="badge error" title=${s.error}>Check failed</span>`;
  return null;
};

/** The audit's counts as short badges. */
export function AuditBadges({ a, kind }) {
  if (!a) return null;
  return html`<div class="chips audit-chips">
    <span class="chip plain" title="Videos listed upstream">${a.listed} listed</span>
    <span class="chip ok" title="Listed and kept">${a.kept} kept</span>
    ${a.missing > 0 && html`<span class="chip missing" title="Wanted and not downloaded yet">${a.missing} missing</span>`}
    ${a.failed > 0 && html`<span class="chip bad" title="Couldn't be downloaded">${a.failed} failed</span>`}
    ${a.not_wanted > 0 && html`<span class="chip plain muted" title=${kind === "playlist" ? "Not downloaded by choice" : "Published before the channel was added, or outside its follow settings"}>${a.not_wanted} not wanted</span>`}
    ${a.skipped > 0 && html`<span class="chip plain muted" title="Skipped by you">${a.skipped} skipped</span>`}
    ${a.waiting > 0 && html`<span class="chip plain muted" title="Live now or upcoming">${a.waiting} live</span>`}
    ${a.gone > 0 && html`<span class="chip plain muted" title="No longer listed on YouTube">${a.gone} gone</span>`}
  </div>`;
}

export function SourceCard({ s }) {
  const open = () => { location.hash = sourceHref(s); };
  const a = s.audit;
  return html`<article class="card source" tabindex="0" data-id=${s.id} onClick=${open} onKeyDown=${(e) => e.key === "Enter" && open()}>
    <div class="card-top">
      <${Avatar} s=${s} size=${44} />
      <div class="card-title"><h3>${s.name}</h3>
        <div class="card-sub">${s.kind === "playlist" ? `playlist · ${s.channel || "?"}` : (s.handle || s.key)}${s.kind === "channel" && s.subscribers ? ` · ${count(s.subscribers)} subscribers` : ""}</div></div>
      ${statusBadge(s)}
    </div>
    <p class="card-desc">${s.description || html`<span class="muted">No description</span>`}</p>
    <${AuditBadges} a=${a} kind=${s.kind} />
    <div class="card-foot">
      <span class="when" title=${`Last checked ${when(s.last_checked) || "never"}\nNext check ${when(s.due) || "by hand"}`}>${s.last_checked ? `Checked ${ago(s.last_checked)}` : "Not checked yet"}${s.followed && s.follow?.mode === "none" ? " · list only" : ""}</span>
      <span class="mono muted">${bytes(s.bytes)}</span>
      ${a && (a.missing > 0 || a.failed > 0) && html`<button type="button" class="small" title="Download every missing video" onClick=${(e) => { e.stopPropagation(); downloadMissing(s.id); }}>${Icon.download(13)} Get ${a.missing + a.failed}</button>`}
      <button type="button" class="ghost small" title="Check for new videos now" disabled=${s.status === "working" || s.status === "queued"} onClick=${(e) => { e.stopPropagation(); check(s.id); }}>${Icon.refresh(13)}</button>
    </div>
  </article>`;
}

export function SourcesPage() {
  const sources = useStore(ui, (s) => s.sources);
  const jobs = useStore(ui, (s) => s.jobs);
  const [showOthers, setShowOthers] = useState(false);
  if (!sources) return html`<div class="page"><p class="muted">Reading…</p></div>`;
  const followed = sources.channels.filter((c) => c.followed);
  const others = sources.channels.filter((c) => !c.followed);
  const adding = [jobs.current?.job, ...(jobs.queued || [])].filter((j) => j && j.type === "add" && j.link?.kind !== "video");
  const total = followed.length + sources.playlists.length;
  // followed channels under their creator (channels without one last, as "Other channels")
  const byCreator = new Map();
  for (const c of followed) byCreator.set(c.creator || "", [...(byCreator.get(c.creator || "") || []), c]);
  const groups = [...byCreator.entries()].sort(([a], [b]) => (a === "") - (b === "") || a.localeCompare(b));
  if (!byCreator.has("") && adding.length) groups.push(["", []]);
  return html`<div class="cmdbar">
      <h1>Sources</h1>
      <span class="count">${plural(followed.length, "channel")} · ${plural(sources.playlists.length, "playlist")}</span>
      <span class="spacer"></span>
      <button type="button" id="check-all" onClick=${checkAll} disabled=${!total} title="Check every source for new videos now">${Icon.refresh(15)} Check all</button>
      <button type="button" class="primary" id="add" onClick=${() => openAdd("")} title="Add channels, playlists or videos by their links (N)">${Icon.plus(15)} Add</button>
    </div>
    <div class="page">
      ${total === 0 && adding.length === 0 && html`<div class="empty-state"><h2>Nothing is followed yet</h2>
        <p class="muted">Add a channel or a playlist by its link. It's listed, its new videos are downloaded on the library's schedule, and the audit shows what's upstream that you don't have.</p>
        <div><button type="button" class="primary" onClick=${() => openAdd("")}>${Icon.plus(15)} Add a channel or playlist</button></div></div>`}
      ${(followed.length > 0 || adding.length > 0) && html`<h2 class="section-head">Channels <span class="n">${followed.length}</span></h2>
        ${groups.map(([creator, list]) => html`<div key=${creator} class="creator-group">
          ${groups.length > 1 || creator ? html`<h3 class="creator-head" data-creator=${creator}>${creator ? html`<a href=${`#/creator/${encodeURIComponent(creator)}`} title="This creator's page: their channels and newest videos">${Icon.creator(14)} ${creator}</a>` : "Other channels"}<span class="n">${list.length}</span></h3>` : null}
          <div class="cards sources">
            ${!creator && adding.map((j) => html`<article class="card source pending" key=${"p" + j.id}><div class="card-top"><span class="avatar">${Icon.channel(22)}</span><div class="card-title"><h3>${j.label}</h3><div class="card-sub">${jobs.current?.job?.id === j.id ? jobs.current.stage : "Waiting its turn"}</div></div><span class="badge working">Adding…</span></div></article>`)}
            ${list.map((c) => html`<${SourceCard} key=${c.id} s=${c} />`)}
          </div></div>`)}`}
      ${sources.playlists.length > 0 && html`<h2 class="section-head" style="margin-top:24px">Playlists <span class="n">${sources.playlists.length}</span></h2>
        <div class="cards sources">${sources.playlists.map((p) => html`<${SourceCard} key=${p.id} s=${p} />`)}</div>`}
      ${others.length > 0 && html`<h2 class="section-head" style="margin-top:24px">Channels with videos, not followed <span class="n">${others.length}</span>
          <button type="button" class="ghost small" id="show-others" onClick=${() => setShowOthers(!showOthers)}>${showOthers ? "Hide" : "Show"}</button></h2>
        <p class="hint">Videos from playlists and pasted links live in their channel's folder. Open a channel and press Follow to start checking it for new videos.</p>
        ${showOthers && html`<div class="cards sources">${others.map((c) => html`<${SourceCard} key=${c.id} s=${c} />`)}</div>`}`}
    </div>`;
}
