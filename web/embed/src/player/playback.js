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
