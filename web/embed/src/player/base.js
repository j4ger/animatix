import * as core from "./core.js";
const {
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, QUALITY_STEPS,
  SLOW_TICK_MS, FAST_TICK_MS, setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS,
  LOAD_MAX_DEFER_MS, LOAD_RETRY_MS, LOAD_STALL_MS, pumpLoads, enqueueLoad, dequeueLoad,
  ASPECTS,
} = core;
export class AmxPlayerBase extends HTMLElement {
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
    "presentation",
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
    this._pausePoints = [];
    this._pausedAtStep = false;
    this._blackout = false;
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
}
