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
//   controls   show a minimal play/pause + scrub bar on hover
//   title      accessibility label; shown on the skeleton while loading
//   aspect     "16:9" | "4:3" | "1:1" | "9:16" — reserve space before first frame
//              (auto-detected from the scene afterwards)
//   data-runtime-base  URL where the engine bundle lives
//                      (default: relative to this script's own URL)
//
// Loading UX (no build-time poster required):
//   1. skeleton with shimmer + title, correct aspect ratio
//   2. lazy: nothing is fetched until the element approaches the viewport
//   3. first rendered frame becomes the poster; autoplay decision follows
//   4. paused instances show a subtle play affordance
//
// Performance: all visible playing instances are driven by ONE shared
// requestAnimationFrame loop; offscreen instances pause automatically. All
// players share one WebGPU engine context inside the wasm module.

const RUNTIME_BASE = (() => {
  for (const script of document.querySelectorAll("script[type=module]")) {
    const src = script.src || "";
    if (src.endsWith("/amx-player.js") || src.includes("/amx-player.js?")) {
      const i = src.lastIndexOf("/amx-player.js");
      return src.slice(0, i);
    }
  }
  return new URL(".", import.meta.url).href.replace(/\/$/, "");
})();

const REDUCED_MOTION = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
const SAVE_DATA = navigator.connection?.saveData === true;

// ── shared engine loading ───────────────────────────────────────────

let enginePromise = null;

function loadEngine() {
  if (!enginePromise) {
    enginePromise = (async () => {
      const module = await import(`${RUNTIME_BASE}/../pkg/animatix_web.js`);
      await module.default();
      await module.init_engine?.();
      return module;
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
  static observedAttributes = ["src", "autoplay", "loop", "controls", "title", "aspect"];

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this._state = "idle"; // idle | loading | ready | error
    this._player = null;
    this._playing = false;
    this._loop = false;
    this._time = 0;
    this._duration = 0;
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
    const title = this.getAttribute("title") || "";
    const style = document.createElement("style");
    style.textContent = `
      :host { display: block; position: relative; overflow: hidden;
              border-radius: 8px; background: #10141b; }
      .skeleton {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        aspect-ratio: ${this._aspectRatio()};
        background: linear-gradient(120deg, #10141b 40%, #1a2130 50%, #10141b 60%);
        background-size: 300% 100%;
        animation: shimmer 2.2s linear infinite;
        color: #5b6575; font: 13px/1.4 system-ui, sans-serif;
        max-height: 100%;
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
    if (this._time >= this._duration) this._time = 0;
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
    this._time += dt;
    if (this._time >= this._duration) {
      if (this.hasAttribute("loop") || this._loop) {
        this._time %= this._duration;
      } else {
        this._time = this._duration;
        this.pause();
        this._renderScene();
        return false;
      }
    }
    this._renderScene();
    if (this._controls) {
      this._controls.scrub.value = Math.round((this._time / this._duration) * 1000);
      this._updateTime(this._controls.time);
    }
    return true;
  }

  _renderScene() {
    if (!this._player) return;
    try {
      // render at the scene's own resolution; CSS scales it down
      this._player.render_frame(this._time);
    } catch (err) {
      console.warn("amx-player: render failed", err);
    }
  }
}

if (!customElements.get("amx-player")) {
  customElements.define("amx-player", AmxPlayerElement);
}

export { AmxPlayerElement };
