// Vid Sync: start-up and routing. The interface itself is in ui/ (Preact + htm,
// no build step); the backend is reached through platform.js.
import { createPlatform } from "./platform.js";
import { mountShell } from "./ui/shell.js";
import { ui, initState, loadVideos, loadSources, startPolling } from "./ui/state.js";

// errors the tests read back (tests/desktop_page.py)
window.__errors = [];
window.addEventListener("error", (e) => window.__errors.push(String(e.message)));
window.addEventListener("unhandledrejection", (e) => window.__errors.push(String(e.reason?.message || e.reason)));

/** "#/source/youtube.com/channels/@x?tab=audit" -> { page: "source", id, tab }; "#/video/@x/abc" -> { page: "video", channel, video }. */
export function routeFromHash(hash) {
  const [path, query = ""] = (hash || "").replace(/^#\/?/, "").split("?");
  const q = new URLSearchParams(query);
  if (path.startsWith("source/")) return { page: "source", id: decodeURIComponent(path.slice(7)), tab: q.get("tab") || "videos" };
  if (path.startsWith("video/")) {
    const rest = path.slice(6);
    const i = rest.lastIndexOf("/");
    return { page: "video", channel: decodeURIComponent(rest.slice(0, i)), video: decodeURIComponent(rest.slice(i + 1)), t: q.get("t") ? Number(q.get("t")) : null };
  }
  if (path.startsWith("creator/")) return { page: "creator", name: decodeURIComponent(path.slice(8)) };
  if (["sources", "audit", "activity", "settings", "about"].includes(path)) return { page: path };
  // "#/?creator=Name" (or channel=, playlist=, tag=) opens the videos filtered
  for (const by of ["creator", "channel", "playlist", "tag"]) {
    if (q.get(by)) return { page: "videos", filter: { by, value: q.get(by), label: q.get(by) } };
  }
  return { page: "videos" };
}

function route() {
  const r = routeFromHash(location.hash);
  const left = ui.get().route.page !== r.page;
  ui.set({ route: r, menu: null, ...(r.filter ? { filter: r.filter } : {}), ...(left ? { selected: [] } : {}) });
  if (r.filter) history.replaceState(null, "", "#/");
  document.querySelector(".source-main, .page")?.scrollTo?.(0, 0);
}

async function start() {
  const platform = await createPlatform();
  initState(platform);
  mountShell();
  route();
  window.addEventListener("hashchange", route);
  await Promise.all([loadVideos(), loadSources()]);
  ui.set({ ready: true });
  startPolling();
}

start();
