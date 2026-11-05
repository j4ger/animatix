const FRAME_RATE = 60;

// `?amxdebug` turns the readout on for every player on the page, so a deployed
// page can be inspected without editing its markup.
const DEBUG_BY_URL = /[?&]amxdebug\b/.test(location.search);

const LOADER_SCRIPT = [...document.querySelectorAll("script[type=module]")].find((s) =>
  (s.src || "").includes("amx-player.js"),
);

const RUNTIME_BASE = (() => {
  const src = LOADER_SCRIPT?.src || "";
  const i = src.lastIndexOf("/amx-player.js");
  if (i >= 0) return src.slice(0, i);
  return new URL(".", import.meta.url).href.replace(/\/$/, "");
})();

// Engine directories per profile. Each embed picks its profile through the
// element's `profile` attribute — "auto" (default), "slim", "assets" or
// "full" — and the directories are derived from the loader script's
// `data-runtime-base`. A value whose last segment is `pkg-slim`,
// `pkg-assets` or `pkg` is the legacy exact-directory form; anything else is
// a parent directory containing the three packages. The default parent is
// beside this component, so an omitted attribute starts at pkg-slim and may
// auto-upgrade from there.
function engineDirs() {
  const override = LOADER_SCRIPT?.getAttribute("data-runtime-base");
  const base = override
    ? new URL(override, document.baseURI).href.replace(/\/$/, "")
    : `${RUNTIME_BASE}/..`;
  const last = base.split("/").pop();
  if (last === "pkg-slim") {
    return {
      slim: base,
      assets: base.replace(/-slim$/, "-assets"),
      full: base.replace(/-slim$/, ""),
    };
  }
  if (last === "pkg-assets") {
    return {
      slim: base.replace(/-assets$/, "-slim"),
      assets: base,
      full: base.replace(/-assets$/, ""),
    };
  }
  if (last === "pkg") {
    return {
      slim: `${base}-slim`,
      assets: `${base}-assets`,
      full: base,
    };
  }
  return {
    slim: `${base}/pkg-slim`,
    assets: `${base}/pkg-assets`,
    full: `${base}/pkg`,
  };
}

function fontBase() {
  const override = LOADER_SCRIPT?.getAttribute("data-font-base");
  if (override) return new URL(override, document.baseURI).href.replace(/\/$/, "");
  // Fonts sit beside the package directories, not inside any one profile.
  const dirs = engineDirs();
  return new URL("../fonts/", `${dirs.slim}/`).href.replace(/\/$/, "");
}

function engineBases(profile) {
  const dirs = engineDirs();
  // A package may fall back to a larger package that contains it, but never
  // to a smaller one: silently running full scenes on slim is how values got
  // dropped. An explicit full request never falls back.
  if (profile === "full") return [dirs.full];
  if (profile === "assets") return [dirs.assets, dirs.full];
  return [dirs.slim, dirs.assets, dirs.full];
}

const REDUCED_MOTION = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
const SAVE_DATA = navigator.connection?.saveData === true;

// Loop treatment. FADE_EACH is the dissolve on either side of a looping
// restart; it never exceeds a third of the cycle so the finished frame always
// gets real rest time.
const FADE_EACH = 0.28;

// Control-bar glyphs (inline SVG, no deps). Shared by the bar's play/pause
// button and the no-controls center affordance.
const ICONS = {
  playBig:
    '<svg width="26" height="26" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>',
  play:
    '<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>',
  pause:
    '<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 3h3v12h-3zm6 0h3v12h-3z"/></svg>',
  replay:
    '<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M9 3a6 6 0 1 0 6 6h-2a4 4 0 1 1-4-4V2l4 3-4 3V5z"/></svg>',
  stepPrev:
    '<svg width="16" height="16" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M12 4l-6 5 6 5zM4 4h2v10H4z"/></svg>',
  stepNext:
    '<svg width="16" height="16" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M6 4l6 5-6 5zM12 4h2v10h-2z"/></svg>',
};

// ── shared engine loading ───────────────────────────────────────────

// Resolved engine directory → the module instance loading for it. Keyed by
// directory (not profile string) so the legacy exact-directory and the new
// parent-directory forms that resolve to the same place share one instance.
const enginePromises = new Map();

function loadEngine(profile) {
  // One wasm instance per resolved directory: embeds sharing a profile share
  // the download, the device and the renderer; a page mixing profiles pays
  // for both (each ES module instantiation owns its engine context).
  const [primary] = engineBases(profile);
  if (!enginePromises.has(primary)) {
    enginePromises.set(
      primary,
      (async () => {
        const bases = engineBases(profile);
        let lastError;
        for (const [i, base] of bases.entries()) {
          let module;
          try {
            module = await import(`${base}/animatix_web.js`);
          } catch (err) {
            // The profile is not deployed at this base — worth retrying
            // with the other one (a single-profile host).
            lastError = err;
            enginePromises.delete(primary);
            if (i + 1 < bases.length) {
              console.info(`amx-player: no engine at ${base}, trying ${bases[i + 1]}`);
            }
            continue;
          }
          try {
            await module.default();
            await module.init_engine?.();
          } catch (err) {
            // The engine is present but cannot run here (typically no
            // WebGPU adapter). That is not profile-specific: falling back
            // to the other build would repeat the failure and mask this
            // error behind a 404 — surface it instead.
            enginePromises.delete(primary);
            throw err;
          }
          return module;
        }
        throw lastError;
      })(),
    );
  }
  return enginePromises.get(primary);
}

// ── shared render loop ──────────────────────────────────────────────

const instances = new Set();
let rafRunning = false;
let lastT = 0;
let pendingDt = 0;

// ── adaptive quality: real frame pressure, then a cadence ladder ─────
//
// The 2026-11-04 "per-instance budget" controller fed on the synchronous CPU
// time of `render_frame()`. That cannot see GPU work: `queue.submit` returns
// before the GPU draws, and docs/performance_evaluation.md §3.7 measures
// 0.49 ms of CPU encode for a frame whose GPU work is 3.5 ms. With that signal
// the controller computed a 60 fps target and never lowered the raster on any
// hardware — measured on six SwiftShader embeds: target 60 fps, achieved
// ~1 fps, zero adaptation.
//
// The signal here is the ACHIEVED frame cadence: a missed-vsync ratio over the
// committed 60 Hz steps. It is measured from the rAF timestamp gaps, which do
// include GPU backpressure, but is gated so an exogenous 300 ms main-thread
// stall, a scroll, or a hidden tab cannot masquerade as render cost.
// `_costMs` is demoted to a relative weight (which embeds to stagger first) and
// a diagnostic; it is NOT the primary control input.
//
// Strategy: a page-wide cadence ladder lowers every playing embed's target fps
// first (cheap: no sharpness loss), and only at the ladder floor with sustained
// pressure does it try a smaller raster — with a trial that is judged by the
// pressure ratio, not by CPU encode time.
const QUALITY_STEPS = [1, 0.85, 0.7, 0.55, 0.42, 0.32, 0.25];
// Kept for modules that still import them; the scheduler no longer uses a
// global threshold pair.
const SLOW_TICK_MS = 24;
const FAST_TICK_MS = 18.5;
let qualityStep = 0;
let slowTicks = 0;
let fastTicks = 0;

const MAX_FPS = 60;
const FLOOR_FPS = 20;
// Headless/occluded windows deliver no rAF callbacks, so a probe may drive the
// scheduler by hand via `?amxmanualtick`. Production never sets this.
const MANUAL_TICK = /(?:\?|&)amxmanualtick\b/.test(location.search);
// Total render time per second the page may spend across all playing embeds.
// This is a *fraction of the frame*: a tick that renders N figures at 60 fps
// has to fit in 16.7 ms, so the sustainable ceiling is near 1000 ms/s; leave a
// margin for scroll, layout and the rest of the page. 0.9 of 900 keeps pages
// with several figures at 60 fps instead of throttling them defensively.
const PAGE_BUDGET_MS_PER_S = 900;
const BUDGET_HEADROOM = 0.9;
// Page-wide raster-pixel budget: the sum of every playing embed's
// display-matched raster (CSS px × devicePixelRatio, before the quality ladder)
// must fit here, or `allocateBudget` scales them all down together. The cadence
// ladder in playback.js is a *frame-rate* concession; this is the *area* one,
// and it applies even on a fast machine where no frame is ever missed — six
// 1080p embeds are 12.4 Mpx of GPU raster per frame regardless of fps.
// `window.__amxPageRasterPixels` (finite and > 0) overrides it for probes and
// pages that know their own budget.
const MAX_PAGE_RASTER_PIXELS = 4_000_000;
// Learned from the first frames so a 120/144 Hz display renders the 60 fps
// timeline at 60, rather than twice. Only short gaps are averaged: a 23–54 ms
// synchronous wasm build during arrival scheduling must not pollute it.
let refreshInterval = 1000 / 60;
let refreshFrames = 0;
let refreshSum = 0;

// Frame-pressure bookkeeping. `lastCommittedT` is updated ONLY on a committed
// 60 Hz step, so a 120/144 Hz display still measures ~16.7 ms per step.
const CADENCE_STEPS = [60, 50, 40, 30, 24, 20];
const MISSED_STEP_MS = (1000 / FRAME_RATE) * 1.4; // >23.3 ms at 60 Hz
const EXOGENOUS_GAP_MS = 200;
let cadenceStep = 0;
let pagePressure = 0;
let lastCommittedT = 0;
let pressureSteps = 0;
let pressureMissed = 0;
let cleanWindows = 0;
let longGapRun = 0;
let maxRendersPerTick = 1;
let lastWindowAt = 0;
const pausedByVisibility = new Set();

/// Manual/debug override: pin every embed to one rung of the scale ladder.
function setQualityStep(step) {
  const next = Math.max(0, Math.min(QUALITY_STEPS.length - 1, step));
  if (next === qualityStep) return;
  qualityStep = next;
  for (const inst of instances) {
    inst._qualityScale = QUALITY_STEPS[next];
    inst._scaleTrial = null;
    inst._scaleIneffectiveUntil = 0;
    inst._applyRenderScale();
  }
}

/// One shared rAF driver. Each tick advances every playing embed's clock, but
/// renders an embed only when its own cadence is due.
function tick(t) {
  // Seed the loop's own clock from the first timestamp it actually receives.
  // In manual-tick mode the probe hands us a virtual clock, so seeding from
  // `performance.now()` at ensureLoop() time (which is later, once the player
  // is ready) would make the first many ticks look like negative gaps and be
  // swallowed. Seeding lazily keeps `rawDt`, the pressure window and the
  // committed-step interval all on the SAME clock.
  if (lastT === 0) {
    lastT = t;
    lastWindowAt = t;
    lastCommittedT = t;
    pendingDt = 0;
    requestAnimationFrame(tick);
    return;
  }
  const rawDt = t - lastT;
  lastT = t;

  // Learn the display refresh interval from the first ~40 frames. Only short
  // gaps count: serialised synchronous wasm builds (23–54 ms each) land exactly
  // in this window and would otherwise cap the target fps for the page's life.
  if (refreshFrames < 40) {
    if (rawDt > 2 && rawDt < 40) {
      refreshSum += rawDt;
      refreshFrames += 1;
    }
    if (refreshFrames === 40) {
      const avg = refreshSum / 40;
      if (avg > 3 && avg < 40) refreshInterval = avg;
    }
  }

  if (!MANUAL_TICK && document.visibilityState === "hidden") {
    requestAnimationFrame(tick);
    return;
  }
  // A SINGLE gap this long is exogenous: a background tab resuming, a GC, a
  // layout storm or a blocking script. Ignore it and reset the clocks. But
  // SUSTAINED long gaps are the shape of a GPU that cannot keep up (a software
  // WebGPU adapter delivers one rAF per second), so from the second one on we
  // clamp and keep measuring instead of going permanently blind.
  if (rawDt > EXOGENOUS_GAP_MS) {
    longGapRun += 1;
    if (longGapRun < 2) {
      pendingDt = 0;
      lastCommittedT = t;
      for (const inst of instances) inst._reseedClock();
      requestAnimationFrame(tick);
      return;
    }
  } else {
    longGapRun = 0;
  }

  pendingDt += Math.min(rawDt / 1000, 0.1);
  // The timeline is authored at FRAME_RATE. Gate on *accumulated* time so a
  // 120/144 Hz display steps on a 60 Hz grid instead of skipping every frame.
  const frameMs = 1000 / FRAME_RATE;
  if (pendingDt * 1000 < frameMs - 1) {
    requestAnimationFrame(tick);
    return;
  }
  const dt = pendingDt;
  pendingDt = 0;
  const now = performance.now();

  // Pressure is measured on committed steps only, from the rAF timestamp gap.
  // The manual-tick probe feeds fake monotonic timestamps with ~40 ms deltas;
  // real `performance.now()` barely advances there, so any pressure signal
  // derived from render cost would be unable to see the injection.
  const committedInterval = lastCommittedT > 0
    ? Math.min(Math.max(t - lastCommittedT, 0), EXOGENOUS_GAP_MS)
    : 0;
  lastCommittedT = t;
  const scrollActive = now - lastScrollAt < SCROLL_SETTLE_MS;
  const measurable = (MANUAL_TICK || document.visibilityState === "visible")
    && !scrollActive && committedInterval > 0;
  if (measurable) {
    pressureSteps += 1;
    if (committedInterval > MISSED_STEP_MS) pressureMissed += 1;
  }

  let anyPlaying = false;
  let rendersThisTick = 0;
  for (const inst of instances) {
    if (!inst.advance(dt)) continue;
    anyPlaying = true;
    if (inst._wantRender && inst._dueRender(now) && rendersThisTick < maxRendersPerTick) {
      const t0 = performance.now();
      inst._renderScene();
      inst._noteRenderMs(performance.now() - t0, now);
      inst._wantRender = false;
      rendersThisTick += 1;
    }
  }

  if (anyPlaying) {
    // Window boundaries are WALL-CLOCK (on the rAF timestamp `t`, the same
    // clock as `rawDt`), not the sum of `dt`: on a machine slow enough to be
    // the reason this exists, one tick can be ~1 s and a clamped `dt` (≤0.1 s)
    // would take ten ticks to fill a 0.5 s window — so pressure would never be
    // computed exactly when it is highest.
    if (t - lastWindowAt >= 500) {
      lastWindowAt = t;
      pagePressure = pressureSteps > 0 ? pressureMissed / pressureSteps : 0;
      pressureSteps = 0;
      pressureMissed = 0;
      if (pagePressure > 0.25) {
        cadenceStep = Math.min(CADENCE_STEPS.length - 1, cadenceStep + 1);
        cleanWindows = 0;
      } else if (pagePressure < 0.05) {
        cleanWindows += 1;
        if (cleanWindows >= 3) {
          cadenceStep = Math.max(0, cadenceStep - 1);
          cleanWindows = 0;
        }
      } else {
        cleanWindows = 0;
      }
      allocateBudget();
    }
    requestAnimationFrame(tick);
  } else {
    rafRunning = false;
  }
}

/// The page's raster-pixel ceiling: the override when a page or probe sets a
/// finite positive `window.__amxPageRasterPixels`, else the built-in budget.
/// Read at allocation time, not module init, so a driver can install it before
/// the first player exists.
function pageRasterBudget() {
  const override = Number(window.__amxPageRasterPixels);
  return Number.isFinite(override) && override > 0 ? override : MAX_PAGE_RASTER_PIXELS;
}

/// Re-split the work across the visible, playing embeds. The cadence ladder is
/// page-wide; each embed gets a phase offset so N renders do not all land in the
/// same rAF tick, plus a hard per-tick cap derived from the ladder. `_costMs`
/// only decides who is staggered first (costliest first) — it no longer decides
/// whether adaptation happens at all.
function allocateBudget() {
  const active = [];
  for (const inst of instances) {
    if (inst._visible && inst._playing && inst._state === "ready") active.push(inst);
  }
  const n = active.length;
  if (!n) return;
  active.sort((a, b) => (b._costMs || 0) - (a._costMs || 0));
  // Page-wide area budget: when the display-matched rasters together exceed it
  // (with 5% slack so a hair over budget does not downscale everything), every
  // embed takes the same square-root factor. Uniform scaling keeps the page
  // looking consistent; a per-embed split would make the largest figure soft
  // while a small one stayed sharp.
  const budget = pageRasterBudget();
  let totalDesired = 0;
  for (const inst of active) totalDesired += inst._desiredRasterPixels?.() ?? 0;
  const pagePixelScale = totalDesired > budget * 1.05
    ? Math.max(0.25, Math.sqrt(budget / totalDesired))
    : 1;
  const pageTargetFps = CADENCE_STEPS[cadenceStep];
  const perInstanceMsPerSec = (PAGE_BUDGET_MS_PER_S * BUDGET_HEADROOM) / n;
  maxRendersPerTick = Math.max(1, Math.ceil(n * pageTargetFps / FRAME_RATE));
  for (let i = 0; i < n; i += 1) {
    active[i]._adaptBudget(pageTargetFps, perInstanceMsPerSec, i, n, pagePressure, pagePixelScale);
  }
}

function ensureLoop() {
  if (!rafRunning) {
    rafRunning = true;
    // Real rAF: seed from the wall clock. Manual tick: leave zeroed so `tick`
    // lazily seeds from the probe's virtual clock on its first timestamp.
    if (MANUAL_TICK) {
      lastT = 0;
      lastWindowAt = 0;
    } else {
      lastT = performance.now();
      lastWindowAt = performance.now();
    }
    lastCommittedT = 0;
    pressureSteps = 0;
    pressureMissed = 0;
    for (const inst of instances) inst._reseedClock();
    requestAnimationFrame(tick);
  }
}

// Test hook: drive the scheduler by hand. A headless or occluded window never
// fires rAF, so the adaptive loop cannot be observed there otherwise.
if (MANUAL_TICK) {
  // Leave lastT zeroed: `tick` seeds from the first virtual timestamp it
  // receives, so the probe's virtual clock and the loop share one time base.
  lastT = 0;
  lastWindowAt = 0;
  window.__amxTick = (t) => tick(typeof t === "number" ? t : performance.now());
  window.__amxInstances = () => [...instances];
}

// Polling `visibilityState` inside tick() is not enough: a hidden tab leaves
// `_playing` flags set, which keeps `allocateBudget`'s active set populated and
// lets a page-driven play() fire into a hidden document. Pause explicitly on
// hide and resume ONLY the embeds this listener paused, and only while they are
// still connected, ready and visible — an embed the page paused on purpose is
// never resurrected.
if (!MANUAL_TICK && typeof document.addEventListener === "function") {
  document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "hidden") {
      for (const inst of [...instances]) {
        if (inst._playing) {
          pausedByVisibility.add(inst);
          inst.pause();
        }
      }
    } else {
      for (const inst of pausedByVisibility) {
        if (inst.isConnected && inst._state === "ready" && inst._visible) inst.play();
      }
      pausedByVisibility.clear();
      lastCommittedT = 0;
      pressureSteps = 0;
      pressureMissed = 0;
    }
  });
}

// ── arrival scheduling ──────────────────────────────────────────────
// A player's first frame is a burst of main-thread work: fetch, one or two
// *synchronous* wasm builds (23–54 ms per tour scene, measured), then the first
// render. Starting that mid-scroll puts a long task inside the frames the reader
// is using to move, and that is the shape of "the site is choppy" — a steady
// frame on real hardware is ~4 ms. So arrivals go through one queue: a page that
// is still scrolling starts nothing, and only one load runs at a time.
// `LOAD_MAX_DEFER_MS` is the safety valve: a page that never stops scrolling
// still loads its figures rather than starving them.
const SCROLL_SETTLE_MS = 140;
const LOAD_MAX_DEFER_MS = 900;
const LOAD_RETRY_MS = 50;
// Serialising arrivals is also a single point of failure, so one wedged load —
// a request that neither resolves nor rejects — must not hold every later
// figure behind it. Well past any honest arrival on a slow connection.
const LOAD_STALL_MS = 8000;
const loadQueue = [];
let arriving = null;
let loadTimer = 0;
let lastScrollAt = -Infinity;

addEventListener(
  "scroll",
  () => {
    // Capture, not bubble: an element scroller's `scroll` event does not bubble,
    // but it still passes window on the way down.
    lastScrollAt = performance.now();
  },
  { capture: true, passive: true },
);

function pumpLoads() {
  clearTimeout(loadTimer);
  loadTimer = 0;
  if (arriving || !loadQueue.length) return;
  const inst = loadQueue[0];
  if (!inst._visible) {
    // Scrolled past before its turn came. The visibility observer queues it
    // again if it comes back, so this is work the reader never waited for.
    loadQueue.shift();
    pumpLoads();
    return;
  }
  const now = performance.now();
  const scrolling = now - lastScrollAt < SCROLL_SETTLE_MS;
  const overdue = now - inst._loadWantedAt > LOAD_MAX_DEFER_MS;
  if (scrolling && !overdue) {
    loadTimer = setTimeout(pumpLoads, LOAD_RETRY_MS);
    return;
  }
  loadQueue.shift();
  arriving = inst;
  let stall = 0;
  const release = () => {
    clearTimeout(stall);
    if (arriving !== inst) return; // a stall already handed the queue on
    arriving = null;
    pumpLoads();
  };
  stall = setTimeout(release, LOAD_STALL_MS);
  Promise.resolve(inst._startLoad()).then(release, release);
}

function enqueueLoad(inst) {
  if (loadQueue.includes(inst)) return;
  inst._loadWantedAt = performance.now();
  loadQueue.push(inst);
  pumpLoads();
}

function dequeueLoad(inst) {
  const i = loadQueue.indexOf(inst);
  if (i >= 0) loadQueue.splice(i, 1);
}

// ── the element ─────────────────────────────────────────────────────

const ASPECTS = {
  "16:9": 16 / 9,
  "4:3": 4 / 3,
  "1:1": 1,
  "9:16": 9 / 16,
};

export {
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, fontBase, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, rafRunning,
  lastT, QUALITY_STEPS, SLOW_TICK_MS, FAST_TICK_MS, qualityStep, slowTicks, fastTicks,
  setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS, LOAD_MAX_DEFER_MS, LOAD_RETRY_MS,
  LOAD_STALL_MS, loadQueue, arriving, loadTimer, lastScrollAt, pumpLoads, enqueueLoad,
  dequeueLoad, ASPECTS, refreshInterval, CADENCE_STEPS, cadenceStep, pagePressure,
};
