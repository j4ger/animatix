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
    this._invalidateRestFrame();
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
      `display ${(this._displayScale ?? 1).toFixed(2)}, pixel ${(this._pixelScale ?? 1).toFixed(2)}, ` +
      `raster ${Math.round(this.rasterPixels)}px, ` +
      `target ${this._targetFps ?? FRAME_RATE}fps, cost ${cost}, ` +
      `pressure ${(this._pressure ?? 0).toFixed(2)}, ${this._why || "ok"}`;
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
  /// Display-matched raster scale (canvas backing before the budget steps).
  get displayScale() {
    return this._displayScale ?? 1;
  }
  /// Page-wide raster-budget multiplier; 1 while the page fits its budget.
  get pixelScale() {
    return this._pixelScale ?? 1;
  }
  /// Raster pixels this embed currently asks the engine for.
  get rasterPixels() {
    return this._rasterPixels();
  }
  /// Fraction of committed 60 Hz steps that missed their vsync (0..1). This is
  /// the real frame-pressure signal; `costMs` is only a relative weight now.
  get pressure() {
    return this._pressure ?? 0;
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
  /// The `qualityStep` / page pixel-scale multiplier on top is the concession
  /// when even that will not hold a frame or fit the page's raster budget; it
  /// lowers the raster below the backing store, so the blit upscales.
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
      // Cache the display match: the shared scheduler compares rasters across
      // embeds every window, and re-reading layout from there would be a
      // forced-synchronous-layout per player per window.
      this._displayScale = cur;
      const bw = Math.max(1, Math.round(sceneW * cur));
      const bh = Math.max(1, Math.round(sceneH * cur));
      if (this._canvas.width !== bw || this._canvas.height !== bh) {
        this._canvas.width = bw;
        this._canvas.height = bh;
        // A reallocation frame is not evidence of steady-state cost.
        this._costMs = 0;
      }
      const wanted = Math.min(2.0, Math.max(0.25, cur * this._effectiveScale()));
      this._renderScale = player.set_render_scale(wanted);
      this._invalidateRestFrame();
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
  /// Display-matched raster, with a hard cap on supersampling. A HiDPI pane
  /// asks for more device pixels than a 1280×720 scene has detail to give
  /// (docs/performance_evaluation.md §3.7), so 2× supersampling quadrupled GPU
  /// work for no visible gain. The cap also bounds the first frames before the
  /// embed's `_applyRenderScale` has run.
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
    const MAX_RASTER_PIXELS = 2000000;
    const MAX_SUPERSAMPLE = 1.5;
    const cap = Math.min(MAX_SUPERSAMPLE, Math.sqrt(MAX_RASTER_PIXELS / (sceneW * sceneH)));
    const display = Math.min(cap, deviceW / sceneW, deviceH / sceneH);
    return display > 0 ? display : 1;
  }

  /// Display-matched raster pixels *before* the budget's quality multiplier:
  /// what this embed would raster if one scene unit mapped to one displayed
  /// device pixel. This is the area the page-wide budget is split across.
  /// Zero until a scene exists: an element that never loaded has no raster, and
  /// reporting its placeholder canvas as one would make a lazy page look over
  /// budget.
  _desiredRasterPixels() {
    if (!this._player) return 0;
    const sceneW = this._player.scene_width?.() || this._canvas?.width || 0;
    const sceneH = this._player.scene_height?.() || this._canvas?.height || 0;
    const display = this._displayScale ?? 1;
    return sceneW * sceneH * display * display;
  }

  /// Raster pixels this embed actually asks the engine for, after both budget
  /// controllers: display match × min(quality step, page pixel scale).
  _rasterPixels() {
    if (!this._player) return 0;
    const sceneW = this._player.scene_width?.() || this._canvas?.width || 0;
    const sceneH = this._player.scene_height?.() || this._canvas?.height || 0;
    const scale = (this._displayScale ?? 1) * this._effectiveScale();
    return sceneW * sceneH * scale * scale;
  }

  /// The raster multiplier the two budget controllers agree on: `_qualityScale`
  /// is the per-embed pressure ladder, `_pixelScale` is the page-wide area
  /// budget. Both are upper bounds on the display match.
  _effectiveScale() {
    return Math.min(this._qualityScale ?? 1, this._pixelScale ?? 1);
  }

  // ── per-instance cadence / phase ──────────────────────────────────
  _dueRender(now) {
    const interval = 1000 / Math.max(1, this._targetFps || FRAME_RATE);
    // Stagger N embeds across one cadence interval instead of aligning every
    // `_lastRenderAt` to the same instant (the old `ensureLoop` behaviour),
    // which piled all their GPU work into a single rAF tick.
    const phase = this._phaseOffset || 0;
    return now - this._lastRenderAt - phase >= interval - 1.5;
  }

  /// Record how long this embed's own render call took. This is NOT the frame
  /// cost — `queue.submit` returns before the GPU draws — it is a relative
  /// weight for deciding which embeds to stagger first, and a diagnostic.
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

  /// Apply the page-wide cadence ladder. `pageTargetFps` comes from the
  /// measured missed-vsync ratio in core.js; `_costMs` is deliberately NOT the
  /// input — it cannot see GPU work (docs/performance_evaluation.md §3.7).
  /// Only at the ladder floor with sustained pressure do we try a smaller
  /// raster, and a trial is judged by the pressure ratio, not by CPU encode
  /// time (which barely changes with resolution, so the old check always
  /// declared every trial ineffective and then locked raster reduction out
  /// for the rest of the document's life).
  /// `pagePixelScale` is the page-wide area budget from core.js: every embed
  /// scales by the same factor so the *sum* of their rasters fits the page's
  /// pixel budget. It is an upper bound like `_qualityScale`, not a multiplier
  /// on top of it — the two budgets must not compound.
  _adaptBudget(pageTargetFps, perInstanceMsPerSec, index, n, pressure, pagePixelScale = 1) {
    const MAX_FPS = 60;
    const FLOOR_FPS = 20;
    const now = performance.now();
    this._phaseOffset = (index / Math.max(1, n)) * (1000 / Math.max(1, pageTargetFps));
    this._pressure = pressure;

    // The page budget is re-split every window (~500 ms). Committing the new
    // scale each time would reconfigure the swapchain and reallocate the
    // offscreen texture for changes too small to see, so only apply it when
    // the effective raster actually moved by more than 2%.
    const scaleBefore = this._effectiveScale();
    this._pixelScale = Number.isFinite(pagePixelScale) && pagePixelScale > 0 ? pagePixelScale : 1;
    const scaleAfter = this._effectiveScale();
    if (Math.abs(scaleAfter - scaleBefore) > scaleBefore * 0.02) {
      this._applyRenderScale(true);
    }

    if (this._scaleTrial) {
      if (pressure >= this._scaleTrial.pressureBefore * 0.9) {
        this._qualityScale = this._scaleTrial.from;
        // Cooldown, not a permanent lock: a transiently ineffective trial must
        // not disable raster reduction for the rest of the page's life.
        this._scaleIneffectiveUntil = now + 30000;
        this._applyRenderScale(true);
      }
      this._scaleTrial = null;
    }

    const desired = this._desiredFps || FRAME_RATE;
    const refreshCap = Math.min(MAX_FPS, Math.max(30, Math.round(1000 / core.refreshInterval)));
    // The 20 fps floor belongs to the page-wide ladder, not to the author's
    // explicit per-embed cap: `data-fps="12"` must actually reach 12, or the
    // 12..60 clamp would be a lie for the bottom of its range. With the default
    // 60 this is identical to the old `max(FLOOR, min(desired, …))`.
    const pageCap = Math.max(FLOOR_FPS, Math.min(refreshCap, pageTargetFps));
    const target = Math.min(desired, pageCap);
    const floorReached = pageTargetFps <= FLOOR_FPS;
    const scaleBlocked = this._scaleIneffectiveUntil && now < this._scaleIneffectiveUntil;
    if (floorReached && pressure > 0.25 && !scaleBlocked) {
      this._maybeReduceScale(pressure);
      this._why = "budget:raster";
    } else {
      if (this._qualityScale < 1 && pressure < 0.05) this._maybeRaiseScale();
      this._why = target < desired ? "budget:fps" : "ok";
    }
    if (this._pixelScale < 0.999) {
      this._why = this._why === "ok" ? "budget:pixel" : `${this._why}+pixel`;
    }
    this._targetFps = target;
    void perInstanceMsPerSec; // kept for callers that log the split budget
  }

  /// Step down the raster ladder under sustained pressure. Large embeds skip
  /// rungs: a single 0.85 step on a 2 Mpx raster still asks the GPU for
  /// ~1.45 Mpx, so the trial would burn a whole window to prove nothing. The
  /// stride is in rungs and the ladder's values are untouched, so `indexOf`
  /// still identifies the current step.
  _maybeReduceScale(pressure) {
    const i = QUALITY_STEPS.indexOf(this._qualityScale);
    if (this._scaleTrial || i < 0 || i >= QUALITY_STEPS.length - 1) return;
    const desiredPixels = this._desiredRasterPixels();
    const extraRungs = desiredPixels >= 1_500_000 ? 2 : desiredPixels >= 600_000 ? 1 : 0;
    const next = Math.min(QUALITY_STEPS.length - 1, i + 1 + extraRungs);
    this._scaleTrial = { from: this._qualityScale, pressureBefore: pressure };
    this._qualityScale = QUALITY_STEPS[next];
    this._applyRenderScale(true);
  }

  _maybeRaiseScale() {
    const i = QUALITY_STEPS.indexOf(this._qualityScale);
    if (i <= 0) return;
    this._qualityScale = QUALITY_STEPS[i - 1];
    this._scaleTrial = null;
    this._applyRenderScale(true);
  }

  /// Forget the cached held-frame key. Every path that can change the presented
  /// pixels without changing the playhead must call this, or the shell dedup
  /// would keep showing a stale canvas forever (the same hazard FrameDedup
  /// documents for a failed present).
  _invalidateRestFrame() {
    this._renderRevision = (this._renderRevision || 0) + 1;
    this._lastRenderKey = null;
  }

  _renderScene() {
    if (!this._player) return;
    // The loop's dissolve is a canvas opacity change, not a new frame: it must
    // keep running even while the held frame itself is deduplicated.
    const alpha = this._loopAlpha();
    if (alpha !== this._lastAlpha) {
      this._canvas.style.opacity = String(alpha);
      this._lastAlpha = alpha;
    }
    // A loop rests on its finished frame for the whole `hold` window. The wasm
    // FrameDedup would skip the raster, but the shell still paid a wasm call, a
    // pair of performance.now() reads and a controls sync 60×/s. Skip the tick
    // here whenever nothing the frame depends on changed. `_renderRevision` is
    // bumped by every path that can change the presented pixels without
    // changing `t` (scale, document rebuild, reload, device, seek, play).
    const t = this._playing ? Math.min(this._time, this._duration) : this._restTime;
    if (this._playing) this._restTime = t;
    const key = `${t}|${this._renderScale}|${this._renderRevision}|${this._playing}`;
    const nowMs = performance.now();
    // A short TTL bounds the damage if anything external cleared the canvas
    // (a swapchain reconfigure, a lost device) without going through one of
    // our invalidation hooks: the held frame is redrawn at most twice a second
    // instead of staying blank forever.
    const stale = this._lastRenderWall === undefined || nowMs - this._lastRenderWall > 500;
    if (key === this._lastRenderKey && !stale) return;
    try {
      this._player.render_frame(t);
      this._lastRenderKey = key;
      this._lastRenderWall = nowMs;
      this._renderFailures = 0;
    } catch (err) {
      // Never cache a failed render: the identical tick after it must retry.
      this._lastRenderKey = null;
      this._lastRenderWall = undefined;
      // One warning per failure run. This is the shared rAF loop: a persistent
      // fault (a lost device, an engine that trapped mid-frame) would otherwise
      // print at frame rate and bury everything else on the page.
      this._renderFailures += 1;
      if (this._renderFailures === 1) console.warn("amx-player: render failed", err);
    }
  }
}
