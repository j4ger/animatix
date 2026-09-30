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
//   controls   control bar BELOW the picture (never covers it): play/pause,
//              a landmark-aware scrubber (the engine's `#` keyframe marks are
//              drawn as ticks and snapping targets; compositions also get
//              scene diamonds and hatched transition spans), a time readout,
//              and a speed cycle (1x -> 1.5x -> 2x -> 0.5x). A loop's hold
//              rest shows as a dimmed trailing segment. Canvas tap toggles
//              playback (mouse and touch alike); dragging freezes the clock
//              until release; the scrubber is a keyboard slider (arrows step
//              between landmarks, Home restarts, Space/K toggles).
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
// Looping: a scene's timeline ends at its last keyframe, so a naive wrap cuts
// from the finished composition straight to an empty first frame. Looping
// embeds therefore run a cycle of `hold` + `duration`: the finished frame rests,
// then dissolves out and back in. A scene author writes no hold of their own —
// `config { duration: N }` does not extend a single-scene timeline; the
// timeline ends at its last keyframe (`Timeline::duration_seconds`).
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
  play:
    '<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4.5 2.2v13.6L15.5 9z"/></svg>',
  pause:
    '<svg width="18" height="18" viewBox="0 0 18 18" fill="currentColor" aria-hidden="true"><path d="M4 2h3.6v14H4zM10.4 2H14v14h-3.6z"/></svg>',
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
          try {
            const module = await import(`${base}/animatix_web.js`);
            await module.default();
            await module.init_engine?.();
            return module;
          } catch (err) {
            lastError = err;
            enginePromises.delete(primary);
            if (i + 1 < bases.length) {
              console.info(`amx-player: no engine at ${base}, trying ${bases[i + 1]}`);
            }
          }
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

function tick(t) {
  const dt = Math.min((t - lastT) / 1000, 0.1);
  lastT = t;
  let anyPlaying = false;
  for (const inst of instances) {
    if (inst.advance(dt)) anyPlaying = true;
  }
  if (anyPlaying) {
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
    this._initialized = false;
    this._scrubbing = false;
    this._rate = 1;
    this._markers = [];
  }

  connectedCallback() {
    if (this._initialized) return;
    this._initialized = true;
    this._renderSkeleton();
    this._setupObserver();
  }

  disconnectedCallback() {
    this._observer?.disconnect();
    instances.delete(this);
    this._playing = false;
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
      :host { display: block; overflow: hidden;
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
      /* The bar lives below the stage, in flow: it never covers the picture,
         so it stays visible permanently and the canvas tap maps straight to
         play/pause. */
      .bar {
        display: flex; align-items: center; gap: 8px;
        padding: 6px 12px 8px;
        border-top: 1px solid rgba(255,255,255,.07);
      }
      .bar button {
        width: 40px; height: 40px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #e8edf4; cursor: pointer; padding: 0;
        display: flex; align-items: center; justify-content: center;
        font: 12px ui-monospace, monospace;
      }
      .bar button:hover { background: rgba(255,255,255,.14); }
      .bar button:focus-visible { outline: 2px solid #f5b942; }
      .bar svg { display: block; }
      .track {
        flex: 1; height: 32px; display: flex; align-items: stretch;
        cursor: pointer; touch-action: none; border-radius: 6px;
      }
      .track:focus-visible { outline: 2px solid #f5b942; }
      .track .zone, .track .rest { position: relative; display: flex; align-items: center; }
      .track .rail {
        position: relative; height: 4px; width: 100%;
        border-radius: 2px; background: rgba(255,255,255,.24);
        transition: height .12s;
      }
      .track:hover .rail, .track.scrubbing .rail { height: 7px; }
      .track .rest .rail { background: rgba(255,255,255,.11); }
      .track .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        border-radius: 2px; background: #f5b942;
        transform-origin: left; transform: scaleX(0);
      }
      .track .thumb {
        position: absolute; top: 50%; width: 13px; height: 13px;
        border-radius: 50%; background: #f5b942;
        transform: translate(-50%, -50%); opacity: 0; transition: opacity .12s;
      }
      .track:hover .thumb, .track.scrubbing .thumb, .track:focus-visible .thumb {
        opacity: 1;
      }
      /* Timeline landmarks. Keyframes are small ticks; scene starts are
         taller diamonds; a transition is a hatched span on the rail. */
      .track .tick {
        position: absolute; top: 50%; width: 2px; height: 8px;
        transform: translate(-50%, -50%);
        background: rgba(255,255,255,.45); border-radius: 1px;
        pointer-events: none;
      }
      .track .diamond {
        position: absolute; top: 50%; width: 7px; height: 7px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 1px;
        pointer-events: none;
      }
      .track .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.4) 0 3px, transparent 3px 6px);
        border-radius: 2px; pointer-events: none;
      }
      .time {
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums; flex: none;
      }
      .speed { min-width: 44px; justify-content: center; color: #c7cfd9; }
      @media (pointer: coarse) {
        .track { height: 40px; }
        .bar button { width: 44px; height: 44px; }
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
      this._renderScene();

      // Timeline landmarks for the scrubber (keyframes, scene starts,
      // transition windows) — the engine knows them, so the bar can show them.
      this._markers = Array.isArray(result.markers) ? result.markers : [];

      // swap skeleton for canvas + interactions
      this._skeleton.remove();
      this._canvas.hidden = false;
      this._stage.appendChild(this._canvas);
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
    const bar = document.createElement("div");
    bar.className = "bar";

    const btn = document.createElement("button");
    btn.setAttribute("aria-label", "Play");
    btn.innerHTML = ICONS.play;
    btn.addEventListener("click", (e) => {
      e.stopPropagation();
      this._playing ? this.pause() : this.play();
    });

    // The scrubber covers the timeline only; the loop's hold rest is a dimmed
    // trailing segment sized proportionally, so the bar explains why a looping
    // embed sits on its finished frame before restarting.
    const track = document.createElement("div");
    track.className = "track";
    track.tabIndex = 0;
    track.setAttribute("role", "slider");
    track.setAttribute("aria-label", "Seek");
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", String(this._duration));
    const zone = document.createElement("div");
    zone.className = "zone";
    zone.style.flexGrow = String(this._duration);
    const rail = document.createElement("div");
    rail.className = "rail";
    const fill = document.createElement("div");
    fill.className = "fill";
    const thumb = document.createElement("div");
    thumb.className = "thumb";
    rail.append(fill, thumb);
    zone.append(rail);
    const rest = document.createElement("div");
    rest.className = "rest";
    rest.style.flexGrow = String(this._holdSeconds);
    const restRail = document.createElement("div");
    restRail.className = "rail";
    rest.append(restRail);
    if (!(this._holdSeconds > 0)) rest.style.display = "none";

    // Landmarks drawn onto the rail: keyframe ticks, scene-start diamonds,
    // and hatched transition spans. Positions are percentages of the timeline
    // so they hold at any width.
    const railWidth = zone.querySelector(".rail");
    for (const m of this._markers) {
      const frac = this._duration > 0 ? Math.min(m.t / this._duration, 1) : 0;
      if (m.kind === "transition" && m.dur > 0) {
        const span = document.createElement("div");
        span.className = "span";
        span.style.left = `${frac * 100}%`;
        span.style.width = `${Math.min(m.dur / this._duration, 1 - frac) * 100}%`;
        railWidth.appendChild(span);
      } else {
        const mark = document.createElement("div");
        mark.className = m.kind === "scene" ? "diamond" : "tick";
        mark.style.left = `${frac * 100}%`;
        railWidth.appendChild(mark);
      }
    }

    const time = document.createElement("span");
    time.className = "time";

    // Playback speed: one button cycling the useful range. dt scales, so the
    // loop's hold and dissolves stay proportionally correct at any rate.
    const RATES = [1, 1.5, 2, 0.5];
    this._rate = 1;
    const speed = document.createElement("button");
    speed.className = "speed";
    speed.textContent = "1\u00d7";
    speed.setAttribute("aria-label", "Playback speed 1\u00d7");
    speed.addEventListener("click", (e) => {
      e.stopPropagation();
      const next = RATES[(RATES.indexOf(this._rate) + 1) % RATES.length];
      this._rate = next;
      speed.textContent = `${next}\u00d7`;
      speed.setAttribute("aria-label", `Playback speed ${next}\u00d7`);
    });

    track.append(zone, rest);
    bar.append(btn, track, time, speed);
    this.shadowRoot.append(bar);

    // ── scrubbing: pointer capture, works for mouse and touch alike ──
    // The engine's landmarks make the scrubber magnetic: within 0.2 s of a
    // keyframe or a transition edge the seek snaps to it.
    const SNAP_S = 0.2;
    const snapTarget = (t) => {
      for (const m of this._markers) {
        if (Math.abs(t - m.t) <= SNAP_S) return m.t;
        if (m.kind === "transition" && m.dur > 0 && Math.abs(t - (m.t + m.dur)) <= SNAP_S) {
          return m.t + m.dur;
        }
      }
      return null;
    };
    const seekFromPointer = (e) => {
      const rect = zone.getBoundingClientRect();
      if (rect.width <= 0) return;
      const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
      const raw = frac * this._duration;
      this._time = snapTarget(raw) ?? raw;
      this._restTime = this._time;
      this._renderScene();
      this._syncControls();
    };
    track.addEventListener("pointerdown", (e) => {
      try {
        track.setPointerCapture(e.pointerId);
      } catch {
        // A detached/invalid pointer id (automation, edge teardown) must not
        // abort the seek.
      }
      track.classList.add("scrubbing");
      this._scrubbing = true;
      seekFromPointer(e);
      e.stopPropagation();
    });
    track.addEventListener("pointermove", (e) => {
      if (this._scrubbing) seekFromPointer(e);
    });
    const endScrub = (e) => {
      if (!this._scrubbing) return;
      this._scrubbing = false;
      track.classList.remove("scrubbing");
      seekFromPointer(e);
    };
    track.addEventListener("pointerup", endScrub);
    track.addEventListener("pointercancel", endScrub);
    track.addEventListener("keydown", (e) => {
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
        this._playing ? this.pause() : this.play();
      }
    });

    this._controls = { bar, btn, track, zone, fill, thumb, time, speed, icons: ICONS };
    this._syncControls();
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
    const shown = Math.min(this._time, this._duration);
    const frac = this._duration > 0 ? shown / this._duration : 0;
    c.fill.style.transform = `scaleX(${frac})`;
    c.thumb.style.left = `${frac * 100}%`;
    c.time.textContent = `${shown.toFixed(1)} / ${this._duration.toFixed(1)}`;
    c.track.setAttribute("aria-valuenow", shown.toFixed(1));
    const want = this._playing ? c.icons.pause : c.icons.play;
    if (c.btn.dataset.icon !== want) {
      c.btn.dataset.icon = want;
      c.btn.innerHTML = want;
      c.btn.setAttribute("aria-label", this._playing ? "Pause" : "Play");
    }
  }

  /// Canvas gesture: the bar lives below the picture, so a tap maps straight
  /// to play/pause — the same rule for mouse and touch, no reveal state.
  _setupGestures() {
    this._canvas.addEventListener("pointerdown", () => {
      if (!this._controls) return;
      this._playing ? this.pause() : this.play();
    });
    this._canvas.addEventListener("keydown", (e) => {
      if (!this._controls) return;
      if (e.key === " " || e.key === "k") {
        e.preventDefault();
        this._playing ? this.pause() : this.play();
      }
    });
  }

  // ── public API ──────────────────────────────────────────────────

  play() {
    if (this._state !== "ready") return;
    if (!this._player?.has_document()) return;
    if (!this._looping() && this._time >= this._duration) this._time = 0;
    this._playing = true;
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
    // Scrubbing owns the playhead until release: the clock freezes and the
    // dragged frame is what renders (seekFromPointer renders directly).
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
