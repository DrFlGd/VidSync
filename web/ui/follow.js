// The Follow fields, shared by the Add dialog, a source's inspector and Settings:
// what's downloaded, which tabs, the quality, subtitles, and how often to check.
import { html } from "../lib/html.js";
import { MODES, QUALITIES, SUBS, EVERY } from "./state.js";

const CHECKS = [[null, "Library's schedule"], ...EVERY.filter(([v]) => v !== 0).map(([v, l]) => [v, l]), [0, "Only by hand"]];

/**
 * f: the follow object; set(patch); kind: "channel" | "playlist" | "both";
 * schedule: whether to show how often to check (not in Settings' defaults);
 * ffmpeg: whether ffmpeg is there (a note otherwise); ids: a prefix for element ids.
 */
export function FollowFields({ f, set, kind = "channel", schedule = true, ffmpeg = true, ids = "follow", libraryEvery }) {
  const tabs = f.tabs || { videos: true, shorts: false, streams: false };
  const setTab = (k, on) => set({ tabs: { ...tabs, [k]: on } });
  return html`<div class="follow">
    <label class="field"><span>Download</span>
      <select id=${ids + "-mode"} value=${f.mode} onChange=${(e) => set({ mode: e.target.value })}>
        ${MODES.map(([k, l]) => html`<option value=${k}>${kind === "playlist" && k === "new" ? "Videos added from now on" : l}</option>`)}</select></label>
    ${kind !== "playlist" && html`<div class="field"><span>From the channel's</span>
      <div class="row">
        <label class="check"><input type="checkbox" id=${ids + "-videos"} checked=${tabs.videos} onChange=${(e) => setTab("videos", e.target.checked)} /> Videos</label>
        <label class="check"><input type="checkbox" id=${ids + "-shorts"} checked=${tabs.shorts} onChange=${(e) => setTab("shorts", e.target.checked)} /> Shorts</label>
        <label class="check"><input type="checkbox" id=${ids + "-streams"} checked=${tabs.streams} onChange=${(e) => setTab("streams", e.target.checked)} /> Live streams</label>
      </div></div>`}
    <label class="field"><span>Quality</span>
      <select id=${ids + "-quality"} value=${f.quality} onChange=${(e) => set({ quality: e.target.value })}>
        ${QUALITIES.map(([k, l]) => html`<option value=${k}>${l}</option>`)}</select></label>
    ${!ffmpeg && f.quality !== "audio" && html`<p class="hint">Without ffmpeg (Settings), yt-dlp can only fetch a single file, which YouTube offers up to 720p.</p>`}
    <label class="field"><span>Subtitles</span>
      <select id=${ids + "-subs"} value=${SUBS.some(([k]) => k === f.subtitles) ? f.subtitles : "custom"} onChange=${(e) => e.target.value !== "custom" && set({ subtitles: e.target.value })}>
        ${SUBS.map(([k, l]) => html`<option value=${k}>${l}</option>`)}
        ${!SUBS.some(([k]) => k === f.subtitles) && html`<option value="custom">${f.subtitles}</option>`}</select></label>
    ${schedule && html`<label class="field"><span>Check</span>
      <select id=${ids + "-every"} value=${f.every_hours == null ? "" : String(f.every_hours)} onChange=${(e) => set({ every_hours: e.target.value === "" ? null : Number(e.target.value) })}>
        ${CHECKS.map(([v, l]) => html`<option value=${v == null ? "" : String(v)}>${l}${v == null && libraryEvery != null ? ` (${(EVERY.find(([x]) => x === libraryEvery) || [0, `every ${libraryEvery} hours`])[1].toLowerCase()})` : ""}</option>`)}</select></label>`}
  </div>`;
}
