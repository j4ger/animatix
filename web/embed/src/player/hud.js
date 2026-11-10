// `?amxdebug` stats HUD — one page-level panel that says what every embed is
// actually doing, so "the page feels choppy" can be read as numbers instead of
// guessed at.
//
// Why a page-level overlay rather than a per-element badge:
//   - the interesting questions are page-wide (achieved cadence, how many
//     figures are drawing, how much raster the page asks for), and a badge per
//     figure cannot answer them;
//   - it must work on embeds without `controls`, which is most of the site;
//   - it lives outside the shadow DOM, so the element's own layout cannot clip
//     it and it never scrolls away from the numbers it describes.
//
// Why 5 Hz and one `textContent` write: an overlay that costs a frame or a
// layout per tick changes the thing it measures. Everything here is a cached
// string written into one `<pre>`; nothing is ever read back from the DOM.

import * as core from "./core.js";

const { DEBUG_BY_URL, FRAME_RATE, stats } = core;

const PANEL_ID = "amx-debug-hud";
const UPDATE_MS = 200;

const STYLE = `
#${PANEL_ID} {
  position: fixed; top: 8px; right: 8px; z-index: 2147483647;
  pointer-events: none; max-width: min(56ch, calc(100vw - 16px));
  margin: 0; padding: 8px 10px; border-radius: 6px;
  background: rgba(5, 8, 12, .82); color: #dfe6ee;
  border: 1px solid rgba(138, 180, 248, .35);
  font: 11px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-variant-numeric: tabular-nums; white-space: pre; overflow: hidden;
  box-shadow: 0 6px 24px rgba(0, 0, 0, .35);
}
`;

/// `drawn`/`deduped` from the engine, when the running build exposes it. Older
/// engine packages do not, and a debug overlay must not throw because of it.
const dedupPct = (player) => {
  try {
    const s = player?.debug_dedup_stats?.();
    const drawn = s?.drawn ?? 0;
    const deduped = s?.deduped ?? 0;
    const total = drawn + deduped;
    return total > 0 ? Math.round((deduped / total) * 100) : null;
  } catch {
    return null;
  }
};

/// One line per embed. `rs` is the scale the engine actually rasters at, `disp`
/// the display match, and `q`/`px` the two budget multipliers — so a row that
/// lost sharpness shows which controller took it, and `▼` marks the case the
/// raster is below the display match (an upscaled blit).
function playerLine(inst, i) {
  const rs = inst._renderScale ?? 1;
  const disp = inst._displayScale ?? 1;
  const q = inst._qualityScale ?? 1;
  const px = inst._pixelScale ?? 1;
  const mark = rs < disp - 0.005 ? "▼" : rs > disp + 0.005 ? "▲" : " ";
  const state = (inst._state ?? "?").padEnd(5);
  const vis = inst._visible ? "vis" : " - ";
  // `park` is the page-level concurrency cap holding this figure off the clock
  // (`window.__amxMaxPlaying`), distinct from a reader's `pause`.
  const play = inst._parkedFromPage ? "park " : inst._playing ? "play " : "pause";
  const w = inst._player?.raster_width?.() ?? 0;
  const h = inst._player?.raster_height?.() ?? 0;
  const cost = (inst.costMs ?? 0) > 0 ? `${inst.costMs.toFixed(2)}ms` : "   —  ";
  const dedup = dedupPct(inst._player);
  return `#${i} ${state} ${vis} ${play} ${mark}rs ${rs.toFixed(2)} ` +
    `(disp ${disp.toFixed(2)} ×q ${q.toFixed(2)} ×px ${px.toFixed(2)}) ` +
    `${w}×${h} ${String(inst.targetFps ?? FRAME_RATE).padStart(2)}fps ${cost} ` +
    `p ${(inst.pressure ?? 0).toFixed(2)} ${inst._why || "ok"}` +
    (dedup === null ? "" : ` dedup ${dedup}%`);
}

function pageLines(fps, drawsPerSecond) {
  // Read the players from the document, not from `core.instances`: a page that
  // imports this bundle twice gets two module instances, and the HUD may be
  // mounted from the copy whose set is empty while the other one drives the
  // elements. Every field read below is an own property of the element, so it
  // does not matter which module defined the class.
  const list = [...document.querySelectorAll("amx-player")].filter((i) => i.isConnected);
  const visible = list.filter((i) => i._visible).length;
  const playing = list.filter((i) => i._playing).length;
  const raster = list.reduce((sum, i) => sum + (i.rasterPixels ?? 0), 0);
  const budget = window.__amxPageRasterPixels ?? 4_000_000;
  const dpr = window.devicePixelRatio || 1;
  const canvas = list.find((i) => i._player)?._canvas;
  const canvasText = canvas ? `${canvas.width}×${canvas.height}` : "—";
  return [
    `animatix ?amxdebug   dpr ${dpr.toFixed(2)}   canvas ${canvasText}`,
    `page  fps ${fps.toFixed(1)} / grid ${FRAME_RATE}   ` +
      `players ${list.length} (vis ${visible}, play ${playing})   ` +
      `draws/s ${drawsPerSecond.toFixed(0)}   ` +
      `raster ${(raster / 1e6).toFixed(2)}/${(budget / 1e6).toFixed(1)} MPx`,
  ];
}

/// Mount the panel. Idempotent: a second call (another module evaluation, a page
/// that imports the bundle twice) returns the existing one.
export function installDebugHud() {
  if (!DEBUG_BY_URL) return null;
  const existing = document.getElementById(PANEL_ID);
  if (existing) return existing;
  // Same duplicate-module hazard as `stats`: one panel, one timer per page.
  if (globalThis.__amxHudInstalled) return document.getElementById(PANEL_ID);
  globalThis.__amxHudInstalled = true;

  const mount = () => {
    if (document.getElementById(PANEL_ID)) return;
    const style = document.createElement("style");
    style.textContent = STYLE;
    const panel = document.createElement("pre");
    panel.id = PANEL_ID;
    panel.setAttribute("aria-hidden", "true"); // decoration, not content
    document.head.appendChild(style);
    document.body.appendChild(panel);
  };
  if (document.body) mount();
  else document.addEventListener("DOMContentLoaded", mount, { once: true });

  let lastAt = performance.now();
  let lastSteps = stats.steps;
  let lastRenders = stats.renders;
  setInterval(() => {
    const panel = document.getElementById(PANEL_ID);
    if (!panel) return;
    const now = performance.now();
    const dt = Math.max(1, now - lastAt);
    const fps = ((stats.steps - lastSteps) * 1000) / dt;
    const draws = ((stats.renders - lastRenders) * 1000) / dt;
    lastAt = now;
    lastSteps = stats.steps;
    lastRenders = stats.renders;
    const list = [...document.querySelectorAll("amx-player")].filter((i) => i.isConnected);
    const lines = pageLines(fps, draws);
    for (const [i, inst] of list.entries()) lines.push(playerLine(inst, i));
    panel.textContent = lines.join("\n");
  }, UPDATE_MS);

  return document.getElementById(PANEL_ID);
}
