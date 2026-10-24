import * as core from "./core.js";
const {
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, QUALITY_STEPS,
  SLOW_TICK_MS, FAST_TICK_MS, setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS,
  LOAD_MAX_DEFER_MS, LOAD_RETRY_MS, LOAD_STALL_MS, pumpLoads, enqueueLoad, dequeueLoad,
  ASPECTS,
} = core;
import { AmxPlayerView } from "./view.js";
export class AmxPlayerLoad extends AmxPlayerView {
  _maybeStartLoading() {
    if (!this.getAttribute("src") || this._state !== "idle") return;
    enqueueLoad(this);
  }

  async _startLoad() {
    const src = this.getAttribute("src");
    if (!src || this._state !== "idle") return;
    this._state = "loading";
    this._skeleton?.classList.add("loading");
    if (!("gpu" in navigator)) {
      this._state = "error";
      const msg = "This embed needs a WebGPU browser (Chrome 113+, Firefox 141+, Safari 26+).";
      console.warn(`amx-player: ${msg}`);
      this._showVeil(msg, true);
      this.dispatchEvent(new CustomEvent("amxerror", { bubbles: true, detail: { src, error: "WebGPU unsupported" } }));
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
        const msg = `Scene error${first?.line ? ` (line ${first.line})` : ""}: ${first?.message ?? "build failed"}`;
        console.error(`amx-player: ${msg} [src="${src}"]`, diags);
        this._showVeil(msg, true);
        this.dispatchEvent(new CustomEvent("amxerror", { bubbles: true, detail: { src, diagnostics: diags, message: msg } }));
        return;
      }

      this._duration = Math.max(result.duration_s, 0.05);
      this._canvas.width = Math.round(result.width || 1280);
      this._canvas.height = Math.round(result.height || 720);
      if (this._fit() === "contain") {
        this._stage.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
      }
      this._configureCycle();
      // If autoplaying: seed at t = 0 so playback starts cleanly without
      // flashing the finished poster frame. Otherwise: show finished frame.
      const willAutoplay = this._shouldAutoplay();
      if (willAutoplay || this._isPresentation()) {
        this._time = 0;
        this._restTime = 0;
      } else {
        this._restTime = this._duration;
        this._time = this._duration;
      }
      this._renderScene();

      // Timeline landmarks for the scrubber (keyframes, scene starts,
      // transition windows) — the engine knows them, so the bar can show them.
      this._markers = Array.isArray(result.markers) ? result.markers : [];
      this._pausePoints = Array.isArray(result.pause_points)
        ? result.pause_points.slice().sort((a, b) => a - b)
        : [];

      // Swap skeleton for canvas with a smooth crossfade
      this._canvas.hidden = false;
      this._stage.appendChild(this._canvas);
      this._skeleton?.classList.add("fade-out");
      setTimeout(() => {
        if (this._skeleton?.isConnected) this._skeleton.remove();
      }, 300);
      // Only now does the canvas have a laid-out size to match the raster to.
      this._applyRenderScale();
      this._renderScaleObserver = new ResizeObserver(() => this._applyRenderScale(false));
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
      } else if (!this.hasAttribute("controls") && !this._isSealed()) {
        this._playbtn.classList.add("show");
      }
      this.dispatchEvent(new CustomEvent("amxready", { bubbles: true }));
    } catch (err) {
      this._state = "error";
      console.error(`amx-player: failed to load scene "${src}":`, err);
      this._showVeil(`Failed to load scene: ${err?.message ?? err}`, true);
      this.dispatchEvent(new CustomEvent("amxerror", { bubbles: true, detail: { src, error: err } }));
    }
  }

  _buildControls() {
    const strip = document.createElement("div");
    strip.className = "strip";
    strip.tabIndex = 0;
    strip.setAttribute("role", "region");
    strip.setAttribute("aria-label", "Playback controls");

    const playToggle = document.createElement("button");
    playToggle.type = "button";
    playToggle.className = "strip-btn play-toggle";
    playToggle.setAttribute("aria-label", "Play");
    playToggle.innerHTML = ICONS.play;
    playToggle.addEventListener("click", (e) => {
      e.stopPropagation();
      this.togglePlay(false);
    });

    const track = document.createElement("div");
    track.className = "track";
    track.setAttribute("role", "slider");
    track.setAttribute("aria-label", "Timeline");
    track.setAttribute("aria-valuemin", "0");
    track.setAttribute("aria-valuemax", String(this._duration));

    const trackBar = document.createElement("div");
    trackBar.className = "track-bar";

    const fill = document.createElement("div");
    fill.className = "fill";
    const cursor = document.createElement("div");
    cursor.className = "cursor";
    const cursorHandle = document.createElement("div");
    cursorHandle.className = "cursor-handle";
    cursor.appendChild(cursorHandle);

    // Landmarks at percentage positions of the pure timeline: keyframe ticks,
    // scene-start diamonds, hatched transition spans. They live in their own
    // layer so `applySource` can rebuild them when an edit changes the marks
    // or the timeline they are positioned against.
    this._marks = document.createElement("div");
    this._marks.className = "marks";
    this._renderMarks();

    trackBar.append(fill, this._marks, cursor);

    const scrubPill = document.createElement("div");
    scrubPill.className = "scrub-pill";

    track.append(trackBar, scrubPill);

    const chip = document.createElement("span");
    chip.className = "chip";
    const speed = document.createElement("button");
    speed.type = "button";
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

    strip.append(playToggle, track, chip, speed);
    this.shadowRoot.append(strip);

    // ── track scrubbing and magnetic snapping ──
    const findSnap = (t) => {
      const SNAP_S = 0.2;
      let closest = null;
      let minDiff = SNAP_S + 1e-4;
      for (const m of this._markers) {
        const d = Math.abs(t - m.t);
        if (d <= SNAP_S && d < minDiff) {
          minDiff = d;
          closest = { t: m.t, kind: m.kind };
        }
        if (m.kind === "transition" && m.dur > 0) {
          const dEnd = Math.abs(t - (m.t + m.dur));
          if (dEnd <= SNAP_S && dEnd < minDiff) {
            minDiff = dEnd;
            closest = { t: m.t + m.dur, kind: "transition" };
          }
        }
      }
      return closest;
    };

    const updateMarksSnap = (snap) => {
      if (!this._marks) return;
      this._marks.querySelectorAll(".snapped").forEach((el) => el.classList.remove("snapped"));
      if (snap) {
        for (const el of this._marks.children) {
          if (el.dataset.t && Math.abs(Number(el.dataset.t) - snap.t) < 1e-3) {
            el.classList.add("snapped");
          }
        }
        if (navigator.vibrate && this._lastSnappedMarker !== snap.t) {
          try { navigator.vibrate(10); } catch {}
        }
        this._lastSnappedMarker = snap.t;
      } else {
        this._lastSnappedMarker = null;
      }
    };

    const timeAt = (e) => {
      const rect = track.getBoundingClientRect();
      if (rect.width <= 0 || !Number.isFinite(e.clientX)) return { time: this._time, pct: 0, snap: null };
      const frac = Math.min(Math.max((e.clientX - rect.left) / rect.width, 0), 1);
      const raw = frac * this._duration;
      const snap = findSnap(raw);
      const targetTime = snap ? snap.t : raw;
      const snapFrac = this._duration > 0 ? targetTime / this._duration : frac;
      return { time: targetTime, pct: snapFrac, snap };
    };

    const updateScrubPill = (t, pct) => {
      scrubPill.style.left = `${pct * 100}%`;
      const frameNum = Math.round(t * FRAME_RATE);
      scrubPill.textContent = `${t.toFixed(2)}s (f${frameNum})`;
      scrubPill.classList.add("show");
    };

    track.addEventListener("pointerenter", (e) => {
      if (e.pointerType !== "mouse" || this._scrubbing) return;
      const { time, pct, snap } = timeAt(e);
      updateScrubPill(time, pct);
      updateMarksSnap(snap);
    });

    track.addEventListener("pointermove", (e) => {
      const { time, pct, snap } = timeAt(e);
      updateScrubPill(time, pct);
      updateMarksSnap(snap);
      if (this._scrubbing) {
        this.seek(time);
      }
    });

    track.addEventListener("pointerleave", (e) => {
      if (this._scrubbing) return;
      scrubPill.classList.remove("show");
      updateMarksSnap(null);
    });

    track.addEventListener("pointerdown", (e) => {
      try {
        track.setPointerCapture(e.pointerId);
      } catch {}
      this._resumeOnScrubEnd = this._playing;
      if (this._playing) {
        this.pause();
      }
      this._scrubbing = true;
      strip.classList.add("scrubbing");
      const { time, pct, snap } = timeAt(e);
      updateScrubPill(time, pct);
      updateMarksSnap(snap);
      this.seek(time);
      e.stopPropagation();
    });

    const finishScrub = (e) => {
      if (!this._scrubbing) return;
      this._scrubbing = false;
      strip.classList.remove("scrubbing");
      scrubPill.classList.remove("show");
      updateMarksSnap(null);
      if (this._resumeOnScrubEnd) {
        this._resumeOnScrubEnd = false;
        this.play();
      }
    };

    track.addEventListener("pointerup", finishScrub);
    track.addEventListener("pointercancel", finishScrub);

    strip.addEventListener("keydown", (e) => this._handleKeyDown(e));

    this._controls = { strip, playToggle, track, fill, cursor, scrubPill, chip, speed };
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
        span.dataset.t = String(m.t);
        marks.appendChild(span);
      } else {
        const mark = document.createElement("div");
        mark.className = m.kind === "scene" ? "diamond" : (m.kind === "step" ? "step" : "tick");
        mark.style.left = `${frac * 100}%`;
        mark.dataset.t = String(m.t);
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

    // Sync play/pause toggle button icon and aria-label
    if (this._playing) {
      c.playToggle.innerHTML = ICONS.pause;
      c.playToggle.setAttribute("aria-label", "Pause");
    } else if (this._time >= this._duration && !this._looping()) {
      c.playToggle.innerHTML = ICONS.replay;
      c.playToggle.setAttribute("aria-label", "Replay");
    } else {
      c.playToggle.innerHTML = ICONS.play;
      c.playToggle.setAttribute("aria-label", "Play");
    }

    if (this._debugFrame) {
      // `rs` is the adaptive render scale the quality controller settled on.
      // It belongs in the report because a shimmer that only appears at a
      // reduced raster scale is a different defect from a geometry one.
      const rs = this._renderScale ?? 1;
      c.chip.textContent =
        `f${this.frame} / ${this.totalFrames}  ${shown.toFixed(3)}s  @${FRAME_RATE}fps  rs${rs.toFixed(2)}`;
      c.chip.classList.add("dbg");
    } else {
      c.chip.textContent = `${shown.toFixed(1)} / ${this._duration.toFixed(1)}`;
      c.chip.classList.remove("dbg");
    }
    c.track.setAttribute("aria-valuenow", shown.toFixed(1));
  }
}
