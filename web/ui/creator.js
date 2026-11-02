// A creator's page (#/creator/<name>): a creator is the user's own grouping of
// channels (a person with several of them). Their channels with totals, and one
// feed of the newest videos across all of them.
import { html, useMemo } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui } from "./state.js";
import { Icon } from "./icons.js";
import { SourceCard } from "./sources.js";
import { VideoCard } from "./videos.js";
import { check } from "./actions.js";
import { bytes, plural } from "../lib/format.js";

export function CreatorPage({ name }) {
  const sources = useStore(ui, (s) => s.sources);
  const videos = useStore(ui, (s) => s.videos);
  const channels = useMemo(() => (sources?.channels || []).filter((c) => c.creator === name), [sources, name]);
  const feed = useMemo(() => videos.filter((v) => v.creator === name).sort((a, b) => (b.upload_date || "").localeCompare(a.upload_date || "") || (b.downloaded || "").localeCompare(a.downloaded || "")), [videos, name]);
  if (!sources) return html`<div class="page"><p class="muted">Reading…</p></div>`;
  const size = channels.reduce((s, c) => s + (c.bytes || 0), 0);
  const missing = channels.reduce((s, c) => s + (c.audit?.missing || 0) + (c.audit?.failed || 0), 0);
  const unwatched = feed.filter((v) => !v.watched).length;
  const newest = channels.map((c) => c.newest).filter(Boolean).sort().pop();
  const checkAll = () => channels.filter((c) => c.followed).forEach((c) => check(c.id));
  return html`<div class="cmdbar">
      <h1>${Icon.creator(18)} ${name}</h1>
      <span class="count">${plural(channels.length, "channel")} · ${plural(feed.length, "video")} · ${bytes(size)}</span>
      <span class="spacer"></span>
      <a class="button" href=${`#/?creator=${encodeURIComponent(name)}`} title="The Videos page, showing only this creator's">${Icon.grid(15)} In Videos</a>
      <button type="button" id="creator-check" onClick=${checkAll} disabled=${!channels.some((c) => c.followed)}>${Icon.refresh(15)} Check their channels</button>
    </div>
    <div class="page" id="creator-page" data-creator=${name}>
      ${channels.length === 0 ? html`<p class="muted">No channel has “${name}” as its creator any more. <a href="#/sources">Sources</a></p>` : html`
        <p class="creator-stats">
          <span>${plural(unwatched, "video")} not watched yet</span>
          ${newest && html`<span>Newest published ${newest}</span>`}
          <span class=${missing ? "err" : ""}>${missing ? `${plural(missing, "video")} missing upstream` : "Nothing missing upstream"}</span>
        </p>
        <h2 class="section-head">Channels <span class="n">${channels.length}</span></h2>
        <div class="cards sources">${channels.map((c) => html`<${SourceCard} key=${c.id} s=${c} />`)}</div>
        <h2 class="section-head" style="margin-top:24px">Newest videos <span class="n">${feed.length}</span></h2>
        ${feed.length ? html`<div class="cards videos" id="creator-feed">${feed.map((v) => html`<${VideoCard} key=${v.channel + v.id} v=${v} />`)}</div>`
          : html`<p class="muted">None of their videos are downloaded yet.</p>`}`}
    </div>`;
}
