// The Activity page: the download in hand (with Stop), what's waiting, and what
// the last checks found.
import { html } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, toast, errText, poke } from "./state.js";
import { Icon } from "./icons.js";
import { checkAll } from "./actions.js";
import { bytes, ago, when, duration } from "../lib/format.js";

const target = (j) => j.target.startsWith("youtube.com/")
  ? html`<a href=${`#/source/${encodeURIComponent(j.target)}`}>${j.label}</a>`
  : html`<span>${j.label}</span>`;
const what = (j) => (j.type === "add" ? (j.link?.kind === "video" ? "Downloading a video" : "Adding")
  : j.type === "missing" ? "Download missing"
  : j.type === "video" ? "Download one video"
  : j.type === "tool" ? "Fetching"
  : j.type === "move" ? "Moving the library"
  : j.by === "schedule" ? "Scheduled check" : "Check");

async function stop(job) {
  try { await api("job_stop", { job }); poke(); } catch (e) { toast(errText(e), 6000); }
}

async function resumeChecks() {
  try { const i = await api("checks_resume"); ui.set({ info: { ...ui.get().info, ...i } }); poke(); toast("Scheduled checks are back on"); } catch (e) { toast(errText(e), 6000); }
}

export function ActivityPage() {
  const jobs = useStore(ui, (s) => s.jobs);
  const paused = useStore(ui, (s) => s.info.paused_until);
  const next = useStore(ui, (s) => s.totals.next_check);
  const cur = jobs.current;
  const pct = cur && cur.total > 0 ? Math.min(100, (cur.done / cur.total) * 100) : null;
  return html`<div class="cmdbar"><h1>Activity</h1><span class="spacer"></span>
      ${next && html`<span class="muted" title=${when(next)}>Next scheduled check ${ago(next)}</span>`}
      <button type="button" onClick=${checkAll}>${Icon.refresh(15)} Check all now</button></div>
    <div class="page page-narrow">
      ${paused && html`<p class="notice" id="paused">YouTube asked to confirm you're not a bot, so scheduled checks wait until ${when(paused)} (${ago(paused)}). Cookies from a browser you're signed in to (<a href="#/settings">Settings</a>) usually stop this.
        <button type="button" class="small" id="resume-checks" style="margin-left:8px" onClick=${resumeChecks}>Check on schedule again now</button></p>`}
      <h2 class="section-head">Now</h2>
      <div class="panel job-now" id="job-now">${cur ? html`
        <div><b>${what(cur.job)}</b> ${target(cur.job)} <span class="muted">· started ${ago(cur.started)}</span></div>
        <div class="stage">${cur.stage}${cur.place ? ` (${cur.place})` : ""}${pct !== null ? ` · ${bytes(cur.done)} of ${bytes(cur.total)}` : cur.done ? ` · ${bytes(cur.done)}` : ""}${cur.speed ? ` · ${bytes(cur.speed)}/s` : ""}${cur.eta ? ` · ${duration(cur.eta)} left` : ""}</div>
        <div class=${"meter big" + (pct === null ? " busy" : "")}><i style=${pct === null ? "" : `width:${pct}%`}></i></div>
        <div><button type="button" class="small" id="stop" onClick=${() => stop(cur.job.id)}>${Icon.x(13)} Stop</button></div>`
        : html`<p class="muted">Nothing is downloading.</p>`}</div>
      ${jobs.queued?.length > 0 && html`<h2 class="section-head">Waiting <span class="n">${jobs.queued.length}</span></h2>
        <ul class="log">${jobs.queued.map((j) => html`<li key=${j.id}><span class="t">${what(j)}</span><span>${target(j)}
          <button type="button" class="ghost small" style="float:right" onClick=${() => stop(j.id)}>Cancel</button></span></li>`)}</ul>
        <p></p>`}
      <h2 class="section-head">Finished <span class="n">${jobs.recent?.length || 0}</span></h2>
      ${jobs.recent?.length ? html`<ul class="log" id="finished">${jobs.recent.map((f) => html`<li key=${f.job.id}>
        <span class="t" title=${when(f.finished)}>${ago(f.finished)}<br />${what(f.job)}</span>
        <span>${target(f.job)}<br /><span class=${!f.ok ? "err" : f.outcome && (f.outcome.downloaded?.length || f.outcome.listed_new) ? "new" : "muted"}>${f.message}</span>
          ${f.outcome?.failed?.length > 0 && html`<ul class="sublist">${f.outcome.failed.map((x) => html`<li class="err">${x}</li>`)}</ul>`}</span>
      </li>`)}</ul>` : html`<p class="muted">Nothing has been checked since the app started.</p>`}
    </div>`;
}
