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
//              toggles.
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
const QUALITY_STEPS = [1, 0.85, 0.72, 0.6, 0.5];
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
    if (dtMs > SLOW_TICK_MS) {
      slowTicks += 1;
      fastTicks = 0;
      if (slowTicks >= 8) {
        setQualityStep(qualityStep + 1);
        slowTicks = 0;
      }
    } else if (dtMs < FAST_TICK_MS) {
      fastTicks += 1;
      slowTicks = 0;
      if (fastTicks >= 90) {
        setQualityStep(qualityStep - 1);
        fastTicks = 0;
      }
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
    this._visible = false;
    this._observer = null;
    this._renderScaleObserver = null;
    this._renderScaleRetry = null;
    this._initialized = false;
    this._scrubbing = false;
    this._peeking = false;
    this._latched = false;
    this._resumeOnLeave = false;
    this._suppressHoverPeek = false;
    this._downX = 0;
    this._rate = 1;
    this._markers = [];
    this._marks = null;
  }

  connectedCallback() {
    if (!this._initialized) {
      this._initialized = true;
      this._renderSkeleton();
      this._setupObserver();
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
    instances.delete(this);
    // `_playing` is deliberately left as-is: connectedCallback re-adds a
    // still-playing embed to the shared loop after a DOM move. Clearing it
    // here would make that re-arm dead code and silently freeze the figure.
  }

  attributeChangedCallback(name) {
    if (name === "aspect" && this._initialized) {
      this._stage.style.aspectRatio = String(this._aspectRatio());
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
    this._stage.style.aspectRatio = String(this._aspectRatio());
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
      }
      @keyframes shimmer { to { background-position: -300% 0; } }
      canvas { position: absolute; inset: 0; width: 100%; height: 100%;
               display: block; object-fit: contain; }
      .veil {
        position: absolute; inset: 0; display: flex; flex-direction: column;
        align-items: center; justify-content: center; gap: 10px;
        color: #8b96a7; font: 13px/1.4 system-ui, sans-serif;
        background: rgba(13,16,22,0.55); opacity: 0; transition: opacity .3s;
        pointer-events: none; text-align: center; padding: 12px;
      }
      .veil.show { opacity: 1; pointer-events: auto; }
      .veil.error { color: #ef6a6a; }
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
      /* The strip lives below the stage, in flow, and spans the full width
         and height of the bar: it never covers the picture. Hovering it
         freezes the clock and peeks the frame under the pointer; the clock
         resumes on leave unless the pause was latched by a click. */
      .strip {
        position: relative; height: 44px;
        display: flex; align-items: center;
        border-top: 1px solid rgba(255,255,255,.07);
        cursor: pointer; touch-action: none;
      }
      .strip:focus-visible { outline: 2px solid #f5b942; outline-offset: -2px; }
      .strip .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        background: rgba(245,185,66,.16);
        transform-origin: left; transform: scaleX(0);
        pointer-events: none;
      }
      .strip .marks { position: absolute; inset: 0; pointer-events: none; }
      /* Timeline landmarks. Keyframes are thin ticks; scene starts are
         diamonds; a transition is a hatched span — all full height. */
      .strip .tick {
        position: absolute; top: 0; bottom: 0; width: 2px;
        transform: translateX(-50%);
        background: rgba(255,255,255,.28);
        pointer-events: none;
      }
      .strip .diamond {
        position: absolute; top: 50%; width: 9px; height: 9px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 2px;
        pointer-events: none;
      }
      .strip .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.35) 0 4px, transparent 4px 8px);
        pointer-events: none;
      }
      /* The playhead: one line, constant width, positioned from the timeline
         position by _syncControls — the single writer. It is deliberately not
         the fill's border-right (scaleX squashes that toward nothing as the
         playhead nears the start) and deliberately not something a pointermove
         positions as a side effect: that made it visible while unpositioned,
         parked at the strip's start edge. */
      .strip .cursor {
        position: absolute; top: 0; bottom: 0; width: 2px; left: 0;
        transform: translateX(-50%);
        background: #f5b942;
        pointer-events: none;
      }
      .chip {
        position: absolute; right: 56px; top: 50%;
        transform: translateY(-50%);
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums;
        background: rgba(5,8,12,.55); border-radius: 6px; padding: 3px 8px;
        pointer-events: none;
      }
      .speed {
        position: absolute; right: 4px; top: 50%;
        transform: translateY(-50%);
        width: 48px; height: 36px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        font: 12px ui-monospace, monospace;
        display: flex; align-items: center; justify-content: center;
      }
      .speed:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .speed:focus-visible { outline: 2px solid #f5b942; }
      @media (pointer: coarse) {
        .strip { height: 52px; }
        .speed { width: 52px; height: 44px; }
      }
    `;
    this.shadowRoot.replaceChildren(style, this._stage);
    this._skeleton = document.createElement("div");
    this._skeleton.className = "skeleton";
    this._skeleton.textContent = title ? `${title}` : "animatix scene";
    this._stage.appendChild(this._skeleton);

    this._canvas = document.createElement("canvas");
    this._canvas.width = 1280;
    this._canvas.height = 720;
    this._canvas.hidden = true;

    this._veil = document.createElement("div");
    this._veil.className = "veil";

    this._playbtn = document.createElement("button");
    this._playbtn.className = "playbtn";
    this._playbtn.setAttribute("aria-label", "Play");
    this._playbtn.innerHTML = ICONS.playBig;
    this._playbtn.addEventListener("click", () => this.play());

    this._controls = null;
    this._marks = null;
  }

  _showVeil(text, isError) {
    this._veil.className = `veil show${isError ? " error" : ""}`;
    this._veil.textContent = text;
    if (!this._veil.isConnected) this.shadowRoot.appendChild(this._veil);
  }

  _hideVeil() {
    this._veil.className = "veil";
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
        }
      },
      { rootMargin: "200px" },
    );
    this._observer.observe(this);
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
      this._stage.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
    }
    this._markers = Array.isArray(result.markers) ? result.markers : [];
    this._configureCycle();
    const strip = this.shadowRoot?.querySelector(".strip");
    if (strip) strip.setAttribute("aria-valuemax", String(this._duration));
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

  async _maybeStartLoading() {
    const src = this.getAttribute("src");
    if (!src || this._state !== "idle") return;
    this._state = "loading";
    if (!("gpu" in navigator)) {
      this._state = "error";
      this._showVeil("This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).", true);
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
        this._showVeil(`Scene error${first?.line ? ` (line ${first.line})` : ""}: ${first?.message ?? "build failed"}`, true);
        return;
      }

      this._duration = Math.max(result.duration_s, 0.05);
      this._canvas.width = Math.round(result.width || 1280);
      this._canvas.height = Math.round(result.height || 720);
      this._stage.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
      this._configureCycle();
      // Poster = the finished composition, not frame 0. These scenes build up
      // from nothing, so frame 0 is an empty stage: a reader who never presses
      // play (or who asked for reduced motion) would see a blank box.
      this._restTime = this._duration;
      this._time = this._duration;
      this._renderScene();

      // Timeline landmarks for the scrubber (keyframes, scene starts,
      // transition windows) — the engine knows them, so the bar can show them.
      this._markers = Array.isArray(result.markers) ? result.markers : [];

      // swap skeleton for canvas + interactions
      this._skeleton.remove();
      this._canvas.hidden = false;
      this._stage.appendChild(this._canvas);
      // Only now does the canvas have a laid-out size to match the raster to.
      this._applyRenderScale();
      this._renderScaleObserver = new ResizeObserver(() => this._applyRenderScale());
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
      } else if (!this.hasAttribute("controls")) {
        this._playbtn.classList.add("show");
      }
    } catch (err) {
      this._state = "error";
      this._showVeil(`Failed to load scene: ${err?.message ?? err}`, true);
    }
  }

  _buildControls() {
    const strip = document.createElement("div");
    strip.className = "strip";
    strip.tabIndex = 0;
    strip.setAttribute("role", "slider");
    strip.setAttribute("aria-label", "Timeline");
    strip.setAttribute("aria-valuemin", "0");
    strip.setAttribute("aria-valuemax", String(this._duration));

    const fill = document.createElement("div");
    fill.className = "fill";
    const cursor = document.createElement("div");
    cursor.className = "cursor";

    // Landmarks at percentage positions of the pure timeline: keyframe ticks,
    // scene-start diamonds, hatched transition spans. They live in their own
    // layer so `applySource` can rebuild them when an edit changes the marks
    // or the timeline they are positioned against.
    this._marks = document.createElement("div");
    this._marks.className = "marks";
    this._renderMarks();

    const chip = document.createElement("span");
    chip.className = "chip";
    const speed = document.createElement("button");
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

    strip.append(fill, this._marks, cursor, chip, speed);
    this.shadowRoot.append(strip);

    // ── strip interaction ──
    // Mouse: entering freezes the clock and peeks the frame under the
    // pointer; leaving resumes *unless* a click latched the pause. A click
    // (press+release without dragging) sets the position and toggles the
    // latched state. Touch has no hover: a tap toggles, a drag scrubs.
    const snapTarget = (t) => {
      const SNAP_S = 0.2;
      for (const m of this._markers) {
        if (Math.abs(t - m.t) <= SNAP_S) return m.t;
        if (m.kind === "transition" && m.dur > 0 && Math.abs(t - (m.t + m.dur)) <= SNAP_S) {
          return m.t + m.dur;
        }
      }
      return null;
    };
    const timeAt = (e) => {
      const rect = strip.getBoundingClientRect();
      if (rect.width <= 0 || !Number.isFinite(e.clientX)) return this._time;
      const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
      const raw = frac * this._duration;
      return snapTarget(raw) ?? raw;
    };
    const peek = (e) => {
      this._time = timeAt(e);
      this._restTime = this._time;
      this._renderScene();
      this._syncControls();
    };

    strip.addEventListener("pointerenter", (e) => {
      if (e.pointerType !== "mouse" || this._suppressHoverPeek) return;
      this._peeking = true;
      this._resumeOnLeave = this._playing;
      strip.classList.add("peeking");
      peek(e);
    });
    strip.addEventListener("pointerdown", (e) => {
      try {
        strip.setPointerCapture(e.pointerId);
      } catch {
        // An invalid pointer id (automation, edge teardown) must not abort.
      }
      this._downX = e.clientX;
      // Any press on the strip is a scrub until proven a tap at release —
      // even while a previous resume suppressed hover peeking.
      this._scrubbing = true;
      if (e.pointerType === "mouse" && !this._peeking && !this._suppressHoverPeek) {
        this._peeking = true;
        this._resumeOnLeave = this._playing;
        strip.classList.add("peeking");
      }
      peek(e);
      e.stopPropagation();
    });
    // Exactly one move handler: `peek` re-renders the frame, so a second
    // listener with the same condition renders every move twice.
    strip.addEventListener("pointermove", (e) => {
      if (!this._scrubbing && !this._peeking) return;
      peek(e);
    });
    const release = (e) => {
      const wasScrub = this._scrubbing;
      this._scrubbing = false;
      const wasTap = wasScrub && Math.abs(e.clientX - this._downX) <= 4;
      if (!wasTap) return; // a drag just leaves the peeked position
      if (e.pointerType === "mouse") {
        if (this._playing) {
          this._latched = true;
          this.pause();
        } else {
          this._latched = false;
          this.play();
          // Keep playback observable while still hovering: no re-peek
          // until the pointer leaves the strip.
          this._suppressHoverPeek = true;
          this._peeking = false;
          strip.classList.remove("peeking");
        }
      } else if (this._playing) {
        this._latched = true;
        this.pause();
      } else {
        this._latched = false;
        this.play();
      }
    };
    strip.addEventListener("pointerup", release);
    strip.addEventListener("pointercancel", release);
    strip.addEventListener("pointerleave", (e) => {
      if (e.pointerType !== "mouse") return;
      strip.classList.remove("peeking");
      const resume = this._peeking && this._resumeOnLeave && !this._latched;
      this._peeking = false;
      this._resumeOnLeave = false;
      this._suppressHoverPeek = false;
      if (resume) {
        this.play();
      } else {
        this._renderScene();
      }
      this._syncControls();
    });
    strip.addEventListener("keydown", (e) => {
      const dir = e.key === "ArrowLeft" ? -1 : e.key === "ArrowRight" ? 1 : 0;
      if (dir !== 0) {
        e.preventDefault();
        e.stopPropagation();
        this.seek(this._nextLandmark(dir));
      } else if (e.key === "Home") {
        e.preventDefault();
        this.seek(0);
      } else if (e.key === " " || e.key === "k") {
        e.preventDefault();
        e.stopPropagation();
        if (this._playing) {
          this._latched = true;
          this.pause();
        } else {
          this._latched = false;
          this.play();
        }
      }
    });

    this._controls = { strip, fill, cursor, chip, speed };
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
        marks.appendChild(span);
      } else {
        const mark = document.createElement("div");
        mark.className = m.kind === "scene" ? "diamond" : "tick";
        mark.style.left = `${frac * 100}%`;
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
    c.chip.textContent = `${shown.toFixed(1)} / ${this._duration.toFixed(1)}`;
    c.strip.setAttribute("aria-valuenow", shown.toFixed(1));
  }

  /// Canvas gesture. A click pauses *latched*: leaving the strip afterwards
  /// does not resume — only a click on the strip does (the inspection model).
  /// An already-latched canvas click does nothing (that is the point of the
  /// latch); a plain paused figure (no autoplay) resumes on click. Touch
  /// behaves the same minus the hover machinery.
  _setupGestures() {
    this._canvas.addEventListener("pointerdown", (e) => {
      if (!this._controls) return;
      if (this._playing) {
        this._latched = true;
        this.pause();
      } else if (!this._latched) {
        this.play();
      }
      e.preventDefault();
    });
    this._canvas.addEventListener("keydown", (e) => {
      if (!this._controls) return;
      if (e.key === " " || e.key === "k") {
        e.preventDefault();
        if (this._playing) {
          this._latched = true;
          this.pause();
        } else {
          this._latched = false;
          this.play();
        }
      }
    });
  }

  // ── public API ──────────────────────────────────────────────────

  play() {
    if (this._state !== "ready") return;
    if (!this._player?.has_document()) return;
    if (!this._looping() && this._time >= this._duration) this._time = 0;
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
      // Without the control bar the center affordance is the only way back.
      if (!this._controls || !this.hasAttribute("autoplay")) {
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

  /// Called by the shared loop each frame. Returns whether it played.
  advance(dt) {
    if (!this._playing || !this._visible) return false;
    // Scrubbing or peeking owns the playhead: the clock freezes and the
    // inspected frame is what renders (the strip's seek renders directly).
    if (this._scrubbing || this._peeking) return false;
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
  ///
  /// The `qualityStep` multiplier on top is the page-wide concession when even
  /// that will not hold a frame; it lowers the raster below the backing store,
  /// so the blit upscales.
  _applyRenderScale() {
    const player = this._player;
    if (!player?.set_render_scale || !player.scene_width) return;
    if (!this.isConnected) return;
    const sceneW = player.scene_width() || this._canvas.width;
    const sceneH = player.scene_height() || this._canvas.height;
    const cssW = this._canvas.clientWidth || this._stage.clientWidth || 0;
    const cssH = this._canvas.clientHeight || this._stage.clientHeight || 0;
    const dpr = window.devicePixelRatio || 1;
    // Before layout (display:none, pre-append) the element reports 0; render
    // at full detail rather than clamping to a blurry minimum.
    const display =
      cssW > 0 && cssH > 0 && sceneW > 0 && sceneH > 0
        ? Math.min(1, (cssW * dpr) / sceneW, (cssH * dpr) / sceneH)
        : 1;
    const backingW = Math.max(1, Math.round(sceneW * display));
    const backingH = Math.max(1, Math.round(sceneH * display));
    if (this._canvas.width !== backingW || this._canvas.height !== backingH) {
      this._canvas.width = backingW;
      this._canvas.height = backingH;
    }
    const wanted = Math.min(1, Math.max(0.25, display * QUALITY_STEPS[qualityStep]));
    this._renderScale = player.set_render_scale(wanted);
  }

  _renderScene() {
    if (!this._player) return;
    try {
      // render at the scene's own resolution; CSS scales it down
      const t = this._playing ? Math.min(this._time, this._duration) : this._restTime;
      if (this._playing) this._restTime = t;
      this._player.render_frame(t);
      const alpha = this._loopAlpha();
      if (alpha !== this._lastAlpha) {
        this._canvas.style.opacity = String(alpha);
        this._lastAlpha = alpha;
      }
    } catch (err) {
      console.warn("amx-player: render failed", err);
    }
  }
}

if (!customElements.get("amx-player")) {
  customElements.define("amx-player", AmxPlayerElement);
}

export { AmxPlayerElement };
