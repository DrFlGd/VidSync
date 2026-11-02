// Actions more than one page offers: checking, downloading what's missing,
// opening folders, files and links.
import { api, ctx, toast, errText, poke } from "./state.js";

export async function check(id) {
  try {
    const r = await api("check", { id });
    toast(r.job ? "Checking for new videos…" : "Already checking this source");
    poke();
  } catch (e) {
    toast(errText(e), 6000);
  }
}

export async function checkAll() {
  try {
    const r = await api("check_all");
    toast(r.queued ? `Checking ${r.queued} ${r.queued === 1 ? "source" : "sources"} for new videos…` : "Everything is being checked already");
    poke();
  } catch (e) {
    toast(errText(e), 6000);
  }
}

export async function downloadMissing(id) {
  try {
    const r = await api("download_missing", { id });
    toast(r.job ? "Downloading what's missing…" : "Already working on this source");
    poke();
  } catch (e) {
    toast(errText(e), 6000);
  }
}

/** Download one listed video of a source (from its audit). */
export async function getVideo(id, video) {
  try {
    const r = await api("video_get", { id, video });
    toast(r.job ? "Downloading…" : "Already downloading that video");
    poke();
    return true;
  } catch (e) {
    toast(errText(e), 6000);
    return false;
  }
}

/** Open a source's folder (or a folder inside it) in the file manager. */
export async function openFolder(id, sub = "") {
  try {
    const p = await api("path", { id, sub });
    await ctx().openPath(p);
  } catch (e) {
    toast(errText(e), 6000);
  }
}

/** Open a file (a video, in the system's player) by its path. */
export async function openPath(path) {
  try { await ctx().openPath(path); } catch (e) { toast(errText(e), 6000); }
}

export const openUrl = (url) => ctx().openUrl(url);
