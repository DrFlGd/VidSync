// A video's page: the app's own player (which carries on where you stopped and
// marks the video watched near its end), its details and description, where
// else it is, and what can be done with it.
import { html, useEffect, useRef, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, toast, errText, reloadAll, loadVideos, poke, qualityLabel, QUALITIES } from "./state.js";
import { Icon } from "./icons.js";
import { confirmDialog, chooseDialog } from "./dialogs.js";
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

/** Saved comments (Follow → Save the top comments), most liked first as YouTube gave them. */
function Comments({ list, total }) {
  const [all, setAll] = useState(false);
  const shown = all ? list : list.slice(0, 20);
  return html`<h2 class="section-head">Comments <span class="n">${list.length}${total > list.length ? ` of ${count(total)}` : ""}</span></h2>
    <ul class="log comments" id="comments">${shown.map((c, i) => html`<li key=${i}>
      <span class="t">${c.author || "Someone"}${c.by_uploader ? html`<br /><span class="badge ok">channel</span>` : ""}${c.pinned ? html`<br /><span class="badge">pinned</span>` : ""}</span>
      <span><span class="comment-text">${c.text}</span><br /><span class="comment-meta">${c.likes ? `${count(c.likes)} likes · ` : ""}${c.when ? day(c.when.slice(0, 10)) : ""}</span></span>
    </li>`)}</ul>
    ${list.length > shown.length && html`<p><button type="button" class="small" id="more-comments" onClick=${() => setAll(true)}>Show all ${list.length}</button></p>`}`;
}

export function VideoPage({ channel, video, t = null }) {
  const [v, setV] = useState(null);
  const [err, setErr] = useState("");
  const [playErr, setPlayErr] = useState("");
  const [resumed, setResumed] = useState(0);
  const player = useRef(null);
  const sent = useRef({ at: 0, pos: -1 });
  const reload = () => api("video", { channel, video }).then((r) => { setV(r); setErr(""); }).catch((e) => setErr(errText(e)));
  useEffect(() => { reload(); }, [channel, video]);
  // how far it was played goes into the video's video.json: every 10 s while playing, on pause, at the end and on leaving
  const report = (ended = false) => {
    const p = player.current;
    if (!p || !(p.currentTime > 0)) return;
    const pos = p.currentTime;
    if (!ended && Math.abs(pos - sent.current.pos) < 1) return;
    sent.current = { at: Date.now(), pos };
    return api("video_progress", { channel, video, position: pos, ended })
      .then((r) => setV((x) => (x ? { ...x, watched: r.watched, position: r.position } : x))).catch(() => {});
  };
  useEffect(() => () => { Promise.resolve(report()).then(() => loadVideos()); }, [channel, video]);
  const media = {
    onLoadedMetadata: (e) => {
      const el = e.currentTarget;
      if (t != null && t >= 0) { el.currentTime = t; return; }
      if (v && !v.watched && v.position > 5 && v.position < (el.duration || Infinity) - 5) { el.currentTime = v.position; setResumed(v.position); }
    },
    onTimeUpdate: () => { if (Date.now() - sent.current.at > 10000) report(); },
    onPause: () => report(),
    onEnded: () => report(true),
  };
  // a link to a moment (a search hit) on the page already open: go there
  useEffect(() => { if (t != null && player.current && player.current.readyState >= 1) { player.current.currentTime = t; player.current.play?.().catch(() => {}); } }, [t]);
  const setWatched = async (on) => {
    try { await api("videos_watched", { items: [{ channel, video }], watched: on }); await reload(); loadVideos(); } catch (e) { toast(errText(e), 6000); }
  };
  if (err && !v) return html`<div class="page"><p class="notice error">${err}</p><a class="button" href="#/">${Icon.back(14)} Back to the videos</a></div>`;
  if (!v) return html`<div class="page"><p class="muted">Reading…</p></div>`;
  const audio = v.quality === "audio" || v.vcodec === "none";
  const del = async () => {
    if (!(await confirmDialog({ title: "Delete this video?", body: `${v.title} (${bytes(v.bytes)}) is deleted from the library folder. Its channel and playlists still list it in the audit, so it can be downloaded again.`, ok: "Delete the video" }))) return;
    try { await api("video_delete", { channel, video }); await reloadAll(); toast("Deleted"); location.hash = `#/source/${encodeURIComponent(v.source)}`; } catch (e) { toast(errText(e), 6000); }
  };
  const again = async () => {
    const quality = await chooseDialog({
      title: "Download it again?",
      body: `${v.title} is kept at ${res(v) || qualityLabel(v.quality).toLowerCase()} (${bytes(v.bytes)}). A new copy is downloaded in the quality you pick; your tags and where you stopped carry over, and the old copy is deleted only once the new one is in.${ui.get().info.ffmpeg === false ? " Without ffmpeg (Settings), YouTube offers a single file only up to 720p." : ""}`,
      choices: QUALITIES.map(([k, l]) => ({ value: k, label: l, cls: k === "best" ? "primary" : "" })),
    });
    if (!quality) return;
    try { await api("video_redownload", { channel, video, quality }); toast(`Downloading ${v.title} again…`); poke(); } catch (e) { toast(errText(e), 6000); }
  };
  const seek = (t) => { if (player.current) { player.current.currentTime = t; player.current.play?.(); } };
  return html`<div class="source-page video-page">
    <div class="source-main">
      <nav class="crumbs" aria-label="Where"><a href="#/">Videos</a><span>/</span><a href=${`#/source/${encodeURIComponent(v.source)}`}>${v.channel_name}</a></nav>
      <div class="player" id="player">
        ${playErr ? html`<div class="player-none"><p>${playErr}</p><button type="button" onClick=${() => openPath(`${v.path}/${v.file}`)}>${Icon.play(14)} Open in the system's player</button></div>`
          : audio ? html`<div class="player-audio">${v.thumbnail_url && html`<img src=${v.thumbnail_url} alt="" />`}<audio ref=${player} controls preload="metadata" src=${v.file_url} ...${media}></audio></div>`
          : html`<video ref=${player} controls preload="metadata" poster=${v.thumbnail_url || null} src=${v.file_url} ...${media}
              onError=${() => setPlayErr("This file can't be played here (the codec isn't one the app's player knows).")}>
              ${v.subtitles.map((s) => html`<track key=${s} kind="subtitles" src=${v.file_url.replace(encodeURIComponent(v.file), encodeURIComponent(s))} srclang=${(s.match(/\.([a-z]{2,3})(-[A-Za-z]+)?\.(vtt|srt)$/) || [])[1] || "en"} label=${s.split(".").slice(-2, -1)[0]} />`)}
            </video>`}
      </div>
      ${resumed > 0 && html`<p class="hint" id="resumed">Carrying on from ${duration(resumed)}, where you stopped. <button type="button" class="ghost small" onClick=${() => { if (player.current) player.current.currentTime = 0; setResumed(0); }}>Start over</button></p>`}
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
          <button type="button" id="watched-toggle" aria-pressed=${v.watched ? "true" : "false"} onClick=${() => setWatched(!v.watched)} title=${v.watched ? "Watched: click to mark it not watched" : "Mark it watched"}>${v.watched ? html`${Icon.check(15)} Watched` : html`${Icon.eye(15)} Mark watched`}</button>
          <button type="button" onClick=${() => openPath(v.path)} title="Open the video's folder">${Icon.folder(15)} Folder</button>
          <button type="button" onClick=${() => openUrl(v.web_url)} title=${v.web_url}>${Icon.external(15)} YouTube</button>
          <${Menu} id="video-more" label=${Icon.dots(16)} title="More" align="left">
            <button type="button" onClick=${() => navigator.clipboard?.writeText(v.web_url).then(() => toast("Link copied"))}>Copy the link</button>
            <button type="button" id="redownload" onClick=${again}>${Icon.download(14)} Download again in another quality…</button>
            <div class="sep"></div>
            <button type="button" class="danger" id="delete-video" onClick=${del}>${Icon.trash(14)} Delete from the library…</button>
          </${Menu}>
        </div>
      </header>
      <h2 class="section-head">Description</h2>
      <div class="panel"><${Description} text=${v.description} /></div>
      ${v.chapters?.length > 0 && html`<h2 class="section-head">Chapters <span class="n">${v.chapters.length}</span></h2>
        <ul class="log chapters">${v.chapters.map((c, i) => html`<li key=${i}><button type="button" class="ghost small mono" onClick=${() => seek(c.start)}>${duration(c.start)}</button><span>${c.title}</span></li>`)}</ul>`}
      ${v.comments?.length > 0 && html`<${Comments} list=${v.comments} total=${v.comment_count || 0} />`}
    </div>
    <aside class="inspector" aria-label="Details">
      <section><h2>Details</h2>
        <dl class="props" id="props">
          <dt>Channel</dt><dd><a href=${`#/source/${encodeURIComponent(v.source)}`}>${v.channel_name}</a></dd>
          ${v.creator && html`<dt>Creator</dt><dd><a href=${`#/creator/${encodeURIComponent(v.creator)}`}>${v.creator}</a></dd>`}
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
