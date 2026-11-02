// A video's page: the app's own player, its details and description, where
// else it is, and what can be done with it.
import { html, useEffect, useRef, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, toast, errText, reloadAll, qualityLabel } from "./state.js";
import { Icon } from "./icons.js";
import { confirmDialog } from "./dialogs.js";
import { openPath, openUrl } from "./actions.js";
import { bytes, ago, when, day, duration, count, res } from "../lib/format.js";
import { Menu } from "./source.js";

/** The description with its links made clickable (in the browser) and line breaks kept. */
function Description({ text }) {
  if (!text) return html`<p class="muted">No description.</p>`;
  const parts = text.split(/(https?:\/\/[^\s<>"']+)/g);
  return html`<p class="description">${parts.map((p, i) => i % 2 ? html`<a href=${p} key=${i}>${p}</a>` : p)}</p>`;
}

function Tags({ v, reload }) {
  const [text, setText] = useState("");
  const save = async (tags) => {
    try { await api("video_tags_set", { channel: v.channel, video: v.id, tags }); reload(); reloadAll(); } catch (e) { toast(errText(e), 6000); }
  };
  const add = (e) => {
    e.preventDefault();
    const t = text.trim();
    if (!t || v.tags.includes(t)) { setText(""); return; }
    save([...v.tags, t]);
    setText("");
  };
  return html`<div>
    <ul class="chips">${v.tags.map((t) => html`<li key=${t} class="chip tag">${t}<span class="x" role="button" tabindex="0" title=${`Remove ${t}`} onClick=${() => save(v.tags.filter((x) => x !== t))}>×</span></li>`)}</ul>
    <form class="tag-edit" onSubmit=${add}><input id="tag-input" type="text" placeholder="Add a tag" value=${text} onInput=${(e) => setText(e.target.value)} />
      <button type="submit" class="small">Add</button></form>
  </div>`;
}

export function VideoPage({ channel, video }) {
  const [v, setV] = useState(null);
  const [err, setErr] = useState("");
  const [playErr, setPlayErr] = useState("");
  const player = useRef(null);
  const reload = () => api("video", { channel, video }).then((r) => { setV(r); setErr(""); }).catch((e) => setErr(errText(e)));
  useEffect(() => { reload(); }, [channel, video]);
  if (err && !v) return html`<div class="page"><p class="notice error">${err}</p><a class="button" href="#/">${Icon.back(14)} Back to the videos</a></div>`;
  if (!v) return html`<div class="page"><p class="muted">Reading…</p></div>`;
  const audio = v.quality === "audio" || v.vcodec === "none";
  const del = async () => {
    if (!(await confirmDialog({ title: "Delete this video?", body: `${v.title} (${bytes(v.bytes)}) is deleted from the library folder. Its channel and playlists still list it in the audit, so it can be downloaded again.`, ok: "Delete the video" }))) return;
    try { await api("video_delete", { channel, video }); await reloadAll(); toast("Deleted"); location.hash = `#/source/${encodeURIComponent(v.source)}`; } catch (e) { toast(errText(e), 6000); }
  };
  const seek = (t) => { if (player.current) { player.current.currentTime = t; player.current.play?.(); } };
  return html`<div class="source-page video-page">
    <div class="source-main">
      <nav class="crumbs" aria-label="Where"><a href="#/">Videos</a><span>/</span><a href=${`#/source/${encodeURIComponent(v.source)}`}>${v.channel_name}</a></nav>
      <div class="player" id="player">
        ${playErr ? html`<div class="player-none"><p>${playErr}</p><button type="button" onClick=${() => openPath(`${v.path}/${v.file}`)}>${Icon.play(14)} Open in the system's player</button></div>`
          : audio ? html`<div class="player-audio">${v.thumbnail_url && html`<img src=${v.thumbnail_url} alt="" />`}<audio ref=${player} controls preload="metadata" src=${v.file_url}></audio></div>`
          : html`<video ref=${player} controls preload="metadata" poster=${v.thumbnail_url || null} src=${v.file_url}
              onError=${() => setPlayErr("This file can't be played here (the codec isn't one the app's player knows).")}>
              ${v.subtitles.map((s) => html`<track key=${s} kind="subtitles" src=${v.file_url.replace(encodeURIComponent(v.file), encodeURIComponent(s))} srclang=${(s.match(/\.([a-z]{2,3})(-[A-Za-z]+)?\.(vtt|srt)$/) || [])[1] || "en"} label=${s.split(".").slice(-2, -1)[0]} />`)}
            </video>`}
      </div>
      <header class="repo-head">
        <div class="repo-title"><h1>${v.title}</h1></div>
        <div class="facts video-facts">
          <span><a href=${`#/source/${encodeURIComponent(v.source)}`}>${v.channel_name}</a></span>
          ${v.upload_date && html`<span title="Published">${Icon.clock(12)} ${day(v.upload_date)}</span>`}
          ${v.view_count > 0 && html`<span title="Views on YouTube when downloaded">${Icon.eye(12)} ${count(v.view_count)}</span>`}
          ${v.duration > 0 && html`<span>${duration(v.duration)}</span>`}
          ${res(v) && html`<span>${res(v)}</span>`}
          <span>${bytes(v.bytes)}</span>
        </div>
        <div class="repo-actions">
          <button type="button" class="primary" id="open-system" onClick=${() => openPath(`${v.path}/${v.file}`)} title="Open the file in the system's player">${Icon.play(15)} Open in player</button>
          <button type="button" onClick=${() => openPath(v.path)} title="Open the video's folder">${Icon.folder(15)} Folder</button>
          <button type="button" onClick=${() => openUrl(v.web_url)} title=${v.web_url}>${Icon.external(15)} YouTube</button>
          <${Menu} id="video-more" label=${Icon.dots(16)} title="More" align="left">
            <button type="button" onClick=${() => navigator.clipboard?.writeText(v.web_url).then(() => toast("Link copied"))}>Copy the link</button>
            <div class="sep"></div>
            <button type="button" class="danger" id="delete-video" onClick=${del}>${Icon.trash(14)} Delete from the library…</button>
          </${Menu}>
        </div>
      </header>
      <h2 class="section-head">Description</h2>
      <div class="panel"><${Description} text=${v.description} /></div>
      ${v.chapters?.length > 0 && html`<h2 class="section-head">Chapters <span class="n">${v.chapters.length}</span></h2>
        <ul class="log chapters">${v.chapters.map((c, i) => html`<li key=${i}><button type="button" class="ghost small mono" onClick=${() => seek(c.start)}>${duration(c.start)}</button><span>${c.title}</span></li>`)}</ul>`}
    </div>
    <aside class="inspector" aria-label="Details">
      <section><h2>Details</h2>
        <dl class="props" id="props">
          <dt>Channel</dt><dd><a href=${`#/source/${encodeURIComponent(v.source)}`}>${v.channel_name}</a></dd>
          ${v.creator && html`<dt>Creator</dt><dd>${v.creator}</dd>`}
          <dt>Published</dt><dd>${day(v.upload_date) || "?"}</dd>
          <dt>Downloaded</dt><dd title=${when(v.downloaded)}>${ago(v.downloaded)}</dd>
          <dt>Asked for</dt><dd>${qualityLabel(v.quality)}</dd>
          ${v.width > 0 && html`<dt>Size</dt><dd>${v.width} × ${v.height}${v.fps ? ` · ${Math.round(v.fps)} fps` : ""}</dd>`}
          <dt>File</dt><dd title=${v.file}>${v.ext}${v.vcodec && v.vcodec !== "none" ? ` · ${v.vcodec.split(".")[0]}` : ""}${v.acodec && v.acodec !== "none" ? ` · ${v.acodec.split(".")[0]}` : ""}</dd>
          <dt>On disk</dt><dd>${bytes(v.bytes)}</dd>
          ${v.subtitles.length > 0 && html`<dt>Subtitles</dt><dd>${v.subtitles.map((s) => s.split(".").slice(-2, -1)[0]).join(", ")}</dd>`}
          ${v.like_count != null && html`<dt>Likes</dt><dd>${count(v.like_count)}</dd>`}
          <dt>Id</dt><dd class="mono">${v.id}</dd>
        </dl>
      </section>
      ${v.playlists.length > 0 && html`<section><h2>In playlists</h2><ul class="plain-list">${v.playlists.map((p) => html`<li key=${p.id}><a href=${`#/source/${encodeURIComponent(p.id)}`}>${Icon.playlist(13)} ${p.name}</a></li>`)}</ul></section>`}
      <section><h2>Your tags</h2><${Tags} v=${v} reload=${reload} /></section>
      ${v.youtube_tags?.length > 0 && html`<section><h2>YouTube's tags</h2><ul class="chips">${v.youtube_tags.map((t) => html`<li class="chip" key=${t}>${t}</li>`)}</ul></section>`}
    </aside>
  </div>`;
}
