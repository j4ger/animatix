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
//   controls   show a minimal play/pause + scrub bar on hover
//   title      accessibility label; shown on the skeleton while loading
//   aspect     "16:9" | "4:3" | "1:1" | "9:16" — reserve space before first frame
//              (auto-detected from the scene afterwards)
//
// The engine bundle directory is a `data-runtime-base` attribute on the
// <script> tag that loads this component (absolute, or relative to the page) —
// e.g. `data-runtime-base="/pkg-slim"`. Default: the `pkg` directory beside
// this component's parent.
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
// requestAnimationFrame loop; offscreen instances pause automatically. All
// players share one WebGPU engine context inside the wasm module.

const LOADER_SCRIPT = [...document.querySelectorAll("script[type=module]")].find((s) =>
  (s.src || "").includes("amx-player.js"),
);

const RUNTIME_BASE = (() => {
  const src = LOADER_SCRIPT?.src || "";
  const i = src.lastIndexOf("/amx-player.js");
  if (i >= 0) return src.slice(0, i);
  return new URL(".", import.meta.url).href.replace(/\/$/, "");
})();

// Directories that may hold the wasm module + its JS glue. The loader script's
// `data-runtime-base` picks the first one — a page of plain-text scenes can
// serve the slim build instead of the full one. The default directory beside
// this component is always the last resort, so a page that points at a profile
// the host did not build degrades to the other one instead of a blank figure.
const ENGINE_BASES = (() => {
  const fallback = `${RUNTIME_BASE}/../pkg`;
  const override = LOADER_SCRIPT?.getAttribute("data-runtime-base");
  if (!override) return [fallback];
  const configured = new URL(override, document.baseURI).href.replace(/\/$/, "");
  return configured === fallback ? [fallback] : [configured, fallback];
})();

const REDUCED_MOTION = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
const SAVE_DATA = navigator.connection?.saveData === true;

// Loop treatment. FADE_EACH is the dissolve on either side of a looping
// restart; it never exceeds a third of the cycle so the finished frame always
// gets real rest time.
const FADE_EACH = 0.28;

// ── shared engine loading ───────────────────────────────────────────

let enginePromise = null;

function loadEngine() {
  if (!enginePromise) {
    enginePromise = (async () => {
      let lastError;
      for (const [i, base] of ENGINE_BASES.entries()) {
        try {
          const module = await import(`${base}/animatix_web.js`);
          await module.default();
          await module.init_engine?.();
          return module;
        } catch (err) {
          lastError = err;
          if (i + 1 < ENGINE_BASES.length) {
            console.info(`amx-player: no engine at ${base}, trying ${ENGINE_BASES[i + 1]}`);
          }
        }
      }
      throw lastError;
    })();
  }
  return enginePromise;
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
  static observedAttributes = ["src", "autoplay", "loop", "hold", "controls", "title", "aspect"];

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
      this.style.aspectRatio = String(this._aspectRatio());
    }
    if (name === "hold" && this._initialized && this._duration > 0) {
      this._configureCycle();
    }
    if (name === "src" && this._initialized) {
      this._state = "idle";
      this._player = null;
      this._time = 0;
      this._renderSkeleton();
      this._maybeStartLoading();
    }
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
    // page: the host carries the aspect ratio until the scene reports its own.
    this.style.aspectRatio = String(this._aspectRatio());
    const title = this.getAttribute("title") || "";
    const style = document.createElement("style");
    style.textContent = `
      :host { display: block; position: relative; overflow: hidden;
              border-radius: 8px; background: #0a0f17; }
      .skeleton {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        background: linear-gradient(120deg, #0a0f17 40%, #141c28 50%, #0a0f17 60%);
        background-size: 300% 100%;
        animation: shimmer 2.2s linear infinite;
        color: #808fa6; font: 13px/1.4 system-ui, sans-serif;
      }
      @keyframes shimmer { to { background-position: -300% 0; } }
      canvas { width: 100%; height: 100%; display: block; object-fit: contain; }
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
        width: 52px; height: 52px; border-radius: 50%;
        border: 1px solid rgba(245,185,66,.5); background: rgba(245,185,66,.14);
        color: #f5b942; font-size: 20px; cursor: pointer; display: none;
        align-items: center; justify-content: center; padding-left: 4px;
      }
      .playbtn.show { display: flex; }
      .controls {
        position: absolute; left: 50%; bottom: 10px; transform: translateX(-50%);
        display: none; align-items: center; gap: 10px; width: min(70%, 420px);
        background: rgba(13,16,22,.66); border: 1px solid rgba(255,255,255,.12);
        border-radius: 10px; padding: 6px 12px; backdrop-filter: blur(6px);
        transition: opacity .25s; opacity: 0;
      }
      :host(:hover) .controls.show { opacity: 1; }
      .controls input { flex: 1; accent-color: #f5b942; height: 3px; cursor: pointer; }
      .controls button { background: none; border: none; color: #f5b942;
                         cursor: pointer; font-size: 14px; width: 20px; }
      .time { color: #8b96a7; font: 11px ui-monospace, monospace; white-space: nowrap; }
    `;
    this.shadowRoot.replaceChildren(style);
    this._skeleton = document.createElement("div");
    this._skeleton.className = "skeleton";
    this._skeleton.textContent = title ? `${title}` : "animatix scene";
    this.shadowRoot.appendChild(this._skeleton);

    this._canvas = document.createElement("canvas");
    this._canvas.width = 1280;
    this._canvas.height = 720;
    this._canvas.hidden = true;

    this._veil = document.createElement("div");
    this._veil.className = "veil";

    this._playbtn = document.createElement("button");
    this._playbtn.className = "playbtn";
    this._playbtn.setAttribute("aria-label", "Play");
    this._playbtn.textContent = "▶";
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
      const [module, source] = await Promise.all([
        loadEngine(),
        fetch(src).then((r) => {
          if (!r.ok) throw new Error(`HTTP ${r.status}`);
          return r.text();
        }),
      ]);
      if (this.getAttribute("src") !== src) return; // changed while loading

      this._player = await module.create_player(this._canvas);
      if (this.getAttribute("src") !== src) return;

      const result = this._player.load_source(source);
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
      this.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
      this._configureCycle();
      // Poster = the finished composition, not frame 0. These scenes build up
      // from nothing, so frame 0 is an empty stage: a reader who never presses
      // play (or who asked for reduced motion) would see a blank box.
      this._restTime = this._duration;
      this._renderScene();

      // swap skeleton for canvas + interactions
      this._skeleton.remove();
      this._canvas.hidden = false;
      this.shadowRoot.appendChild(this._canvas);
      this.shadowRoot.appendChild(this._playbtn);
      this.shadowRoot.appendChild(this._veil);
      if (this.hasAttribute("controls")) this._buildControls();
      if (diags.length > 0) {
        console.warn(`amx-player: ${src} built with ${diags.length} diagnostic(s)`, diags[0]);
      }

      this._state = "ready";
      if (this._shouldAutoplay()) {
        this._loop = this.hasAttribute("loop");
        this.play();
      } else {
        this._playbtn.classList.add("show");
      }
    } catch (err) {
      this._state = "error";
      this._showVeil(`Failed to load scene: ${err?.message ?? err}`, true);
    }
  }

  _buildControls() {
    const controls = document.createElement("div");
    controls.className = "controls show";
    const btn = document.createElement("button");
    btn.textContent = "▶";
    const scrub = document.createElement("input");
    scrub.type = "range";
    scrub.min = 0;
    scrub.max = 1000;
    scrub.value = 0;
    const time = document.createElement("span");
    time.className = "time";
    btn.addEventListener("click", () => (this._playing ? this.pause() : this.play()));
    scrub.addEventListener("input", () => {
      this._time = (Number(scrub.value) / 1000) * this._duration;
      this._restTime = this._time;
      this._renderScene();
      this._updateTime(time);
    });
    controls.append(btn, scrub, time);
    this.shadowRoot.appendChild(controls);
    this._controls = { btn, scrub, time };
  }

  _updateTime(el) {
    if (el) el.textContent = `${this._time.toFixed(1)}s`;
  }

  // ── public API ──────────────────────────────────────────────────

  play() {
    if (this._state !== "ready") return;
    if (!this._player?.has_document()) return;
    if (!this._looping() && this._time >= this._duration) this._time = 0;
    this._playing = true;
    this._playbtn.classList.remove("show");
    if (this._controls) this._controls.btn.textContent = "⏸";
    instances.add(this);
    ensureLoop();
  }

  pause() {
    this._playing = false;
    instances.delete(this);
    if (this._state === "ready") {
      if (this.hasAttribute("controls") || !this.hasAttribute("autoplay")) {
        this._playbtn.classList.add("show");
      }
      if (this._controls) this._controls.btn.textContent = "▶";
    }
  }

  /// Called by the shared loop each frame. Returns whether it played.
  advance(dt) {
    if (!this._playing || !this._visible) return false;
    const looping = this._looping();
    const limit = looping ? this._cycle : this._duration;
    this._time += dt;
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
    if (this._controls) {
      this._controls.scrub.value = Math.round(
        (Math.min(this._time, this._duration) / this._duration) * 1000,
      );
      this._updateTime(this._controls.time);
    }
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
