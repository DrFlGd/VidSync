// Sizes, dates, durations and counts as the interface shows them.

export function bytes(n) {
  if (n == null || isNaN(n)) return "";
  if (n < 1024) return `${n} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = n / 1024, i = 0;
  while (v >= 1024 && i < units.length - 1) { v /= 1024; i++; }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

const parse = (iso) => (iso ? new Date(iso) : null);

/** "3 days ago", "in 2 hours", "just now". */
export function ago(iso) {
  const d = parse(iso);
  if (!d || isNaN(d)) return "";
  const s = Math.round((Date.now() - d.getTime()) / 1000);
  const future = s < 0;
  const a = Math.abs(s);
  const say = (n, unit) => { const t = `${n} ${unit}${n === 1 ? "" : "s"}`; return future ? `in ${t}` : `${t} ago`; };
  if (a < 45) return future ? "in a moment" : "just now";
  if (a < 3600) return say(Math.max(1, Math.round(a / 60)), "minute");
  if (a < 86400) return say(Math.round(a / 3600), "hour");
  if (a < 86400 * 45) return say(Math.round(a / 86400), "day");
  if (a < 86400 * 365) return say(Math.round(a / (86400 * 30)), "month");
  return say(Math.round(a / (86400 * 365)), "year");
}

/** "29 Oct 2026, 03:25" in the computer's time. */
export function when(iso) {
  const d = parse(iso);
  if (!d || isNaN(d)) return "";
  return d.toLocaleString(undefined, { day: "numeric", month: "short", year: "numeric", hour: "2-digit", minute: "2-digit" });
}

/** "29 Oct 2026"; a plain date ("2026-10-29") is shown as that day, not shifted by the zone. */
export function day(iso) {
  if (!iso) return "";
  if (/^\d{4}-\d{2}-\d{2}$/.test(iso)) {
    const [y, m, d] = iso.split("-").map(Number);
    return new Date(y, m - 1, d).toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
  }
  const d = parse(iso);
  if (!d || isNaN(d)) return "";
  return d.toLocaleDateString(undefined, { day: "numeric", month: "short", year: "numeric" });
}

/** "12:34" or "1:02:03" from seconds. */
export function duration(secs) {
  if (!secs || isNaN(secs)) return "";
  const s = Math.round(secs);
  const h = Math.floor(s / 3600), m = Math.floor((s % 3600) / 60), r = s % 60;
  const two = (n) => String(n).padStart(2, "0");
  return h ? `${h}:${two(m)}:${two(r)}` : `${m}:${two(r)}`;
}

/** "1.2M", "34K", "980". */
export function count(n) {
  if (n == null || isNaN(n)) return "";
  if (n >= 1e9) return `${(n / 1e9).toFixed(1)}B`;
  if (n >= 1e6) return `${(n / 1e6).toFixed(n >= 1e7 ? 0 : 1)}M`;
  if (n >= 1e3) return `${(n / 1e3).toFixed(n >= 1e4 ? 0 : 1)}K`;
  return String(n);
}

/** "1080p", "720p", "audio" from a video's size. */
export function res(v) {
  if (!v) return "";
  if (v.quality === "audio" || (v.ext === "m4a" || v.ext === "mp3" || v.ext === "opus")) return "audio";
  if (v.height >= 2000) return "4K";
  if (v.height >= 1400) return "1440p";
  if (v.height) return `${v.height}p`;
  return "";
}

export const plural = (n, one, many = one + "s") => `${n} ${n === 1 ? one : many}`;
