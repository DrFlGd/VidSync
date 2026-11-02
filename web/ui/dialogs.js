// Dialogs: Add (links to channels, playlists or videos, one or many), and asking
// before something is deleted.
import { html, useEffect, useRef, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, toast, errText, poke, DEFAULT_FOLLOW } from "./state.js";
import { Icon } from "./icons.js";
import { FollowFields } from "./follow.js";

export const openAdd = (text = "") => ui.set({ dialog: { type: "add", text } });

/** Ask before doing something that can't be undone. Resolves true to go ahead. */
export function confirmDialog({ title, body, ok = "Remove", danger = true }) {
  return new Promise((resolve) => ui.set({ dialog: { type: "confirm", title, body, ok, danger, resolve } }));
}

const KINDS = { channel: "channel", playlist: "playlist", video: "video" };

function AddDialog({ text }) {
  const [value, setValue] = useState(text || "");
  const [checks, setChecks] = useState([]);
  const [follow, setFollow] = useState(null);
  const [pfollow, setPfollow] = useState(null);
  const [withPlaylists, setWithPlaylists] = useState(false);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState("");
  const info = useStore(ui, (s) => s.info);
  const box = useRef(null);
  useEffect(() => {
    box.current?.focus();
    api("settings_get").then((s) => { setFollow({ ...(s.library?.new_follow || DEFAULT_FOLLOW) }); setPfollow({ ...(s.library?.new_follow_playlist || { ...DEFAULT_FOLLOW, mode: "all" }) }); })
      .catch(() => { setFollow({ ...DEFAULT_FOLLOW }); setPfollow({ ...DEFAULT_FOLLOW, mode: "all" }); });
  }, []);
  // what each line names, as it's typed (no network: YouTube is asked when it's added)
  useEffect(() => {
    if (!value.trim()) { setChecks([]); return; }
    const t = setTimeout(() => api("link_check", { input: value }).then(setChecks).catch(() => {}), 120);
    return () => clearTimeout(t);
  }, [value]);
  const good = checks.filter((c) => c.ok && !c.in_library);
  const kinds = new Set(good.map((c) => c.link.kind));
  const alsoPlaylists = good.filter((c) => c.also_playlist).map((c) => c.also_playlist);
  const add = async (e) => {
    e?.preventDefault();
    if (!good.length || busy) return;
    setBusy(true);
    setErr("");
    try {
      const inputs = good.map((c) => c.input);
      if (withPlaylists) for (const p of alsoPlaylists) inputs.push(`https://www.youtube.com/playlist?list=${p}`);
      const res = await api("add", { inputs, follow, playlist_follow: pfollow });
      const queued = res.filter((r) => r.job);
      ui.set({ dialog: null });
      toast(queued.length ? `Adding ${queued.length === 1 ? good[0].label : `${queued.length} links`}…` : "Nothing new to add");
      poke();
      if (!location.hash.startsWith("#/sources") && !location.hash.startsWith("#/activity")) location.hash = "#/sources";
    } catch (e2) {
      setErr(errText(e2));
      setBusy(false);
    }
  };
  const set = (patch) => setFollow((f) => ({ ...f, ...patch }));
  const pset = (patch) => setPfollow((f) => ({ ...f, ...patch }));
  const what = (c) => c.link.kind === "channel" ? `channel ${c.link.key.value}` : c.link.kind === "playlist" ? `playlist ${c.link.id}` : `video ${c.link.id}`;
  return html`<form class="dialog" role="dialog" aria-modal="true" aria-labelledby="add-title" onSubmit=${add}>
    <h2 id="add-title">Add to the library</h2>
    <label class="field"><span>Links, one per line</span>
      <textarea id="add-links" ref=${box} rows="4" placeholder=${"https://www.youtube.com/@channel\nhttps://www.youtube.com/playlist?list=…\nhttps://www.youtube.com/watch?v=… (one video)"}
        value=${value} onInput=${(e) => setValue(e.target.value)}
        onKeyDown=${(e) => { if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) add(e); }}></textarea></label>
    ${checks.length > 0 && html`<ul class="checks" aria-label="What the links name">${checks.map((c) => html`<li>
      ${!c.ok ? html`<span class="bad">${Icon.x(12)}</span><span class="bad">${c.error}</span>`
        : c.in_library ? html`<span class="have">${Icon.alert(12)}</span><span class="mono">${what(c)}</span><span class="have">already in the library</span>`
        : html`<span class="good">${Icon.check(12)}</span><span class="mono">${what(c)}</span><span class="muted">${c.link.kind === "video" ? "downloaded into its channel's folder" : "listed, then followed"}</span>`}
    </li>`)}</ul>`}
    ${alsoPlaylists.length > 0 && html`<label class="check"><input type="checkbox" id="add-with-playlists" checked=${withPlaylists} onChange=${(e) => setWithPlaylists(e.target.checked)} /> Also add the ${alsoPlaylists.length === 1 ? "playlist the video link is in" : `${alsoPlaylists.length} playlists the video links are in`}</label>`}
    ${follow && (kinds.has("channel") || kinds.has("video") || !good.length) && html`<div class="options">
      <div class="section-head">${kinds.has("video") && !kinds.has("channel") ? "The video" : "Channels"}</div>
      <${FollowFields} f=${follow} set=${set} kind="channel" ids="add" ffmpeg=${info.ffmpeg !== false} />
      ${kinds.has("channel") && html`<p class="hint">${follow.mode === "new" ? "Videos the channel publishes from now on are downloaded; what's there already is listed in the audit, where any of it can be fetched." : follow.mode === "all" ? "Every video the channel has published is downloaded. A big channel takes a while and a lot of space." : "The channel is listed for the audit; nothing is downloaded until asked."}</p>`}
    </div>`}
    ${pfollow && (kinds.has("playlist") || (withPlaylists && alsoPlaylists.length > 0)) && html`<div class="options">
      <div class="section-head">Playlists</div>
      <${FollowFields} f=${pfollow} set=${pset} kind="playlist" ids="add-pl" ffmpeg=${info.ffmpeg !== false} />
      <p class="hint">Each video goes into its own channel's folder; the playlist keeps the order.</p>
    </div>`}
    ${err && html`<p class="notice error">${err}</p>`}
    <div class="buttons">
      <button type="button" onClick=${() => ui.set({ dialog: null })}>Cancel</button>
      <button type="submit" class="primary" id="add-go" disabled=${!good.length || busy}>${Icon.plus(15)} ${good.length > 1 ? `Add ${good.length} links` : "Add"}</button>
    </div>
  </form>`;
}

function ConfirmDialog({ d }) {
  const close = (v) => { ui.set({ dialog: null }); d.resolve(v); };
  const okRef = useRef(null);
  useEffect(() => okRef.current?.focus(), []);
  return html`<div class="dialog" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title">
    <h2 id="confirm-title">${d.title}</h2>
    <p>${d.body}</p>
    <div class="buttons">
      <button type="button" onClick=${() => close(false)}>Cancel</button>
      <button type="button" id="confirm-ok" ref=${okRef} class=${d.danger ? "danger" : "primary"} onClick=${() => close(true)}>${d.ok}</button>
    </div>
  </div>`;
}

export function Dialogs() {
  const d = useStore(ui, (s) => s.dialog);
  if (!d) return null;
  const dismiss = (e) => { if (e.target === e.currentTarget) { if (d.resolve) d.resolve(false); ui.set({ dialog: null }); } };
  return html`<div class="backdrop" onMouseDown=${dismiss}>
    ${d.type === "add" ? html`<${AddDialog} text=${d.text} />` : html`<${ConfirmDialog} d=${d} />`}
  </div>`;
}
