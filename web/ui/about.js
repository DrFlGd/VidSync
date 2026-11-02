// About: the version, what the app does, where it keeps things, and the licences.
import { html, useEffect, useState } from "../lib/html.js";
import { useStore } from "../lib/store.js";
import { ui } from "./state.js";
import { Mark } from "./icons.js";

export function AboutPage() {
  const info = useStore(ui, (s) => s.info);
  const [third, setThird] = useState("");
  const [build, setBuild] = useState(null);
  useEffect(() => {
    fetch("THIRD_PARTY.md").then((r) => (r.ok ? r.text() : "")).then(setThird).catch(() => {});
    fetch("build-info.json").then((r) => (r.ok ? r.json() : null)).then(setBuild).catch(() => {});
  }, []);
  return html`<div class="cmdbar"><h1>About</h1></div>
    <div class="page page-narrow about">
      <div class="panel">
        <div style="display:flex;gap:14px;align-items:center"><span style="width:48px;height:48px;display:inline-flex"><${Mark} /></span>
          <div><h2>Vid Sync ${info.version}</h2><p class="muted">${build ? `Built ${build.date}${build.commit ? ` from ${build.commit}` : ""}` : ""}</p></div></div>
        <p>A library of YouTube videos that works offline. Add a channel, a playlist or a video by its link: Vid Sync downloads it with yt-dlp into a folder you can open without the app, checks followed sources for new videos on a schedule, and keeps an audit of what each source lists upstream against what you have. Videos are kept by channel; playlists and your own creator groups and tags are other ways to look at them.</p>
        <p><a href="https://github.com/DrFlGd/VidSync">github.com/DrFlGd/VidSync</a> · <a href="https://github.com/DrFlGd/VidSync/releases">Releases</a> · <a href="https://github.com/yt-dlp/yt-dlp">yt-dlp</a></p>
      </div>
      ${third && html`<h2 class="section-head">Third-party code</h2><pre>${third}</pre>`}
    </div>`;
}
