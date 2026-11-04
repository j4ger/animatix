import * as core from "./core.js";
const {
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, QUALITY_STEPS,
  SLOW_TICK_MS, FAST_TICK_MS, setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS,
  LOAD_MAX_DEFER_MS, LOAD_RETRY_MS, LOAD_STALL_MS, pumpLoads, enqueueLoad, dequeueLoad,
  ASPECTS,
} = core;
import { AmxPlayerInput } from "./input.js";
export class AmxPlayerPlayback extends AmxPlayerInput {
  _triggerStepHud() {
    const stepIdx = this._pausePoints.findIndex((p) => Math.abs(p - this._time) < 0.02);
    const text = stepIdx >= 0
      ? `Step ${stepIdx + 1} / ${this._pausePoints.length}`
      : "Paused";
    this._triggerSkipHud(text, true);
  }

  pause() {
    this._playing = false;
    instances.delete(this);
    if (this._state === "ready") {
      // Without the control bar the center affordance is the only way back —
      // except for a sealed embed, where the page is the way back, or in
      // presentation mode where slides rest cleanly without an obscuring overlay.
      if (!this._controls && !this._isSealed() && !this.hasAttribute("autoplay") && !this._isPresentation()) {
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
    const cost = this._costMs > 0 ? `${this._costMs.toFixed(2)}ms` : "—";
    return `frame ${this.frame}/${this.totalFrames} @${FRAME_RATE}fps, ` +
      `time ${this.time.toFixed(3)}s, render scale ${(this._renderScale ?? 1).toFixed(2)}, ` +
      `target ${this._targetFps ?? FRAME_RATE}fps, cost ${cost}, ${this._why || "ok"}`;
  }

  /// Per-instance budget controller state, read by the probe and the debug chip.
  get targetFps() {
    return this._targetFps ?? FRAME_RATE;
  }
  get costMs() {
    return this._costMs ?? 0;
  }
  get qualityScale() {
    return this._qualityScale ?? 1;
  }

  /// Called by the shared loop each frame. Returns whether it played.
  advance(dt) {
    if (!this._playing || !this._visible) return false;
    // Scrubbing owns the playhead: the clock freezes and the inspected frame
    // is what renders (the strip's seek renders directly).
    if (this._scrubbing) return false;
    const looping = this._looping();
    const limit = looping ? this._cycle : this._duration;
    const oldTime = this._time;
    const dtEff = dt * (this._rate ?? 1);
    const nextTime = oldTime + dtEff;

    if (this._isPresentation() && this._pausePoints.length > 0) {
      const nextPause = this._pausePoints.find((p) => p > oldTime + 1e-4 && p <= nextTime + 1e-4);
      if (nextPause !== undefined) {
        this._time = nextPause;
        this._restTime = nextPause;
        this._pausedAtStep = true;
        this.pause();
        this._renderScene();
        if (this._controls) this._syncControls();
        this._triggerStepHud();
        return false;
      }
    }

    this._time = nextTime;
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
    // A scroll is a compositor job; a synchronous GPU frame in the middle of
    // it is what makes a long page feel stuck. Keep the timeline moving but
    // skip the draw until the scroll settles (the next tick renders the new
    // position).
    if (performance.now() - core.lastScrollAt < SCROLL_SETTLE_MS) {
      this._restTime = this._time;
      this._wantRender = false; // never measure or adapt on a scroll frame
      if (this._controls) this._syncControls();
      return true;
    }
    this._wantRender = true; // the scheduler renders when the instance is due
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
    const geom = this._displayGeometry(sceneW, sceneH);
    const sizeChanged =
      this._canvas.width !== Math.max(1, Math.round(sceneW * geom)) ||
      this._canvas.height !== Math.max(1, Math.round(sceneH * geom));

    const commit = () => {
      this._resizeDebounceTimer = null;
      if (!this.isConnected || !this._player) return;
      const cur = this._displayGeometry(sceneW, sceneH);
      const bw = Math.max(1, Math.round(sceneW * cur));
      const bh = Math.max(1, Math.round(sceneH * cur));
      if (this._canvas.width !== bw || this._canvas.height !== bh) {
        this._canvas.width = bw;
        this._canvas.height = bh;
        // A reallocation frame is not evidence of steady-state cost.
        this._costMs = 0;
      }
      const wanted = Math.min(2.0, Math.max(0.25, cur * (this._qualityScale || 1)));
      this._renderScale = player.set_render_scale(wanted);
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

  /// The raster scale where one scene unit maps to one device pixel, allowed to
  /// supersample up to a pixel cap so a figure displayed larger than its
  /// authored resolution still renders crisply. The canvas is always backed at
  /// this size; `_qualityScale` is the separate, budget-driven multiplier.
  _displayGeometry(sceneW, sceneH) {
    if (!(sceneW > 0 && sceneH > 0)) return 1;
    const cssW = this._canvas.clientWidth || this._stage.clientWidth || 0;
    const cssH = this._canvas.clientHeight || this._stage.clientHeight || 0;
    if (!(cssW > 0 && cssH > 0)) return 1;
    const dpr = window.devicePixelRatio || 1;
    let deviceW = cssW * dpr;
    let deviceH = cssH * dpr;
    if (this._deviceBox && this._deviceBox.w > 0) {
      deviceW = this._deviceBox.w;
      deviceH = this._deviceBox.h;
    }
    const maxPixels = 4000000;
    const cap = Math.min(2.5, Math.sqrt(maxPixels / (sceneW * sceneH)));
    const display = Math.min(cap, deviceW / sceneW, deviceH / sceneH);
    return display > 0 ? display : 1;
  }

  // ── per-instance budget controller ──────────────────────────────
  _dueRender(now) {
    const interval = 1000 / Math.max(1, this._targetFps || FRAME_RATE);
    return now - this._lastRenderAt >= interval - 1.5;
  }

  /// Record how long this embed's own render call took. This is the feedback
  /// signal: it includes GPU backpressure on acquire but not the page's
  /// scroll/GC/layout time.
  _noteRenderMs(ms, now) {
    this._lastRenderAt = now;
    if (!(ms >= 0)) return;
    const clamped = Math.min(ms, 200);
    this._costMs = this._costMs > 0 ? this._costMs * 0.82 + clamped * 0.18 : clamped;
  }

  _reseedClock() {
    this._lastRenderAt = performance.now();
  }

  /// Re-arm the raster when devicePixelRatio changes (moving between monitors,
  /// browser zoom). ResizeObserver covers element size; this covers dpr.
  _watchDpr() {
    if (this._dprQuery && this._onDprChange) {
      try {
        this._dprQuery.removeEventListener("change", this._onDprChange);
      } catch {
        /* ignore */
      }
      this._dprQuery = null;
      this._onDprChange = null;
    }
    if (!window.matchMedia) return;
    const dpr = window.devicePixelRatio || 1;
    try {
      const query = window.matchMedia(`(resolution: ${dpr}dppx)`);
      this._dprQuery = query;
      this._onDprChange = () => {
        this._deviceBox = null;
        this._watchDpr();
        this._applyRenderScale(true);
      };
      query.addEventListener("change", this._onDprChange);
    } catch {
      /* older browser: no dpr-change signal; ResizeObserver still covers size */
    }
  }

  /// Turn this embed's slice of the page budget (ms of render time per second)
  /// into a target cadence and, only if the cadence floor is still too slow, a
  /// smaller raster — reverting the raster if it did not actually lower cost.
  _adaptBudget(perInstanceMsPerSec) {
    const MAX_FPS = 60;
    const FLOOR_FPS = 20;
    if (this._scaleTrial) {
      if (this._costMs >= this._scaleTrial.costBefore * 0.9) {
        this._qualityScale = this._scaleTrial.from;
        this._scaleIneffective = true;
        this._applyRenderScale(true);
      }
      this._scaleTrial = null;
    }
    const cost = Math.max(0.15, this._costMs || 1);
    const desired = this._desiredFps || FRAME_RATE;
    const refreshCap = Math.min(MAX_FPS, Math.max(30, Math.round(1000 / core.refreshInterval)));
    const target = Math.min(desired, refreshCap, Math.max(FLOOR_FPS, Math.round(perInstanceMsPerSec / cost)));
    const floorTooSlow = FLOOR_FPS * cost > perInstanceMsPerSec * 1.15;
    if (floorTooSlow && !this._scaleIneffective) {
      this._maybeReduceScale();
      this._why = "budget:raster";
    } else {
      if (this._qualityScale < 1 && !floorTooSlow) this._maybeRaiseScale();
      this._why = target < desired ? "budget:fps" : "ok";
    }
    this._targetFps = target;
  }

  _maybeReduceScale() {
    const i = QUALITY_STEPS.indexOf(this._qualityScale);
    if (this._scaleTrial || i < 0 || i >= QUALITY_STEPS.length - 1) return;
    this._scaleTrial = { from: this._qualityScale, costBefore: this._costMs };
    this._qualityScale = QUALITY_STEPS[i + 1];
    this._applyRenderScale(true);
  }

  _maybeRaiseScale() {
    const i = QUALITY_STEPS.indexOf(this._qualityScale);
    if (i <= 0) return;
    this._qualityScale = QUALITY_STEPS[i - 1];
    this._scaleTrial = null;
    this._applyRenderScale(true);
  }

  _renderScene() {
    if (!this._player) return;
    try {
      // The player rasterizes at its configured scale; the canvas backs the
      // displayed device size, so a scale below display match upscales on blit.
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
