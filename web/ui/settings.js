// Settings: the library folder, the schedule, what new sources follow, yt-dlp
// and ffmpeg, cookies for YouTube, and the theme.
import { html, useEffect, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, ctx, toast, errText, setPref, reloadAll, poke, EVERY, DEFAULT_FOLLOW } from "./state.js";
import { Icon } from "./icons.js";
import { FollowFields } from "./follow.js";
import { ago, when, plural } from "../lib/format.js";
import { chooseDialog } from "./dialogs.js";

const BROWSERS = [["chrome", "Chrome"], ["edge", "Edge"], ["firefox", "Firefox"], ["brave", "Brave"], ["vivaldi", "Vivaldi"], ["opera", "Opera"], ["chromium", "Chromium"], ["safari", "Safari"]];

function Tool({ name, t, path, setPath, action, actionLabel, hint, busy }) {
  const [v, setV] = useState(path || "");
  useEffect(() => setV(path || ""), [path]);
  const pick = async () => {
    const p = await ctx().pickFile?.(`Choose ${name}`);
    if (p) { setV(p); setPath(p); }
  };
  return html`<div class="tool" id=${"tool-" + name}>
    <div class="row">
      <b style="width:70px">${name}</b>
      ${t.path ? html`<span class="badge ok">${t.version}</span><span class="muted" title=${t.path}>found ${t.from === "PATH" ? "on PATH" : `in the ${t.from}`}</span>`
        : html`<span class="badge error">not found</span>`}
      <span class="spacer" style="flex:1"></span>
      <button type="button" class="small" id=${"get-" + name} disabled=${busy} onClick=${action}>${Icon.download(13)} ${actionLabel}</button>
    </div>
    ${t.error && html`<p class="hint" style="color:var(--danger)">${t.error}</p>`}
    <div class="row">
      <input type="text" style="flex:1" placeholder=${`Or point at your own ${name} (a path)`} value=${v} onInput=${(e) => setV(e.target.value)} onBlur=${() => v !== (path || "") && setPath(v)} aria-label=${`Path to ${name}`} />
      <button type="button" class="small" onClick=${pick}>Browse…</button>
    </div>
    <p class="hint">${hint}</p>
  </div>`;
}

export function SettingsPage() {
  const info = useStore(ui, (s) => s.info);
  const theme = useStore(ui, (s) => s.theme);
  const jobs = useStore(ui, (s) => s.jobs);
  const [s, setS] = useState(null);
  const load = () => api("settings_get").then(setS).catch(() => setS(null));
  useEffect(() => { load(); }, [info.library?.path, jobs.recent?.[0]?.finished]);
  const save = async (patch) => {
    try { setS(await api("settings_set", patch)); if (patch.library) reloadAll(); if ("ytdlp_path" in patch || "ffmpeg_path" in patch) ctx().refreshInfo?.().then((i) => ui.set({ info: { ...i } })); } catch (e) { toast(errText(e), 6000); }
  };
  const saveLib = (patch) => save({ library: { ...s.library, ...patch } });
  const change = async () => {
    const p = await ctx().pickFolder("Choose the library folder");
    if (!p) return;
    try {
      const at = await api("library_inspect", { path: p });
      if (at.same) { toast("That's the library's folder already"); return; }
      let action = "open";
      // this library has something in it and the new place isn't a library: move it there, or start afresh?
      if (!at.is_library && at.current_has_content) {
        const cur = at.current;
        const held = `${plural(cur.sources, "source")} and ${plural(cur.videos, "video")}`;
        action = await chooseDialog({
          title: "Move the library there?",
          body: at.can_move
            ? `The library at ${cur.path} holds ${held}. Move all of its files to ${p}, or start a new, empty library there and leave this one where it is?`
            : `${at.why_not} Starting a new, empty library at ${p} leaves this one (${held}) where it is.`,
          choices: [
            ...(at.can_move ? [{ value: "move", label: "Move the library there", cls: "primary" }] : []),
            { value: "open", label: "Start a new library there" },
          ],
        });
        if (!action) return;
      }
      if (action === "move") {
        await api("library_move", { path: p });
        toast(`Moving the library to ${p}…`);
        poke();
        location.hash = "#/activity";
        return;
      }
      const i = await api("library_open", { path: p });
      ui.set({ info: i, filter: null });
      await reloadAll();
      toast(`Library: ${p}`);
    } catch (e) { toast(errText(e), 6000); }
  };
  const fetchTool = async (name) => {
    try { await api("tool_update", { name }); toast(`Downloading ${name}…`); poke(); } catch (e) { toast(errText(e), 6000); }
  };
  const busy = (name) => !!(jobs.current?.job?.target === `tool:${name}` || jobs.queued?.some((j) => j.target === `tool:${name}`));
  const lib = s?.library;
  const cookies = s?.cookies || { mode: "", browser: "", file: "" };
  const setCookies = (patch) => save({ cookies: { ...cookies, ...patch } });
  return html`<div class="cmdbar"><h1>Settings</h1></div>
    <div class="page"><div class="settings">
      <section><h2 class="section-head">Library</h2><div class="panel">
        <div class="row"><span class="path" id="library-path">${info.library?.path || "No library open"}</span>
          <button type="button" id="change-library" onClick=${change}>Change…</button>
          ${info.library && html`<button type="button" onClick=${() => ctx().openPath(info.library.path)}>${Icon.folder(15)} Open</button>`}</div>
        <p class="hint">Each channel gets a folder in it (<span class="mono">youtube.com/channels/@handle</span>) holding <span class="mono">channel.json</span> and one folder per video under <span class="mono">videos/</span>: the video, its picture, its metadata and subtitles, and its own <span class="mono">video.json</span>. Playlists are lists in <span class="mono">youtube.com/playlists/</span>. The folder works without the app and can be moved, synced or opened on another computer.</p>
      </div></section>
      ${lib && html`<section><h2 class="section-head">Checks</h2><div class="panel">
        <label class="field"><span>Check every followed source</span>
          <select id="check-every" value=${String(lib.check_every_hours)} onChange=${(e) => saveLib({ check_every_hours: Number(e.target.value) })}>
            ${EVERY.map(([v, l]) => html`<option value=${String(v)}>${l}</option>`)}
            ${!EVERY.some(([v]) => v === lib.check_every_hours) && html`<option value=${String(lib.check_every_hours)}>Every ${lib.check_every_hours} hours</option>`}</select></label>
        <p class="hint">Checks run while Vid Sync is open (one download at a time); a source can have its own schedule on its page. A check lists the source on YouTube, then downloads what's new. Nothing is ever deleted by a check.
          ${s.next_check && html` Next check ${ago(s.next_check)} (${when(s.next_check)}).`}</p>
        <p class="hint">To check while the app is closed, schedule <span class="mono">vidsync-cli check-all</span> (for example with Windows Task Scheduler).</p>
      </div></section>
      <section><h2 class="section-head">New channels follow</h2><div class="panel">
        <${FollowFields} f=${lib.new_follow || DEFAULT_FOLLOW} set=${(p) => saveLib({ new_follow: { ...(lib.new_follow || DEFAULT_FOLLOW), ...p } })} kind="channel" schedule=${false} ids="new" ffmpeg=${info.ffmpeg !== false} />
        <p class="hint">What a channel added from now on follows; each channel can be changed on its page. “New videos from now on” lists everything but downloads only what's published after the channel is added.</p>
      </div></section>
      <section><h2 class="section-head">New playlists follow</h2><div class="panel">
        <${FollowFields} f=${lib.new_follow_playlist || { ...DEFAULT_FOLLOW, mode: "all" }} set=${(p) => saveLib({ new_follow_playlist: { ...(lib.new_follow_playlist || { ...DEFAULT_FOLLOW, mode: "all" }), ...p } })} kind="playlist" schedule=${false} ids="newpl" ffmpeg=${info.ffmpeg !== false} />
      </div></section>`}
      ${s && html`<section><h2 class="section-head">yt-dlp and ffmpeg</h2><div class="panel tools">
        <p class="hint">yt-dlp does the listing and downloading; ffmpeg joins video and audio (YouTube serves them apart above 720p). Both are looked for in the app's tools folder (<span class="mono">${s.tools.folder}</span>), the app's own folder and PATH; the buttons download them into the tools folder, and Update swaps yt-dlp for the newest release once it runs.</p>
        <${Tool} name="yt-dlp" t=${s.tools.ytdlp} path=${s.ytdlp_path} setPath=${(p) => save({ ytdlp_path: p })} action=${() => fetchTool("yt-dlp")} actionLabel=${s.tools.ytdlp.path ? "Update yt-dlp" : "Download yt-dlp"} busy=${busy("yt-dlp")}
          hint="When YouTube changes and downloads start failing, updating yt-dlp is the first thing to try." />
        <${Tool} name="ffmpeg" t=${s.tools.ffmpeg} path=${s.ffmpeg_path} setPath=${(p) => save({ ffmpeg_path: p })} action=${() => fetchTool("ffmpeg")} actionLabel=${s.tools.ffmpeg.path ? "Update ffmpeg" : "Get ffmpeg"} busy=${busy("ffmpeg")}
          hint=${info.os === "windows" ? "A build from BtbN's FFmpeg-Builds (about 100 MB)." : "On Linux and Mac, install ffmpeg with your package manager; it's found on PATH."} />
        <div><button type="button" class="small ghost" onClick=${() => api("tools_find").then(setS).then(() => ctx().refreshInfo?.().then((i) => ui.set({ info: { ...i } })))}>${Icon.refresh(13)} Look again</button></div>
      </div></section>
      <section><h2 class="section-head">Cookies</h2><div class="panel">
        <p class="hint">Members-only, age-restricted and private videos need your YouTube sign-in. yt-dlp can read it from a browser you're signed in to, or from a cookies file exported by a browser extension. Cookies stay on this computer, never in the library.</p>
        <label class="field"><span>Use</span>
          <select id="cookies-mode" value=${cookies.mode} onChange=${(e) => setCookies({ mode: e.target.value })}>
            <option value="">No cookies</option><option value="browser">A browser's cookies</option><option value="file">A cookies file</option></select></label>
        ${cookies.mode === "browser" && html`<label class="field"><span>Browser</span>
          <select value=${cookies.browser} onChange=${(e) => setCookies({ browser: e.target.value })}><option value="">Choose…</option>${BROWSERS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}</select></label>
          <p class="hint">Recent versions of Chrome and Edge on Windows lock their cookies; if yt-dlp can't read them, export a cookies file instead (an extension such as “Get cookies.txt LOCALLY”).</p>`}
        ${cookies.mode === "file" && html`<div class="row"><input type="text" style="flex:1" placeholder="Path to a Netscape cookies.txt" value=${cookies.file} onChange=${(e) => setCookies({ file: e.target.value })} aria-label="Cookies file" />
          <button type="button" class="small" onClick=${async () => { const p = await ctx().pickFile?.("Choose the cookies file"); if (p) setCookies({ file: p }); }}>Browse…</button></div>`}
      </div></section>`}
      <section><h2 class="section-head">Appearance</h2><div class="panel">
        <label class="field"><span>Theme</span>
          <select id="theme" value=${theme} onChange=${(e) => setPref({ theme: e.target.value })}>
            <option value="system">Follow the system</option><option value="light">Light</option><option value="dark">Dark</option></select></label>
      </div></section>
    </div></div>`;
}
