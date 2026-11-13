import * as core from "./core.js";
import { cancellationError, dispatchPlayerError } from "./errors.js";
const {
  FRAME_RATE, DEBUG_BY_URL, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, instances, QUALITY_STEPS,
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
    "quality",
    "presentation",
    "data-fonts",
    "data-vars",
  ];

  constructor() {
    super();
    this.attachShadow({ mode: "open" });
    this._state = "idle"; // idle | loading | ready | error
    this._player = null;
    this._runtime = null;
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
    // True while the *visibility observer* is the reason playback is
    // stopped, so re-entering the viewport can undo exactly that pause and
    // never a user's. See `_pauseForVisibility` in playback.js.
    this._pausedByVisibility = false;
    // True while the page-level concurrency cap is the reason playback is
    // stopped (`window.__amxMaxPlaying`). The scheduler owns resumption; see
    // `arbitrateConcurrency` in core.js.
    this._parkedFromPage = false;
    // Wall-clock of the last *explicit* play (a control press, a page's
    // `play()`), 0 when none. The concurrency cap gives a recently explicit
    // figure the live slot so a reader who starts one is not overruled by the
    // scheduler a window later; `_schedulerResume` keeps scheduler/visibility
    // resumptions out of this signal.
    this._pinPlayAt = 0;
    this._schedulerResume = false;
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
    this._committedVars = {};
    this._vars = null; // merged build-time overrides; null = read `data-vars`
    this._varsTimer = null; // setVars/data-vars debounce; see view.js
    this._loadedSource = null; // scene text, cached for an in-place rebuild
    this._loadedBundle = null; // committed prepared source and resources
    this._operationGeneration = 0;
    this._operationController = null;
    this._destroyed = false;
    this._varsWaiters = [];
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
    // Per-instance adaptive quality (see core.js tick / playback controller).
    this._qualityScale = 1; // budget-driven raster multiplier (1 = display match)
    this._displayScale = 1; // display-matched scale cached by _applyRenderScale
    this._pixelScale = 1; // page-wide raster-budget multiplier (1 = within budget)
    this._desiredFps = FRAME_RATE;
    this._targetFps = FRAME_RATE;
    this._costMs = 0; // EMA of this embed's own render time
    this._lastRenderAt = 0;
    this._wantRender = false;
    this._scaleTrial = null;
    this._scaleIneffective = false;
    this._scaleIneffectiveUntil = 0;
    this._pressure = 0;
    this._phaseOffset = 0;
    this._renderRevision = 0; // bumped by anything that changes pixels at a fixed t
    this._lastRenderKey = null; // shell-side held-frame dedup (see _invalidateRestFrame)
    this._lastRenderWall = undefined;
    this._why = "ok";
    this._deviceBox = null; // exact device-pixel size from ResizeObserver
    this._dprQuery = null;
    this._onDprChange = null;
  }

  connectedCallback() {
    if (this._destroyed) return;
    // `data-fps` / `fps` caps this embed's cadence (12..60). Re-read on every
    // attach so a DOM move cannot strand a stale value; `load.js` re-reads it
    // once the document is built.
    this._desiredFps = this._fpsCap();
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
    if (this._dprQuery && this._onDprChange) {
      try {
        this._dprQuery.removeEventListener("change", this._onDprChange);
      } catch {
        /* ignore */
      }
      this._dprQuery = null;
      this._onDprChange = null;
    }
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
    if (name === "quality" && this._initialized && this._state === "ready") {
      this._cancelOperation();
      this._rebuildWithQuality().catch((error) => {
        if (error?.name !== "AbortError") {
          console.warn(`amx-player: quality rebuild failed (${error.message})`);
          dispatchPlayerError(this, this.getAttribute("src"), error, "build");
        }
      }).finally(() => {
        // A quality change supersedes an in-flight setVars transaction. Once
        // the current document has been restored at the new quality, run the
        // queued variable batch so its promises cannot remain pending.
        if (this._varsWaiters.length && this._state === "ready") this._scheduleVarsRebuild();
      });
    }
    if (name === "data-vars" && this._initialized) {
      // Build-time variable overrides. The attribute is the declarative set, so
      // it replaces what is in effect (an earlier `setVars` merge does not
      // survive it), and the rebuild is debounced because a page wiring a
      // slider writes it on every input event. Vars are baked in at build time,
      // so like `quality` this means a rebuild — but an in-place one: the loaded
      // source, fonts, fetched imports and assets all stay registered.
      this._varsFromAttribute();
      if (this._state === "loading") this._resetForReload();
      else {
        if (this._state === "ready") this._cancelOperation();
        this._scheduleVarsRebuild(true);
      }
    }
    if ((name === "src" || name === "data-fonts") && this._initialized) {
      this._resetForReload(name);
    }
  }

  _cancelOperation() {
    this._operationGeneration += 1;
    this._operationController?.abort();
    this._operationController = null;
    return this._operationGeneration;
  }

  _beginOperation() {
    const generation = this._cancelOperation();
    const controller = new AbortController();
    this._operationController = controller;
    return { generation, controller };
  }

  _isCurrentOperation(generation) {
    return !this._destroyed && generation === this._operationGeneration;
  }

  _freePlayer(player = this._player) {
    if (!player) return;
    try {
      player.free?.();
    } catch (error) {
      console.warn("amx-player: could not release player", error);
    }
  }

  /// Explicitly destroy this player. A DOM detach is not destruction because
  /// live editors reparent players while keeping their wasm state.
  destroy() {
    if (this._destroyed) return;
    this._destroyed = true;
    this._cancelOperation();
    this.disconnectedCallback();
    core.forgetInstance(this);
    this.pause?.();
    this._freePlayer();
    this._player = null;
    this._loadedBundle?.prepared?.free?.();
    this._loadedBundle = null;
    this._state = "destroyed";
    if (this._canvas) this._canvas.width = this._canvas.height = 1;
    this.shadowRoot.replaceChildren();
    this._varsWaiters.splice(0).forEach(({ reject }) => reject(cancellationError("Player destroyed")));
  }

  _resetForReload(reason) {
    if (this._state === "ready" && this._player) {
      this._reloadFromAttributes(reason).catch((error) => {
        dispatchPlayerError(this, this.getAttribute("src"), error, "resource");
      });
      return;
    }
    this._cancelOperation();
    this._freePlayer();
    this._loadedBundle?.prepared?.free?.();
    this._state = "idle";
    this._player = null;
    this._time = 0;
    // A reload builds a fresh player, so nothing has been pushed to it yet and
    // the cached document inputs belonged to the player being dropped. `_vars`
    // itself survives: the page's parameter set outlives one engine instance.
    clearTimeout(this._varsTimer);
    this._varsTimer = null;
    this._loadedSource = null;
    this._loadedBundle = null;
    this._invalidateRestFrame();
    this._renderSkeleton();
    this._maybeStartLoading();
  }

  /// `sealed` — page-driven mode (see the attribute docs above). Re-read on
  /// change so a page can seal or unseal an embed mid-flight.
  _isSealed() {
    return this.hasAttribute("sealed");
  }

  /// Show the center play affordance — the only way back for an embed with no
  /// control bar. Suppressed where something else already resumes: the strip's
  /// own play button (`controls`), page-driven playback (`sealed`), and
  /// presentation mode (slides rest without an obscuring overlay).
  ///
  /// `autoplay` is deliberately *not* in that list. It used to be, on the
  /// reasoning that an autoplay embed is paused by the page rather than by the
  /// user — but that left every controls-less autoplay figure with no way back
  /// at all once anything stopped it. A visible button is strictly better than a
  /// frozen picture.
  _showPlayAffordance() {
    if (!this._playbtn) return;
    if (this._controls || this._isSealed() || this._isPresentation()) return;
    this._playbtn.classList.add("show");
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

  /// `data-fps` / `fps` — the per-embed cadence cap, clamped to 12..60. Absent
  /// (or unparseable) keeps the full 60 Hz timeline cadence. The page-wide
  /// ladder in core.js can still lower it under pressure; this only caps.
  _fpsCap() {
    const raw = this.getAttribute("data-fps") ?? this.getAttribute("fps");
    if (raw === null || raw === "") return FRAME_RATE;
    const value = Number(raw);
    if (!Number.isFinite(value)) return FRAME_RATE;
    return Math.min(60, Math.max(12, Math.round(value)));
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
