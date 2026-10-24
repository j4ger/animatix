import * as core from "./core.js";
const {
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, QUALITY_STEPS,
  SLOW_TICK_MS, FAST_TICK_MS, setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS,
  LOAD_MAX_DEFER_MS, LOAD_RETRY_MS, LOAD_STALL_MS, pumpLoads, enqueueLoad, dequeueLoad,
  ASPECTS,
} = core;
import { AmxPlayerLoad } from "./load.js";
export class AmxPlayerInput extends AmxPlayerLoad {
  _setupGestures() {
    this._canvas.tabIndex = 0;
    this._canvas.addEventListener("pointerdown", (e) => {
      if (!this._controls && !this.hasAttribute("controls") && this._isSealed()) return;
      this._canvas.focus({ preventScroll: true });

      if (e.pointerType === "mouse") {
        if (this._isPresentation()) {
          this.stepNext();
        } else {
          this.togglePlay(true);
        }
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

        if (this._isPresentation()) {
          if (relX < 0.35) {
            this.stepPrev();
          } else {
            this.stepNext();
          }
        } else if (relX < 0.35) {
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
          if (this._isPresentation()) {
            this.stepNext();
          } else {
            this.togglePlay(true);
          }
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
    if (this._isPresentation()) {
      if (
        e.key === " " ||
        e.key === "ArrowRight" ||
        e.key === "PageDown" ||
        e.key === "Enter" ||
        e.key === "n" ||
        e.key === "N"
      ) {
        e.preventDefault();
        e.stopPropagation();
        this.stepNext();
        return;
      }
      if (
        e.key === "ArrowLeft" ||
        e.key === "PageUp" ||
        e.key === "p" ||
        e.key === "P" ||
        e.key === "Backspace"
      ) {
        e.preventDefault();
        e.stopPropagation();
        this.stepPrev();
        return;
      }
      if (e.key === "b" || e.key === "B") {
        e.preventDefault();
        e.stopPropagation();
        this._blackout = !this._blackout;
        this._stage.classList.toggle("blackout", this._blackout);
        this._triggerSkipHud(this._blackout ? "Blackout" : "Resume", true);
        return;
      }
      if (e.key === "f" || e.key === "F") {
        e.preventDefault();
        e.stopPropagation();
        if (!document.fullscreenElement) {
          this.requestFullscreen?.().catch(() => {});
        } else {
          document.exitFullscreen?.().catch(() => {});
        }
        return;
      }
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

  _isPresentation() {
    return this.hasAttribute("presentation") || this.getAttribute("mode") === "presentation";
  }

  get presentation() {
    return this._isPresentation();
  }

  set presentation(val) {
    if (val) {
      this.setAttribute("presentation", "");
    } else {
      this.removeAttribute("presentation");
    }
  }

  get pausePoints() {
    return [...this._pausePoints];
  }

  /// Advance to the next presentation pause point, or resume playing towards it.
  stepNext() {
    if (this._state !== "ready") return;
    const nextP = this._pausePoints.find((p) => p > this._time + 0.005);
    if (nextP !== undefined) {
      if (this._playing) {
        // Fast-forward immediately to the target step and pause
        this.seek(nextP);
        this.pause();
        this._triggerStepHud();
      } else {
        // Resume playing towards nextP
        this._pausedAtStep = false;
        this.play();
        this._triggerSkipHud("Step »", true);
      }
    } else if (this._time >= this._duration) {
      this.seek(0);
      this._pausedAtStep = false;
      this.play();
      this._triggerSkipHud("Restart", true);
    } else {
      this._pausedAtStep = false;
      this.play();
      this._triggerSkipHud("End »", true);
    }
  }

  /// Step backward to the previous presentation pause point.
  stepPrev() {
    if (this._state !== "ready") return;
    const prevP = [...this._pausePoints].reverse().find((p) => p < this._time - 0.005);
    if (prevP !== undefined) {
      this.seek(prevP);
      this.pause();
      this._triggerStepHud();
    } else {
      this.seek(0);
      this.pause();
      this._triggerSkipHud("« Start", false);
    }
  }
}
