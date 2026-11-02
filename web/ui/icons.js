// Inline line icons (no icon font, no network), drawn for Drafting Sheet's fine square-ended lines.
import { html } from "../lib/html.js";

const svg = (d, size = 16) => html`<svg class="icon" width=${size} height=${size} viewBox="0 0 24 24" fill="none" stroke="currentColor" aria-hidden="true">${d}</svg>`;

const D = {
  video: html`<rect x="3" y="5" width="18" height="14" /><path d="M3 9h18M3 15h18" stroke-dasharray="2 2" /><path d="M10.5 9.5v5l4-2.5z" fill="currentColor" stroke="none" />`,
  sources: html`<circle cx="8" cy="8" r="3.5" /><path d="M2.5 19c0-3 2.5-5 5.5-5s5.5 2 5.5 5" /><path d="M15 5h7M15 9h7M15 13h7" />`,
  channel: html`<circle cx="12" cy="8" r="4" /><path d="M4 21c0-4 3.5-7 8-7s8 3 8 7" />`,
  playlist: html`<path d="M3 6h12M3 11h12M3 16h7" /><path d="M16 13v7l5-3.5z" fill="currentColor" stroke="none" />`,
  creator: html`<circle cx="9" cy="7" r="3" /><circle cx="17" cy="9" r="2.5" /><path d="M2 20c0-3.5 3-6 7-6s7 2.5 7 6M16 14c3 0 6 1.5 6 5" />`,
  audit: html`<path d="M5 3h11l3 3v15H5z" /><path d="M8 9h8M8 13h8M8 17h5" /><path d="M14 16.5l1.5 1.5L19 14.5" />`,
  activity: html`<path d="M2 12h4l3-7 4 14 3-7h6" />`,
  cog: html`<circle cx="12" cy="12" r="6.5" /><circle cx="12" cy="12" r="2.5" /><path d="M12 2v3M12 19v3M2 12h3M19 12h3M4.9 4.9 7 7M17 17l2.1 2.1M4.9 19.1 7 17M17 7l2.1-2.1" />`,
  info: html`<circle cx="12" cy="12" r="9" /><path d="M12 11v6M12 7v1.5" />`,
  plus: html`<path d="M12 4v16M4 12h16" />`,
  refresh: html`<path d="M20 12a8 8 0 1 1-2.3-5.7" /><path d="M20 4v5h-5" />`,
  play: html`<path d="M7 4.5v15l12-7.5z" />`,
  download: html`<path d="M12 3v12M6 10l6 6 6-6" /><path d="M4 20h16" />`,
  folder: html`<path d="M3 6h6l2 2h10v11H3z" /><path d="M3 11h18" stroke-dasharray="1.5 1.5" />`,
  external: html`<path d="M14 4h6v6M20 4l-9 9" /><path d="M18 14v6H4V6h6" />`,
  search: html`<circle cx="10.5" cy="10.5" r="6.5" /><path d="M15.2 15.2 21 21" />`,
  grid: html`<path d="M4 4h7v7H4zM13 4h7v7h-7zM4 13h7v7H4zM13 13h7v7h-7z" />`,
  list: html`<path d="M4 6h16M4 12h16M4 18h16" />`,
  trash: html`<path d="M4 7h16M9 7V4h6v3M6 7l1 13h10l1-13" />`,
  down: html`<path d="M6 9l6 6 6-6" />`,
  x: html`<path d="M6 6l12 12M18 6 6 18" />`,
  eye: html`<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z" /><circle cx="12" cy="12" r="3" />`,
  eyeoff: html`<path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z" /><path d="M4 4l16 16" />`,
  tag: html`<path d="M3 3h8l10 10-8 8L3 11z" /><circle cx="7.5" cy="7.5" r="1.5" />`,
  back: html`<path d="M15 5l-7 7 7 7" />`,
  dots: html`<circle cx="5" cy="12" r="1.2" /><circle cx="12" cy="12" r="1.2" /><circle cx="19" cy="12" r="1.2" />`,
  check: html`<path d="M4 12.5l5 5L20 6.5" />`,
  alert: html`<path d="M12 3 2 20h20z" /><path d="M12 10v5M12 17v1.5" />`,
  clock: html`<circle cx="12" cy="12" r="9" /><path d="M12 7v5l3 2" />`,
  sun: html`<circle cx="12" cy="12" r="4" /><path d="M12 2v3M12 19v3M2 12h3M19 12h3M5 5l2 2M17 17l2 2M5 19l2-2M17 7l2-2" />`,
  moon: html`<path d="M20 14.5A8 8 0 0 1 9.5 4 8 8 0 1 0 20 14.5z" />`,
  auto: html`<circle cx="12" cy="12" r="8" /><path d="M12 4v16" /><path d="M12 4a8 8 0 0 1 0 16z" fill="currentColor" stroke="none" />`,
  wrench: html`<path d="M14.5 5.5a4 4 0 0 0 4 6.5L10 20.5 3.5 14l8.5-8.5a4 4 0 0 1 6.5-2z" />`,
  skip: html`<path d="M5 5v14l9-7z" /><path d="M17 5v14" />`,
};

export const Icon = Object.fromEntries(Object.entries(D).map(([k, d]) => [k, (size) => svg(d, size)]));

/** The app's mark: a strip of film with a play mark. */
export const Mark = () => html`<svg viewBox="0 0 40 40" aria-hidden="true" fill="none">
  <rect x="1.5" y="1.5" width="37" height="37" stroke="var(--ink)" stroke-width="1.4" />
  <g stroke="var(--tool)" stroke-width="1.8"><rect x="7" y="11" width="26" height="18" /></g>
  <g fill="var(--tool)"><rect x="10" y="7.5" width="4" height="2" /><rect x="18" y="7.5" width="4" height="2" /><rect x="26" y="7.5" width="4" height="2" /><rect x="10" y="30.5" width="4" height="2" /><rect x="18" y="30.5" width="4" height="2" /><rect x="26" y="30.5" width="4" height="2" /></g>
  <path d="M17 15.5v9l8-4.5z" fill="var(--mark)" />
  <path d="M6 36h28M6 34.6v2.8M34 34.6v2.8" stroke="var(--ink)" stroke-width=".8" />
</svg>`;
