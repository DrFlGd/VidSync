// The Audit page: every source with what's listed upstream against what's kept.
import { html, useEffect, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui, api, errText } from "./state.js";
import { Icon } from "./icons.js";
import { checkAll, downloadMissing } from "./actions.js";
import { ago, when } from "../lib/format.js";
import { sourceHref } from "./sources.js";

export function AuditPage() {
  const sources = useStore(ui, (s) => s.sources);
  const [data, setData] = useState(null);
  const [err, setErr] = useState("");
  const stamp = sources ? [...sources.channels, ...sources.playlists].map((s) => `${s.id}:${s.last_checked}:${s.videos}:${s.status}`).join("|") : "";
  useEffect(() => { api("audit").then((d) => { setData(d); setErr(""); }).catch((e) => setErr(errText(e))); }, [stamp]);
  if (err) return html`<div class="page"><p class="notice error">${err}</p></div>`;
  if (!data) return html`<div class="page"><p class="muted">Reading…</p></div>`;
  const rows = [...data.rows].sort((a, b) => (b.audit.missing + b.audit.failed) - (a.audit.missing + a.audit.failed) || a.name.localeCompare(b.name));
  const t = data.total;
  const n = (x, cls = "") => html`<td class=${"num " + cls}>${x || html`<span class="muted">·</span>`}</td>`;
  return html`<div class="cmdbar"><h1>Audit</h1>
      <span class="count">${t.missing + t.failed ? `${t.missing + t.failed} to get` : "nothing missing"}</span>
      <span class="spacer"></span>
      <button type="button" onClick=${checkAll} disabled=${!rows.length}>${Icon.refresh(15)} Check all now</button></div>
    <div class="page">
      <p class="hint">What each source lists on YouTube against what the library keeps. <b>Missing</b> is wanted and not downloaded yet; <b>not wanted</b> is outside the source's follow settings (such as videos published before a channel was added), and any of it can be fetched from the source's Audit tab.</p>
      ${!rows.length ? html`<p class="muted">No source has been listed yet.</p>` : html`<table class="list audit" id="audit">
        <thead><tr><th>Source</th><th class="num">Listed</th><th class="num">Kept</th><th class="num">Missing</th><th class="num">Failed</th><th class="num">Not wanted</th><th class="num">Skipped</th><th class="num">Gone</th><th>Checked</th><th></th></tr></thead>
        <tbody>${rows.map((r) => html`<tr key=${r.id} data-id=${r.id}>
          <td><a class="name" href=${sourceHref(r)}>${r.name}</a><div class="sub">${r.kind === "playlist" ? `playlist · ${r.channel || ""}` : (r.handle || r.key)}${r.followed === false ? " · not followed" : ""}</div></td>
          ${n(r.audit.listed)}${n(r.audit.kept, "good")}${n(r.audit.missing, r.audit.missing ? "missing" : "")}${n(r.audit.failed, r.audit.failed ? "bad" : "")}${n(r.audit.not_wanted)}${n(r.audit.skipped)}${n(r.audit.gone)}
          <td class="date" title=${when(r.last_checked)}>${r.last_checked ? ago(r.last_checked) : "never"}${r.error ? html`<div class="err" title=${r.error}>failed</div>` : ""}</td>
          <td class="num">${(r.audit.missing + r.audit.failed) > 0 && html`<button type="button" class="small" onClick=${() => downloadMissing(r.id)} disabled=${r.status === "working" || r.status === "queued"}>${Icon.download(13)} Get ${r.audit.missing + r.audit.failed}</button>`}</td>
        </tr>`)}</tbody>
        <tfoot><tr><td><b>Every source</b></td>${n(t.listed)}${n(t.kept, "good")}${n(t.missing, t.missing ? "missing" : "")}${n(t.failed, t.failed ? "bad" : "")}${n(t.not_wanted)}${n(t.skipped)}${n(t.gone)}<td></td><td></td></tr></tfoot>
      </table>`}
    </div>`;
}
