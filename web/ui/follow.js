// The Follow fields, shared by the Add dialog, a source's inspector and Settings:
// what's downloaded, which tabs, the quality, subtitles, and how often to check.
import { html } from "../lib/html.js";
import { MODES, QUALITIES, SUBS, EVERY } from "./state.js";

const KEEP = [[0, "Every video it downloads"], [5, "Only the newest 5"], [10, "Only the newest 10"], [25, "Only the newest 25"], [50, "Only the newest 50"], [100, "Only the newest 100"]];
const DROP = [[0, "Never"], [1, "A day after watching"], [7, "A week after watching"], [30, "A month after watching"]];

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
    <label class="check"><input type="checkbox" id=${ids + "-comments"} checked=${!!f.comments} onChange=${(e) => set({ comments: e.target.checked })} /> Save the top comments with each video (up to 200)</label>
    ${kind === "channel" && html`<label class="field"><span>Keep</span>
      <select id=${ids + "-keep"} value=${String(f.keep_newest || 0)} onChange=${(e) => set({ keep_newest: Number(e.target.value) })}>
        ${KEEP.map(([v, l]) => html`<option value=${String(v)}>${l}</option>`)}
        ${!KEEP.some(([v]) => v === (f.keep_newest || 0)) && html`<option value=${String(f.keep_newest)}>Only the newest ${f.keep_newest}</option>`}</select></label>
    <label class="field"><span>Delete watched videos</span>
      <select id=${ids + "-drop"} value=${String(f.drop_watched_days || 0)} onChange=${(e) => set({ drop_watched_days: Number(e.target.value) })}>
        ${DROP.map(([v, l]) => html`<option value=${String(v)}>${l}</option>`)}
        ${!DROP.some(([v]) => v === (f.drop_watched_days || 0)) && html`<option value=${String(f.drop_watched_days)}>${f.drop_watched_days} days after watching</option>`}</select></label>
    ${(f.keep_newest > 0 || f.drop_watched_days > 0) && html`<p class="hint">Older or watched videos are deleted after each check (Activity lists them); the audit still lists them, so they aren't downloaded again. A video you've tagged is always kept.</p>`}`}
    ${schedule && html`<label class="field"><span>Check</span>
      <select id=${ids + "-every"} value=${f.every_hours == null ? "" : String(f.every_hours)} onChange=${(e) => set({ every_hours: e.target.value === "" ? null : Number(e.target.value) })}>
        ${CHECKS.map(([v, l]) => html`<option value=${v == null ? "" : String(v)}>${l}${v == null && libraryEvery != null ? ` (${(EVERY.find(([x]) => x === libraryEvery) || [0, `every ${libraryEvery} hours`])[1].toLowerCase()})` : ""}</option>`)}</select></label>`}
  </div>`;
}
