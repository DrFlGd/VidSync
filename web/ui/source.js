// A channel's or a playlist's page: its kept videos, the audit of what's upstream
// against them, the playlists its videos are in, and on the right its details,
// what it follows, its creator, tags and notes.
import { html, useEffect, useRef, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, toast, errText, reloadAll, modeLabel, qualityLabel } from "./state.js";
import { Icon } from "./icons.js";
import { confirmDialog } from "./dialogs.js";
import { check, downloadMissing, getVideo, openFolder, openUrl } from "./actions.js";
import { FollowFields } from "./follow.js";
import { bytes, ago, when, day, duration, count, plural } from "../lib/format.js";
import { VideoCard, Rows } from "./videos.js";
import { Avatar, AuditBadges, statusBadge, SourceCard } from "./sources.js";

/** A small menu under a button; closes on a click outside or Escape. */
export function Menu({ label, title, cls = "", children, align = "left", id }) {
  const [open, setOpen] = useState(false);
  const ref = useRef(null);
  useEffect(() => {
    if (!open) return;
    const off = (e) => { if (!ref.current?.contains(e.target)) setOpen(false); };
    const esc = (e) => e.key === "Escape" && setOpen(false);
    document.addEventListener("mousedown", off);
    document.addEventListener("keydown", esc);
    return () => { document.removeEventListener("mousedown", off); document.removeEventListener("keydown", esc); };
  }, [open]);
  return html`<span class="anchor" ref=${ref}>
    <button type="button" id=${id} class=${cls} title=${title} aria-haspopup="menu" aria-expanded=${open ? "true" : "false"} onClick=${() => setOpen(!open)}>${label}</button>
    ${open && html`<div class="menu" role="menu" style=${`top:calc(100% + 4px);${align === "right" ? "right:0" : "left:0"}`} onClick=${(e) => e.target.closest("button") && setOpen(false)}>${children}</div>`}
  </span>`;
}

const STATE = {
  wanted: ["missing", "Missing"],
  "not-wanted": ["plain", "Not wanted"],
  skipped: ["plain", "Skipped"],
  failed: ["bad", "Failed"],
};

function stateOf(x) {
  if (x.kept) return ["ok", "Kept"];
  if (x.gone) return ["plain", "Gone from YouTube"];
  if (x.live === "is_live") return ["warn", "Live now"];
  if (x.live === "is_upcoming") return ["warn", "Upcoming"];
  if (x.waiting) return ["working", "Downloading…"];
  return STATE[x.state] || ["plain", x.state];
}

// Everything the source lists upstream, with what the library has of each.
function Audit({ source, reload }) {
  const id = source.id;
  const [show, setShow] = useState("all");
  const list = source.upstream;
  const a = source.audit;
  const shown = list.filter((x) => {
    if (show === "all") return !x.gone;
    if (show === "missing") return !x.kept && !x.gone && (x.state === "wanted" || x.state === "failed");
    if (show === "not-wanted") return !x.kept && !x.gone && x.state === "not-wanted";
    if (show === "kept") return !!x.kept;
    if (show === "gone") return x.gone;
    return true;
  });
  const skip = async (x, on) => {
    try { await api("video_skip", { id, video: x.id, skip: on }); reload(); } catch (e) { toast(errText(e), 6000); }
  };
  const get = async (x) => { if (await getVideo(id, x.id)) reload(); };
  const getAll = async (ids) => {
    for (const v of ids) await api("video_get", { id, video: v }).catch(() => {});
    toast(`Downloading ${plural(ids.length, "video")}…`);
    reload();
  };
  const notWanted = list.filter((x) => !x.kept && !x.gone && x.state === "not-wanted").map((x) => x.id);
  return html`<div>
    <div class="audit-head">
      <${AuditBadges} a=${a} kind=${source.kind} />
      <span class="spacer"></span>
      <div class="seg" role="group" aria-label="Show">
        ${[["all", "Listed"], ["missing", "Missing"], ["not-wanted", "Not wanted"], ["kept", "Kept"], ["gone", "Gone"]].map(([k, l]) => html`<button type="button" key=${k} aria-pressed=${show === k ? "true" : "false"} onClick=${() => setShow(k)}>${l}</button>`)}
      </div>
    </div>
    <p class="hint">What YouTube lists for this ${source.kind}, as of the last check (${source.last_checked ? ago(source.last_checked) : "never"}). ${a.missing + a.failed > 0 ? html`<button type="button" class="small" id="get-missing" onClick=${() => downloadMissing(id)}>${Icon.download(13)} Download the ${plural(a.missing + a.failed, "missing video")}</button>` : "Nothing wanted is missing."}
      ${notWanted.length > 0 && show === "not-wanted" && html` <button type="button" class="small" id="get-not-wanted" onClick=${() => getAll(notWanted)}>${Icon.download(13)} Download all ${notWanted.length}</button>`}</p>
    ${!shown.length ? html`<p class="muted">${list.length ? "Nothing in this part of the list." : "Nothing has been listed yet; press Check now."}</p>` : html`<table class="list audit-list" id="upstream">
      <thead><tr><th></th><th>Video</th><th class="num">Length</th><th class="num">Views</th><th>State</th><th></th></tr></thead>
      <tbody>${shown.map((x, i) => { const [cls, label] = stateOf(x); return html`<tr key=${x.id} data-video=${x.id} data-state=${x.kept ? "kept" : x.state}>
        <td class="num mono muted">${source.kind === "playlist" ? i + 1 : ""}</td>
        <td><div class="name">${x.kept ? html`<a href=${`#/video/${encodeURIComponent(x.kept.channel)}/${x.id}`}>${x.title}</a>` : x.title}</div>
          <div class="sub">${x.tab ? `${x.tab} · ` : ""}${x.upload_date ? `${day(x.upload_date)} · ` : ""}${source.kind === "playlist" && x.channel ? `${x.channel} · ` : ""}<span class="mono">${x.id}</span>${x.error ? html` · <span class="err">${x.error}</span>` : ""}</div></td>
        <td class="num">${duration(x.duration)}</td>
        <td class="num">${count(x.view_count)}</td>
        <td><span class=${"badge " + cls}>${label}</span></td>
        <td class="num actions">
          ${!x.kept && !x.gone && !x.waiting && x.live !== "is_live" && x.live !== "is_upcoming" && html`<button type="button" class="ghost small" title="Download this video" onClick=${() => get(x)}>${Icon.download(14)}</button>`}
          ${!x.kept && !x.gone && x.state !== "skipped" && html`<button type="button" class="ghost small" title="Skip: leave this one out on purpose" onClick=${() => skip(x, true)}>${Icon.skip(14)}</button>`}
          ${!x.kept && x.state === "skipped" && html`<button type="button" class="ghost small" title="Want it after all" onClick=${() => skip(x, false)}>${Icon.refresh(14)}</button>`}
          <button type="button" class="ghost small" title="Open on YouTube" onClick=${() => openUrl(x.web_url)}>${Icon.external(14)}</button>
        </td></tr>`; })}</tbody>
    </table>`}
  </div>`;
}

function Following({ source, save }) {
  const info = useStore(ui, (s) => s.info);
  const [lib, setLib] = useState({});
  useEffect(() => { api("settings_get").then((s) => setLib(s.library || {})).catch(() => {}); }, []);
  // kept here too, so a second change made before the first is saved builds on it
  const [f, setF] = useState(source.follow);
  const latest = useRef(source.follow);
  const key = JSON.stringify(source.follow);
  useEffect(() => { latest.current = source.follow; setF(source.follow); }, [key]);
  const set = (patch) => {
    latest.current = { ...latest.current, ...patch };
    setF(latest.current);
    save(latest.current);
  };
  return html`<div id="follow">
    <${FollowFields} f=${f} set=${set} kind=${source.kind} ids="follow" ffmpeg=${info.ffmpeg !== false} libraryEvery=${lib.check_every_hours} />
    ${source.kind === "channel" && f.mode === "new" && html`<p class="hint">Videos listed before the channel was added count as “not wanted” in the audit; download any of them from there.</p>`}
  </div>`;
}

function Tags({ tags, saveTags }) {
  const [text, setText] = useState("");
  const add = (e) => {
    e.preventDefault();
    const t = text.trim();
    if (!t || tags.includes(t)) { setText(""); return; }
    saveTags([...tags, t]);
    setText("");
  };
  return html`<div>
    <ul class="chips">${tags.map((t) => html`<li key=${t} class="chip tag">${t}<span class="x" role="button" tabindex="0" title=${`Remove ${t}`} onClick=${() => saveTags(tags.filter((x) => x !== t))}>×</span></li>`)}</ul>
    <form class="tag-edit" onSubmit=${add}><input id="tag-input" type="text" placeholder="Add a tag" value=${text} onInput=${(e) => setText(e.target.value)} />
      <button type="submit" class="small">Add</button></form>
  </div>`;
}

function Creator({ source, save }) {
  const [list, setList] = useState([]);
  const [text, setText] = useState(source.creator || "");
  useEffect(() => { setText(source.creator || ""); }, [source.creator]);
  useEffect(() => { api("creators").then(setList).catch(() => {}); }, [source.creator]);
  const commit = () => { if (text.trim() !== (source.creator || "")) save(text.trim()); };
  return html`<div class="field">
    <input id="creator-input" type="text" list="creator-names" placeholder="A person or studio with several channels" value=${text} onInput=${(e) => setText(e.target.value)} onBlur=${commit} onKeyDown=${(e) => e.key === "Enter" && (e.preventDefault(), commit())} />
    <datalist id="creator-names">${list.map((c) => html`<option value=${c.name} />`)}</datalist>
    <p class="hint">Your own grouping of channels, listed in the menu. Leave it empty for none.${source.creator ? html` <a href=${`#/creator/${encodeURIComponent(source.creator)}`} id="creator-link">${source.creator}'s page</a>` : ""}</p>
  </div>`;
}

function Inspector({ source, reload }) {
  const s = source;
  const id = s.id;
  // one save at a time, in the order made, and the page reloaded once the last one is in
  const queue = useRef({ last: Promise.resolve(), pending: 0 });
  const send = (cmd, args) => {
    const q = queue.current;
    q.pending++;
    q.last = q.last.then(() => api(cmd, args)).catch((e) => toast(errText(e), 6000)).finally(() => {
      if (--q.pending === 0) { reload(); reloadAll(); }
    });
  };
  const a = s.audit;
  return html`<aside class="inspector" aria-label="Details">
    <section>
      <h2>Details</h2>
      <dl class="props" id="props">
        <dt>On YouTube</dt><dd><a href=${s.url} title=${s.url}>${s.kind === "playlist" ? "playlist" : (s.handle || "channel")}</a></dd>
        ${s.kind === "playlist" && s.channel && html`<dt>By</dt><dd>${s.channel_source ? html`<a href=${`#/source/${encodeURIComponent(s.channel_source)}`}>${s.channel}</a>` : s.channel}</dd>`}
        ${s.kind === "channel" && s.subscribers > 0 && html`<dt>Subscribers</dt><dd>${s.subscribers.toLocaleString()}</dd>`}
        <dt>Upstream</dt><dd>${plural(a.listed, "video")}</dd>
        <dt>Kept</dt><dd>${plural(s.videos, "video")}, ${bytes(s.bytes)}</dd>
        ${a.missing > 0 && html`<dt>Missing</dt><dd class="missing">${a.missing}</dd>`}
        ${a.failed > 0 && html`<dt>Failed</dt><dd class="bad">${a.failed}</dd>`}
        <dt>Added</dt><dd title=${when(s.added)}>${day(s.added)}</dd>
        <dt>Last download</dt><dd title=${when(s.last_changed)}>${s.last_changed ? ago(s.last_changed) : "never"}</dd>
        <dt>Last checked</dt><dd title=${when(s.last_checked)}>${s.last_checked ? ago(s.last_checked) : "never"}</dd>
        <dt>Next check</dt><dd title=${when(s.due)}>${s.due ? ago(s.due) : (s.followed ? "by hand" : "not followed")}</dd>
        ${s.newest && html`<dt>Newest kept</dt><dd>${day(s.newest)}</dd>`}
      </dl>
    </section>
    <section><h2>Follow</h2>
      ${s.kind === "channel" && !s.followed ? html`<p class="hint">This channel only holds videos (from a playlist or a pasted link). Follow it to list it and download its new videos on the schedule.</p>
        <button type="button" class="primary" id="follow-start" onClick=${() => send("followed_set", { id, followed: true })}>${Icon.plus(14)} Follow this channel</button>`
        : html`<${Following} source=${s} save=${(follow) => send("follow_set", { id, follow })} />`}
    </section>
    ${s.kind === "channel" && html`<section><h2>Creator</h2><${Creator} source=${s} save=${(creator) => send("creator_set", { id, creator })} /></section>`}
    <section><h2>Your tags</h2><${Tags} tags=${s.tags} saveTags=${(tags) => send("tags_set", { id, tags })} /></section>
    <section><h2>Notes</h2><textarea id="notes" rows="3" placeholder="Anything you want to remember about this source" value=${s.notes} onChange=${(e) => send("notes_set", { id, notes: e.target.value })}></textarea></section>
  </aside>`;
}

export function SourcePage({ id, tab }) {
  const summary = useStore(ui, (s) => s.sources && [...s.sources.channels, ...s.sources.playlists].find((x) => x.id === id));
  const view = useStore(ui, (s) => s.view);
  const sort = useStore(ui, (s) => s.sort);
  const [source, setSource] = useState(null);
  const [err, setErr] = useState("");
  const reload = () => api("source", { id }).then((r) => { setSource(r); setErr(""); }).catch((e) => setErr(errText(e)));
  // read the source again whenever a check changes it
  const stamp = summary ? `${summary.last_checked}|${summary.status}|${summary.videos}|${summary.last_changed}|${summary.audit?.missing}|${summary.audit?.failed}` : "";
  useEffect(() => { reload(); }, [id, stamp]);
  if (err && !source) {
    return html`<div class="page"><p class="notice error">${err}</p><a class="button" href="#/sources">${Icon.back(14)} Back to the sources</a></div>`;
  }
  if (!source) return html`<div class="page"><p class="muted">Reading…</p></div>`;
  const s = source;
  const setTab = (t) => { location.hash = `#/source/${encodeURIComponent(id)}${t === "videos" ? "" : `?tab=${t}`}`; };
  const remove = async () => {
    const body = s.kind === "channel"
      ? `Every video kept from ${s.name} (${plural(s.videos, "video")}, ${bytes(s.bytes)}) is deleted from the library folder. This can't be undone.`
      : `The playlist ${s.name} is forgotten. Its videos stay in their channels' folders.`;
    if (!(await confirmDialog({ title: `Remove ${s.name}?`, body, ok: s.kind === "channel" ? "Delete the channel and its videos" : "Remove the playlist" }))) return;
    try { await api("remove", { id }); await reloadAll(); toast(`Removed ${s.name}`); location.hash = "#/sources"; } catch (e) { toast(errText(e), 6000); }
  };
  const unfollow = async () => {
    try { await api("followed_set", { id, followed: false }); reload(); reloadAll(); toast(`No longer following ${s.name}; its videos are kept.`); } catch (e) { toast(errText(e), 6000); }
  };
  const a = s.audit;
  const tabs = [["videos", "Videos", s.videos_kept.length], ["audit", "Audit", a.missing + a.failed || null], ...(s.kind === "channel" ? [["playlists", "Playlists", s.playlists.length]] : [])];
  return html`<div class="source-page">
    <div class="source-main">
      <header class="repo-head source-head">
        <nav class="crumbs" aria-label="Where"><a href="#/sources">Sources</a><span>/</span><span>${s.kind === "playlist" ? "playlist" : "channel"}</span></nav>
        <div class="source-title">
          <${Avatar} s=${s} size=${56} />
          <div class="repo-title"><h1>${s.name}</h1>
            ${statusBadge(s)}
            ${s.kind === "channel" && !s.followed && html`<span class="badge plain">not followed</span>`}
            ${s.followed && s.follow.mode === "none" && html`<span class="badge plain">list only</span>`}
          </div>
        </div>
        ${s.description && html`<p class="repo-desc">${s.description}</p>`}
        ${s.error && html`<p class="notice error" id="source-error">The last check didn't finish: ${s.error}</p>`}
        <div class="repo-actions">
          <button type="button" class="primary" id="check-now" onClick=${() => check(id)} disabled=${s.status === "working" || s.status === "queued"}>${Icon.refresh(15)} Check now</button>
          ${a.missing + a.failed > 0 && html`<button type="button" id="download-missing" onClick=${() => downloadMissing(id)} disabled=${s.status === "working" || s.status === "queued"}>${Icon.download(15)} Download ${plural(a.missing + a.failed, "missing video")}</button>`}
          <button type="button" onClick=${() => openFolder(id)} title="Open the folder">${Icon.folder(15)} Folder</button>
          <button type="button" onClick=${() => openUrl(s.url)} title=${s.url}>${Icon.external(15)} YouTube</button>
          <${Menu} id="source-more" label=${Icon.dots(16)} title="More" align="left">
            <button type="button" onClick=${() => navigator.clipboard?.writeText(s.url).then(() => toast("Link copied"))}>Copy the link</button>
            ${s.kind === "channel" && s.followed && html`<button type="button" id="unfollow" onClick=${unfollow}>Stop following (keep the videos)</button>`}
            <div class="sep"></div>
            <button type="button" class="danger" id="remove" onClick=${remove}>${Icon.trash(14)} ${s.kind === "channel" ? "Remove the channel and its videos…" : "Remove the playlist…"}</button>
          </${Menu}>
        </div>
      </header>
      <div class="tabs" role="tablist">${tabs.map(([k, l, n]) => html`<button type="button" role="tab" id=${"tab-" + k} aria-selected=${tab === k ? "true" : "false"} onClick=${() => setTab(k)}>${l}${n != null && n !== 0 && html`<span class=${"n" + (k === "audit" ? " missing" : "")}>${n}</span>`}</button>`)}</div>
      ${tab === "audit" ? html`<${Audit} source=${s} reload=${reload} />`
        : tab === "playlists" ? (s.playlists.length ? html`<div class="cards sources">${s.playlists.map((p) => html`<${SourceCard} key=${p.id} s=${p} />`)}</div>` : html`<p class="muted">None of this channel's videos is in a playlist in the library.</p>`)
        : !s.videos_kept.length ? html`<p class="muted" id="no-videos">${s.kind === "channel" && !s.followed ? "No video of this channel is kept." : a.missing ? `Nothing downloaded yet; ${plural(a.missing, "video")} ${a.missing === 1 ? "is" : "are"} missing.` : s.follow.mode === "none" ? "This source is listed only. Download videos from its Audit tab, or change what it follows on the right." : "No video has been downloaded yet. New ones are fetched at each check."}</p>`
        : view === "list" ? html`<${Rows} list=${s.videos_kept} sort=${sort} showChannel=${s.kind === "playlist"} />`
        : html`<div class="cards videos" id="kept">${s.videos_kept.map((v) => html`<${VideoCard} key=${v.id} v=${v} showChannel=${s.kind === "playlist"} />`)}</div>`}
    </div>
    <${Inspector} source=${s} reload=${reload} />
  </div>`;
}
