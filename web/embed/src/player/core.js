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
// element's `profile` attribute — "slim" (the default) or "full" — and the
// two directories are derived from the loader script's `data-runtime-base`:
// a value whose last segment is `pkg`/`pkg-slim` is the legacy exact-directory
// form (its ±slim sibling completes the pair); anything else is a parent
// directory containing both. The default parent is beside this component, so
// an omitted attribute resolves to pkg-slim with pkg as the fallback when the
// host did not build that profile.
function engineBases(profile) {
  const wantSlim = profile !== "full";
  const override = LOADER_SCRIPT?.getAttribute("data-runtime-base");
  const base = override
    ? new URL(override, document.baseURI).href.replace(/\/$/, "")
    : `${RUNTIME_BASE}/..`;
  const last = base.split("/").pop();
  let slimDir;
  let fullDir;
  if (last === "pkg-slim") {
    slimDir = base;
    fullDir = base.slice(0, -"-slim".length);
  } else if (last === "pkg") {
    fullDir = base;
    slimDir = `${base}-slim`;
  } else {
    slimDir = `${base}/pkg-slim`;
    fullDir = `${base}/pkg`;
  }
  const primary = wantSlim ? slimDir : fullDir;
  const fallback = wantSlim ? fullDir : slimDir;
  return primary === fallback ? [primary] : [primary, fallback];
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

// Page-wide render-scale adaptation. A frame's GPU cost tracks the number of
// pixels the offscreen target holds — an empty 1280x720 scene measures the same
// as a full one, because the cost is vello's full-screen pass — so the lever for
// "several figures on one page are choppy" is raster resolution, not content.
// Every playing embed already renders at its own displayed size (see
// `_applyRenderScale`); when the shared rAF still cannot hold a frame, the whole
// page steps down a multiplier together, and steps back up only after a long
// stretch of comfortable frames. Stepping one notch at a time and reading the
// clock is deliberate: the alternative — sizing by how many embeds happen to be
// playing — guesses at work it cannot measure.
const QUALITY_STEPS = [1, 0.88, 0.75];
const SLOW_TICK_MS = 24; // a 60 Hz tick that misses its vsync lands at ~33 ms
const FAST_TICK_MS = 18.5;
let qualityStep = 0;
let slowTicks = 0;
let fastTicks = 0;

function setQualityStep(step) {
  const next = Math.max(0, Math.min(QUALITY_STEPS.length - 1, step));
  if (next === qualityStep) return;
  qualityStep = next;
  for (const inst of instances) inst._applyRenderScale();
}

/// Drive `advance` for every playing instance, then adapt the page's render
/// scale to how long the tick actually took.
function tick(t) {
  const dtMs = t - lastT;
  const dt = Math.min(dtMs / 1000, 0.1);
  lastT = t;
  let anyPlaying = false;
  for (const inst of instances) {
    if (inst.advance(dt)) anyPlaying = true;
  }
  if (anyPlaying) {
    if (dtMs > 100) {
      // Background tab resumption or long GC stall — ignore for quality adjustment.
    } else if (dtMs > SLOW_TICK_MS) {
      slowTicks += 1;
      fastTicks = 0;
      if (slowTicks >= 12) {
        setQualityStep(qualityStep + 1);
        slowTicks = 0;
      }
    } else if (dtMs < FAST_TICK_MS) {
      fastTicks += 1;
      if (slowTicks > 0) slowTicks -= 1;
      if (fastTicks >= 30) {
        setQualityStep(qualityStep - 1);
        fastTicks = 0;
      }
    } else if (slowTicks > 0) {
      slowTicks -= 1;
    }
    requestAnimationFrame(tick);
  } else {
    rafRunning = false;
  }
}

function ensureLoop() {
  if (!rafRunning) {
    rafRunning = true;
    lastT = performance.now();
    requestAnimationFrame(tick);
  }
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
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, rafRunning,
  lastT, QUALITY_STEPS, SLOW_TICK_MS, FAST_TICK_MS, qualityStep, slowTicks, fastTicks,
  setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS, LOAD_MAX_DEFER_MS, LOAD_RETRY_MS,
  LOAD_STALL_MS, loadQueue, arriving, loadTimer, lastScrollAt, pumpLoads, enqueueLoad,
  dequeueLoad, ASPECTS,
};
