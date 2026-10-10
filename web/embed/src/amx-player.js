// <amx-player> — embed a .amx scene in any page, like <img> for animations.
//
// Usage:
//   <script type="module" src="…/amx-player.js"></script>
//   <amx-player src="./scenes/attention.amx" autoplay loop controls></amx-player>
//
// Attributes:
//   src        (required) URL of the .amx file
//   autoplay   play when scrolled into view (default unless reduced-motion/save-data)
//   loop       restart from the beginning at the end of the timeline
//   hold       seconds to keep the finished timeline on screen before a looping
//              restart (default 0.7; 0 loops with no rest at all)
//   controls   a full-height timeline strip BELOW the picture (never covers
//              it): the engine's `#` keyframe marks are drawn as ticks and
//              snapping targets (compositions add scene diamonds and hatched
//              transition spans), plus a time chip and a speed cycle
//              (1x -> 1.5x -> 2x -> 0.5x). Mouse: hovering freezes the clock
//              and peeks the frame under the pointer; leaving resumes unless
//              a click latched the pause; canvas click pauses latched, strip
//              click resumes. Touch: tap toggles, drag scrubs. Keyboard:
//              arrows step between landmarks, Home restarts, Space/K
//              toggles. With the debug readout on, Shift+arrows step exactly
//              one frame (pausing first) and `c` copies the readout.
//   title      accessibility label; shown on the skeleton while loading
//   aspect     "16:9" | "4:3" | "1:1" | "9:16" — reserve space before first frame
//              (auto-detected from the scene afterwards)
//   profile    "slim" (default) | "full" — which engine build backs this
//              element. Slim plays plain-text scenes; full adds Typst markup,
//              Code/Math highlighting and Image/Svg assets. Embeds sharing a
//              profile share one engine download and device; a page mixing
//              both downloads each once.
//   quality    "draft" (default) | "preview" | "production" — build fidelity.
//              Production matches a desktop export; changing it rebuilds.
//   sealed     page-driven mode: no center play affordance and no click
//              gestures — playback belongs to the page's own JS (play/pause/
//              seek below). With `autoplay`, the scene also resumes by itself
//              whenever it re-enters the viewport (a sealed embed has no
//              visible control to resume it with). Used for full-bleed hero
//              plates, scroll-scrubbed figures and hover-play cards.
//   debug-frame  show a frame readout in the timeline strip: the frame index
//              on a nominal 60 fps reporting grid, the exact time in seconds
//              (what `animatix image --time` takes), and the active render
//              scale. For locating and reporting a broken frame. Needs
//              `controls` (the readout lives in the strip); `?amxdebug` on the
//              page URL turns it on for every player without editing markup,
//              and `d` toggles it while the strip has focus.
//   fit        "contain" (default — the whole frame, letterboxed) | "cover"
//              — the frame fills the element box, cropping overflow. Cover
//              also unlocks the stage from its aspect-ratio box so the
//              picture fills whatever box the page gives the element (the
//              full-bleed hero). Design cover scenes with a safe center.
//
// The engine directories come from the loader <script>'s `data-runtime-base`
// (absolute, or relative to the page). A value ending in `pkg`/`pkg-slim` is
// the legacy exact-directory form (its ±slim sibling completes the pair);
// anything else is a parent directory containing both. Default: the parent of
// the directory holding this component — so an omitted attribute resolves to
// `pkg-slim`, falling back to `pkg` when that profile was not built.
//
// Loading UX (no build-time poster required):
//   1. skeleton with shimmer + title, correct aspect ratio
//   2. lazy: nothing is fetched until the element approaches the viewport
//   3. first rendered frame becomes the poster; autoplay decision follows
//   4. paused instances show a subtle play affordance
//
// Looping: a scene's timeline ends at its last keyframe — or at a declared
// `config { duration: N }`, which overrides the inferred extent — so a naive
// wrap cuts from the finished composition straight to an empty first frame.
// Looping embeds therefore run a cycle of `hold` + `duration`: the finished
// frame rests, then dissolves out and back in. Playback length comes from the
// engine's playback duration (`Timeline::playback_duration_seconds`), so a
// scene can buy itself a trailing rest by declaring a longer duration.
//
// Performance: all visible playing instances are driven by ONE shared
// requestAnimationFrame loop; offscreen instances pause automatically. Embeds
// using the same engine directory share one WebGPU context inside that wasm
// instance; a page mixing `profile` values holds one context per profile.

// Nominal frame rate for the debug readout. The player renders on rAF, so this
// is a reporting grid rather than a clock: `frame = round(t * FRAME_RATE)`. The
// seconds beside it are the reproducible value — they go straight into
// `animatix image --time` — and the grid exists so two people can name the same
// frame out loud.
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

class AmxPlayerElement extends HTMLElement {
  static observedAttributes = [
    "src",
    "autoplay",
    "loop",
    "hold",
    "controls",
    "sealed",
    "fit",
    "title",
    "aspect",
    "profile",
    "quality",
  ];

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this._state = "idle"; // idle | loading | ready | error
    this._player = null;
    this._playing = false;
    this._loop = false;
    this._time = 0; // position within the playback cycle
    this._duration = 0;
    this._holdSeconds = 0;
    this._cycle = 0;
    this._fade = 0;
    this._restTime = 0; // frame shown while paused
    this._lastAlpha = 1;
    this._renderFailures = 0; // consecutive _renderScene failures; see below
    this._debugFrame = this.hasAttribute("debug-frame") || DEBUG_BY_URL;
    this._visible = false;
    this._observer = null;
    this._renderScaleObserver = null;
    this._renderScaleRetry = null;
    this._resizeDebounceTimer = null;
    this._initialized = false;
    this._scrubbing = false;
    this._rate = 1;
    this._markers = [];
    this._marks = null;
    this._loadWantedAt = 0; // set when the scheduler queues this player; see pumpLoads
    this._hudFlash = null;
    this._hudSkip = null;
    this._hudTimer = null;
    this._skipHudTimer = null;
    this._singleTapTimer = null;
    this._lastTapTime = 0;
    this._lastTapX = 0;
    this._resumeOnScrubEnd = false;
    this._lastSnappedMarker = null;
  }

  connectedCallback() {
    if (!this._initialized) {
      this._initialized = true;
      this._renderSkeleton();
      this._setupObserver();
      if (this.hasAttribute("eager")) this.loadNow();
      return;
    }
    // Re-attached after a DOM move — a live editor re-parents figures, and
    // the detach above disconnected the visibility observer and evicted the
    // element from the shared rAF loop. Re-arm both; an embed that was mid
    // load resumes through the observer, a paused one keeps its frame.
    if (this._state !== "error") {
      this._setupObserver();
      if (this._player && this._playing) {
        instances.add(this);
        ensureLoop();
      }
    }
  }

  disconnectedCallback() {
    this._observer?.disconnect();
    this._renderScaleObserver?.disconnect();
    clearTimeout(this._renderScaleRetry);
    clearTimeout(this._resizeDebounceTimer);
    clearTimeout(this._hudTimer);
    clearTimeout(this._skipHudTimer);
    clearTimeout(this._singleTapTimer);
    instances.delete(this);
    dequeueLoad(this);
    // `_playing` is deliberately left as-is: connectedCallback re-adds a
    // still-playing embed to the shared loop after a DOM move. Clearing it
    // here would make that re-arm dead code and silently freeze the figure.
    // A re-attached player re-arms the visibility observer, which re-queues the
    // load if it had not started yet.
  }

  loadNow() {
    this._visible = true;
    if (this._state === "idle") this._maybeStartLoading();
  }

  attributeChangedCallback(name) {
    if ((name === "aspect" || name === "fit") && this._initialized) {
      this._applyFit();
    }
    if (name === "hold" && this._initialized && this._duration > 0) {
      this._configureCycle();
    }
    if (name === "quality" && this._initialized && this._player?.set_quality) {
      // Quality is baked in at build time (plot sampling tolerance): push it
      // to the player and rebuild. A reload refetches the scene text, which
      // is cheap and keeps the flow identical to a fresh load.
      try {
        this._player.set_quality(this._quality());
      } catch (err) {
        console.warn(`amx-player: ${err.message}`);
      }
      this._resetForReload();
    }
    if ((name === "src" || name === "profile") && this._initialized) {
      // profile selects which wasm instance backs this element; switching it
      // is a full reload (and, for a first switch on the page, a second
      // engine download — that is the page author's tradeoff).
      this._resetForReload();
    }
  }

  _resetForReload() {
    this._state = "idle";
    this._player = null;
    this._time = 0;
    this._renderSkeleton();
    this._maybeStartLoading();
  }

  /// `profile` — which engine build backs this element: "slim" (default) or
  /// "full". A slim engine cannot render `Svg` actors (warned and skipped)
  /// and falls back to the plain text path; see web/README.md's differences
  /// table. Directories come from the loader's `data-runtime-base`.
  _profile() {
    return this.getAttribute("profile") === "full" ? "full" : "slim";
  }

  /// `sealed` — page-driven mode (see the attribute docs above). Re-read on
  /// change so a page can seal or unseal an embed mid-flight.
  _isSealed() {
    return this.hasAttribute("sealed");
  }

  /// `fit` — "contain" (default) or "cover" (see the attribute docs above).
  _fit() {
    return this.getAttribute("fit") === "cover" ? "cover" : "contain";
  }

  /// Apply the current fit mode to the stage/canvas pair. Cover unlocks the
  /// stage from the aspect-ratio box (the page's element box is the frame)
  /// and lets the canvas crop; contain restores the aspect lock.
  _applyFit() {
    if (this._fit() === "cover") {
      this._stage.style.aspectRatio = "";
      this._stage.style.position = "absolute";
      this._stage.style.inset = "0";
      this._canvas.style.objectFit = "cover";
    } else {
      this._stage.style.position = "";
      this._stage.style.inset = "";
      this._canvas.style.objectFit = "contain";
      this._stage.style.aspectRatio = String(this._aspectRatio());
    }
  }

  /// `quality` — build fidelity: "draft" (default, the GUI editing preview),
  /// "preview", or "production" (what a desktop export renders). Unknown
  /// values fall back to draft with a warning; the wasm side re-validates.
  _quality() {
    const value = this.getAttribute("quality") ?? "draft";
    if (!["draft", "preview", "production"].includes(value)) {
      console.warn(`amx-player: unknown quality '${value}', using draft`);
      return "draft";
    }
    return value;
  }

  // ── skeleton / surfaces ─────────────────────────────────────────

  _aspectRatio() {
    const attr = this.getAttribute("aspect");
    if (attr && ASPECTS[attr]) return ASPECTS[attr];
    const [w, h] = (attr || "").split(":").map(Number);
    if (w > 0 && h > 0) return w / h;
    return 16 / 9;
  }

  _renderSkeleton() {
    // Reserve the figure's box up front so lazy loading never reflows the
    // page: the stage carries the aspect ratio until the scene reports its
    // own, and the control bar (when present) sits below it in flow.
    this._stage = document.createElement("div");
    this._stage.className = "stage";
    const title = this.getAttribute("title") || "";
    const style = document.createElement("style");
    style.textContent = `
      :host { display: block; position: relative; overflow: hidden;
              border-radius: 8px; background: #0a0f17; }
      .stage { position: relative; overflow: hidden; }
      .skeleton {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        background: linear-gradient(120deg, #0a0f17 40%, #141c28 50%, #0a0f17 60%);
        background-size: 300% 100%;
        animation: shimmer 2.2s linear infinite;
        color: #808fa6; font: 13px/1.4 system-ui, sans-serif;
        transition: opacity 0.28s cubic-bezier(0.2, 0, 0, 1);
      }
      .skeleton.fade-out { opacity: 0; pointer-events: none; }
      .skeleton-badge {
        display: inline-flex; align-items: center; gap: 8px;
        background: rgba(5,8,12,0.65); border: 1px solid rgba(255,255,255,0.1);
        border-radius: 20px; padding: 6px 14px; font: 12px system-ui, sans-serif;
        color: #c7cfd9;
      }
      .skeleton-spinner {
        width: 12px; height: 12px; border: 2px solid rgba(245,185,66,0.25);
        border-top-color: #f5b942; border-radius: 50%;
        animation: spin 0.8s linear infinite; display: none;
      }
      .skeleton.loading .skeleton-spinner { display: inline-block; }
      @keyframes spin { to { transform: rotate(360deg); } }
      @keyframes shimmer { to { background-position: -300% 0; } }
      canvas { position: absolute; inset: 0; width: 100%; height: 100%;
               display: block; object-fit: contain; }
      .hud-flash {
        position: absolute; left: 50%; top: 50%;
        transform: translate(-50%, -50%) scale(0.85);
        width: 64px; height: 64px; border-radius: 50%;
        background: rgba(5,8,12,0.75); border: 1px solid rgba(245,185,66,0.45);
        color: #f5b942; display: flex; align-items: center; justify-content: center;
        pointer-events: none; opacity: 0;
        transition: opacity 0.3s ease-out, transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
        z-index: 5;
      }
      .hud-flash.show { opacity: 1; transform: translate(-50%, -50%) scale(1.15); }
      .hud-flash svg { width: 26px; height: 26px; display: block; }
      .hud-skip {
        position: absolute; top: 50%; transform: translateY(-50%);
        padding: 6px 14px; border-radius: 18px;
        background: rgba(5,8,12,0.85); border: 1px solid rgba(255,255,255,0.15);
        color: #f5b942; font: 12px ui-monospace, monospace;
        pointer-events: none; opacity: 0;
        transition: opacity 0.25s, transform 0.25s; z-index: 5;
      }
      .hud-skip.left { left: 10%; }
      .hud-skip.right { right: 10%; }
      .hud-skip.show { opacity: 1; }
      .veil {
        position: absolute; inset: 0; display: flex; flex-direction: column;
        align-items: center; justify-content: center; gap: 10px;
        color: #8b96a7; font: 13px/1.4 system-ui, sans-serif;
        background: rgba(13,16,22,0.75); opacity: 0; transition: opacity .3s;
        pointer-events: none; text-align: center; padding: 16px;
      }
      .veil.show { opacity: 1; pointer-events: auto; }
      .veil.error { color: #ef6a6a; }
      .veil .retry-btn {
        margin-top: 8px; padding: 6px 16px; border-radius: 6px;
        border: 1px solid rgba(255,255,255,0.25); background: rgba(255,255,255,0.12);
        color: #fff; cursor: pointer; font: 12px system-ui, sans-serif;
        transition: background 0.15s;
      }
      .veil .retry-btn:hover { background: rgba(255,255,255,0.22); }
      .playbtn {
        position: absolute; left: 50%; top: 50%;
        transform: translate(-50%, -50%);
        width: 52px; height: 52px; border-radius: 50%;
        border: 1px solid rgba(245,185,66,.5); background: rgba(245,185,66,.14);
        color: #f5b942; cursor: pointer; display: none;
        align-items: center; justify-content: center; padding: 0;
      }
      .playbtn.show { display: flex; }
      .playbtn svg { display: block; }
      .strip {
        position: relative; height: 44px;
        display: flex; align-items: center;
        border-top: 1px solid rgba(255,255,255,.07);
        cursor: pointer; touch-action: none;
        user-select: none; -webkit-user-select: none;
        padding: 0 4px; gap: 2px;
      }
      .strip:focus-visible { outline: 2px solid #f5b942; outline-offset: -2px; }
      .strip-btn {
        width: 36px; height: 36px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        display: flex; align-items: center; justify-content: center;
        transition: background 0.15s, color 0.15s;
      }
      .strip-btn:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .strip-btn:focus-visible { outline: 2px solid #f5b942; }
      .strip-btn svg { width: 18px; height: 18px; display: block; }
      .track {
        position: relative; flex: 1; height: 100%;
        display: flex; align-items: center; margin: 0 4px;
        cursor: pointer;
      }
      .track-bar {
        position: absolute; left: 0; right: 0; height: 4px;
        background: rgba(255,255,255,0.14); border-radius: 2px;
        overflow: visible; transition: height 0.15s;
      }
      .strip:hover .track-bar, .strip.scrubbing .track-bar { height: 6px; }
      .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        background: #f5b942; border-radius: 2px;
        transform-origin: left; transform: scaleX(0);
        pointer-events: none;
      }
      .marks { position: absolute; inset: 0; pointer-events: none; }
      .strip .tick {
        position: absolute; top: -3px; bottom: -3px; width: 2px;
        transform: translateX(-50%);
        background: rgba(255,255,255,.32);
        pointer-events: none; transition: background 0.15s, transform 0.15s, box-shadow 0.15s;
      }
      .strip .tick.snapped {
        background: #f5b942; transform: translateX(-50%) scaleY(1.3);
        box-shadow: 0 0 6px rgba(245,185,66,0.6);
      }
      .strip .diamond {
        position: absolute; top: 50%; width: 9px; height: 9px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 2px;
        pointer-events: none; transition: transform 0.15s, box-shadow 0.15s;
      }
      .strip .diamond.snapped {
        background: #f5b942; transform: translate(-50%, -50%) rotate(45deg) scale(1.3);
        box-shadow: 0 0 8px rgba(245,185,66,0.8);
      }
      .strip .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.35) 0 4px, transparent 4px 8px);
        pointer-events: none;
      }
      .cursor {
        position: absolute; top: 0; bottom: 0; width: 2px; left: 0;
        transform: translateX(-50%);
        background: #f5b942; pointer-events: none;
      }
      .cursor-handle {
        position: absolute; top: 50%; left: 50%; width: 10px; height: 10px;
        transform: translate(-50%, -50%) scale(0);
        background: #f5b942; border-radius: 50%;
        box-shadow: 0 1px 4px rgba(0,0,0,0.5);
        transition: transform 0.15s ease-out; pointer-events: none;
      }
      .strip:hover .cursor-handle, .strip.scrubbing .cursor-handle {
        transform: translate(-50%, -50%) scale(1);
      }
      .scrub-pill {
        position: absolute; bottom: 100%; left: 0;
        transform: translate(-50%, -8px);
        background: rgba(10,15,23,0.92); border: 1px solid rgba(255,255,255,0.18);
        box-shadow: 0 4px 12px rgba(0,0,0,0.4);
        color: #e8edf4; font: 11px/1.3 ui-monospace, monospace;
        padding: 3px 8px; border-radius: 6px; white-space: nowrap;
        pointer-events: none; opacity: 0; transition: opacity 0.15s;
        z-index: 10;
      }
      .scrub-pill.show { opacity: 1; }
      .chip {
        flex: none;
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums;
        background: rgba(5,8,12,.55); border-radius: 6px; padding: 3px 8px;
        pointer-events: none;
      }
      .chip.dbg { color: #8ab4f8; background: rgba(5,8,12,.8); }
      .speed {
        flex: none; width: 44px; height: 36px;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        font: 12px ui-monospace, monospace;
        display: flex; align-items: center; justify-content: center;
      }
      .speed:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .speed:focus-visible { outline: 2px solid #f5b942; }
      @media (pointer: coarse) {
        .strip { height: 52px; padding: 0 6px; gap: 4px; }
        .strip-btn { width: 44px; height: 44px; }
        .strip-btn svg { width: 22px; height: 22px; }
        .speed { width: 48px; height: 44px; }
        .cursor-handle { width: 14px; height: 14px; transform: translate(-50%, -50%) scale(1); }
        .scrub-pill {
          transform: translate(-50%, -46px);
          font-size: 13px; padding: 5px 10px; border-radius: 8px;
        }
      }
      @media (max-width: 480px) {
        .chip { font-size: 11px; padding: 2px 6px; }
      }
      @media (max-width: 360px) {
        .chip { display: none; }
      }
    `;
    this.shadowRoot.replaceChildren(style, this._stage);
    this._skeleton = document.createElement("div");
    this._skeleton.className = "skeleton";
    const badge = document.createElement("div");
    badge.className = "skeleton-badge";
    const spinner = document.createElement("span");
    spinner.className = "skeleton-spinner";
    const titleSpan = document.createElement("span");
    titleSpan.className = "skeleton-title";
    titleSpan.textContent = title ? `${title}` : "animatix scene";
    badge.append(spinner, titleSpan);
    this._skeleton.appendChild(badge);
    this._stage.appendChild(this._skeleton);

    this._canvas = document.createElement("canvas");
    this._canvas.width = 1280;
    this._canvas.height = 720;
    this._canvas.hidden = true;

    this._hudFlash = document.createElement("div");
    this._hudFlash.className = "hud-flash";
    this._stage.appendChild(this._hudFlash);

    this._hudSkip = document.createElement("div");
    this._hudSkip.className = "hud-skip";
    this._stage.appendChild(this._hudSkip);

    this._veil = document.createElement("div");
    this._veil.className = "veil";

    this._playbtn = document.createElement("button");
    this._playbtn.className = "playbtn";
    this._playbtn.setAttribute("aria-label", "Play");
    this._playbtn.innerHTML = ICONS.playBig;
    this._playbtn.addEventListener("click", () => this.play());

    this._controls = null;
    this._marks = null;
    this._applyFit();
  }

  _showVeil(text, isError) {
    this._veil.className = `veil show${isError ? " error" : ""}`;
    this._veil.replaceChildren();
    const msg = document.createElement("div");
    msg.textContent = text;
    this._veil.appendChild(msg);
    if (isError) {
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "retry-btn";
      retry.textContent = "Retry";
      retry.addEventListener("click", () => this._resetForReload());
      this._veil.appendChild(retry);
    }
    if (!this._veil.isConnected) this.shadowRoot.appendChild(this._veil);
  }

  _hideVeil() {
    this._veil.className = "veil";
  }

  _triggerHud(iconSvg) {
    if (!this._hudFlash) return;
    this._hudFlash.innerHTML = iconSvg;
    this._hudFlash.classList.remove("show");
    void this._hudFlash.offsetWidth; // re-trigger animation
    this._hudFlash.classList.add("show");
    clearTimeout(this._hudTimer);
    this._hudTimer = setTimeout(() => {
      this._hudFlash?.classList.remove("show");
    }, 320);
  }

  _triggerSkipHud(text, isRight) {
    if (!this._hudSkip) return;
    this._hudSkip.className = `hud-skip ${isRight ? "right" : "left"} show`;
    this._hudSkip.textContent = text;
    clearTimeout(this._skipHudTimer);
    this._skipHudTimer = setTimeout(() => {
      this._hudSkip?.classList.remove("show");
    }, 450);
  }

  // ── loop timing ─────────────────────────────────────────────────

  _looping() {
    return this.hasAttribute("loop") || this._loop === true;
  }

  /// A looping embed plays `duration` + `hold`: the finished composition rests
  /// on screen, then dissolves out and the build-up starts again. Without the
  /// hold the wrap is a hard cut from the full diagram to an empty stage.
  _configureCycle() {
    const attr = this.getAttribute("hold");
    const parsed = attr === null ? NaN : Number(attr);
    const hold = Number.isFinite(parsed) && parsed >= 0 ? Math.min(parsed, 30) : 0.7;
    this._holdSeconds = hold;
    this._cycle = this._duration + hold;
    // Both dissolves live inside the rest window and never eat the timeline.
    this._fade = hold > 0 ? Math.min(FADE_EACH, hold / 2, this._duration / 4) : 0;
  }

  /// Canvas opacity at the current cycle position.
  _loopAlpha() {
    if (!this._playing || !this._looping() || this._fade <= 0) return 1;
    if (this._time < this._fade) return this._time / this._fade;
    const fadeOutStart = this._cycle - this._fade;
    if (this._time > fadeOutStart) return Math.max(0, (this._cycle - this._time) / this._fade);
    return 1;
  }

  // ── lifecycle ───────────────────────────────────────────────────

  _setupObserver() {
    this._observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          this._visible = entry.isIntersecting;
          if (this._visible && this._state === "idle") this._maybeStartLoading();
          if (!this._visible && this._playing) this.pause();
          // A sealed embed with autoplay has no visible control to bring it
          // back after the offscreen pause above — resuming on re-entry is
          // the whole contract of the attribute pair.
          if (this._visible && this._isSealed() && !this._playing) this._resumeSealed();
        }
      },
      { rootMargin: "200px" },
    );
    this._observer.observe(this);
  }

  /// Re-play a sealed embed that autoplayed once and was paused (offscreen,
  /// or an earlier seek-pause from the driving page). Guarded by the same
  /// conditions as the first autoplay, so reduced-motion and save-data pages
  /// keep their static poster.
  _resumeSealed() {
    if (this._state !== "ready" || !this.hasAttribute("autoplay")) return;
    if (REDUCED_MOTION || SAVE_DATA) return;
    this.play();
  }

  _shouldAutoplay() {
    return this.hasAttribute("autoplay") && !REDUCED_MOTION && !SAVE_DATA;
  }

  /// `data-fonts` — space-separated TTF/OTF URLs, registered before the scene
  /// compiles so `font_family` can name them. A failed font is skipped with a
  /// console warning rather than blocking the figure.
  async _loadFonts(module) {
    const fonts = this.getAttribute("data-fonts");
    if (!fonts || !this._player.add_font) return;
    for (const url of fonts.split(/\s+/).filter(Boolean)) {
      try {
        const response = await fetch(new URL(url, document.baseURI));
        if (!response.ok) throw new Error(`HTTP ${response.status}`);
        this._player.add_font(new Uint8Array(await response.arrayBuffer()));
      } catch (err) {
        console.warn(`amx-player: font '${url}' skipped (${err.message})`);
      }
    }
  }

  /// Fetch the scene's imports and assets, then build. Imports beyond the
  /// bundled library close through the engine's retry protocol: a build that
  /// stops on an unsupplied import reports the resolved path in
  /// `missing_imports`, we fetch it relative to the scene and retry (bounded,
  /// so a failed fetch or an engine without the API cannot spin). Assets keep
  /// the desktop semantics — a missing one is a build error — and share one
  /// flat namespace relative to the scene file, because the sandboxed build
  /// resolves asset urls lexically with no per-module root.
  async _loadScene(player, source, sceneSrc) {
    const base = new URL(sceneSrc, document.baseURI).href;
    const supportsImports = typeof player.add_module === "function";
    const modules = new Map(); // resolved import path -> file text
    const failed = new Set(); // paths whose fetch failed; never retried
    const assets = new Map(); // resolved asset URL -> { key, payload }

    let result = null;
    for (let round = 0; round < 24; round += 1) {
      for (const [path, text] of modules) player.add_module(path, text);
      await this._fetchAssets(player, source, modules, base, assets);
      result = this._buildWithAssets(player, source, assets);

      const missing = supportsImports ? (result?.missing_imports ?? []) : [];
      const todo = missing.filter((path) => !modules.has(path) && !failed.has(path));
      if (!missing.length || !todo.length) break;
      await Promise.all(
        todo.map(async (path) => {
          try {
            const response = await fetch(new URL(path, base));
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            modules.set(path, await response.text());
          } catch (err) {
            failed.add(path);
            console.warn(`amx-player: import '${path}' skipped (${err.message})`);
          }
        }),
      );
    }
    return result;
  }

  /// Fetch every asset referenced by the scene or any fetched module, once.
  /// Each source's urls resolve against that source's own URL; the engine
  /// receives the literal url string as the cache key.
  async _fetchAssets(player, source, modules, base, assets) {
    const wanted = [];
    const collect = (text, sourceUrl) => {
      let urls = [];
      try {
        urls = player.list_asset_urls(text) ?? [];
      } catch (err) {
        console.warn(`amx-player: could not list asset urls (${err.message})`);
      }
      for (const key of urls) {
        const resolved = new URL(key, sourceUrl).href;
        if (!assets.has(resolved) && !wanted.some(([seen]) => seen === resolved)) {
          wanted.push([resolved, key]);
        }
      }
    };
    collect(source, base);
    for (const [path, text] of modules) collect(text, new URL(path, base).href);

    await Promise.all(
      wanted.map(async ([resolved, key]) => {
        try {
          const response = await fetch(resolved);
          if (!response.ok) throw new Error(`HTTP ${response.status}`);
          // Images cross the boundary as Uint8Array (the Rust side decodes
          // them); SVG as text.
          const payload = key.toLowerCase().endsWith(".svg")
            ? await response.text()
            : new Uint8Array(await response.arrayBuffer());
          assets.set(resolved, { key, payload });
        } catch (err) {
          console.warn(`amx-player: asset '${key}' skipped (${err.message})`);
        }
      }),
    );
  }

  _buildWithAssets(player, source, assets) {
    const assetUrls = [];
    const assetPayloads = [];
    for (const { key, payload } of assets.values()) {
      assetUrls.push(key);
      assetPayloads.push(payload);
    }
    if (typeof player.load_source_with_assets === "function") {
      return player.load_source_with_assets(source, assetUrls, assetPayloads);
    }
    return player.load_source(source);
  }

  /// Rebuild the scene from edited text — the live-editor entry point. The
  /// player and everything it has registered (fonts, fetched imports, assets)
  /// stays; only the document changes. Returns the build's diagnostics array
  /// on success. Throws on a failed build (with `.diagnostics` attached) and
  /// the previous scene keeps playing, so a typo never blanks the figure.
  applySource(text) {
    if (this._state !== "ready" || !this._player) {
      throw new Error("the player has not finished loading yet");
    }
    const result = this._player.load_source(text);
    const diags = result?.diagnostics ?? [];
    if (!result?.ok) {
      const err = new Error(
        diags.find((d) => d.severity === "error")?.message ?? "build failed",
      );
      err.diagnostics = diags;
      throw err;
    }
    this._duration = Math.max(result.duration_s, 0.05);
    if (result.width && result.height) {
      this._canvas.width = Math.round(result.width);
      this._canvas.height = Math.round(result.height);
      if (this._fit() === "contain") {
        this._stage.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
      }
    }
    this._markers = Array.isArray(result.markers) ? result.markers : [];
    this._configureCycle();
    const strip = this.shadowRoot?.querySelector(".strip");
    if (strip) strip.setAttribute("aria-valuemax", String(this._duration));
    const track = this.shadowRoot?.querySelector(".track");
    if (track) track.setAttribute("aria-valuemax", String(this._duration));
    // The edit may have moved or added keyframes, so re-draw the landmark
    // layer the strip's ticks come from; snapping already reads `_markers`,
    // and leaving the drawn marks stale would point the playhead at nothing.
    this._renderMarks();
    // A resolution change above reset the backing store to the scene's own
    // size; put the display-matched scale back.
    this._applyRenderScale();
    if (this._playing) {
      this._time = 0;
      this._restTime = 0;
    } else {
      // Park on the finished frame — frame 0 of these scenes is an empty
      // stage, and an editor who hit Apply on a paused figure should see
      // what they built, not nothing.
      this._restTime = this._duration;
      this._time = this._duration;
    }
    this._renderScene();
    return diags;
  }

  /// Queue the scene load. Everything that could make a player start on its own
  /// — becoming visible, a `src`/`profile` change — goes through the scheduler.
  _maybeStartLoading() {
    if (!this.getAttribute("src") || this._state !== "idle") return;
    enqueueLoad(this);
  }

  async _startLoad() {
    const src = this.getAttribute("src");
    if (!src || this._state !== "idle") return;
    this._state = "loading";
    this._skeleton?.classList.add("loading");
    if (!("gpu" in navigator)) {
      this._state = "error";
      const msg = "This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).";
      console.warn(`amx-player: ${msg}`);
      this._showVeil(msg, true);
      this.dispatchEvent(new CustomEvent("amxerror", { bubbles: true, detail: { src, error: "WebGPU unsupported" } }));
      return;
    }
    try {
      const profile = this._profile();
      const [module, source] = await Promise.all([
        loadEngine(profile),
        fetch(src).then((r) => {
          if (!r.ok) throw new Error(`HTTP ${r.status}`);
          return r.text();
        }),
      ]);
      if (this.getAttribute("src") !== src) return; // changed while loading

      this._player = await module.create_player(this._canvas);
      if (this.getAttribute("src") !== src) return;
      if (typeof this._player.set_quality === "function") {
        this._player.set_quality(this._quality());
      }

      await this._loadFonts(module);
      if (this.getAttribute("src") !== src) return;
      const result = await this._loadScene(this._player, source, src);
      const diags = result.diagnostics ?? [];
      const errors = diags.filter((d) => d.severity === "error");
      if (!result.ok) {
        this._state = "error";
        const first = errors[0] ?? diags[0];
        const msg = `Scene error${first?.line ? ` (line ${first.line})` : ""}: ${first?.message ?? "build failed"}`;
        console.error(`amx-player: ${msg} [src="${src}"]`, diags);
        this._showVeil(msg, true);
        this.dispatchEvent(new CustomEvent("amxerror", { bubbles: true, detail: { src, diagnostics: diags, message: msg } }));
        return;
      }

      this._duration = Math.max(result.duration_s, 0.05);
      this._canvas.width = Math.round(result.width || 1280);
      this._canvas.height = Math.round(result.height || 720);
      if (this._fit() === "contain") {
        this._stage.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
      }
      this._configureCycle();
      // If autoplaying: seed at t = 0 so playback starts cleanly without
      // flashing the finished poster frame. Otherwise: show finished frame.
      const willAutoplay = this._shouldAutoplay();
      if (willAutoplay) {
        this._time = 0;
        this._restTime = 0;
      } else {
        this._restTime = this._duration;
        this._time = this._duration;
      }
      this._renderScene();

      // Timeline landmarks for the scrubber (keyframes, scene starts,
      // transition windows) — the engine knows them, so the bar can show them.
      this._markers = Array.isArray(result.markers) ? result.markers : [];

      // Swap skeleton for canvas with a smooth crossfade
      this._canvas.hidden = false;
      this._stage.appendChild(this._canvas);
      this._skeleton?.classList.add("fade-out");
      setTimeout(() => {
        if (this._skeleton?.isConnected) this._skeleton.remove();
      }, 300);
      // Only now does the canvas have a laid-out size to match the raster to.
      this._applyRenderScale();
      this._renderScaleObserver = new ResizeObserver(() => this._applyRenderScale(false));
      this._renderScaleObserver.observe(this._stage);
      // A cold first load can still report a zero-sized box at this point, and
      // an environment that never delivers ResizeObserver would then keep the
      // full-resolution fallback for the life of the page. Re-apply once the
      // task settles; the call is idempotent.
      this._renderScaleRetry = setTimeout(() => this._applyRenderScale(), 0);
      if (this.hasAttribute("controls")) {
        this._buildControls();
        this._setupGestures();
      } else {
        this._stage.appendChild(this._playbtn);
      }
      this._stage.appendChild(this._veil);
      if (diags.length > 0) {
        console.warn(`amx-player: ${src} built with ${diags.length} diagnostic(s)`, diags[0]);
      }

      this._state = "ready";
      if (this._shouldAutoplay()) {
        this._loop = this.hasAttribute("loop");
        this.play();
      } else if (!this.hasAttribute("controls") && !this._isSealed()) {
        this._playbtn.classList.add("show");
      }
      this.dispatchEvent(new CustomEvent("amxready", { bubbles: true }));
    } catch (err) {
      this._state = "error";
      console.error(`amx-player: failed to load scene "${src}":`, err);
      this._showVeil(`Failed to load scene: ${err?.message ?? err}`, true);
      this.dispatchEvent(new CustomEvent("amxerror", { bubbles: true, detail: { src, error: err } }));
    }
  }

  _buildControls() {
    const strip = document.createElement("div");
    strip.className = "strip";
    strip.tabIndex = 0;
    strip.setAttribute("role", "region");
    strip.setAttribute("aria-label", "Playback controls");

    const playToggle = document.createElement("button");
    playToggle.type = "button";
    playToggle.className = "strip-btn play-toggle";
    playToggle.setAttribute("aria-label", "Play");
    playToggle.innerHTML = ICONS.play;
    playToggle.addEventListener("click", (e) => {
      e.stopPropagation();
      this.togglePlay(false);
    });

    const track = document.createElement("div");
    track.className = "track";
    track.setAttribute("role", "slider");
    track.setAttribute("aria-label", "Timeline");
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", String(this._duration));

    const trackBar = document.createElement("div");
    trackBar.className = "track-bar";

    const fill = document.createElement("div");
    fill.className = "fill";
    const cursor = document.createElement("div");
    cursor.className = "cursor";
    const cursorHandle = document.createElement("div");
    cursorHandle.className = "cursor-handle";
    cursor.appendChild(cursorHandle);

    // Landmarks at percentage positions of the pure timeline: keyframe ticks,
    // scene-start diamonds, hatched transition spans. They live in their own
    // layer so `applySource` can rebuild them when an edit changes the marks
    // or the timeline they are positioned against.
    this._marks = document.createElement("div");
    this._marks.className = "marks";
    this._renderMarks();

    trackBar.append(fill, this._marks, cursor);

    const scrubPill = document.createElement("div");
    scrubPill.className = "scrub-pill";

    track.append(trackBar, scrubPill);

    const chip = document.createElement("span");
    chip.className = "chip";
    const speed = document.createElement("button");
    speed.type = "button";
    speed.className = "speed";
    speed.textContent = "1\u00d7";
    speed.setAttribute("aria-label", "Playback speed 1\u00d7");
    speed.addEventListener("pointerdown", (e) => e.stopPropagation());
    speed.addEventListener("click", (e) => {
      e.stopPropagation();
      const RATES = [1, 1.5, 2, 0.5];
      this._rate = RATES[(RATES.indexOf(this._rate) + 1) % RATES.length];
      speed.textContent = `${this._rate}\u00d7`;
      speed.setAttribute("aria-label", `Playback speed ${this._rate}\u00d7`);
    });

    strip.append(playToggle, track, chip, speed);
    this.shadowRoot.append(strip);

    // ── track scrubbing and magnetic snapping ──
    const findSnap = (t) => {
      const SNAP_S = 0.2;
      let closest = null;
      let minDiff = SNAP_S + 1e-4;
      for (const m of this._markers) {
        const d = Math.abs(t - m.t);
        if (d <= SNAP_S && d < minDiff) {
          minDiff = d;
          closest = { t: m.t, kind: m.kind };
        }
        if (m.kind === "transition" && m.dur > 0) {
          const dEnd = Math.abs(t - (m.t + m.dur));
          if (dEnd <= SNAP_S && dEnd < minDiff) {
            minDiff = dEnd;
            closest = { t: m.t + m.dur, kind: "transition" };
          }
        }
      }
      return closest;
    };

    const updateMarksSnap = (snap) => {
      if (!this._marks) return;
      this._marks.querySelectorAll(".snapped").forEach((el) => el.classList.remove("snapped"));
      if (snap) {
        for (const el of this._marks.children) {
          if (el.dataset.t && Math.abs(Number(el.dataset.t) - snap.t) < 1e-3) {
            el.classList.add("snapped");
          }
        }
        if (navigator.vibrate && this._lastSnappedMarker !== snap.t) {
          try { navigator.vibrate(10); } catch {}
        }
        this._lastSnappedMarker = snap.t;
      } else {
        this._lastSnappedMarker = null;
      }
    };

    const timeAt = (e) => {
      const rect = track.getBoundingClientRect();
      if (rect.width <= 0 || !Number.isFinite(e.clientX)) return { time: this._time, pct: 0, snap: null };
      const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
      const raw = frac * this._duration;
      const snap = findSnap(raw);
      const targetTime = snap ? snap.t : raw;
      const snapFrac = this._duration > 0 ? targetTime / this._duration : frac;
      return { time: targetTime, pct: snapFrac, snap };
    };

    const updateScrubPill = (t, pct) => {
      scrubPill.style.left = `${pct * 100}%`;
      const frameNum = Math.round(t * FRAME_RATE);
      scrubPill.textContent = `${t.toFixed(2)}s (f${frameNum})`;
      scrubPill.classList.add("show");
    };

    track.addEventListener("pointerenter", (e) => {
      if (e.pointerType !== "mouse" || this._scrubbing) return;
      const { time, pct, snap } = timeAt(e);
      updateScrubPill(time, pct);
      updateMarksSnap(snap);
    });

    track.addEventListener("pointermove", (e) => {
      const { time, pct, snap } = timeAt(e);
      updateScrubPill(time, pct);
      updateMarksSnap(snap);
      if (this._scrubbing) {
        this.seek(time);
      }
    });

    track.addEventListener("pointerleave", (e) => {
      if (this._scrubbing) return;
      scrubPill.classList.remove("show");
      updateMarksSnap(null);
    });

    track.addEventListener("pointerdown", (e) => {
      try {
        track.setPointerCapture(e.pointerId);
      } catch {}
      this._resumeOnScrubEnd = this._playing;
      if (this._playing) {
        this.pause();
      }
      this._scrubbing = true;
      strip.classList.add("scrubbing");
      const { time, pct, snap } = timeAt(e);
      updateScrubPill(time, pct);
      updateMarksSnap(snap);
      this.seek(time);
      e.stopPropagation();
    });

    const finishScrub = (e) => {
      if (!this._scrubbing) return;
      this._scrubbing = false;
      strip.classList.remove("scrubbing");
      scrubPill.classList.remove("show");
      updateMarksSnap(null);
      if (this._resumeOnScrubEnd) {
        this._resumeOnScrubEnd = false;
        this.play();
      }
    };

    track.addEventListener("pointerup", finishScrub);
    track.addEventListener("pointercancel", finishScrub);

    strip.addEventListener("keydown", (e) => this._handleKeyDown(e));

    this._controls = { strip, playToggle, track, fill, cursor, scrubPill, chip, speed };
    this._syncControls();
  }

  /// Draw the landmark layer from the current markers and duration. Runs when
  /// the control bar is built and again after `applySource` rebuilds the
  /// document: an edit can change both the marks and the timeline they are
  /// positioned against, so the bar cannot be a one-time snapshot.
  _renderMarks() {
    if (!this._marks) return;
    const marks = document.createDocumentFragment();
    for (const m of this._markers) {
      const frac = this._duration > 0 ? Math.min(m.t / this._duration, 1) : 0;
      if (m.kind === "transition" && m.dur > 0) {
        const span = document.createElement("div");
        span.className = "span";
        span.style.left = `${frac * 100}%`;
        span.style.width = `${Math.min(m.dur / this._duration, 1 - frac) * 100}%`;
        span.dataset.t = String(m.t);
        marks.appendChild(span);
      } else {
        const mark = document.createElement("div");
        mark.className = m.kind === "scene" ? "diamond" : "tick";
        mark.style.left = `${frac * 100}%`;
        mark.dataset.t = String(m.t);
        marks.appendChild(mark);
      }
    }
    this._marks.replaceChildren(marks);
  }

  /// The next (dir=1) or previous (dir=-1) landmark at or after the playhead:
  /// keyframes, scene starts, and transition edges. Falls back to a 1 s step
  /// when the document has no landmarks.
  _nextLandmark(dir) {
    const pts = [];
    for (const m of this._markers) {
      pts.push(m.t);
      if (m.kind === "transition" && m.dur > 0) pts.push(m.t + m.dur);
    }
    pts.sort((a, b) => a - b);
    const eps = 1e-3;
    if (dir > 0) {
      const next = pts.find((t) => t > this._time + eps);
      return next ?? Math.min(this._time + 1, this._duration);
    }
    const prev = [...pts].reverse().find((t) => t < this._time - eps);
    return prev ?? Math.max(this._time - 1, 0);
  }

  /// Push the current time/state into the control bar. Called from the shared
  /// rAF (`advance`) and from scrub/keyboard seeks; DOM writes are transform
  /// and text only, so per-frame updates are cheap.
  _syncControls() {
    const c = this._controls;
    if (!c) return;
    // The playhead can legally sit past the timeline (a loop's hold rest),
    // but every control reads in timeline units: clamp so the chip, the fill
    // and the slider value can never report more than `duration`.
    const shown = Math.min(this._time, this._duration);
    const frac = this._duration > 0 ? shown / this._duration : 0;
    c.fill.style.transform = `scaleX(${frac})`;
    c.cursor.style.left = `${frac * 100}%`;

    // Sync play/pause toggle button icon and aria-label
    if (this._playing) {
      c.playToggle.innerHTML = ICONS.pause;
      c.playToggle.setAttribute("aria-label", "Pause");
    } else if (this._time >= this._duration && !this._looping()) {
      c.playToggle.innerHTML = ICONS.replay;
      c.playToggle.setAttribute("aria-label", "Replay");
    } else {
      c.playToggle.innerHTML = ICONS.play;
      c.playToggle.setAttribute("aria-label", "Play");
    }

    if (this._debugFrame) {
      // `rs` is the adaptive render scale the quality controller settled on.
      // It belongs in the report because a shimmer that only appears at a
      // reduced raster scale is a different defect from a geometry one.
      const rs = this._renderScale ?? 1;
      c.chip.textContent =
        `f${this.frame} / ${this.totalFrames}  ${shown.toFixed(3)}s  @${FRAME_RATE}fps  rs${rs.toFixed(2)}`;
      c.chip.classList.add("dbg");
    } else {
      c.chip.textContent = `${shown.toFixed(1)} / ${this._duration.toFixed(1)}`;
      c.chip.classList.remove("dbg");
    }
    c.track.setAttribute("aria-valuenow", shown.toFixed(1));
  }

  _setupGestures() {
    this._canvas.tabIndex = 0;
    this._canvas.addEventListener("pointerdown", (e) => {
      if (!this._controls && !this.hasAttribute("controls") && this._isSealed()) return;
      this._canvas.focus({ preventScroll: true });

      if (e.pointerType === "mouse") {
        this.togglePlay(true);
        e.preventDefault();
        return;
      }

      // Touch / mobile double-tap detection
      const now = Date.now();
      const dt = now - this._lastTapTime;
      const dx = Math.abs(e.clientX - this._lastTapX);

      if (dt < 280 && dx < 48) {
        clearTimeout(this._singleTapTimer);
        this._lastTapTime = 0;
        const rect = this._canvas.getBoundingClientRect();
        const relX = rect.width > 0 ? (e.clientX - rect.left) / rect.width : 0.5;

        if (relX < 0.35) {
          const prev = this._nextLandmark(-1);
          this.seek(prev);
          this._triggerSkipHud("« Keyframe", false);
        } else if (relX > 0.65) {
          const next = this._nextLandmark(1);
          this.seek(next);
          this._triggerSkipHud("Keyframe »", true);
        } else {
          this.togglePlay(true);
        }
      } else {
        this._lastTapTime = now;
        this._lastTapX = e.clientX;
        clearTimeout(this._singleTapTimer);
        this._singleTapTimer = setTimeout(() => {
          this.togglePlay(true);
        }, 280);
      }
      e.preventDefault();
    });

    this._canvas.addEventListener("keydown", (e) => this._handleKeyDown(e));
  }

  _handleKeyDown(e) {
    if (e.target && (e.target.tagName === "INPUT" || e.target.tagName === "TEXTAREA" || e.target.isContentEditable)) {
      return;
    }
    if (e.key === " " || e.key === "k" || e.key === "K") {
      e.preventDefault();
      e.stopPropagation();
      this.togglePlay(true);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      e.stopPropagation();
      if (e.shiftKey) {
        this.stepFrame(-1);
      } else {
        const prev = this._nextLandmark(-1);
        this.seek(prev);
        this._triggerSkipHud("« Keyframe", false);
      }
    } else if (e.key === "ArrowRight") {
      e.preventDefault();
      e.stopPropagation();
      if (e.shiftKey) {
        this.stepFrame(1);
      } else {
        const next = this._nextLandmark(1);
        this.seek(next);
        this._triggerSkipHud("Keyframe »", true);
      }
    } else if (e.key === "," || e.key === "<") {
      e.preventDefault();
      e.stopPropagation();
      this.stepFrame(-1);
    } else if (e.key === "." || e.key === ">") {
      e.preventDefault();
      e.stopPropagation();
      this.stepFrame(1);
    } else if (e.key === "j" || e.key === "J") {
      e.preventDefault();
      e.stopPropagation();
      this.seek(Math.max(this._time - 1, 0));
      this._triggerSkipHud("« 1s", false);
    } else if (e.key === "l" || e.key === "L") {
      e.preventDefault();
      e.stopPropagation();
      this.seek(Math.min(this._time + 1, this._duration));
      this._triggerSkipHud("1s »", true);
    } else if (e.key === "Home" || e.key === "0") {
      e.preventDefault();
      e.stopPropagation();
      this.seek(0);
      this._triggerSkipHud("0:00", false);
    } else if (e.key === "End") {
      e.preventDefault();
      e.stopPropagation();
      this.seek(this._duration);
      this._triggerSkipHud("End", true);
    } else if (e.key === "d" || e.key === "D") {
      e.preventDefault();
      e.stopPropagation();
      this._debugFrame = !this._debugFrame;
      this._syncControls();
    } else if (e.key === "c" || e.key === "C") {
      e.preventDefault();
      e.stopPropagation();
      const line = this.debugReport();
      navigator.clipboard?.writeText(line).catch(() => {});
      console.log(line);
      this._triggerSkipHud("Copied report", true);
    }
  }

  togglePlay(showHud = true) {
    if (this._playing) {
      this.pause();
      if (showHud) this._triggerHud(ICONS.pause);
    } else {
      this.play();
      if (showHud) this._triggerHud(ICONS.play);
    }
  }

  stepFrame(dir) {
    if (this._playing) {
      this.pause();
    }
    const target = Math.min(Math.max(this._time + dir / FRAME_RATE, 0), this._duration);
    this.seek(target);
    const frameNum = Math.round(this._time * FRAME_RATE);
    this._triggerSkipHud(dir > 0 ? `+1f (f${frameNum})` : `-1f (f${frameNum})`, dir > 0);
  }

  // ── public API ──────────────────────────────────────────────────

  play() {
    if (this._state !== "ready") return;
    if (!this._player?.has_document()) return;
    // Starting from the finished frame (the poster position, or a play that
    // ran to the end) restarts — a loop should begin its build-up, not its
    // hold rest.
    if (this._time >= this._duration) this._time = 0;
    this._playing = true;
    // A paused embed is not in `instances`, so it misses both the quality-step
    // notifications and its own layout changes; re-align on the way in.
    this._applyRenderScale();
    this._playbtn.classList.remove("show");
    this._syncControls();
    instances.add(this);
    ensureLoop();
  }

  pause() {
    this._playing = false;
    instances.delete(this);
    if (this._state === "ready") {
      // Without the control bar the center affordance is the only way back —
      // except for a sealed embed, where the page is the way back.
      if (!this._controls && !this._isSealed() && !this.hasAttribute("autoplay")) {
        this._playbtn.classList.add("show");
      }
      this._syncControls();
    }
  }

  /// Jump to `time` (seconds within the timeline) and show the frame. Keeps
  /// the playing state; the shared loop resumes from here.
  seek(time) {
    if (this._state !== "ready") return;
    this._time = Math.min(Math.max(time, 0), this._duration);
    this._restTime = this._time;
    this._renderScene();
    this._syncControls();
  }

  /// Seconds in the compiled timeline (excludes a loop's hold rest). 0 until
  /// the scene has built; pages driving a sealed embed can wait for the
  /// `amxready` event before reading it.
  get duration() {
    return this._duration;
  }

  /// Current position within the playback cycle (may exceed `duration` during
  /// a loop's hold rest; clamp for timeline math).
  get time() {
    return Math.min(this._time, this._duration);
  }

  /// Position on the nominal `FRAME_RATE` reporting grid — a label for a
  /// frame, not a clock. `time` stays the value the CLI takes.
  get frame() {
    return Math.round(this.time * FRAME_RATE);
  }

  get totalFrames() {
    return Math.round(this._duration * FRAME_RATE);
  }

  /// The debug line in the same words the chip shows, for `player.debugReport()`
  /// in a console or for the `c` key.
  debugReport() {
    return `frame ${this.frame}/${this.totalFrames} @${FRAME_RATE}fps, ` +
      `time ${this.time.toFixed(3)}s, render scale ${(this._renderScale ?? 1).toFixed(2)}`;
  }

  /// Called by the shared loop each frame. Returns whether it played.
  advance(dt) {
    if (!this._playing || !this._visible) return false;
    // Scrubbing owns the playhead: the clock freezes and the inspected frame
    // is what renders (the strip's seek renders directly).
    if (this._scrubbing) return false;
    const looping = this._looping();
    const limit = looping ? this._cycle : this._duration;
    this._time += dt * (this._rate ?? 1);
    if (this._time >= limit) {
      if (looping) {
        this._time %= limit;
      } else {
        this._time = this._duration;
        this._restTime = this._duration;
        this.pause();
        this._renderScene();
        return false;
      }
    }
    this._renderScene();
    if (this._controls) this._syncControls();
    return true;
  }

  /// Size the canvas backing store and the engine's offscreen raster to what
  /// the element actually shows.
  ///
  /// Both ends of the frame are sized here, because sizing only one is a
  /// half-measure: the canvas backs the *scene's* resolution by default, so a
  /// figure displayed at 990 px renders at 1280 and the compositor downsamples
  /// it on the way to the screen, and the offscreen then rasters pixels nothing
  /// displays. Taking the displayed pixel count (`CSS px × devicePixelRatio`)
  /// as the target, capped at the scene's own resolution, gives one 1:1 blit
  /// with no second resample and the fewest possible raster pixels — which is
  /// where a browser frame's cost lives.
  /// The `qualityStep` multiplier on top is the page-wide concession when even
  /// that will not hold a frame; it lowers the raster below the backing store,
  /// so the blit upscales.
  ///
  /// Resizing is debounced (100 ms) during dynamic drag to eliminate WebGPU
  /// swapchain reconfigurations and texture reallocation churn while CSS stretches
  /// the canvas at compositor speed; settling immediately commits the new backing
  /// store and re-renders if paused.
  _applyRenderScale(immediate = true) {
    const player = this._player;
    if (!player?.set_render_scale || !player.scene_width) return;
    if (!this.isConnected) return;
    const sceneW = player.scene_width() || this._canvas.width;
    const sceneH = player.scene_height() || this._canvas.height;
    const cssW = this._canvas.clientWidth || this._stage.clientWidth || 0;
    const cssH = this._canvas.clientHeight || this._stage.clientHeight || 0;
    const dpr = window.devicePixelRatio || 1;
    const maxScale = Math.min(Math.max(1, dpr), 2.0);
    // Before layout (display:none, pre-append) the element reports 0; render
    // at full detail rather than clamping to a blurry minimum. Supports up to
    // 2x Retina rendering for crisp vector display.
    const display =
      cssW > 0 && cssH > 0 && sceneW > 0 && sceneH > 0
        ? Math.min(maxScale, (cssW * dpr) / sceneW, (cssH * dpr) / sceneH)
        : 1;
    const backingW = Math.max(1, Math.round(sceneW * display));
    const backingH = Math.max(1, Math.round(sceneH * display));
    const sizeChanged = this._canvas.width !== backingW || this._canvas.height !== backingH;

    const commit = () => {
      this._resizeDebounceTimer = null;
      if (!this.isConnected || !this._player) return;
      const curCssW = this._canvas.clientWidth || this._stage.clientWidth || 0;
      const curCssH = this._canvas.clientHeight || this._stage.clientHeight || 0;
      const curDpr = window.devicePixelRatio || 1;
      const curMaxScale = Math.min(Math.max(1, curDpr), 2.0);
      const curDisplay =
        curCssW > 0 && curCssH > 0 && sceneW > 0 && sceneH > 0
          ? Math.min(curMaxScale, (curCssW * curDpr) / sceneW, (curCssH * curDpr) / sceneH)
          : 1;
      const curBackingW = Math.max(1, Math.round(sceneW * curDisplay));
      const curBackingH = Math.max(1, Math.round(sceneH * curDisplay));
      if (this._canvas.width !== curBackingW || this._canvas.height !== curBackingH) {
        this._canvas.width = curBackingW;
        this._canvas.height = curBackingH;
      }
      const curWanted = Math.min(2.0, Math.max(0.25, curDisplay * QUALITY_STEPS[qualityStep]));
      this._renderScale = player.set_render_scale(curWanted);
      if (!this._playing && this._state === "ready") {
        this._renderScene();
      }
    };

    if (this._resizeDebounceTimer) {
      clearTimeout(this._resizeDebounceTimer);
      this._resizeDebounceTimer = null;
    }

    if (immediate || !sizeChanged) {
      commit();
    } else {
      this._resizeDebounceTimer = setTimeout(commit, 100);
    }
  }

  _renderScene() {
    if (!this._player) return;
    try {
      // render at the scene's own resolution; CSS scales it down
      const t = this._playing ? Math.min(this._time, this._duration) : this._restTime;
      if (this._playing) this._restTime = t;
      this._player.render_frame(t);
      this._renderFailures = 0;
      const alpha = this._loopAlpha();
      if (alpha !== this._lastAlpha) {
        this._canvas.style.opacity = String(alpha);
        this._lastAlpha = alpha;
      }
    } catch (err) {
      // One warning per failure run. This is the shared rAF loop: a persistent
      // fault (a lost device, an engine that trapped mid-frame) would otherwise
      // print at frame rate and bury everything else on the page.
      this._renderFailures += 1;
      if (this._renderFailures === 1) console.warn("amx-player: render failed", err);
    }
  }
}

if (!customElements.get("amx-player")) {
  customElements.define("amx-player", AmxPlayerElement);
}

export { AmxPlayerElement };
