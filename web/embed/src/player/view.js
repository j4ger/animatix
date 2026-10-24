import * as core from "./core.js";
const {
  FRAME_RATE, DEBUG_BY_URL, LOADER_SCRIPT, RUNTIME_BASE, engineBases, REDUCED_MOTION,
  SAVE_DATA, FADE_EACH, ICONS, enginePromises, loadEngine, instances, QUALITY_STEPS,
  SLOW_TICK_MS, FAST_TICK_MS, setQualityStep, tick, ensureLoop, SCROLL_SETTLE_MS,
  LOAD_MAX_DEFER_MS, LOAD_RETRY_MS, LOAD_STALL_MS, pumpLoads, enqueueLoad, dequeueLoad,
  ASPECTS,
} = core;
import { AmxPlayerBase } from "./base.js";
export class AmxPlayerView extends AmxPlayerBase {
  _renderSkeleton() {
    // Reserve the figure's box up front so lazy loading never reflows the
    // page: the stage carries the aspect ratio until the scene reports its
    // own, and the control bar (when present) sits below it in flow.
    this._stage = document.createElement("div");
    this._stage.className = "stage";
    const title = this.getAttribute("title") || "";
    const style = document.createElement("style");
    style.textContent = `
      :host { display: block; position: relative; overflow: hidden;
              border-radius: 8px; background: #0a0f17; }
      .stage { position: relative; overflow: hidden; }
      .skeleton {
        position: absolute; inset: 0;
        display: flex; align-items: center; justify-content: center;
        background: linear-gradient(120deg, #0a0f17 40%, #141c28 50%, #0a0f17 60%);
        background-size: 300% 100%;
        animation: shimmer 2.2s linear infinite;
        color: #808fa6; font: 13px/1.4 system-ui, sans-serif;
        transition: opacity 0.28s cubic-bezier(0.2, 0, 0, 1);
      }
      .skeleton.fade-out { opacity: 0; pointer-events: none; }
      .skeleton-badge {
        display: inline-flex; align-items: center; gap: 8px;
        background: rgba(5,8,12,0.65); border: 1px solid rgba(255,255,255,0.1);
        border-radius: 20px; padding: 6px 14px; font: 12px system-ui, sans-serif;
        color: #c7cfd9;
      }
      .skeleton-spinner {
        width: 12px; height: 12px; border: 2px solid rgba(245,185,66,0.25);
        border-top-color: #f5b942; border-radius: 50%;
        animation: spin 0.8s linear infinite; display: none;
      }
      .skeleton.loading .skeleton-spinner { display: inline-block; }
      @keyframes spin { to { transform: rotate(360deg); } }
      @keyframes shimmer { to { background-position: -300% 0; } }
      canvas { position: absolute; inset: 0; width: 100%; height: 100%;
               display: block; object-fit: contain; }
      .hud-flash {
        position: absolute; left: 50%; top: 50%;
        transform: translate(-50%, -50%) scale(0.85);
        width: 64px; height: 64px; border-radius: 50%;
        background: rgba(5,8,12,0.75); border: 1px solid rgba(245,185,66,0.45);
        color: #f5b942; display: flex; align-items: center; justify-content: center;
        pointer-events: none; opacity: 0;
        transition: opacity 0.3s ease-out, transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
        z-index: 5;
      }
      .hud-flash.show { opacity: 1; transform: translate(-50%, -50%) scale(1.15); }
      .hud-flash svg { width: 26px; height: 26px; display: block; }
      .hud-skip {
        position: absolute; top: 50%; transform: translateY(-50%);
        padding: 6px 14px; border-radius: 18px;
        background: rgba(5,8,12,0.85); border: 1px solid rgba(255,255,255,0.15);
        color: #f5b942; font: 12px ui-monospace, monospace;
        pointer-events: none; opacity: 0;
        transition: opacity 0.25s, transform 0.25s; z-index: 5;
      }
      .hud-skip.left { left: 10%; }
      .hud-skip.right { right: 10%; }
      .hud-skip.show { opacity: 1; }
      .veil {
        position: absolute; inset: 0; display: flex; flex-direction: column;
        align-items: center; justify-content: center; gap: 10px;
        color: #8b96a7; font: 13px/1.4 system-ui, sans-serif;
        background: rgba(13,16,22,0.75); opacity: 0; transition: opacity .3s;
        pointer-events: none; text-align: center; padding: 16px;
      }
      .veil.show { opacity: 1; pointer-events: auto; }
      .veil.error { color: #ef6a6a; }
      .veil .retry-btn {
        margin-top: 8px; padding: 6px 16px; border-radius: 6px;
        border: 1px solid rgba(255,255,255,0.25); background: rgba(255,255,255,0.12);
        color: #fff; cursor: pointer; font: 12px system-ui, sans-serif;
        transition: background 0.15s;
      }
      .veil .retry-btn:hover { background: rgba(255,255,255,0.22); }
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
      .strip {
        position: relative; height: 44px;
        display: flex; align-items: center;
        border-top: 1px solid rgba(255,255,255,.07);
        cursor: pointer; touch-action: none;
        user-select: none; -webkit-user-select: none;
        padding: 0 4px; gap: 2px;
      }
      .strip:focus-visible { outline: 2px solid #f5b942; outline-offset: -2px; }
      .strip-btn {
        width: 36px; height: 36px; flex: none;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        display: flex; align-items: center; justify-content: center;
        transition: background 0.15s, color 0.15s;
      }
      .strip-btn:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .strip-btn:focus-visible { outline: 2px solid #f5b942; }
      .strip-btn svg { width: 18px; height: 18px; display: block; }
      .track {
        position: relative; flex: 1; height: 100%;
        display: flex; align-items: center; margin: 0 4px;
        cursor: pointer;
      }
      .track-bar {
        position: absolute; left: 0; right: 0; height: 4px;
        background: rgba(255,255,255,0.14); border-radius: 2px;
        overflow: visible; transition: height 0.15s;
      }
      .strip:hover .track-bar, .strip.scrubbing .track-bar { height: 6px; }
      .fill {
        position: absolute; left: 0; top: 0; bottom: 0; width: 100%;
        background: #f5b942; border-radius: 2px;
        transform-origin: left; transform: scaleX(0);
        pointer-events: none;
      }
      .marks { position: absolute; inset: 0; pointer-events: none; }
      .strip .tick {
        position: absolute; top: -3px; bottom: -3px; width: 2px;
        transform: translateX(-50%);
        background: rgba(255,255,255,.32);
        pointer-events: none; transition: background 0.15s, transform 0.15s, box-shadow 0.15s;
      }
      .strip .tick.snapped {
        background: #f5b942; transform: translateX(-50%) scaleY(1.3);
        box-shadow: 0 0 6px rgba(245,185,66,0.6);
      }
      .strip .diamond {
        position: absolute; top: 50%; width: 9px; height: 9px;
        transform: translate(-50%, -50%) rotate(45deg);
        background: #8ab4f8; border-radius: 2px;
        pointer-events: none; transition: transform 0.15s, box-shadow 0.15s;
      }
      .strip .diamond.snapped {
        background: #f5b942; transform: translate(-50%, -50%) rotate(45deg) scale(1.3);
        box-shadow: 0 0 8px rgba(245,185,66,0.8);
      }
      .strip .step {
        position: absolute; top: -4px; bottom: -4px; width: 3px;
        transform: translateX(-50%);
        background: #34d399; border-radius: 1px;
        pointer-events: none; transition: background 0.15s, transform 0.15s, box-shadow 0.15s;
      }
      .strip .step.snapped {
        background: #f5b942; transform: translateX(-50%) scaleY(1.3);
        box-shadow: 0 0 8px rgba(52,211,153,0.8);
      }
      .stage.blackout canvas {
        opacity: 0 !important;
      }
      .strip .span {
        position: absolute; top: 0; bottom: 0;
        background: repeating-linear-gradient(135deg,
          rgba(138,180,248,.35) 0 4px, transparent 4px 8px);
        pointer-events: none;
      }
      .cursor {
        position: absolute; top: 0; bottom: 0; width: 2px; left: 0;
        transform: translateX(-50%);
        background: #f5b942; pointer-events: none;
      }
      .cursor-handle {
        position: absolute; top: 50%; left: 50%; width: 10px; height: 10px;
        transform: translate(-50%, -50%) scale(0);
        background: #f5b942; border-radius: 50%;
        box-shadow: 0 1px 4px rgba(0,0,0,0.5);
        transition: transform 0.15s ease-out; pointer-events: none;
      }
      .strip:hover .cursor-handle, .strip.scrubbing .cursor-handle {
        transform: translate(-50%, -50%) scale(1);
      }
      .scrub-pill {
        position: absolute; bottom: 100%; left: 0;
        transform: translate(-50%, -8px);
        background: rgba(10,15,23,0.92); border: 1px solid rgba(255,255,255,0.18);
        box-shadow: 0 4px 12px rgba(0,0,0,0.4);
        color: #e8edf4; font: 11px/1.3 ui-monospace, monospace;
        padding: 3px 8px; border-radius: 6px; white-space: nowrap;
        pointer-events: none; opacity: 0; transition: opacity 0.15s;
        z-index: 10;
      }
      .scrub-pill.show { opacity: 1; }
      .chip {
        flex: none;
        color: #c7cfd9; font: 12px ui-monospace, monospace; white-space: nowrap;
        font-variant-numeric: tabular-nums;
        background: rgba(5,8,12,.55); border-radius: 6px; padding: 3px 8px;
        pointer-events: none;
      }
      .chip.dbg { color: #8ab4f8; background: rgba(5,8,12,.8); }
      .speed {
        flex: none; width: 44px; height: 36px;
        border: none; border-radius: 8px; background: none;
        color: #c7cfd9; cursor: pointer; padding: 0;
        font: 12px ui-monospace, monospace;
        display: flex; align-items: center; justify-content: center;
      }
      .speed:hover { background: rgba(255,255,255,.14); color: #e8edf4; }
      .speed:focus-visible { outline: 2px solid #f5b942; }
      @media (pointer: coarse) {
        .strip { height: 52px; padding: 0 6px; gap: 4px; }
        .strip-btn { width: 44px; height: 44px; }
        .strip-btn svg { width: 22px; height: 22px; }
        .speed { width: 48px; height: 44px; }
        .cursor-handle { width: 14px; height: 14px; transform: translate(-50%, -50%) scale(1); }
        .scrub-pill {
          transform: translate(-50%, -46px);
          font-size: 13px; padding: 5px 10px; border-radius: 8px;
        }
      }
      @media (max-width: 480px) {
        .chip { font-size: 11px; padding: 2px 6px; }
      }
      @media (max-width: 360px) {
        .chip { display: none; }
      }
    `;
    this.shadowRoot.replaceChildren(style, this._stage);
    this._skeleton = document.createElement("div");
    this._skeleton.className = "skeleton";
    const badge = document.createElement("div");
    badge.className = "skeleton-badge";
    const spinner = document.createElement("span");
    spinner.className = "skeleton-spinner";
    const titleSpan = document.createElement("span");
    titleSpan.className = "skeleton-title";
    titleSpan.textContent = title ? `${title}` : "animatix scene";
    badge.append(spinner, titleSpan);
    this._skeleton.appendChild(badge);
    this._stage.appendChild(this._skeleton);

    this._canvas = document.createElement("canvas");
    this._canvas.width = 1280;
    this._canvas.height = 720;
    this._canvas.hidden = true;

    this._hudFlash = document.createElement("div");
    this._hudFlash.className = "hud-flash";
    this._stage.appendChild(this._hudFlash);

    this._hudSkip = document.createElement("div");
    this._hudSkip.className = "hud-skip";
    this._stage.appendChild(this._hudSkip);

    this._veil = document.createElement("div");
    this._veil.className = "veil";

    this._playbtn = document.createElement("button");
    this._playbtn.className = "playbtn";
    this._playbtn.setAttribute("aria-label", "Play");
    this._playbtn.innerHTML = ICONS.playBig;
    this._playbtn.addEventListener("click", () => this.play());

    this._controls = null;
    this._marks = null;
    this._applyFit();
  }

  _showVeil(text, isError) {
    this._veil.className = `veil show${isError ? " error" : ""}`;
    this._veil.replaceChildren();
    const msg = document.createElement("div");
    msg.textContent = text;
    this._veil.appendChild(msg);
    if (isError) {
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "retry-btn";
      retry.textContent = "Retry";
      retry.addEventListener("click", () => this._resetForReload());
      this._veil.appendChild(retry);
    }
    if (!this._veil.isConnected) this.shadowRoot.appendChild(this._veil);
  }

  _hideVeil() {
    this._veil.className = "veil";
  }

  _triggerHud(iconSvg) {
    if (!this._hudFlash) return;
    this._hudFlash.innerHTML = iconSvg;
    this._hudFlash.classList.remove("show");
    void this._hudFlash.offsetWidth; // re-trigger animation
    this._hudFlash.classList.add("show");
    clearTimeout(this._hudTimer);
    this._hudTimer = setTimeout(() => {
      this._hudFlash?.classList.remove("show");
    }, 320);
  }

  _triggerSkipHud(text, isRight) {
    if (!this._hudSkip) return;
    this._hudSkip.className = `hud-skip ${isRight ? "right" : "left"} show`;
    this._hudSkip.textContent = text;
    clearTimeout(this._skipHudTimer);
    this._skipHudTimer = setTimeout(() => {
      this._hudSkip?.classList.remove("show");
    }, 450);
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
          // A sealed embed with autoplay has no visible control to bring it
          // back after the offscreen pause above — resuming on re-entry is
          // the whole contract of the attribute pair.
          if (this._visible && this._isSealed() && !this._playing) this._resumeSealed();
        }
      },
      { rootMargin: "200px" },
    );
    this._observer.observe(this);
  }

  /// Re-play a sealed embed that autoplayed once and was paused (offscreen,
  /// or an earlier seek-pause from the driving page). Guarded by the same
  /// conditions as the first autoplay, so reduced-motion and save-data pages
  /// keep their static poster.
  _resumeSealed() {
    if (this._state !== "ready" || !this.hasAttribute("autoplay")) return;
    if (REDUCED_MOTION || SAVE_DATA) return;
    this.play();
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

  /// Rebuild the scene from edited text — the live-editor entry point. The
  /// player and everything it has registered (fonts, fetched imports, assets)
  /// stays; only the document changes. Returns the build's diagnostics array
  /// on success. Throws on a failed build (with `.diagnostics` attached) and
  /// the previous scene keeps playing, so a typo never blanks the figure.
  applySource(text) {
    if (this._state !== "ready" || !this._player) {
      throw new Error("the player has not finished loading yet");
    }
    const result = this._player.load_source(text);
    const diags = result?.diagnostics ?? [];
    if (!result?.ok) {
      const err = new Error(
        diags.find((d) => d.severity === "error")?.message ?? "build failed",
      );
      err.diagnostics = diags;
      throw err;
    }
    this._duration = Math.max(result.duration_s, 0.05);
    if (result.width && result.height) {
      this._canvas.width = Math.round(result.width);
      this._canvas.height = Math.round(result.height);
      if (this._fit() === "contain") {
        this._stage.style.aspectRatio = `${this._canvas.width} / ${this._canvas.height}`;
      }
    }
    this._markers = Array.isArray(result.markers) ? result.markers : [];
    this._pausePoints = Array.isArray(result.pause_points)
      ? result.pause_points.slice().sort((a, b) => a - b)
      : [];
    this._configureCycle();
    const strip = this.shadowRoot?.querySelector(".strip");
    if (strip) strip.setAttribute("aria-valuemax", String(this._duration));
    const track = this.shadowRoot?.querySelector(".track");
    if (track) track.setAttribute("aria-valuemax", String(this._duration));
    // The edit may have moved or added keyframes, so re-draw the landmark
    // layer the strip's ticks come from; snapping already reads `_markers`,
    // and leaving the drawn marks stale would point the playhead at nothing.
    this._renderMarks();
    // A resolution change above reset the backing store to the scene's own
    // size; put the display-matched scale back.
    this._applyRenderScale();
    if (this._playing) {
      this._time = 0;
      this._restTime = 0;
    } else {
      // Park on the finished frame — frame 0 of these scenes is an empty
      // stage, and an editor who hit Apply on a paused figure should see
      // what they built, not nothing.
      this._restTime = this._duration;
      this._time = this._duration;
    }
    this._renderScene();
    return diags;
  }

  /// Queue the scene load. Everything that could make a player start on its own
}
